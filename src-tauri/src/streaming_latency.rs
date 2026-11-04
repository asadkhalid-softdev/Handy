//! Verified streaming operating points. Keep these model-specific: identical
//! latency labels can require different encoder context settings.
use std::collections::HashMap;

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;
use transcribe_cpp::{
    ParakeetBufferedStreamOptions, ParakeetStreamOptions, StreamExtension, StreamOptions,
};

use crate::settings::{get_settings, write_settings, AppSettings};

#[derive(Clone, Serialize, Type)]
pub struct StreamingLatencyChoices {
    pub default_ms: u32,
    pub presets: Vec<StreamingLatencyPreset>,
}

#[derive(Clone, Serialize, Type)]
pub struct StreamingLatencyPreset {
    pub latency_ms: u32,
    pub accuracy_warning: bool,
}

#[derive(Clone, Copy)]
enum ModelKind {
    NemotronEnglish,
    UnifiedEnglish,
}

fn model_kind(model_id: &str) -> Option<ModelKind> {
    let (repo, filename) = model_id.rsplit_once('/')?;
    // Accept every catalog quant, but not similarly named foreign models.
    crate::catalog::file_in_catalog(filename, Some(repo))?;
    match repo {
        "handy-computer/nemotron-speech-streaming-en-0.6b-gguf" => Some(ModelKind::NemotronEnglish),
        "handy-computer/parakeet-unified-en-0.6b-gguf" => Some(ModelKind::UnifiedEnglish),
        _ => None,
    }
}

// Source: transcribe.cpp/docs/models/{nemotron-speech-streaming-en-0.6b,
// parakeet-unified-en-0.6b}.md. Unified latency is chunk + right context.
fn extension(kind: ModelKind, ms: u32) -> Option<StreamExtension> {
    match kind {
        ModelKind::NemotronEnglish => {
            let right = match ms {
                80 => 0,
                160 => 1,
                560 => 6,
                1120 => 13,
                _ => return None,
            };
            Some(StreamExtension::ParakeetStream(ParakeetStreamOptions {
                att_context_right: Some(right),
            }))
        }
        ModelKind::UnifiedEnglish => {
            let (chunk, right) = match ms {
                80 => (80, 0),
                160 => (80, 80),
                320 => (160, 160),
                480 => (160, 320),
                1120 => (560, 560),
                2080 => (1040, 1040),
                _ => return None,
            };
            Some(StreamExtension::ParakeetBuffered(
                ParakeetBufferedStreamOptions {
                    left_ms: Some(5600),
                    chunk_ms: Some(chunk),
                    right_ms: Some(right),
                },
            ))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_streaming_latency_choices(model_id: String) -> Option<StreamingLatencyChoices> {
    let kind = model_kind(&model_id)?;
    let (default_ms, values): (u32, &[u32]) = match kind {
        ModelKind::NemotronEnglish => (1120, &[80, 160, 560, 1120]),
        ModelKind::UnifiedEnglish => (2080, &[80, 160, 320, 480, 1120, 2080]),
    };
    Some(StreamingLatencyChoices {
        default_ms,
        presets: values
            .iter()
            .map(|&latency_ms| StreamingLatencyPreset {
                latency_ms,
                accuracy_warning: matches!(kind, ModelKind::UnifiedEnglish) && latency_ms == 80,
            })
            .collect(),
    })
}

fn validate(values: &HashMap<String, u32>) -> Result<(), String> {
    for (id, &ms) in values {
        if model_kind(id)
            .and_then(|kind| extension(kind, ms))
            .is_none()
        {
            return Err(format!("Unsupported streaming latency {ms} ms for {id}"));
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_streaming_latency_settings(
    app: AppHandle,
    values: HashMap<String, u32>,
) -> Result<(), String> {
    validate(&values)?;
    let mut settings = get_settings(&app);
    settings.streaming_latency_ms = values;
    write_settings(&app, settings);
    Ok(())
}

/// A new recording snapshots this setting; active streams are never changed.
/// Invalid persisted values safely retain the engine default.
pub fn stream_options(model_id: &str, settings: &AppSettings) -> StreamOptions {
    let family = settings
        .streaming_latency_ms
        .get(model_id)
        .and_then(|&ms| model_kind(model_id).and_then(|kind| extension(kind, ms)));
    StreamOptions {
        family,
        ..StreamOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(base: &str) -> String {
        format!("handy-computer/{base}-gguf/{base}-Q8_0.gguf")
    }

    #[test]
    fn operating_points_survive_worker_protocol() {
        for base in [
            "parakeet-unified-en-0.6b",
            "nemotron-speech-streaming-en-0.6b",
        ] {
            let id = id(base);
            let choices = get_streaming_latency_choices(id.clone()).unwrap();
            let mut settings = AppSettings::default();
            assert_eq!(stream_options(&id, &settings), StreamOptions::default());
            for preset in choices.presets {
                settings
                    .streaming_latency_ms
                    .insert(id.clone(), preset.latency_ms);
                validate(&settings.streaming_latency_ms).unwrap();
                let options = stream_options(&id, &settings);
                let wire = serde_json::to_string(&options).unwrap();
                let decoded: StreamOptions = serde_json::from_str(&wire).unwrap();
                assert_eq!(options, decoded);
                match decoded.family.unwrap() {
                    StreamExtension::ParakeetBuffered(o) => {
                        assert_eq!(o.left_ms, Some(5600));
                        assert_eq!(
                            (o.chunk_ms.unwrap() + o.right_ms.unwrap()) as u32,
                            preset.latency_ms
                        );
                    }
                    StreamExtension::ParakeetStream(o) => {
                        assert_eq!(
                            ((o.att_context_right.unwrap() + 1) * 80) as u32,
                            preset.latency_ms
                        );
                    }
                    _ => panic!("Wrong model family"),
                }
            }
        }
    }

    #[test]
    fn invalid_and_unrelated_settings_do_not_override_defaults() {
        let id = id("parakeet-unified-en-0.6b");
        let mut settings = AppSettings::default();
        settings.streaming_latency_ms.insert(id.clone(), 161);
        assert!(validate(&settings.streaming_latency_ms).is_err());
        assert_eq!(stream_options(&id, &settings), StreamOptions::default());
        assert!(get_streaming_latency_choices(id.replace("handy-computer/", "foreign/")).is_none());
        assert!(get_streaming_latency_choices("whisper-small".into()).is_none());
        let mut saved = serde_json::to_value(&settings).unwrap();
        saved
            .as_object_mut()
            .unwrap()
            .remove("streaming_latency_ms");
        let legacy: AppSettings = serde_json::from_value(saved).unwrap();
        assert!(legacy.streaming_latency_ms.is_empty());
    }
}

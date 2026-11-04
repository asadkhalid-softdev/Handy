//! Windows wake-phrase dictation and its persistent floating control.
//!
//! The OS recognizer spots only the wake phrase on the default Windows mic.
//! Handy's existing recorder/model owns transcription. Pause kills the detector
//! and cancels this controller's session, including an in-flight paste.
pub(crate) mod endpoint;

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Listener, Manager};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct VoiceControlSettings {
    /// Master feature switch; pausing listening does not disable the feature.
    pub feature_enabled: bool,
    pub enabled: bool,
    pub phrase: String,
    pub silence_ms: u64,
    /// Physical screen coordinates, including negative multi-monitor positions.
    pub position: Option<(i32, i32)>,
}

impl Default for VoiceControlSettings {
    fn default() -> Self {
        Self {
            feature_enabled: true,
            enabled: false,
            phrase: "hey handy".into(),
            silence_ms: 2000,
            position: None,
        }
    }
}

impl VoiceControlSettings {
    fn listening_enabled(&self) -> bool {
        self.feature_enabled && self.enabled
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct VoiceStatus {
    pub feature_enabled: bool,
    pub enabled: bool,
    /// disabled, paused, starting, listening, arming, recording, processing, error
    pub phase: String,
    pub phrase: String,
    pub silence_ms: u64,
}

impl VoiceStatus {
    fn new(settings: &VoiceControlSettings) -> Self {
        Self {
            feature_enabled: settings.feature_enabled,
            enabled: false,
            phase: if settings.feature_enabled {
                "paused"
            } else {
                "disabled"
            }
            .into(),
            phrase: settings.phrase.clone(),
            silence_ms: settings.silence_ms,
        }
    }
}

// The capture delivery thread updates this only for a voice-owned take.
// A keyboard recording never acquires an automatic silence deadline.
static ENDPOINT: OnceLock<Mutex<Option<endpoint::Endpoint>>> = OnceLock::new();

pub fn speech_frame() {
    crate::codex_talk::speech_frame();
    if let Some(endpoint) = ENDPOINT.get() {
        if let Ok(mut guard) = endpoint.lock() {
            if let Some(endpoint) = guard.as_mut() {
                endpoint.speech(Instant::now());
            }
        }
    }
}

fn set_endpoint(endpoint: Option<endpoint::Endpoint>) {
    if let Ok(mut guard) = ENDPOINT.get_or_init(|| Mutex::new(None)).lock() {
        *guard = endpoint;
    }
}

pub(crate) enum Message {
    Configure(VoiceControlSettings),
    Detector(u64, String),
    DetectorError(u64, String),
    Finished(u64),
    CaptureReady,
    Position(i32, i32),
    Shutdown,
}

pub struct VoiceControl {
    tx: mpsc::Sender<Message>,
    status: Arc<Mutex<VoiceStatus>>,
}

impl VoiceControl {
    fn publish(&self, app: &AppHandle, phase: &str, enabled: bool) {
        if let Ok(mut state) = self.status.lock() {
            state.phase = phase.into();
            state.enabled = enabled;
            let _ = app.emit("voice-control-status", state.clone());
        }
    }
}

pub fn pipeline_finished(app: &AppHandle, token: Option<u64>) {
    let Some(token) = token else {
        return;
    };
    crate::codex_talk::pipeline_finished(app, token);
    if let Some(control) = app.try_state::<VoiceControl>() {
        let _ = control.tx.send(Message::Finished(token));
    }
}

pub fn shutdown(app: &AppHandle) {
    if let Some(control) = app.try_state::<VoiceControl>() {
        let _ = control.tx.send(Message::Shutdown);
    }
}

fn validate(settings: &VoiceControlSettings) -> Result<(), String> {
    let words = settings.phrase.split_whitespace().count();
    if !(1..=5).contains(&words)
        || settings.phrase.len() > 60
        || !settings
            .phrase
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '\'')
    {
        return Err(
            "Use an English wake word or phrase of one to five words (letters and spaces).".into(),
        );
    }
    if !(800..=10_000).contains(&settings.silence_ms) {
        return Err("Silence timeout must be between 800 and 10000 milliseconds.".into());
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_voice_control_status(app: AppHandle) -> Result<VoiceStatus, String> {
    app.state::<VoiceControl>()
        .status
        .lock()
        .map(|state| state.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn configure_voice_control(
    app: AppHandle,
    feature_enabled: bool,
    enabled: bool,
    phrase: String,
    silence_ms: u64,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Wake-phrase control currently requires Windows.".into());
    }
    let mut settings = crate::settings::get_settings(&app);
    let config = VoiceControlSettings {
        feature_enabled,
        enabled: enabled && feature_enabled,
        phrase: phrase
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
        silence_ms,
        position: settings.voice_control.position,
    };
    validate(&config)?;
    crate::codex_talk::validate_wake_conflict(&settings.codex_talk, &config.phrase)?;
    // The detector owns the idle microphone. Keep the transcription recorder
    // on demand so Pause really releases background microphone capture.
    if config.enabled && settings.always_on_microphone {
        return Err("Turn off Always-on Microphone before enabling wake-phrase control.".into());
    }
    settings.voice_control = config.clone();
    crate::settings::write_settings(&app, settings);
    app.state::<VoiceControl>()
        .tx
        .send(Message::Configure(config))
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn show_voice_control(app: AppHandle) -> Result<(), String> {
    if !crate::settings::get_settings(&app)
        .voice_control
        .feature_enabled
    {
        return Err("Enable Wake-Talk to show the floating control.".into());
    }
    let window = app
        .get_webview_window("voice_control")
        .ok_or("Voice control is available on Windows only")?;
    window.show().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn open_wake_talk(app: AppHandle) -> Result<(), String> {
    crate::show_main_window(&app);
    app.emit_to("main", "show-wake-talk", ())
        .map_err(|e| e.to_string())
}

pub fn initialize(app: &AppHandle) {
    let settings = crate::settings::get_settings(app).voice_control;
    let (tx, rx) = mpsc::channel();
    let status = Arc::new(Mutex::new(VoiceStatus::new(&settings)));
    app.manage(VoiceControl {
        tx: tx.clone(),
        status,
    });
    let ready = tx.clone();
    app.listen("recording-ready", move |_| {
        let _ = ready.send(Message::CaptureReady);
    });
    #[cfg(target_os = "windows")]
    {
        if let Err(error) = create_window(app, &settings) {
            report_error(app, &error);
        }
        let handle = app.clone();
        std::thread::spawn(move || run(handle, tx, rx, settings));
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (rx, tx);
}

#[cfg(target_os = "windows")]
fn report_error(app: &AppHandle, detail: &str) {
    use crate::notices::{self, Notice, NoticeCode, NoticeSource};
    app.state::<VoiceControl>().publish(app, "error", false);
    notices::raise_without_overlay(
        app,
        Notice::new(NoticeCode::WakeWordFailed, NoticeSource::WakeWord)
            .with_detail(Some(detail.to_owned())),
    );
}

#[cfg(target_os = "windows")]
pub(crate) struct Detector(pub(crate) std::process::Child);

#[cfg(target_os = "windows")]
impl Drop for Detector {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn spawn_detector(
    phrase: &str,
    generation: u64,
    tx: mpsc::Sender<Message>,
) -> Result<Detector, String> {
    use std::io::{BufRead, BufReader};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let windows = std::env::var_os("SystemRoot").ok_or("Windows directory unavailable")?;
    let executable =
        std::path::PathBuf::from(windows).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(executable)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("detector.ps1"),
        ])
        .env("HANDY_WAKE_PHRASE", phrase)
        .env("HANDY_WAKE_PARENT", std::process::id().to_string())
        .env_remove("HANDY_WAKE_TEST_WAV")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("Could not start the Windows wake detector: {e}"))?;
    if let Some(stdout) = child.stdout.take() {
        let output = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if output.send(Message::Detector(generation, line)).is_err() {
                    break;
                }
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(Message::DetectorError(generation, line));
            }
        });
    }
    Ok(Detector(child))
}

#[cfg(target_os = "windows")]
fn run(
    app: AppHandle,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    initial: VoiceControlSettings,
) {
    use crate::transcription_coordinator::TranscriptionCoordinator;
    use std::time::Duration;
    let mut detector: Option<Detector> = None;
    let mut generation = 0;
    let mut session: Option<u64> = None;
    let mut next_session = 0;
    let mut phase = "paused";
    let mut enabled = false;
    let mut config = initial.clone();
    let mut helper_started = Instant::now();
    let mut pending_position: Option<(i32, i32, Instant)> = None;
    let mut cooldown = Instant::now();
    let _ = tx.send(Message::Configure(initial));
    loop {
        let message = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => Some(message),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match message {
            Some(Message::Shutdown) => break,
            Some(Message::Configure(next)) => {
                // Advancing generation invalidates queued output from the old child.
                generation += 1;
                detector.take();
                set_endpoint(None);
                if let Some(token) = session.take() {
                    app.state::<TranscriptionCoordinator>().cancel_voice(token);
                }
                config = next;
                enabled = config.listening_enabled();
                if let Ok(mut status) = app.state::<VoiceControl>().status.lock() {
                    status.feature_enabled = config.feature_enabled;
                    status.phrase = config.phrase.clone();
                    status.silence_ms = config.silence_ms;
                }
                if let Some(window) = app.get_webview_window("voice_control") {
                    let result = if config.feature_enabled {
                        window.show()
                    } else {
                        window.hide()
                    };
                    if let Err(error) = result {
                        report_error(&app, &error.to_string());
                    }
                }
                if !enabled {
                    phase = if config.feature_enabled {
                        "paused"
                    } else {
                        "disabled"
                    };
                    app.state::<VoiceControl>().publish(&app, phase, false);
                    continue;
                }
                if crate::settings::get_settings(&app).always_on_microphone {
                    let _ = tx.send(Message::DetectorError(
                        generation,
                        "Turn off Always-on Microphone before enabling wake-phrase control.".into(),
                    ));
                    continue;
                }
                if let Err(error) = validate(&config) {
                    let _ = tx.send(Message::DetectorError(generation, error));
                    continue;
                }
                crate::notices::clear(&app, crate::notices::NoticeSource::WakeWord);
                phase = "starting";
                app.state::<VoiceControl>().publish(&app, phase, true);
                helper_started = Instant::now();
                match spawn_detector(&config.phrase, generation, tx.clone()) {
                    Ok(child) => detector = Some(child),
                    Err(error) => {
                        let _ = tx.send(Message::DetectorError(generation, error));
                    }
                }
            }
            Some(Message::Detector(token, event)) if token == generation && enabled => {
                if event == "ready" && phase == "starting" {
                    phase = "listening";
                    app.state::<VoiceControl>().publish(&app, phase, true);
                } else if event == "wake" && phase == "listening" && Instant::now() >= cooldown {
                    next_session += 1;
                    let token = next_session;
                    // Arm before capture begins so the first speech frame is retained.
                    set_endpoint(Some(endpoint::Endpoint::new(
                        Instant::now(),
                        config.silence_ms,
                    )));
                    if app.state::<TranscriptionCoordinator>().start_voice(token) {
                        session = Some(token);
                        phase = "arming";
                        app.state::<VoiceControl>().publish(&app, phase, true);
                    } else {
                        set_endpoint(None);
                        app.state::<TranscriptionCoordinator>().cancel_voice(token);
                        cooldown = Instant::now() + Duration::from_secs(1);
                    }
                }
            }
            Some(Message::DetectorError(token, error)) if token == generation && enabled => {
                generation += 1;
                detector.take();
                set_endpoint(None);
                if let Some(token) = session.take() {
                    app.state::<TranscriptionCoordinator>().cancel_voice(token);
                }
                enabled = false;
                phase = "error";
                // Fail closed, including on the next login.
                let mut settings = crate::settings::get_settings(&app);
                settings.voice_control.enabled = false;
                crate::settings::write_settings(&app, settings);
                report_error(&app, &error);
            }
            Some(Message::Finished(token)) if session == Some(token) => {
                session = None;
                set_endpoint(None);
                phase = if enabled { "listening" } else { "paused" };
                cooldown = Instant::now() + Duration::from_secs(1);
                app.state::<VoiceControl>().publish(&app, phase, enabled);
            }
            Some(Message::CaptureReady) if session.is_some() && phase == "arming" => {
                phase = "recording";
                app.state::<VoiceControl>().publish(&app, phase, enabled);
            }
            Some(Message::Position(x, y)) => pending_position = Some((x, y, Instant::now())),
            _ => {}
        }
        if let Some((x, y, moved)) = pending_position {
            if moved.elapsed() >= Duration::from_millis(500) {
                let mut settings = crate::settings::get_settings(&app);
                settings.voice_control.position = Some((x, y));
                crate::settings::write_settings(&app, settings);
                pending_position = None;
            }
        }
        if let Some(child) = detector.as_mut() {
            if !matches!(child.0.try_wait(), Ok(None)) {
                let _ = tx.send(Message::DetectorError(generation, "The Windows wake detector stopped. Check the default microphone and English speech installation, then resume.".into()));
            } else if phase == "starting" && helper_started.elapsed() > Duration::from_secs(20) {
                let _ = tx.send(Message::DetectorError(
                    generation,
                    "The Windows wake detector did not become ready. Check microphone permissions."
                        .into(),
                ));
            }
        }
        if phase == "recording" || phase == "arming" {
            let decision = ENDPOINT.get().and_then(|lock| {
                lock.lock().ok().and_then(|guard| {
                    guard.as_ref().map(|endpoint| {
                        (endpoint.finished(Instant::now()), endpoint.heard_speech())
                    })
                })
            });
            if let (Some(token), Some((true, heard_speech))) = (session, decision) {
                set_endpoint(None);
                if heard_speech {
                    app.state::<TranscriptionCoordinator>().stop_voice(token);
                    phase = "processing";
                } else {
                    app.state::<TranscriptionCoordinator>().cancel_voice(token);
                    session = None;
                    phase = "listening";
                    cooldown = Instant::now() + Duration::from_secs(1);
                }
                app.state::<VoiceControl>().publish(&app, phase, enabled);
            }
        }
    }
    // Child Drop releases the Windows mic, even when the UI is closed mid-take.
    set_endpoint(None);
    drop(detector);
}

#[cfg(target_os = "windows")]
fn create_window(app: &AppHandle, settings: &VoiceControlSettings) -> Result<(), String> {
    let window = tauri::WebviewWindowBuilder::new(
        app,
        "voice_control",
        tauri::WebviewUrl::App("src/voice-control/index.html".into()),
    )
    .title("Handy Voice Control")
    .inner_size(128.0, 58.0)
    .decorations(false)
    .transparent(true)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(false)
    .visible(settings.feature_enabled)
    .build()
    .map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let saved = settings.position.filter(|(x, y)| {
        monitors.iter().any(|monitor| {
            let area = monitor.work_area();
            i64::from(*x) >= i64::from(area.position.x)
                && i64::from(*y) >= i64::from(area.position.y)
                && i64::from(*x) + i64::from(size.width)
                    <= i64::from(area.position.x) + i64::from(area.size.width)
                && i64::from(*y) + i64::from(size.height)
                    <= i64::from(area.position.y) + i64::from(area.size.height)
        })
    });
    if let Some((x, y)) = saved {
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    } else if let Some(monitor) = window.primary_monitor().map_err(|e| e.to_string())? {
        let area = monitor.work_area();
        let x =
            i64::from(area.position.x) + i64::from(area.size.width) - i64::from(size.width) - 16;
        let y =
            i64::from(area.position.y) + i64::from(area.size.height) - i64::from(size.height) - 16;
        window
            .set_position(tauri::PhysicalPosition::new(x as i32, y as i32))
            .map_err(|e| e.to_string())?;
    }
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(position) = event {
            if let Some(control) = handle.try_state::<VoiceControl>() {
                let _ = control.tx.send(Message::Position(position.x, position.y));
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_config_without_interpreting_phrases_as_code() {
        let mut settings = VoiceControlSettings::default();
        assert!(validate(&settings).is_ok());
        for phrase in ["hey; Start-Process calc", "$(whoami)", "hey\nhandy", ""] {
            settings.phrase = phrase.into();
            assert!(validate(&settings).is_err(), "{phrase:?}");
        }
    }

    #[test]
    fn a_single_wake_word_is_supported() {
        let settings = VoiceControlSettings {
            phrase: "computer".into(),
            ..Default::default()
        };
        assert!(validate(&settings).is_ok());
    }

    #[test]
    fn existing_settings_keep_the_feature_and_listening_preference() {
        let settings: VoiceControlSettings =
            serde_json::from_str(r#"{"enabled":true,"phrase":"hey handy"}"#).unwrap();
        assert!(settings.feature_enabled);
        assert!(settings.listening_enabled());
    }

    #[test]
    fn disabled_feature_cannot_listen_even_with_a_stale_listening_preference() {
        let settings = VoiceControlSettings {
            feature_enabled: false,
            enabled: true,
            ..Default::default()
        };
        assert!(!settings.listening_enabled());
        let status = VoiceStatus::new(&settings);
        assert_eq!(status.phase, "disabled");
        assert!(!status.feature_enabled);
        assert!(!status.enabled);
    }
}

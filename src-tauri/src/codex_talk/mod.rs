//! Independent wake/key capture with a fixed destination per recording.
mod desktop;
pub use desktop::CodexConversation;

use crate::voice_control::endpoint::Endpoint;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Listener, Manager};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct CodexTalkSettings {
    pub feature_enabled: bool,
    pub listening: bool,
    pub phrase: String,
    pub shortcut: String,
    pub thread_id: String,
}
impl Default for CodexTalkSettings {
    fn default() -> Self {
        Self {
            feature_enabled: false,
            listening: false,
            phrase: "hey codex".into(),
            shortcut: "Ctrl+Alt+Shift+Space".into(),
            thread_id: "01a1a777-2733-73c3-999d-c7d0e0f7cfaf".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CodexTalkMessage {
    pub timestamp: i64,
    pub text: String,
    pub thread_id: String,
    pub delivery: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CodexTalkStatus {
    pub settings: CodexTalkSettings,
    pub phase: String,
    pub model: Option<String>,
    pub error: Option<String>,
    pub delivery: Option<String>,
    pub pending_text: Option<String>,
    pub pending_thread: Option<String>,
    pub history: Vec<CodexTalkMessage>,
    pub conversation_busy: bool,
}

impl CodexTalkStatus {
    fn begin_delivery(&mut self, thread: &str, text: &str) {
        // Retry updates the failed message rather than duplicating its text.
        let retry = self.pending_text.as_deref() == Some(text)
            && self.pending_thread.as_deref() == Some(thread);
        if let Some(last) = self.history.last_mut().filter(|entry| {
            retry && entry.delivery == "failed" && entry.thread_id == thread && entry.text == text
        }) {
            last.delivery = "sending".into();
        } else {
            self.history.push(CodexTalkMessage {
                timestamp: chrono::Utc::now().timestamp_millis(),
                text: text.into(),
                thread_id: thread.into(),
                delivery: "sending".into(),
            });
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
    }

    fn finish_delivery(&mut self, delivery: &str) {
        if let Some(last) = self.history.last_mut() {
            last.delivery = delivery.into();
        }
    }

    fn ensure_history_idle(&self) -> Result<(), String> {
        if self.conversation_busy
            || matches!(self.phase.as_str(), "arming" | "recording" | "processing")
            || self.delivery.as_deref() == Some("sending")
        {
            return Err(
                "Wait for the current recording or delivery to finish before clearing history."
                    .into(),
            );
        }
        Ok(())
    }

    fn clear_history(&mut self) -> Result<(), String> {
        self.ensure_history_idle()?;
        self.history.clear();
        self.pending_text = None;
        self.pending_thread = None;
        self.delivery = None;
        self.error = None;
        Ok(())
    }
}
enum Message {
    Configure(CodexTalkSettings),
    Trigger,
    Ready,
    Finished(u64),
    Shutdown,
}
pub struct CodexTalk {
    tx: mpsc::Sender<Message>,
    status: Arc<Mutex<CodexTalkStatus>>,
    target: Mutex<Option<String>>,
}
static ENDPOINT: OnceLock<Mutex<Option<Endpoint>>> = OnceLock::new();
pub fn speech_frame() {
    if let Some(lock) = ENDPOINT.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(endpoint) = guard.as_mut() {
                endpoint.speech(Instant::now());
            }
        }
    }
}
fn endpoint(value: Option<Endpoint>) {
    if let Ok(mut guard) = ENDPOINT.get_or_init(|| Mutex::new(None)).lock() {
        *guard = value;
    }
}
fn update(app: &AppHandle, f: impl FnOnce(&mut CodexTalkStatus)) {
    if let Some(control) = app.try_state::<CodexTalk>() {
        if let Ok(mut state) = control.status.lock() {
            f(&mut state);
            let _ = app.emit("codex-talk-status", state.clone());
        }
    }
}
pub fn pipeline_finished(app: &AppHandle, token: u64) {
    if let Some(control) = app.try_state::<CodexTalk>() {
        let _ = control.tx.send(Message::Finished(token));
    }
}
pub fn shutdown(app: &AppHandle) {
    if let Some(control) = app.try_state::<CodexTalk>() {
        let _ = control.tx.send(Message::Shutdown);
    }
}
pub fn recording_target(app: &AppHandle) -> Option<String> {
    app.state::<CodexTalk>()
        .target
        .lock()
        .ok()
        .and_then(|t| t.clone())
}
fn phrase_conflicts(a: &str, b: &str) -> bool {
    let a = format!(" {} ", a.to_ascii_lowercase());
    let b = format!(" {} ", b.to_ascii_lowercase());
    a.contains(&b) || b.contains(&a)
}
pub fn validate_wake_conflict(config: &CodexTalkSettings, phrase: &str) -> Result<(), String> {
    if config.feature_enabled && phrase_conflicts(&config.phrase, phrase) {
        return Err("Codex-Talk and Wake-Talk must use distinct wake phrases.".into());
    }
    Ok(())
}
fn validate(
    config: &CodexTalkSettings,
    settings: &crate::settings::AppSettings,
) -> Result<(), String> {
    uuid::Uuid::parse_str(&config.thread_id)
        .map_err(|_| "Enter a valid Codex thread UUID".to_string())?;
    if !(1..=5).contains(&config.phrase.split_whitespace().count())
        || config.phrase.len() > 60
        || !config
            .phrase
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '\'')
    {
        return Err("Use an English wake phrase of one to five words.".into());
    }
    if phrase_conflicts(&config.phrase, &settings.voice_control.phrase) {
        return Err("Codex-Talk and Wake-Talk must use distinct wake phrases.".into());
    }
    crate::shortcut::tauri_impl::validate_shortcut(&config.shortcut)?;
    use tauri_plugin_global_shortcut::Shortcut;
    let shortcut = config
        .shortcut
        .parse::<Shortcut>()
        .map_err(|e| e.to_string())?;
    for binding in settings.bindings.values() {
        // Both backends accept these accelerator forms. Normalize aliases/order.
        if binding.current_binding.parse::<Shortcut>().ok() == Some(shortcut) {
            return Err("Choose a shortcut different from Handy's existing shortcuts.".into());
        }
    }
    if config.feature_enabled && settings.always_on_microphone {
        return Err("Turn off Always-on Microphone before enabling Codex-Talk.".into());
    }
    Ok(())
}
#[tauri::command]
#[specta::specta]
pub fn get_codex_talk_status(app: AppHandle) -> Result<CodexTalkStatus, String> {
    app.state::<CodexTalk>()
        .status
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())
}
#[tauri::command]
#[specta::specta]
pub async fn check_codex_thread(thread_id: String) -> Result<String, String> {
    uuid::Uuid::parse_str(&thread_id).map_err(|_| "Enter a valid Codex thread UUID".to_string())?;
    #[cfg(target_os = "windows")]
    {
        desktop::Desktop::connect().await?.check(&thread_id).await
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Codex-Talk requires Windows Codex desktop.".into())
    }
}
#[tauri::command]
#[specta::specta]
pub async fn configure_codex_talk(
    app: AppHandle,
    mut config: CodexTalkSettings,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("Codex-Talk requires Windows.".into());
    }
    config.phrase = config
        .phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    config.thread_id = config.thread_id.trim().into();
    config.shortcut = config.shortcut.trim().into();
    config.listening &= config.feature_enabled;
    let mut settings = crate::settings::get_settings(&app);
    if config.feature_enabled {
        validate(&config, &settings)?;
    }
    let pausing = settings.codex_talk.feature_enabled
        && !config.listening
        && config.thread_id == settings.codex_talk.thread_id
        && config.phrase == settings.codex_talk.phrase
        && config.shortcut == settings.codex_talk.shortcut;
    // Do not stop a functioning controller or persist an enabled route on failure.
    if config.feature_enabled && !pausing {
        let model = check_codex_thread(config.thread_id.clone()).await?;
        update(&app, |s| {
            s.model = Some(model);
            s.error = None;
        });
    }
    // Read fresh settings after awaiting desktop, preserving concurrent edits.
    settings = crate::settings::get_settings(&app);
    if config.feature_enabled {
        validate(&config, &settings)?;
    }
    settings.codex_talk = config.clone();
    crate::settings::write_settings(&app, settings);
    app.state::<CodexTalk>()
        .tx
        .send(Message::Configure(config))
        .map_err(|e| e.to_string())
}
pub async fn deliver(app: &AppHandle, thread: &str, text: &str, cancel_generation: Option<u64>) {
    if text.trim().is_empty() {
        return;
    }
    update(app, |s| {
        s.begin_delivery(thread, text);
        s.pending_text = Some(text.into());
        s.pending_thread = Some(thread.into());
        s.delivery = Some("sending".into());
        s.error = None;
    });
    #[cfg(target_os = "windows")]
    let result = match desktop::Desktop::connect().await {
        Ok(mut desktop) => {
            desktop
                .send(thread, text, || {
                    let settings = crate::settings::get_settings(app);
                    !settings.codex_talk.feature_enabled
                        || cancel_generation.is_some_and(|generation| {
                            app.state::<Arc<crate::managers::audio::AudioRecordingManager>>()
                                .was_cancelled_since(generation)
                        })
                })
                .await
        }
        Err(e) => Err(e),
    };
    #[cfg(not(target_os = "windows"))]
    let result: Result<&str, String> = Err("Codex-Talk requires Windows".into());
    match result {
        Ok(delivery) => {
            crate::notices::clear(app, crate::notices::NoticeSource::CodexTalk);
            update(app, |s| {
                s.finish_delivery(delivery);
                s.delivery = Some(delivery.into());
                s.pending_text = None;
                s.pending_thread = None;
            });
        }
        Err(error) if error == "Recording cancelled" => update(app, |s| {
            s.history.pop();
            s.delivery = None;
            s.pending_text = None;
            s.pending_thread = None;
        }),
        Err(error) => {
            update(app, |s| {
                s.finish_delivery("failed");
                s.delivery = Some("failed".into());
                s.error = Some(error.clone());
            });
            report(app, &error);
        }
    }
}
#[tauri::command]
#[specta::specta]
pub async fn retry_codex_message(app: AppHandle) -> Result<(), String> {
    let status = get_codex_talk_status(app.clone())?;
    if !status.settings.feature_enabled {
        return Err("Enable Codex-Talk before retrying.".into());
    }
    if status.delivery.as_deref() != Some("failed") {
        return Err("No failed delivery to retry".into());
    }
    let text = status.pending_text.ok_or("No retained message")?;
    let target = status.pending_thread.ok_or("No retained destination")?;
    // Reserve before awaiting so a double click cannot submit the same message twice.
    {
        let control = app.state::<CodexTalk>();
        let mut status = control.status.lock().map_err(|e| e.to_string())?;
        if status.conversation_busy {
            return Err("Wait for the conversation reset to finish.".into());
        }
        if status.delivery.as_deref() != Some("failed") {
            return Err("Delivery already in progress".into());
        }
        status.delivery = Some("sending".into());
    }
    deliver(&app, &target, &text, None).await;
    Ok(())
}
#[tauri::command]
#[specta::specta]
pub fn discard_codex_message(app: AppHandle) -> Result<(), String> {
    let control = app.state::<CodexTalk>();
    let mut status = control.status.lock().map_err(|e| e.to_string())?;
    if status.delivery.as_deref() != Some("failed") {
        return Err("No failed delivery to discard".into());
    }
    status.pending_text = None;
    status.pending_thread = None;
    status.delivery = None;
    status.error = None;
    let _ = app.emit("codex-talk-status", status.clone());
    crate::notices::clear(&app, crate::notices::NoticeSource::CodexTalk);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn load_codex_conversation(thread_id: String) -> Result<CodexConversation, String> {
    uuid::Uuid::parse_str(&thread_id).map_err(|_| "Enter a valid Codex thread UUID".to_string())?;
    #[cfg(target_os = "windows")]
    {
        desktop::Desktop::connect()
            .await?
            .conversation(&thread_id)
            .await
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Codex-Talk requires Windows Codex desktop.".into())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn clear_codex_conversation(
    app: AppHandle,
    thread_id: String,
) -> Result<CodexConversation, String> {
    uuid::Uuid::parse_str(&thread_id).map_err(|_| "Enter a valid Codex thread UUID".to_string())?;
    {
        let control = app.state::<CodexTalk>();
        let mut status = control.status.lock().map_err(|e| e.to_string())?;
        status.ensure_history_idle()?;
        status.conversation_busy = true;
        let _ = app.emit("codex-talk-status", status.clone());
    }
    #[cfg(target_os = "windows")]
    let result = match desktop::Desktop::connect().await {
        Ok(mut desktop) => desktop.reset_conversation(&thread_id).await,
        Err(error) => Err(error),
    };
    #[cfg(not(target_os = "windows"))]
    let result: Result<CodexConversation, String> = Err("Codex-Talk requires Windows".into());
    update(&app, |status| {
        status.conversation_busy = false;
        if result.is_ok() {
            let _ = status.clear_history();
        }
    });
    if result.is_ok() {
        crate::notices::clear(&app, crate::notices::NoticeSource::CodexTalk);
    }
    result
}

#[tauri::command]
#[specta::specta]
pub fn clear_codex_message_history(app: AppHandle) -> Result<(), String> {
    let control = app.state::<CodexTalk>();
    {
        let mut status = control.status.lock().map_err(|e| e.to_string())?;
        status.clear_history()?;
        let _ = app.emit("codex-talk-status", status.clone());
    }
    crate::notices::clear(&app, crate::notices::NoticeSource::CodexTalk);
    Ok(())
}

fn report(app: &AppHandle, error: &str) {
    use crate::notices::{self, Notice, NoticeCode, NoticeSource};
    notices::raise_without_overlay(
        app,
        Notice::new(NoticeCode::CodexTalkFailed, NoticeSource::CodexTalk)
            .with_detail(Some(error.to_owned())),
    );
}
pub fn initialize(app: &AppHandle) {
    let settings = crate::settings::get_settings(app).codex_talk;
    let (tx, rx) = mpsc::channel();
    app.manage(CodexTalk {
        tx: tx.clone(),
        target: Mutex::new(None),
        status: Arc::new(Mutex::new(CodexTalkStatus {
            settings: settings.clone(),
            phase: "disabled".into(),
            model: None,
            error: None,
            delivery: None,
            pending_text: None,
            pending_thread: None,
            history: Vec::new(),
            conversation_busy: false,
        })),
    });
    let ready = tx.clone();
    app.listen("recording-ready", move |_| {
        let _ = ready.send(Message::Ready);
    });
    #[cfg(target_os = "windows")]
    {
        let app = app.clone();
        std::thread::spawn(move || run(app, tx, rx, settings));
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (tx, rx);
}
#[cfg(target_os = "windows")]
fn run(
    app: AppHandle,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    initial: CodexTalkSettings,
) {
    use crate::transcription_coordinator::TranscriptionCoordinator;
    use crate::voice_control::{spawn_detector, Message as WakeMessage};
    use std::time::Duration;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
    let (wake_tx, wake_rx) = mpsc::channel();
    let mut detector = None;
    let mut registered: Option<Shortcut> = None;
    let mut config = initial.clone();
    let mut generation = 0;
    let mut session = None;
    let mut next_token = 1u64 << 63;
    let mut processing = false;
    let mut cooldown = Instant::now();
    let mut detector_started = Instant::now();
    let mut detector_ready = false;
    let _ = tx.send(Message::Configure(initial));
    loop {
        let message = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => Some(message),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(_) => break,
        };
        match message {
            Some(Message::Shutdown) => break,
            Some(Message::Configure(next)) => {
                generation += 1;
                detector.take();
                detector_ready = false;
                if let Some(key) = registered.take() {
                    let _ = app.global_shortcut().unregister(key);
                }
                if let Some(token) = session.take() {
                    app.state::<TranscriptionCoordinator>().cancel_voice(token);
                }
                endpoint(None);
                processing = false;
                config = next;
                update(&app, |s| {
                    s.settings = config.clone();
                    s.phase = if config.feature_enabled {
                        "paused"
                    } else {
                        "disabled"
                    }
                    .into();
                });
                if !config.feature_enabled {
                    continue;
                }
                let setup = (|| -> Result<(), String> {
                    validate(&config, &crate::settings::get_settings(&app))?;
                    let model = tauri::async_runtime::block_on(check_codex_thread(
                        config.thread_id.clone(),
                    ))?;
                    update(&app, |s| {
                        s.model = Some(model);
                        s.error = None;
                    });
                    let key = config
                        .shortcut
                        .parse::<Shortcut>()
                        .map_err(|e| e.to_string())?;
                    let keys = tx.clone();
                    app.global_shortcut()
                        .on_shortcut(key, move |_, _, event| {
                            if event.state == ShortcutState::Pressed {
                                let _ = keys.send(Message::Trigger);
                            }
                        })
                        .map_err(|e| e.to_string())?;
                    registered = Some(key);
                    if config.listening {
                        detector =
                            Some(spawn_detector(&config.phrase, generation, wake_tx.clone())?);
                        detector_started = Instant::now();
                        update(&app, |s| s.phase = "starting".into());
                    }
                    Ok(())
                })();
                if let Err(error) = setup {
                    if let Some(key) = registered.take() {
                        let _ = app.global_shortcut().unregister(key);
                    }
                    config.feature_enabled = false;
                    config.listening = false;
                    update(&app, |s| {
                        s.settings = config.clone();
                        s.phase = "error".into();
                        s.error = Some(error.clone());
                    });
                    let mut settings = crate::settings::get_settings(&app);
                    settings.codex_talk = config.clone();
                    crate::settings::write_settings(&app, settings);
                    report(&app, &error);
                }
            }
            Some(Message::Trigger) if config.feature_enabled && Instant::now() >= cooldown => {
                cooldown = Instant::now() + Duration::from_millis(500);
                if let Some(token) = session {
                    if !processing {
                        app.state::<TranscriptionCoordinator>().stop_voice(token);
                        processing = true;
                        endpoint(None);
                        update(&app, |s| s.phase = "processing".into());
                    }
                } else {
                    let blocked = app.state::<CodexTalk>().status.lock().map(|mut status| {
                        if status.conversation_busy { Some("Wait for the Codex conversation reset to finish.") }
                        else if status.pending_text.is_some() { Some("Resolve the previous failed message in Codex-Talk before recording another.") }
                        else { status.phase = "arming".into(); None }
                    }).unwrap_or(Some("Codex-Talk status is unavailable."));
                    if let Some(error) = blocked {
                        report(&app, error);
                        continue;
                    }
                    next_token += 1;
                    if let Ok(mut target) = app.state::<CodexTalk>().target.lock() {
                        *target = Some(config.thread_id.clone());
                    }
                    endpoint(Some(Endpoint::new(Instant::now(), 2000)));
                    if app
                        .state::<TranscriptionCoordinator>()
                        .start_targeted_voice(next_token, true)
                    {
                        session = Some(next_token);
                        processing = false;
                        update(&app, |s| s.phase = "arming".into());
                    } else {
                        endpoint(None);
                        app.state::<TranscriptionCoordinator>()
                            .cancel_voice(next_token);
                        update(&app, |s| {
                            s.phase = if config.listening && detector_ready {
                                "listening"
                            } else {
                                "paused"
                            }
                            .into()
                        });
                    }
                }
            }
            Some(Message::Ready) if session.is_some() && !processing => {
                update(&app, |s| s.phase = "recording".into())
            }
            Some(Message::Finished(token)) if session == Some(token) => {
                session = None;
                processing = false;
                endpoint(None);
                if let Ok(mut target) = app.state::<CodexTalk>().target.lock() {
                    *target = None;
                }
                cooldown = Instant::now() + Duration::from_secs(1);
                update(&app, |s| {
                    s.phase = if config.listening && detector_ready {
                        "listening"
                    } else {
                        "paused"
                    }
                    .into()
                });
            }
            _ => {}
        }
        while let Ok(event) = wake_rx.try_recv() {
            match event {
                WakeMessage::Detector(g, line) if g == generation => {
                    if line == "ready" {
                        detector_ready = true;
                        if session.is_none() {
                            update(&app, |s| s.phase = "listening".into());
                        }
                    } else if line == "wake"
                        && detector_ready
                        && session.is_none()
                        && Instant::now() >= cooldown
                    {
                        let _ = tx.send(Message::Trigger);
                    }
                }
                WakeMessage::DetectorError(g, error) if g == generation => {
                    detector.take();
                    detector_ready = false;
                    config.listening = false;
                    let mut settings = crate::settings::get_settings(&app);
                    settings.codex_talk.listening = false;
                    crate::settings::write_settings(&app, settings);
                    update(&app, |s| {
                        s.settings.listening = false;
                        s.error = Some(error.clone());
                        if session.is_none() {
                            s.phase = "paused".into();
                        }
                    });
                    report(&app, &error);
                }
                _ => {}
            }
        }
        if let Some(child) = detector.as_mut() {
            if !matches!(child.0.try_wait(), Ok(None))
                || (!detector_ready && detector_started.elapsed() > Duration::from_secs(20))
            {
                let _ = wake_tx.send(WakeMessage::DetectorError(
                    generation,
                    "Codex-Talk wake detector stopped. Check the microphone and resume listening."
                        .into(),
                ));
            }
        }
        if session.is_some() && !processing {
            let decision = ENDPOINT.get().and_then(|l| l.lock().ok()).and_then(|g| {
                g.as_ref()
                    .map(|e| (e.finished(Instant::now()), e.heard_speech()))
            });
            if let Some((true, speech)) = decision {
                endpoint(None);
                if let Some(token) = session {
                    if speech {
                        app.state::<TranscriptionCoordinator>().stop_voice(token);
                        processing = true;
                        update(&app, |s| s.phase = "processing".into());
                    } else {
                        app.state::<TranscriptionCoordinator>().cancel_voice(token);
                    }
                }
            }
        }
    }
    endpoint(None);
    if let Some(key) = registered {
        let _ = app.global_shortcut().unregister(key);
    }
    drop(detector);
}
#[cfg(test)]
mod tests {
    use super::*;
    fn history_status() -> CodexTalkStatus {
        CodexTalkStatus {
            settings: CodexTalkSettings::default(),
            phase: "paused".into(),
            model: Some("thread-model".into()),
            error: Some("old error".into()),
            delivery: Some("failed".into()),
            pending_text: Some("retained".into()),
            pending_thread: Some("original-thread".into()),
            history: Vec::new(),
            conversation_busy: false,
        }
    }

    #[test]
    fn clearing_history_removes_local_messages_and_keeps_configuration() {
        let mut status = history_status();
        status.begin_delivery("original-thread", "retained");
        status.finish_delivery("failed");
        let settings = serde_json::to_value(&status.settings).unwrap();
        status.clear_history().unwrap();
        assert!(status.history.is_empty());
        assert!(status.pending_text.is_none());
        assert!(status.pending_thread.is_none());
        assert!(status.delivery.is_none());
        assert!(status.error.is_none());
        assert_eq!(serde_json::to_value(&status.settings).unwrap(), settings);
        assert_eq!(status.model.as_deref(), Some("thread-model"));
        assert_eq!(status.phase, "paused");
    }

    #[test]
    fn clearing_cannot_erase_an_active_recording_or_delivery() {
        for (phase, delivery) in [
            ("arming", None),
            ("recording", None),
            ("processing", None),
            ("paused", Some("sending")),
        ] {
            let mut status = history_status();
            status.begin_delivery("target", "message");
            status.phase = phase.into();
            status.delivery = delivery.map(str::to_owned);
            assert!(status.clear_history().is_err());
            assert_eq!(status.history.len(), 1);
            assert_eq!(status.pending_text.as_deref(), Some("retained"));
        }
    }

    #[test]
    fn message_history_is_bounded_and_retry_updates_the_failed_message() {
        let mut status = history_status();
        for i in 0..105 {
            status.begin_delivery("target", &format!("message {i}"));
            status.finish_delivery("sent");
        }
        assert_eq!(status.history.len(), 100);
        assert_eq!(status.history[0].text, "message 5");
        status.finish_delivery("failed");
        status.pending_text = Some("message 104".into());
        status.pending_thread = Some("target".into());
        status.begin_delivery("target", "message 104");
        assert_eq!(status.history.len(), 100);
        assert_eq!(status.history.last().unwrap().delivery, "sending");
        status.finish_delivery("steered");
        assert_eq!(status.history.last().unwrap().delivery, "steered");
    }

    #[test]
    fn routes_require_distinct_phrases_and_shortcuts() {
        let settings = crate::settings::get_default_settings();
        let mut config = CodexTalkSettings::default();
        assert!(validate(&config, &settings).is_ok());
        config.phrase = "hey handy".into();
        assert!(validate(&config, &settings).is_err());
        config.phrase = "hey handy please".into();
        assert!(validate(&config, &settings).is_err());
        config.phrase = "hey codex".into();
        config.shortcut = settings.bindings["transcribe"].current_binding.clone();
        assert!(validate(&config, &settings).is_err());
    }
}

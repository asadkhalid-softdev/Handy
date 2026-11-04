//! Recording deadlines for voice-triggered sessions. No audio is retained here.
use std::time::{Duration, Instant};

pub const NO_SPEECH_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_RECORDING: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct Endpoint {
    started: Instant,
    last_speech: Option<Instant>,
    silence: Duration,
}

impl Endpoint {
    pub fn new(now: Instant, silence_ms: u64) -> Self {
        Self {
            started: now,
            last_speech: None,
            silence: Duration::from_millis(silence_ms.clamp(800, 10_000)),
        }
    }

    pub fn speech(&mut self, now: Instant) {
        self.last_speech = Some(now);
    }

    pub fn finished(&self, now: Instant) -> bool {
        now.duration_since(self.started) >= MAX_RECORDING
            || match self.last_speech {
                Some(last) => now.duration_since(last) >= self.silence,
                None => now.duration_since(self.started) >= NO_SPEECH_TIMEOUT,
            }
    }

    pub fn heard_speech(&self) -> bool {
        self.last_speech.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_deadline_tracks_the_last_speech_not_session_start() {
        let now = Instant::now();
        let mut endpoint = Endpoint::new(now, 1500);
        endpoint.speech(now + Duration::from_secs(2));
        assert!(!endpoint.finished(now + Duration::from_millis(3400)));
        endpoint.speech(now + Duration::from_millis(3400));
        assert!(!endpoint.finished(now + Duration::from_millis(4800)));
        assert!(endpoint.finished(now + Duration::from_millis(4900)));
    }

    #[test]
    fn silent_and_continuous_sessions_are_bounded() {
        let now = Instant::now();
        let mut endpoint = Endpoint::new(now, 2000);
        assert!(!endpoint.finished(now + Duration::from_secs(9)));
        assert!(endpoint.finished(now + NO_SPEECH_TIMEOUT));
        endpoint.speech(now + MAX_RECORDING);
        assert!(endpoint.finished(now + MAX_RECORDING));
    }
}

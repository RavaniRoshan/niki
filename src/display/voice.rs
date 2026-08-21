//! Voice input (push-to-talk) for NIKI's TUI.
//!
//! Records audio to a temporary WAV file using `ffmpeg` (which is the one audio
//! tool we know is present) and transcribes it through the configured LLM
//! provider's speech-to-text endpoint (`LlmProvider::transcribe`, added in the
//! same change). Falls back to a clear error when no provider supports STT.
//!
//! The recording is a single-shot capture: press the voice key, speak, release.
//! The TUI gates the key with `VoiceState` so the composer only accepts the
//! dictated text when recording is active.

use std::path::PathBuf;
use std::time::Duration;

/// How long to record per push-to-talk press (bounded so a stuck mic can't
/// run forever).
pub const RECORDING_DURATION: Duration = Duration::from_secs(15);

/// Voice input state held by the TUI.
#[derive(Debug, Clone, Default)]
pub struct VoiceState {
    /// Whether recording is currently active.
    pub recording: bool,
    /// The most recent transcription result, if any.
    pub last_transcript: Option<String>,
}

impl VoiceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_recording(&mut self) {
        self.recording = true;
        self.last_transcript = None;
    }

    pub fn stop_recording(&mut self) {
        self.recording = false;
    }
}

/// Record a single push-to-talk clip to a WAV file.
///
/// Uses `ffmpeg` (already present) with the ALSA input on Linux. Returns the
/// path to the recorded WAV. On any failure (no ffmpeg, no input device,
/// interrupted) the error is surfaced so the caller can notify the user.
pub fn record_clip(duration: Duration) -> anyhow::Result<PathBuf> {
    let out = PathBuf::from(format!(
        "/tmp/niki-voice-{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    let seconds = duration.as_secs_f64();
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "alsa",
            "-i",
            "default",
            "-t",
            &format!("{seconds}"),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            out.to_string_lossy().as_ref(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(out),
        Ok(_) => Err(anyhow::anyhow!("ffmpeg recording failed (exit code non-zero)")),
        Err(e) => Err(anyhow::anyhow!("failed to run ffmpeg for voice recording: {e}")),
    }
}

/// Read a WAV file into memory.
pub fn read_wav(path: &PathBuf) -> anyhow::Result<Vec<u8>> {
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_state_starts_and_stops() {
        let mut state = VoiceState::new();
        assert!(!state.recording);
        state.start_recording();
        assert!(state.recording);
        state.stop_recording();
        assert!(!state.recording);
    }

    #[test]
    fn voice_state_default_is_clean() {
        let state = VoiceState::new();
        assert!(!state.recording);
        assert!(state.last_transcript.is_none());
    }
}

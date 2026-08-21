//! `niki voice` - record a push-to-talk clip and transcribe it.
//!
//! Records audio via `ffmpeg` (the one audio tool known to be present),
//! then transcribes through the configured LLM provider's speech-to-text
//! endpoint (`LlmProvider::transcribe`). Prints the transcript to stdout.
//! Mirrors the async provider pattern used by `niki chat`.

use clap::Args;
use std::path::PathBuf;

/// Record and transcribe a voice message.
#[derive(Args, Clone, Default)]
pub struct VoiceArgs {
    /// Path to the project directory (for config / language hint)
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,

    /// Recording duration in seconds (capped at 60).
    #[arg(short, long, default_value = "15")]
    pub seconds: u64,

    /// ISO-639-1 language hint (e.g. `en`). Overrides config.
    #[arg(short, long)]
    pub language: Option<String>,
}

/// Entry point for `niki voice`.
pub async fn handle(args: &VoiceArgs) -> anyhow::Result<()> {
    use crate::display::voice::{read_wav, record_clip};
    use crate::llm::provider::create_provider;
    use std::time::Duration;

    let project_dir = args
        .project
        .canonicalize()
        .unwrap_or_else(|_| args.project.clone());
    let config = crate::config::NikiConfig::load(&project_dir)?;

    let seconds = args.seconds.min(60);
    eprintln!("🎤 recording {}s... (Ctrl+C to stop)", seconds);
    let wav_path = record_clip(Duration::from_secs(seconds))?;
    let audio = read_wav(&wav_path)?;
    let _ = std::fs::remove_file(&wav_path);

    let language = args
        .language
        .clone()
        .or_else(|| config.general.language.clone());

    let mut last_err = None;
    for (name, pc) in &config.providers {
        match create_provider(name, pc) {
            Ok(provider) => match provider.transcribe(&audio, language.as_deref()).await {
                Ok(text) => {
                    println!("{}", text.trim());
                    return Ok(());
                }
                Err(e) => last_err = Some(e),
            },
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        anyhow::anyhow!(
            "no configured provider supports speech-to-text. Set up a provider \
             (e.g. OpenAI) in niki.toml or via ANTHROPIC_API_KEY/OPENAI_API_KEY."
        )
    }))
}

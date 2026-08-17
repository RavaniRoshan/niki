use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::config::NikiConfig;
use crate::display::tui::DisplayEvent;
use crate::llm::provider::{LlmProvider, create_provider};

#[derive(Args, Clone, Default)]
pub struct ChatArgs {
    /// Path to the project directory
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,

    /// Initial message to send (optional)
    #[arg(short, long)]
    pub message: Option<String>,
}

/// Build a provider from the configured providers map or environment variables.
fn build_provider(config: &NikiConfig) -> Option<(Box<dyn LlmProvider>, String)> {
    if let Some((name, pc)) = config.providers.iter().next() {
        if let Ok(provider) = create_provider(name, pc) {
            let model = if pc.default_model.is_empty() {
                "claude-sonnet-4-20250514".to_string()
            } else {
                pc.default_model.clone()
            };
            return Some((provider, model));
        }
    }

    // Auto-detect from environment variables if not specified in niki.toml
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            let pc = crate::config::types::ProviderConfig {
                api_key: Some(key),
                default_model: "claude-3-7-sonnet-20250219".to_string(),
                ..Default::default()
            };
            if let Ok(p) = create_provider("anthropic", &pc) {
                return Some((p, "claude-3-7-sonnet-20250219".to_string()));
            }
        }
    }

    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            let pc = crate::config::types::ProviderConfig {
                api_key: Some(key),
                default_model: "gpt-4o".to_string(),
                ..Default::default()
            };
            if let Ok(p) = create_provider("openai", &pc) {
                return Some((p, "gpt-4o".to_string()));
            }
        }
    }

    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        if !key.is_empty() {
            let pc = crate::config::types::ProviderConfig {
                api_key: Some(key),
                default_model: "gemini-2.5-pro".to_string(),
                ..Default::default()
            };
            if let Ok(p) = create_provider("google", &pc) {
                return Some((p, "gemini-2.5-pro".to_string()));
            }
        }
    }

    None
}

/// Process a submitted user message: ask the LLM and stream the reply back into
/// the chat session as an assistant turn (Phase 6 — user messages mid-session).
fn process_message(tx: &mpsc::Sender<DisplayEvent>, config: &NikiConfig, user_text: &str) {
    let reply = match build_provider(config) {
        Some((provider, model)) => {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => {
                    return send_assistant(
                        tx,
                        "(offline) could not start async runtime".to_string(),
                    );
                }
            };
            rt.block_on(async {
                let req = crate::llm::provider::CompletionRequest {
                    model,
                    system_prompt: concat!(
                        "You are NIKI, a concise and high-precision coding assistant embedded in a terminal chat.\n",
                        "Rule: Never run shell shims (cat, grep, sed, head, tail) when dedicated native tools are available.\n",
                        "Rule: Always read files before modifying them.\n",
                        "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\n"
                    ).to_string(),
                    user_message: user_text.to_string(),
                    max_tokens: 1024,
                    temperature: 0.7,
                    json_schema: None,
                    tools: None,
                };
                match provider.complete(req).await {
                    Ok(resp) => resp.content,
                    Err(e) => format!("(offline) LLM error: {}", e),
                }
            })
        }
        None => {
            "Hello! I am NIKI, your autonomous coding assistant. No LLM provider is configured yet. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or run /help to explore commands."
                .to_string()
        }
    };
    send_assistant(tx, reply);
}

fn send_assistant(tx: &mpsc::Sender<DisplayEvent>, text: String) {
    let _ = tx.send(DisplayEvent::ChatMessage {
        role: "assistant".to_string(),
        text,
    });
}

pub async fn handle(args: &ChatArgs) -> Result<()> {
    let project_path = if args.project.is_relative() {
        std::env::current_dir()?.join(&args.project)
    } else {
        args.project.clone()
    };

    let config = NikiConfig::load(&project_path).unwrap_or_default();

    // Create a long-lived channel so the TUI doesn't see Disconnect.
    let (tx, rx) = mpsc::channel::<DisplayEvent>();

    // Channel for submitted user messages, consumed by the session processor.
    let (on_submit_tx, on_submit_rx) = mpsc::channel::<String>();

    // Spawn the TUI in a background thread.
    let desc = args
        .message
        .clone()
        .unwrap_or_else(|| "chat session".to_string());
    let tui_tx = tx.clone();
    let tui_on_submit = on_submit_tx.clone();
    let handle = std::thread::spawn(move || {
        crate::display::tui::run_chat(rx, desc, project_path, Some(tui_on_submit));
    });

    // Spawn the message processor: each submitted user message gets an LLM reply
    // streamed back as an assistant turn.
    let proc_tx = tui_tx.clone();
    let proc_config = config.clone();
    std::thread::spawn(move || {
        while let Ok(user_text) = on_submit_rx.recv() {
            process_message(&proc_tx, &proc_config, &user_text);
        }
    });

    // Send an initial message if provided.
    if let Some(msg) = &args.message {
        let _ = tui_tx.send(DisplayEvent::ChatMessage {
            role: "user".to_string(),
            text: msg.clone(),
        });
        let _ = tui_tx.send(DisplayEvent::ChatMessage {
            role: "assistant".to_string(),
            text: "(thinking…)".to_string(),
        });
        let _ = on_submit_tx.send(msg.clone());
    }

    // Keep the sender alive so the TUI thread doesn't see Disconnect.
    // The TUI exits when the user presses Ctrl+C or quit.
    let _ = handle.join();

    Ok(())
}

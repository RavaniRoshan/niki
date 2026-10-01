use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::mpsc;

use crate::config::NikiConfig;
use crate::display::tui::{ChatSubmit, DisplayEvent};
use crate::llm::provider::{LlmProvider, StreamChunk, TokenUsage, create_provider};

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
/// Chat is a single-agent surface: it honors the `[agents.coder]` provider
/// first (the coder does the hands-on work), then any configured provider
/// entry, then environment auto-detection.
fn build_provider(config: &NikiConfig) -> Option<(Box<dyn LlmProvider>, String)> {
    // 1. The coder agent's configured provider, when its entry exists.
    let coder = &config.agents.coder;
    if let Some(pc) = config.providers.get(&coder.provider)
        && let Ok(provider) = create_provider(&coder.provider, pc)
    {
        let model = if coder.model.is_empty() {
            pc.default_model.clone()
        } else {
            coder.model.clone()
        };
        return Some((provider, model));
    }

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

    // Canonical env var is GOOGLE_API_KEY (config + auth use it); GEMINI_API_KEY
    // is honored as a deprecated fallback so existing setups keep working.
    let google_key = std::env::var("GOOGLE_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            std::env::var("GEMINI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
        });
    if let Some(key) = google_key {
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

    // Single-key AI gateways, in order of generality. Each needs an explicit
    // base_url: unlike anthropic/openai/google, the shared OpenAI-compatible
    // implementation cannot guess the endpoint.
    for (env_var, provider, model) in [
        (
            "OPENROUTER_API_KEY",
            "openrouter",
            "anthropic/claude-sonnet-4",
        ),
        ("OPENCODE_API_KEY", "zen", "kimi-k2.5"),
        ("KIMI_API_KEY", "kimi", "k3-256k"),
        ("KILO_API_KEY", "kilo", "anthropic/claude-sonnet-4.5"),
        ("NVIDIA_API_KEY", "nvidia", "meta/llama-3.1-405b-instruct"),
        ("GROQ_API_KEY", "groq", "llama-3.1-70b-versatile"),
    ] {
        if let Ok(key) = std::env::var(env_var) {
            if key.is_empty() {
                continue;
            }
            let pc = crate::config::types::ProviderConfig {
                api_key: Some(key),
                base_url: crate::llm::provider::default_base_url(provider).map(str::to_string),
                default_model: model.to_string(),
            };
            if let Ok(p) = create_provider(provider, &pc) {
                return Some((p, model.to_string()));
            }
        }
    }

    // Local-first fallback: a running Ollama needs no key and no config.
    if crate::cli::auth::ollama_running() {
        let pc = crate::config::types::ProviderConfig {
            default_model: "qwen2.5-coder".to_string(),
            ..Default::default()
        };
        if let Ok(p) = create_provider("ollama", &pc) {
            return Some((p, "qwen2.5-coder".to_string()));
        }
    }

    None
}

/// The system prompt for a chat turn.
///
/// It used to end with a literal `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` marker
/// that nothing in the repository substitutes, so every request carried that
/// string to the provider as part of its instructions.
///
/// The two tool rules it carries are also removed here rather than left for
/// the tool slice: this surface sends `tools: None`, so the model is being
/// told about "dedicated native tools" it was never given, and about reading
/// files it cannot read. It narrates doing both. The rules come back with the
/// tools.
const CHAT_SYSTEM_PROMPT: &str = "You are NIKI, a concise and high-precision coding assistant \
embedded in a terminal chat. Answer the user's question directly, and say plainly when you \
do not know something rather than guessing.";

/// Stream one assistant turn into the chat session.
///
/// Streams rather than waiting for a whole reply: `complete()` is a single
/// non-streaming POST, so a twenty-second answer used to be twenty seconds of
/// a completely static screen on the one surface a person is most likely to be
/// waiting on. The provider's `stream()` already existed and `niki run`
/// already consumed it.
///
/// Returns `Err` with a human-readable message. The caller renders it as an
/// error — see `DisplayEvent::ChatError` for why that is not the same thing
/// as an assistant turn.
async fn stream_reply(
    tx: &mpsc::Sender<DisplayEvent>,
    config: &NikiConfig,
    submit: &ChatSubmit,
) -> Result<String, String> {
    let (provider, model) =
        build_provider(config).ok_or_else(|| NO_PROVIDER_MESSAGE.to_string())?;
    // Kept for pricing after the request has taken the model name by value.
    // Cloning a `String` once per turn is cheaper than being unable to say what
    // the turn cost.
    let priced_model = model.clone();

    let req = crate::llm::provider::CompletionRequest {
        model,
        system_prompt: CHAT_SYSTEM_PROMPT.to_string(),
        user_message: submit.text.clone(),
        max_tokens: 4096,
        temperature: 0.7,
        json_schema: None,
        tools: None,
        reasoning_effort: None,
        // The whole point: the model sees the conversation it is in.
        history: submit.history.clone(),
    };

    let stream = match provider.stream(req).await {
        Ok(s) => s,
        Err(e) => return Err(describe_error(&e)),
    };

    let (full, finish_reason, usage) = consume_reply(stream, tx, &submit.cancel).await?;
    let cost_usd = price_chat_turn(provider.provider_name(), &priced_model, usage.as_ref());
    let _ = tx.send(DisplayEvent::ChatFinished {
        finish_reason,
        usage,
        cost_usd,
    });
    Ok(full)
}

/// What one chat turn cost, in dollars.
///
/// Extracted so a test can reach it. `stream_reply` prices the turn inline, and a
/// test that drives `AppState::apply_display_event` with an explicit `cost_usd`
/// is **green whether or not the chat ever computes one** — measured in batch 9,
/// where replacing this expression with `|_| 0.0` left every test passing while
/// `/cost` went back to reporting $0.0000 after paid API calls.
fn price_chat_turn(provider: &str, model: &str, usage: Option<&TokenUsage>) -> f64 {
    usage
        .map(|u| crate::cost::compute_cost(provider, model, u))
        .unwrap_or(0.0)
}

/// Drain one reply stream into text, a finish reason, and its usage.
///
/// Split out of [`stream_reply`] because **this is where the usage was thrown
/// away** — `StreamChunk::Usage` was matched with `{}` — and a test that cannot
/// reach the code that lost it proves nothing about the loss. A test that
/// drives `AppState::apply_display_event` with a usage value is green whether
/// or not this function ever produces one, and it was.
async fn consume_reply(
    mut stream: Pin<Box<dyn futures::Stream<Item = anyhow::Result<StreamChunk>> + Send>>,
    tx: &mpsc::Sender<DisplayEvent>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(String, Option<String>, Option<TokenUsage>), String> {
    use futures::StreamExt;

    let mut full = String::new();
    let mut finish_reason: Option<String> = None;
    // `None` means the provider reported none — which is different from
    // reporting zero, and the surface distinguishes the two.
    let mut usage: Option<TokenUsage> = None;
    while let Some(chunk) = stream.next().await {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = tx.send(DisplayEvent::ChatError {
                message: "Cancelled — the request was stopped before it finished.".to_string(),
                cancelled: true,
            });
            return Ok((full, finish_reason, usage));
        }
        match chunk {
            Ok(StreamChunk::Text(t)) => {
                full.push_str(&t);
                let _ = tx.send(DisplayEvent::ChatDelta { text: t });
            }
            Ok(StreamChunk::Finish { reason }) => {
                finish_reason = Some(reason);
            }
            Ok(StreamChunk::Usage(u)) => usage = Some(u),
            Err(e) => return Err(describe_error(&e)),
        }
    }
    Ok((full, finish_reason, usage))
}

/// Name the failure, not the transport.
///
/// Every error used to be rendered as `(offline) LLM error: …` inside an
/// assistant bubble. A 401 is a wrong key, a 404 on Ollama is a model that was
/// never pulled, and a timeout is neither — and all three told the user to go
/// look at their network.
fn describe_error(e: &anyhow::Error) -> String {
    let raw = e.to_string();
    if raw.contains("401") || raw.to_lowercase().contains("authentication") {
        format!(
            "Authentication failed. Your API key was rejected. Check it with `niki auth status`, or set the provider's key environment variable. ({raw})"
        )
    } else if raw.contains("404") {
        format!(
            "The provider does not recognise that model. Check the model name in your config, or pull it if you are using Ollama. ({raw})"
        )
    } else if raw.contains("429") {
        format!("Rate limited by the provider. Wait a moment and try again. ({raw})")
    } else if raw.contains("timed out") || raw.contains("timeout") {
        format!(
            "The request timed out. The model may be too slow for this machine, or the network stalled. ({raw})"
        )
    } else if raw.contains("body error") {
        format!(
            "The connection dropped part-way through the reply — the provider stopped sending before the response was complete. ({raw})"
        )
    } else {
        format!("LLM error: {raw}")
    }
}

const NO_PROVIDER_MESSAGE: &str = "No LLM provider is configured yet. Run `niki init` (or `niki auth login`) to set one up — Ollama works fully offline.";

/// Process a submitted user message: stream the reply back into the chat
/// session as an assistant turn.
///
/// Public because it is the seam a property is asserted at, not because a test
/// wanted it: "dispatching a message does not run the pipeline" is a claim
/// about the product, and the only honest way to assert it is to call the
/// dispatcher and look at what it left behind. The same convention
/// `run_task_to_sink` follows, above.
pub fn process_message(
    tx: &mpsc::Sender<DisplayEvent>,
    config: &NikiConfig,
    project_dir: &std::path::Path,
    submit: ChatSubmit,
) {
    // `/run <task>` starts the pipeline. Everything else is a conversation
    // turn. The split is explicit on purpose: see `run_task_from_chat`.
    if let Some(task) = submit.text.strip_prefix("/run ") {
        let task = task.trim();
        if task.is_empty() {
            let _ = tx.send(DisplayEvent::ChatError {
                message: "`/run` needs a task. Try: /run Add a GET /health endpoint".to_string(),
                cancelled: false,
            });
            return;
        }
        // On its own thread, so the chat keeps answering while a run is in
        // flight.
        //
        // It used to run inline, and `run_task_from_chat` builds a runtime and
        // `block_on`s the whole pipeline — so for the length of a run this
        // function never returned, its caller's `on_submit_rx.recv()` loop
        // never reached its next iteration, and **every message typed during a
        // run sat in the channel** until the run finished, then was answered
        // as if it had just been sent. A second `/run` waited behind the first
        // with nothing on screen saying so.
        //
        // It was hard to see because the TUI is a different thread and kept
        // reading keys: the *interface* stayed live and answered a question
        // mid-run while the *chat* was deaf. `tests/chat_stays_responsive_
        // during_a_run.rs` pins the cost of dispatch, which is where the two
        // differ.
        let run_tx = tx.clone();
        let run_config = config.clone();
        let run_project = project_dir.to_path_buf();
        let run_cancel = submit.cancel.clone();
        let run_mode = submit.permission_mode.clone();
        let run_task = task.to_string();
        std::thread::spawn(move || {
            run_task_from_chat(
                &run_tx,
                &run_config,
                &run_project,
                run_task,
                run_cancel,
                run_mode,
            );
        });
        return;
    }

    // TUI mode runs on a plain thread: bridge into async with a fresh
    // runtime. (The headless `--message` path awaits directly on the caller's
    // runtime instead — never nest runtimes here.)
    let result = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt.block_on(stream_reply(tx, config, &submit)),
        Err(e) => Err(format!("Could not start the async runtime: {e}")),
    };
    if let Err(message) = result {
        let _ = tx.send(DisplayEvent::ChatError {
            message,
            cancelled: false,
        });
    }
}

/// One-shot completion for the headless `--message` path.
///
/// No streaming: stdout is a pipe and the consumer wants the whole reply on
/// one line. History is still threaded through, so `--message` and the TUI
/// cannot disagree about what the model sees.
/// The headless reply, and whether it failed.
///
/// The second value is what makes the exit code honest. Everything here —
/// no provider, an authentication failure, an unknown model, a timeout — is a
/// failure the caller must be able to act on, and all of it was previously
/// returned as ordinary text.
async fn reply_text_with_status(config: &NikiConfig, user_text: &str) -> (String, bool) {
    let Some((provider, model)) = build_provider(config) else {
        return (NO_PROVIDER_MESSAGE.to_string(), true);
    };
    let req = crate::llm::provider::CompletionRequest {
        model,
        system_prompt: CHAT_SYSTEM_PROMPT.to_string(),
        user_message: user_text.to_string(),
        max_tokens: 4096,
        temperature: 0.7,
        json_schema: None,
        tools: None,
        reasoning_effort: None,
        history: Vec::new(),
    };
    match provider.complete(req).await {
        Ok(resp) if resp.content.trim().is_empty() => {
            ("(the model returned an empty response)".to_string(), true)
        }
        Ok(resp) => (resp.content, false),
        Err(e) => (describe_error(&e), true),
    }
}

pub async fn handle(args: &ChatArgs) -> Result<()> {
    let project_path = if args.project.is_relative() {
        std::env::current_dir()?.join(&args.project)
    } else {
        args.project.clone()
    };

    // A broken `niki.toml` is not the same as no `niki.toml`.
    //
    // This is the landing page — bare `niki` opens it — and a parse error was
    // swallowed into the default config. The user then talked to an assistant
    // configured with nothing they had written, and nothing anywhere said why.
    // They debugged the wrong thing for as long as it took to notice.
    //
    // The default is still right when there is no file; it is wrong when the
    // file exists and cannot be read. `NikiConfig::load` returns an error for
    // exactly the second case and `Ok` for the first, so the error can be
    // told apart from the absence.
    let config = NikiConfig::load(&project_path).map_err(|e| {
        anyhow::anyhow!(
            "Could not read your configuration: {e}\n\
             Nothing has been changed. Fix the file, or move it aside to run with \
             defaults.\n  {}\n\
             `niki config check` will tell you what is wrong with it.",
            project_path.join("niki.toml").display()
        )
    })?;

    // Headless contract: `--message` with piped stdout prints the reply as
    // plain text and exits, instead of launching the TUI (which would render
    // alternate-screen codes to the pipe and drop the reply on teardown).
    if let Some(msg) = &args.message
        && !std::io::IsTerminal::is_terminal(&std::io::stdout())
    {
        // A failed exchange must not exit 0.
        //
        // `reply_text` renders a provider failure as an ordinary assistant
        // message, so `niki chat --message hi` with a bad key printed the
        // error text on stdout and exited 0 — indistinguishable, to a script,
        // from a successful answer. `--message` is the documented non-
        // interactive path, so it is exactly the one a pipeline would use.
        let (reply, failed) = reply_text_with_status(&config, msg).await;
        println!("{reply}");
        if failed {
            anyhow::bail!("the model request failed; see the message above");
        }
        return Ok(());
    }

    // A TUI needs a terminal. Asking for one that is not there used to fail
    // silently: `run_tui` cannot enter raw mode, returns, and `handle` returns
    // `Ok(())` — so `niki chat` exited **0 having printed nothing**, which a
    // script cannot tell from a conversation. An explicit `niki chat` in a
    // pipeline is a mistake worth naming rather than absorbing.
    if !std::io::IsTerminal::is_terminal(&std::io::stdout())
        || !std::io::IsTerminal::is_terminal(&std::io::stdin())
    {
        anyhow::bail!(
            "`niki chat` needs an interactive terminal and there isn't one.\n\
             For a one-shot exchange, pipe it instead:\n  \
             niki chat --message \"your question\"\n\
             For a coding task:  niki run \"<task>\""
        );
    }

    // Create a long-lived channel so the TUI doesn't see Disconnect.
    let (tx, rx) = mpsc::channel::<DisplayEvent>();

    // Channel for submitted user messages, consumed by the session processor.
    let (on_submit_tx, on_submit_rx) = mpsc::channel::<ChatSubmit>();

    // One cancel handle, shared by the TUI and the processor thread. Esc sets
    // it in the TUI; the streaming loop reads it. Two separate flags is how Esc
    // came to say "Stopping…" and stop nothing.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Spawn the TUI in a background thread.
    let desc = args
        .message
        .clone()
        .unwrap_or_else(|| "chat session".to_string());
    let tui_tx = tx.clone();
    let tui_on_submit = on_submit_tx.clone();
    let tui_cancel = cancel.clone();
    let tui_project = project_path.clone();
    let handle = std::thread::spawn(move || {
        crate::display::tui::run_chat(rx, desc, tui_project, Some(tui_on_submit), tui_cancel)
    });

    // Spawn the message processor: each submitted user message gets a streamed
    // LLM reply.
    let proc_tx = tui_tx.clone();
    let proc_config = config.clone();
    let proc_project = project_path.clone();
    std::thread::spawn(move || {
        while let Ok(submit) = on_submit_rx.recv() {
            // A new turn supersedes a cancelled previous one.
            submit
                .cancel
                .store(false, std::sync::atomic::Ordering::Relaxed);
            process_message(&proc_tx, &proc_config, &proc_project, submit);
        }
    });

    // Send an initial message if provided.
    if let Some(msg) = &args.message {
        let _ = tui_tx.send(DisplayEvent::ChatMessage {
            role: "user".to_string(),
            text: msg.clone(),
        });
        // `chat_pending` is what draws the "thinking" line. The literal
        // "(thinking…)" bubble this replaces was an assistant turn, so it
        // stayed in the transcript forever and read as something the model had
        // said.
        let _ = tui_tx.send(DisplayEvent::ChatPending);
        let _ = on_submit_tx.send(ChatSubmit {
            text: msg.clone(),
            history: Vec::new(),
            cancel: cancel.clone(),
            // Headless: no badge, no TUI key press, so nothing to override.
            permission_mode: None,
        });
    }

    // Keep the sender alive so the TUI thread doesn't see Disconnect.
    // The TUI exits when the user presses Ctrl+C or quit.
    // A panic in the TUI thread must not exit 0.
    //
    // `let _ = handle.join()` discarded the `JoinError`, and `handle()` ended
    // `Ok(())`. `RestoreGuard` correctly restored raw mode and the alternate
    // screen on the way out, so the user got their terminal back — and a
    // script wrapping `niki` recorded success for a run that produced nothing.
    // The only way a panic reaches here is the render loop or a key handler,
    // which is exactly where the latent slicing and index defects live.
    if handle.join().is_err() {
        anyhow::bail!(
            "the chat interface stopped unexpectedly. \
             Your terminal has been restored; nothing was sent to the provider."
        );
    }

    Ok(())
}

/// Run a coding task from the chat: the four agents, in a sandbox, ending in a
/// reviewable `niki/<id>` branch.
///
/// This is the product's core promise, and until now the front door could not
/// honour it — `cli/chat.rs` contained no reference to the orchestrator at
/// all, so a bare `niki` could talk but never act. Every TUI page that shows a
/// pipeline (Run, Pipeline, Diff, Verdict, Cost, Artifacts, TestLog) rendered
/// fabricated content because nothing ever populated it.
///
/// `/run <task>` rather than inferring intent from every message: a chat that
/// silently starts a four-agent run on a message the user meant as a question
/// is a chat that spends money and writes to git without being asked, and this
/// product's whole character is being honest about what it did.
fn run_task_from_chat(
    tx: &mpsc::Sender<DisplayEvent>,
    config: &NikiConfig,
    project_dir: &std::path::Path,
    description: String,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    permission_mode: Option<String>,
) {
    // The posture the TUI badge is showing, if the user moved it.
    //
    // The badge used to be decoration — `state.permission_mode` was read by
    // the status bar and nothing else, so cycling it changed a label and not a
    // run. The posture that governs a stage is `config.permissions.mode`,
    // which `ToolContext` reads when it builds the stage's context, so
    // overriding it here lands on the stages that have not started yet.
    //
    // The stage **in flight** keeps the posture it was built with. Saying so is
    // the difference between a control that works and one that appears broken.
    let mut effective = config.clone();
    if let Some(mode) = permission_mode.as_deref() {
        effective.permissions.mode = crate::runtime::ToolContext::parse_permission_mode(mode);
    }
    let effective = &effective;
    let outcome = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Could not start the async runtime: {e}"))
        .and_then(|rt| {
            rt.block_on(async move {
                run_task_to_sink(tx, effective, project_dir, description, cancel).await
            })
        });

    if let Err(message) = outcome {
        let _ = tx.send(DisplayEvent::ChatError {
            message,
            cancelled: false,
        });
    }
}

/// Run a coding task and stream every stage into `tx`.
///
/// Public because it is a real capability, not a test hook: the chat is one
/// caller, and this is the same sequence `niki run` performs — pipeline, then
/// delivery — with the difference that progress goes to an event sink the caller
/// owns instead of a render thread of its own.
pub async fn run_task_to_sink(
    tx: &mpsc::Sender<DisplayEvent>,
    config: &NikiConfig,
    project_dir: &std::path::Path,
    description: String,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    use crate::display::agent_stream::AgenticDisplay;
    use crate::orchestrator::pipeline::{Task, execute_pipeline};
    use crate::sandbox::docker::ActiveContainers;
    use uuid::Uuid;

    let task = Task {
        id: Uuid::new_v4(),
        description: description.clone(),
        project_path: project_dir.to_path_buf(),
    };
    let task_dir = project_dir
        .join(&config.general.output_dir)
        .join("tasks")
        .join(task.id.to_string());
    std::fs::create_dir_all(&task_dir)
        .map_err(|e| format!("Could not create {}: {e}", task_dir.display()))?;

    let branch_name = format!("niki/{}", &task.id.to_string()[..8]);

    // The display feeds the chat's own event loop. `muted` is set by
    // `attach_sink` so stage progress does not also print to stdout and
    // corrupt the alternate screen.
    let mut display = AgenticDisplay::new();
    display.attach_sink(tx.clone(), cancel.clone());

    // The backend comes from the configuration, NOT from whether a container
    // runtime happens to be reachable. Deriving it from reachability looks
    // equivalent and is not: on a machine with Podman installed and
    // `backend = "worktree"` in `niki.toml`, the sandbox writes into a git
    // worktree while delivery was told the diff was already on the host — so
    // it is never replayed, the commit comes out empty, and the hermetic proof
    // fails with "committed state changed during the run". A reachable runtime
    // says nothing about which backend this run uses.
    let uses_docker = matches!(
        config.docker.backend,
        crate::sandbox::SandboxBackend::Docker
    );
    // Only the container backend needs a runtime; the worktree path needs
    // none, and that is what the zero-setup install takes.
    let docker = if uses_docker {
        crate::cli::run::connect_container_runtime().await.ok()
    } else {
        None
    };
    let docker_ref = docker.as_ref();

    let pre_snapshot = crate::safety::snapshot(project_dir).ok();
    // `ActiveContainers` is `Arc<Mutex<Vec<String>>>` over whichever Mutex the
    // sandbox module imports, so name the alias rather than re-spelling it.
    let containers: ActiveContainers = Default::default();

    let mut result = execute_pipeline(
        &task,
        config,
        docker_ref,
        &mut display,
        containers,
        false, // dry_run
        cancel,
        &task_dir,
        None,  // plan_override_json
        false, // bare
    )
    .await
    .map_err(|e| format!("The pipeline failed: {e}"))?;

    // Delivery is the same function `niki run` uses, so a task started here
    // produces exactly the same artefacts: a branch carrying the Coder's
    // change, a report, a patch, per-agent JSON, and a hermetic proof.
    let delivered =
        crate::orchestrator::deliver::deliver(crate::orchestrator::deliver::DeliverInput {
            task: &task,
            config,
            result: &mut result,
            project_dir,
            task_dir: &task_dir,
            branch_name: branch_name.clone(),
            pre_snapshot: pre_snapshot.as_ref(),
            uses_docker,
            dry_run: false,
            force: false,
            json_mode: false,
            display: Some(&mut display),
        })
        .map_err(|e| format!("Delivery failed: {e}"))?;

    // Say what actually happened. The pipeline's own events already filled the
    // stage views; this is the sentence the user reads.
    let summary = if let Some(note) = &delivered.block_note {
        format!(
            "**No branch.** {note}\n\nEvidence is in `{}`.",
            task_dir.display()
        )
    } else if delivered.error.is_some() {
        format!(
            "**No branch.** {}\n\nEvidence is in `{}`.",
            delivered.error.unwrap_or_default(),
            task_dir.display()
        )
    } else if delivered.branch_created {
        format!(
            "**Done.** Branch `{}` · verdict **{:?}** · {} revision(s).\n\n\
             Review it with `git diff main...{}` — the report and per-agent artifacts are in `{}`.",
            branch_name,
            result.verdict,
            result.revision_rounds,
            branch_name,
            task_dir.display()
        )
    } else {
        format!(
            "**No branch.** The agents produced no file changes.\n\nEvidence is in `{}`.",
            task_dir.display()
        )
    };
    // Three events, in this order, and the order is the point:
    //
    //   1. `ChatFinished` commits whatever the pipeline streamed into the
    //      in-flight buffer, so the run's own last words become a real turn
    //      rather than being discarded when the summary arrives.
    //   2. `ChatDelta` streams the summary.
    //   3. `ChatFinished` commits the summary as its own assistant turn.
    //
    // Sending only the delta would leave the pipeline's trailing text stuck in
    // `chat_stream` forever, and sending the summary as a `ChatMessage` would
    // skip the streaming path the rest of the surface already uses.
    let _ = tx.send(DisplayEvent::ChatFinished {
        finish_reason: None,
        // The pipeline billed this turn itself; re-counting it here would
        // double it. `None` means "not counted here", not "costed nothing".
        usage: None,
        cost_usd: 0.0,
    });
    let _ = tx.send(DisplayEvent::ChatDelta { text: summary });
    let _ = tx.send(DisplayEvent::ChatFinished {
        finish_reason: None,
        usage: None,
        cost_usd: 0.0,
    });
    Ok(())
}

#[cfg(test)]
mod reply_stream_tests {
    use super::*;
    use futures::Stream;

    fn chunk_stream(
        items: Vec<StreamChunk>,
    ) -> Pin<Box<dyn Stream<Item = anyhow::Result<StreamChunk>> + Send>> {
        Box::pin(futures::stream::iter(items.into_iter().map(Ok)))
    }

    /// The reply stream's **usage** reaches the caller.
    ///
    /// This is the code that lost it: `StreamChunk::Usage` was matched with
    /// `{}`, so a chat conversation never moved `token_count`, the `ctx` gauge
    /// in the status bar read 0% for ever, and `/context` reported
    /// "Utilized: 0%" while the model was being handed the whole conversation
    /// on every request.
    ///
    /// The first version of this test drove `AppState::apply_display_event`
    /// with a usage value instead, and was **green against the defect** — it
    /// could not see whether the chat ever produced one. This one starts at
    /// the stream.
    #[tokio::test]
    async fn the_reply_streams_usage_back_to_the_caller() {
        let (tx, _rx) = mpsc::channel();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let stream = chunk_stream(vec![
            StreamChunk::Text("hello".into()),
            StreamChunk::Usage(TokenUsage {
                input_tokens: 1200,
                output_tokens: 340,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
            }),
            StreamChunk::Finish {
                reason: "stop".into(),
            },
        ]);

        let (text, reason, usage) = consume_reply(stream, &tx, &cancel).await.unwrap();
        assert_eq!(text, "hello");
        assert_eq!(reason.as_deref(), Some("stop"));
        let u = usage.expect(
            "the provider reported usage and the stream dropped it — the context \
             gauge is decoration that reads 0% for ever",
        );
        assert_eq!(u.input_tokens, 1200);
        assert_eq!(u.output_tokens, 340);
    }

    /// A provider that reports no usage says so, rather than reporting zero.
    #[tokio::test]
    async fn no_usage_is_not_zero_usage() {
        let (tx, _rx) = mpsc::channel();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let stream = chunk_stream(vec![
            StreamChunk::Text("hi".into()),
            StreamChunk::Finish {
                reason: "stop".into(),
            },
        ]);
        let (_, _, usage) = consume_reply(stream, &tx, &cancel).await.unwrap();
        assert!(
            usage.is_none(),
            "a provider that sent no Usage chunk reported none, which is not the \
             same as reporting zero: {usage:?}"
        );
    }

    /// A chat turn is priced, and the **model name is the rate card**.
    ///
    /// This is the layer the last slice recorded as uncovered. The
    /// state-level test drives `apply_display_event` with an explicit
    /// `cost_usd`, so it was green whether or not the chat computed one —
    /// replacing the pricing with `|_| 0.0` left every test passing while
    /// `/cost` went back to reporting `$0.0000` after paid API calls.
    ///
    /// Two things have to be right at once, and each breaks differently:
    /// pricing with the *provider's own* name, and pricing with the *model the
    /// user is actually talking to* rather than a hard-coded one.
    #[test]
    fn a_chat_turn_is_priced_from_the_model_the_user_is_using() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
        };
        // A million input tokens is not free at any known rate card.
        let priced = price_chat_turn("anthropic", "claude-sonnet-4", Some(&usage));
        assert!(
            priced > 0.0,
            "a chat turn must be priced; /cost said $0.0000 after a paid call"
        );

        // A different model, same usage: a different price. Pricing from the
        // provider name alone, or from a constant, would return the same number
        // and this would pass.
        let other = price_chat_turn("anthropic", "claude-haiku-4", Some(&usage));
        assert!(
            (other - priced).abs() > f64::EPSILON,
            "the model name is the rate card: {priced} vs {other}"
        );

        // And nothing reported is nothing charged.
        assert_eq!(price_chat_turn("anthropic", "claude-sonnet-4", None), 0.0);
    }

    /// A cancelled reply says it was cancelled, and stops.
    ///
    /// The first draft of this asserted that the partial text is *returned*,
    /// and it failed with `left: ""`. That was the test being wrong: the loop
    /// checks the flag before accumulating, so a stream cancelled up front
    /// yields no text — which is right. Text already streamed reached the user
    /// as `ChatDelta` events and is on their screen; this function returning it
    /// again would duplicate it.
    ///
    /// What matters, and is asserted here, is that cancelling is *visible* —
    /// a silent stop would leave a user waiting for an answer that was never
    /// coming.
    #[tokio::test]
    async fn a_cancelled_reply_says_so_and_stops() {
        let (tx, rx) = mpsc::channel();
        let cancel = std::sync::atomic::AtomicBool::new(true);
        let stream = chunk_stream(vec![
            StreamChunk::Text("partial".into()),
            StreamChunk::Usage(TokenUsage {
                input_tokens: 900,
                output_tokens: 100,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
            }),
        ]);
        let _ = consume_reply(stream, &tx, &cancel).await.unwrap();

        let mut saw_cancel = false;
        while let Ok(ev) = rx.try_recv() {
            if let DisplayEvent::ChatError { cancelled, .. } = ev {
                assert!(cancelled, "the event must be marked as a cancellation");
                saw_cancel = true;
            }
        }
        assert!(
            saw_cancel,
            "a cancelled reply must tell the user it was cancelled; a silent stop \
             is indistinguishable from a hang"
        );
    }
}

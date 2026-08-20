//! Hook bus — 22 predefined hook events with an exact exit-code contract.
//!
//! Hooks are arbitrary shell commands registered per event. The contract is
//! identical to Claude Code / kimi:
//!
//! - exit code `2`  → **block** the triggering action
//! - stdout parses to JSON `{"deny": true}` → **block**
//! - command not found / crashed / non-zero (other than 2) → **no-op**
//! - otherwise → **allow**
//!
//! Events are fired before and after the corresponding phase so users can
//! enforce policy (e.g. block `git push`, redact secrets, audit writes).

use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

/// The 20 predefined hook events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PreToolEdit,
    PostToolEdit,
    PreToolRead,
    PostToolRead,
    PreToolBash,
    PostToolBash,
    PreToolEditFormat,
    PostToolEditFormat,
    PreAgentStart,
    PostAgentStart,
    PreAgentStop,
    PostAgentStop,
    PreSubagentSpawn,
    PostSubagentSpawn,
    PreTaskStart,
    PostTaskStart,
    PreTaskStop,
    PostTaskStop,
    PreCompact,
    PostCompact,
}

impl HookEvent {
    pub const ALL: [HookEvent; 22] = [
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::PreToolEdit,
        HookEvent::PostToolEdit,
        HookEvent::PreToolRead,
        HookEvent::PostToolRead,
        HookEvent::PreToolBash,
        HookEvent::PostToolBash,
        HookEvent::PreToolEditFormat,
        HookEvent::PostToolEditFormat,
        HookEvent::PreAgentStart,
        HookEvent::PostAgentStart,
        HookEvent::PreAgentStop,
        HookEvent::PostAgentStop,
        HookEvent::PreSubagentSpawn,
        HookEvent::PostSubagentSpawn,
        HookEvent::PreTaskStart,
        HookEvent::PostTaskStart,
        HookEvent::PreTaskStop,
        HookEvent::PostTaskStop,
        HookEvent::PreCompact,
        HookEvent::PostCompact,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::PreToolEdit => "PreToolEdit",
            HookEvent::PostToolEdit => "PostToolEdit",
            HookEvent::PreToolRead => "PreToolRead",
            HookEvent::PostToolRead => "PostToolRead",
            HookEvent::PreToolBash => "PreToolBash",
            HookEvent::PostToolBash => "PostToolBash",
            HookEvent::PreToolEditFormat => "PreToolEditFormat",
            HookEvent::PostToolEditFormat => "PostToolEditFormat",
            HookEvent::PreAgentStart => "PreAgentStart",
            HookEvent::PostAgentStart => "PostAgentStart",
            HookEvent::PreAgentStop => "PreAgentStop",
            HookEvent::PostAgentStop => "PostAgentStop",
            HookEvent::PreSubagentSpawn => "PreSubagentSpawn",
            HookEvent::PostSubagentSpawn => "PostSubagentSpawn",
            HookEvent::PreTaskStart => "PreTaskStart",
            HookEvent::PostTaskStart => "PostTaskStart",
            HookEvent::PreTaskStop => "PreTaskStop",
            HookEvent::PostTaskStop => "PostTaskStop",
            HookEvent::PreCompact => "PreCompact",
            HookEvent::PostCompact => "PostCompact",
        }
    }
}

/// Outcome of running a hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Allow,
    Block(String),
    Noop,
}

/// A registry of hook commands keyed by event, plus the runner.
pub struct HookBus {
    scripts: HashMap<HookEvent, Vec<String>>,
}

impl Default for HookBus {
    fn default() -> Self {
        Self::new()
    }
}

impl HookBus {
    pub fn new() -> Self {
        Self {
            scripts: HashMap::new(),
        }
    }

    /// Register a hook command for an event (multiple commands run in order).
    pub fn register(&mut self, event: HookEvent, command: String) {
        self.scripts.entry(event).or_default().push(command);
    }

    /// Whether any hook is registered for an event.
    pub fn has_hooks(&self, event: HookEvent) -> bool {
        self.scripts.contains_key(&event)
    }

    /// Fire all hooks for an event. The first `Block` short-circuits the rest.
    pub fn run(&self, event: HookEvent, payload: &str) -> HookOutcome {
        let Some(commands) = self.scripts.get(&event) else {
            return HookOutcome::Allow;
        };
        for command in commands {
            match run_one(command, event, payload) {
                HookOutcome::Block(reason) => return HookOutcome::Block(reason),
                HookOutcome::Noop => continue,
                HookOutcome::Allow => continue,
            }
        }
        HookOutcome::Allow
    }
}

/// Run a single hook command and interpret its exit contract.
fn run_one(command: &str, event: HookEvent, payload: &str) -> HookOutcome {
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("NIKI_HOOK_EVENT", event.as_str())
        .env("NIKI_HOOK_PAYLOAD", payload)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return HookOutcome::Noop,
    };

    let stdin = child.stdin.take();
    if let Some(mut s) = stdin {
        if s.write_all(payload.as_bytes()).is_err() {
            return HookOutcome::Noop;
        }
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(_) => return HookOutcome::Noop,
    };

    if !output.status.success() {
        // Exit code 2 is the explicit "block" signal.
        if output.status.code() == Some(2) {
            let reason = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return HookOutcome::Block(if reason.is_empty() {
                format!("hook for {} exited 2", event.as_str())
            } else {
                reason
            });
        }
        // Any other failure is a no-op (don't block on hook crashes).
        return HookOutcome::Noop;
    }

    // A hook that prints `{"deny": true}` also blocks.
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout)
        && value.get("deny").and_then(|v| v.as_bool()) == Some(true)
    {
        return HookOutcome::Block(
            if let Some(r) = value.get("reason").and_then(|v| v.as_str()) {
                r.to_string()
            } else {
                format!("hook for {} denied the action", event.as_str())
            },
        );
    }

    HookOutcome::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_events_are_22() {
        assert_eq!(HookEvent::ALL.len(), 22);
        let mut seen = std::collections::HashSet::new();
        for e in HookEvent::ALL {
            assert!(seen.insert(e), "duplicate event: {}", e.as_str());
        }
    }

    #[test]
    fn no_hooks_allows() {
        let bus = HookBus::new();
        assert_eq!(
            bus.run(HookEvent::PreToolBash, "rm -rf /"),
            HookOutcome::Allow
        );
    }

    #[test]
    fn exit_code_two_blocks() {
        let mut bus = HookBus::new();
        bus.register(
            HookEvent::PreToolBash,
            "echo blocked-by-exit-2; exit 2".to_string(),
        );
        let outcome = bus.run(HookEvent::PreToolBash, "git push");
        assert!(matches!(outcome, HookOutcome::Block(_)));
    }

    #[test]
    fn json_deny_blocks() {
        let mut bus = HookBus::new();
        bus.register(
            HookEvent::PreToolBash,
            "printf '{\"deny\": true, \"reason\": \"no push\"}'".to_string(),
        );
        let outcome = bus.run(HookEvent::PreToolBash, "git push");
        match &outcome {
            HookOutcome::Block(r) => assert!(r.contains("no push")),
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn json_deny_false_allows() {
        let mut bus = HookBus::new();
        bus.register(
            HookEvent::PreToolBash,
            "printf '{\"deny\": false}'".to_string(),
        );
        assert_eq!(
            bus.run(HookEvent::PreToolBash, "cargo test"),
            HookOutcome::Allow
        );
    }

    #[test]
    fn missing_command_is_noop() {
        let mut bus = HookBus::new();
        bus.register(
            HookEvent::PreToolBash,
            "this-command-does-not-exist-xyz".to_string(),
        );
        // A missing hook must NOT block the action.
        assert_eq!(
            bus.run(HookEvent::PreToolBash, "cargo test"),
            HookOutcome::Allow
        );
    }
}

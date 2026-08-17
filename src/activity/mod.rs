//! Activity grammar — canonical agent activity states.
//!
//! Every agent state has: semantic meaning, icon, label, animation behavior,
//! attention priority, and transition rules.
//!
//! States:
//! - IDLE, THINKING, PLANNING, SEARCHING, READING, WRITING, RUNNING,
//!   WAITING, REVIEWING, BLOCKED, ERROR, COMPLETE

use std::fmt;

/// Canonical agent activity states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentState {
    /// Agent is not doing anything.
    Idle,
    /// Agent is reasoning (thinking step).
    Thinking,
    /// Agent is planning next actions.
    Planning,
    /// Agent is searching codebase or web.
    Searching,
    /// Agent is reading files.
    Reading,
    /// Agent is writing/editing files.
    Writing,
    /// Agent is running a command or test.
    Running,
    /// Agent is waiting for dependency, approval, or input.
    Waiting,
    /// Agent is reviewing code.
    Reviewing,
    /// Agent is blocked — cannot proceed without human intervention.
    Blocked,
    /// Agent encountered an error.
    Error,
    /// Agent completed its task.
    Complete,
}

impl AgentState {
    /// Icon for display.
    pub fn icon(&self) -> &'static str {
        match self {
            AgentState::Idle => "·",
            AgentState::Thinking => "◌",
            AgentState::Planning => "◇",
            AgentState::Searching => "⌕",
            AgentState::Reading => "→",
            AgentState::Writing => "✎",
            AgentState::Running => ">",
            AgentState::Waiting => "◷",
            AgentState::Reviewing => "⊙",
            AgentState::Blocked => "!",
            AgentState::Error => "x",
            AgentState::Complete => "✓",
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            AgentState::Idle => "Idle",
            AgentState::Thinking => "Reasoning",
            AgentState::Planning => "Planning",
            AgentState::Searching => "Searching",
            AgentState::Reading => "Reading",
            AgentState::Writing => "Editing",
            AgentState::Running => "Running",
            AgentState::Waiting => "Waiting",
            AgentState::Reviewing => "Reviewing",
            AgentState::Blocked => "Blocked",
            AgentState::Error => "Error",
            AgentState::Complete => "Completed",
        }
    }

    /// Whether the agent is actively doing something (for animation).
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentState::Thinking
                | AgentState::Planning
                | AgentState::Searching
                | AgentState::Reading
                | AgentState::Writing
                | AgentState::Running
                | AgentState::Reviewing
        )
    }

    /// Whether the agent needs human attention.
    pub fn needs_attention(&self) -> bool {
        matches!(
            self,
            AgentState::Blocked | AgentState::Error | AgentState::Waiting
        )
    }

    /// Attention priority (higher = more attention needed).
    pub fn attention_priority(&self) -> u8 {
        match self {
            AgentState::Idle | AgentState::Complete => 0,
            AgentState::Thinking
            | AgentState::Planning
            | AgentState::Searching
            | AgentState::Reading
            | AgentState::Writing
            | AgentState::Running
            | AgentState::Reviewing => 1,
            AgentState::Waiting => 2,
            AgentState::Blocked | AgentState::Error => 3,
        }
    }

    /// Allowed transitions from this state.
    pub fn allowed_transitions(&self) -> &'static [AgentState] {
        match self {
            AgentState::Idle => &[
                AgentState::Thinking,
                AgentState::Planning,
                AgentState::Searching,
                AgentState::Reading,
                AgentState::Writing,
                AgentState::Running,
                AgentState::Waiting,
                AgentState::Reviewing,
            ],
            AgentState::Thinking => &[
                AgentState::Planning,
                AgentState::Searching,
                AgentState::Reading,
                AgentState::Writing,
                AgentState::Running,
                AgentState::Complete,
                AgentState::Error,
            ],
            AgentState::Planning => &[
                AgentState::Thinking,
                AgentState::Searching,
                AgentState::Reading,
                AgentState::Writing,
                AgentState::Running,
                AgentState::Complete,
                AgentState::Error,
            ],
            AgentState::Searching => &[
                AgentState::Reading,
                AgentState::Writing,
                AgentState::Running,
                AgentState::Complete,
                AgentState::Error,
            ],
            AgentState::Reading => &[
                AgentState::Thinking,
                AgentState::Writing,
                AgentState::Running,
                AgentState::Complete,
                AgentState::Error,
            ],
            AgentState::Writing => &[AgentState::Running, AgentState::Complete, AgentState::Error],
            AgentState::Running => &[
                AgentState::Thinking,
                AgentState::Reading,
                AgentState::Writing,
                AgentState::Complete,
                AgentState::Error,
                AgentState::Waiting,
            ],
            AgentState::Waiting => &[
                AgentState::Thinking,
                AgentState::Running,
                AgentState::Blocked,
                AgentState::Complete,
                AgentState::Error,
            ],
            AgentState::Reviewing => &[
                AgentState::Thinking,
                AgentState::Writing,
                AgentState::Complete,
                AgentState::Error,
            ],
            AgentState::Blocked => &[AgentState::Thinking, AgentState::Running, AgentState::Error],
            AgentState::Error => &[AgentState::Idle, AgentState::Thinking],
            AgentState::Complete => &[AgentState::Idle],
        }
    }

    /// Check if transition from self to target is allowed.
    pub fn can_transition_to(&self, target: &AgentState) -> bool {
        self.allowed_transitions().contains(target)
    }
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.icon(), self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_icons() {
        assert_eq!(AgentState::Idle.icon(), "·");
        assert_eq!(AgentState::Thinking.icon(), "◌");
        assert_eq!(AgentState::Complete.icon(), "✓");
        assert_eq!(AgentState::Error.icon(), "x");
        assert_eq!(AgentState::Blocked.icon(), "!");
    }

    #[test]
    fn state_labels() {
        assert_eq!(AgentState::Idle.label(), "Idle");
        assert_eq!(AgentState::Thinking.label(), "Reasoning");
        assert_eq!(AgentState::Writing.label(), "Editing");
    }

    #[test]
    fn state_active() {
        assert!(AgentState::Thinking.is_active());
        assert!(!AgentState::Idle.is_active());
        assert!(!AgentState::Complete.is_active());
    }

    #[test]
    fn state_needs_attention() {
        assert!(AgentState::Blocked.needs_attention());
        assert!(AgentState::Error.needs_attention());
        assert!(AgentState::Waiting.needs_attention());
        assert!(!AgentState::Thinking.needs_attention());
    }

    #[test]
    fn state_attention_priority() {
        assert_eq!(AgentState::Idle.attention_priority(), 0);
        assert_eq!(AgentState::Thinking.attention_priority(), 1);
        assert_eq!(AgentState::Waiting.attention_priority(), 2);
        assert_eq!(AgentState::Blocked.attention_priority(), 3);
    }

    #[test]
    fn state_transitions() {
        assert!(AgentState::Idle.can_transition_to(&AgentState::Thinking));
        assert!(AgentState::Thinking.can_transition_to(&AgentState::Complete));
        assert!(!AgentState::Complete.can_transition_to(&AgentState::Thinking));
        assert!(!AgentState::Error.can_transition_to(&AgentState::Complete));
    }

    #[test]
    fn state_display() {
        let s = format!("{}", AgentState::Thinking);
        assert!(s.contains("◌"));
        assert!(s.contains("Reasoning"));
    }
}

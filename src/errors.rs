use thiserror::Error;

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("LLM provider timeout after {timeout_ms}ms")]
    ProviderTimeout { timeout_ms: u64 },

    #[error("LLM provider error: {message}")]
    ProviderError { message: String },

    #[error("JSON schema validation failed: {detail}")]
    SchemaViolation { detail: String },

    #[error("Context window overflow: {tokens} tokens exceeds limit")]
    ContextOverflow { tokens: u64 },

    #[error("Scope violation: attempted to modify {path} which is outside scope_lock")]
    ScopeViolation { path: String },

    #[error("Hermeticity violation: {detail}")]
    HermeticityViolation { detail: String },

    #[error("Edit application failed: {kind}: {message}")]
    EditFailure { kind: String, message: String },

    #[error("Tool execution denied: {command} is not allowed for role {role}")]
    ToolNotAllowed { command: String, role: String },

    #[error("Execution timeout: command '{command}' exceeded {max_seconds}s")]
    ExecTimeout { command: String, max_seconds: u64 },

    #[error("Goal halted: {reason}")]
    GoalHalted { reason: String },

    #[error("Goal cancelled by user")]
    GoalCancelled,

    #[error("Goal paused")]
    GoalPaused,

    #[error("Configuration error: {detail}")]
    ConfigError { detail: String },

    #[error("IO error: {detail}")]
    IoError { detail: String },

    #[error("Serialization error: {detail}")]
    SerializationError { detail: String },
}

impl PipelineError {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            PipelineError::ProviderTimeout { .. }
                | PipelineError::ProviderError { .. }
                | PipelineError::ExecTimeout { .. }
        )
    }

    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            PipelineError::SchemaViolation { .. }
                | PipelineError::ScopeViolation { .. }
                | PipelineError::HermeticityViolation { .. }
                | PipelineError::ToolNotAllowed { .. }
        )
    }
}

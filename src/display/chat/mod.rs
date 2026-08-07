//! Chat display module — conversational message rendering with streaming markdown.
//!
//! Provides:
//! - [`message`] — render user/assistant/system messages
//! - [`streaming`] — real-time token streaming with incomplete markdown handling
//! - [`markdown`] — pulldown-cmark based parser + renderer
//! - [`code_block`] — syntax-highlighted code blocks

pub mod code_block;
pub mod markdown;
pub mod message;
pub mod streaming;

// Re-exports for convenience
pub use message::{MessageRenderConfig, render_message};
pub use streaming::StreamingMessage;

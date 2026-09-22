//! Runtime cancellation tokens and hierarchical cancellation management.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

/// A thread-safe cancellation token supporting hierarchical cancellation,
/// cancellation reasons, and async waiting.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
    reason: Arc<std::sync::RwLock<Option<String>>>,
    parent: Option<Box<CancellationToken>>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Create a new un-cancelled root token.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            reason: Arc::new(std::sync::RwLock::new(None)),
            parent: None,
        }
    }

    /// Wrap an existing `Arc<AtomicBool>` for interoperability with existing NIKI cancellation.
    pub fn from_atomic(atomic: Arc<AtomicBool>) -> Self {
        Self {
            cancelled: atomic,
            notify: Arc::new(Notify::new()),
            reason: Arc::new(std::sync::RwLock::new(None)),
            parent: None,
        }
    }

    /// Create a child cancellation token. If the parent cancels, the child is cancelled.
    pub fn child(&self) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(self.is_cancelled())),
            notify: Arc::new(Notify::new()),
            reason: Arc::new(std::sync::RwLock::new(self.reason())),
            parent: Some(Box::new(self.clone())),
        }
    }

    /// Check if this token or any parent token is cancelled.
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return true;
        }
        if let Some(ref parent) = self.parent {
            if parent.is_cancelled() {
                self.cancelled.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Cancel this token with an optional reason.
    pub fn cancel(&self, reason: Option<String>) {
        if !self.cancelled.swap(true, Ordering::Relaxed) {
            if let Ok(mut lock) = self.reason.write() {
                *lock = reason;
            }
            self.notify.notify_waiters();
        }
    }

    /// Get the cancellation reason, if one was provided.
    pub fn reason(&self) -> Option<String> {
        if let Ok(lock) = self.reason.read() {
            if lock.is_some() {
                return lock.clone();
            }
        }
        if let Some(ref parent) = self.parent {
            return parent.reason();
        }
        None
    }

    /// Asynchronously wait until the token is cancelled.
    pub async fn wait_cancelled(&self) {
        while !self.is_cancelled() {
            self.notify.notified().await;
        }
    }

    /// Underlying atomic bool for backwards compatibility.
    pub fn as_atomic(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel(Some("test abort".to_string()));
        assert!(token.is_cancelled());
        assert_eq!(token.reason().as_deref(), Some("test abort"));
    }

    #[test]
    fn test_child_cancellation() {
        let parent = CancellationToken::new();
        let child = parent.child();
        assert!(!child.is_cancelled());
        parent.cancel(Some("parent stopped".to_string()));
        assert!(child.is_cancelled());
        assert_eq!(child.reason().as_deref(), Some("parent stopped"));
    }
}

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    User,
    Deadline,
    OwnerDropped,
}

struct CancellationState {
    reason: Mutex<Option<CancelReason>>,
    signal: watch::Sender<()>,
    parent: Option<Cancellation>,
}

/// A cancellation scope.
///
/// Cloning a value preserves the same scope. A child scope has its own
/// cancellation state and observes its parent, so cancellation only flows
/// from parent to child and never in the other direction.
#[derive(Clone)]
pub struct Cancellation(Arc<CancellationState>);

impl Default for Cancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancellation {
    pub fn new() -> Self {
        let (signal, _) = watch::channel(());
        Self(Arc::new(CancellationState {
            reason: Mutex::new(None),
            signal,
            parent: None,
        }))
    }

    /// Creates a scope whose cancellation is independent until the parent is
    /// cancelled. Parent cancellation is observed by all descendants.
    pub fn child_scope(&self) -> Self {
        let (signal, _) = watch::channel(());
        Self(Arc::new(CancellationState {
            reason: Mutex::new(None),
            signal,
            parent: Some(self.clone()),
        }))
    }

    /// Cancels this scope for a user initiated operation.
    pub fn cancel(&self) {
        self.cancel_with_reason(CancelReason::User);
    }

    pub fn cancel_with_reason(&self, reason: CancelReason) {
        let should_signal = {
            let mut current = self
                .0
                .reason
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if current.is_some()
                || self
                    .0
                    .parent
                    .as_ref()
                    .and_then(Cancellation::reason)
                    .is_some()
            {
                false
            } else {
                *current = Some(reason);
                true
            }
        };
        if should_signal {
            self.0.signal.send_replace(());
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    pub fn reason(&self) -> Option<CancelReason> {
        let own = *self
            .0
            .reason
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        own.or_else(|| self.0.parent.as_ref().and_then(Cancellation::reason))
    }

    pub async fn cancelled(&self) {
        let mut receiver = self.0.signal.subscribe();
        if self.is_cancelled() {
            return;
        }
        if let Some(parent) = self.0.parent.clone() {
            let parent_wait = Box::pin(parent.cancelled());
            tokio::select! {
                biased;
                _ = receiver.changed() => {}
                _ = parent_wait => {}
            }
        } else {
            let _ = receiver.changed().await;
        }
    }
}

impl std::fmt::Debug for Cancellation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Cancellation")
            .field("reason", &self.reason())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn child_cancellation_does_not_reach_parent_or_sibling() {
        let parent = Cancellation::default();
        let child = parent.child_scope();
        let sibling = parent.child_scope();

        child.cancel();

        assert_eq!(child.reason(), Some(CancelReason::User));
        assert!(!parent.is_cancelled());
        assert!(!sibling.is_cancelled());
    }

    #[tokio::test]
    async fn parent_cancellation_wakes_all_descendants() {
        let parent = Cancellation::default();
        let child = parent.child_scope();
        let grandchild = child.child_scope();
        let child_started = Arc::new(tokio::sync::Notify::new());
        let grandchild_started = Arc::new(tokio::sync::Notify::new());
        let child_wait = child.clone();
        let grandchild_wait = grandchild.clone();
        let child_started_wait = child_started.clone();
        let grandchild_started_wait = grandchild_started.clone();
        let child_woke = tokio::spawn(async move {
            child_started_wait.notify_one();
            child_wait.cancelled().await;
            child_wait.reason()
        });
        let grandchild_woke = tokio::spawn(async move {
            grandchild_started_wait.notify_one();
            grandchild_wait.cancelled().await;
            grandchild_wait.reason()
        });

        child_started.notified().await;
        grandchild_started.notified().await;
        parent.cancel_with_reason(CancelReason::Deadline);

        assert_eq!(child_woke.await.unwrap(), Some(CancelReason::Deadline));
        assert_eq!(grandchild_woke.await.unwrap(), Some(CancelReason::Deadline));
    }

    #[test]
    fn first_reason_is_preserved() {
        let cancellation = Cancellation::default();
        cancellation.cancel_with_reason(CancelReason::Deadline);
        cancellation.cancel_with_reason(CancelReason::OwnerDropped);
        assert_eq!(cancellation.reason(), Some(CancelReason::Deadline));
    }

    #[test]
    fn inherited_reason_is_preserved_when_child_owner_drops() {
        let parent = Cancellation::default();
        let child = parent.child_scope();
        parent.cancel();
        child.cancel_with_reason(CancelReason::OwnerDropped);
        assert_eq!(child.reason(), Some(CancelReason::User));
    }
}

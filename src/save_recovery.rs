//! Typed state for retrying or abandoning one failed local save.
//!
//! The controller owns both semantic snapshots while recovery is unresolved. Callers keep
//! their displayed working model separate, then promote the working snapshot on Retry or
//! restore the baseline on Cancel.

/// A failed save's retained semantic snapshots and visible error.
#[derive(Debug)]
pub struct SaveFailure<T> {
    baseline: T,
    working: T,
    error: String,
}

/// One local save boundary: clean, or holding the exact baseline and failed working state.
#[derive(Debug, Default)]
pub struct SaveRecovery<T> {
    failed: Option<SaveFailure<T>>,
}

impl<T> SaveRecovery<T> {
    /// Empty controller with no unresolved persistence failure.
    pub fn new() -> Self {
        Self { failed: None }
    }

    /// Start recovery after a persistence error. Replaces no existing failure state.
    pub fn fail(&mut self, baseline: T, working: T, error: impl Into<String>) {
        debug_assert!(
            self.failed.is_none(),
            "only one save recovery may be unresolved"
        );
        self.failed = Some(SaveFailure {
            baseline,
            working,
            error: error.into(),
        });
    }

    /// Whether a failed save still needs an explicit Retry or Cancel.
    pub fn is_pending(&self) -> bool {
        self.failed.is_some()
    }

    /// Last persisted semantic baseline, while recovery is unresolved.
    pub fn baseline(&self) -> Option<&T> {
        self.failed.as_ref().map(|failure| &failure.baseline)
    }

    /// Intended working semantic state, while recovery is unresolved.
    pub fn working(&self) -> Option<&T> {
        self.failed.as_ref().map(|failure| &failure.working)
    }

    /// Last persistence error, while recovery is unresolved.
    pub fn error(&self) -> Option<&str> {
        self.failed.as_ref().map(|failure| failure.error.as_str())
    }

    /// Retry the same working state. Success returns it and clears recovery; another failure
    /// retains both snapshots and replaces the visible error.
    pub fn retry<E: std::fmt::Display>(
        &mut self,
        persist: impl FnOnce(&mut T) -> Result<(), E>,
    ) -> Option<T> {
        let SaveFailure {
            baseline,
            mut working,
            error: _,
        } = self.failed.take()?;
        match persist(&mut working) {
            Ok(()) => Some(working),
            Err(next_error) => {
                self.failed = Some(SaveFailure {
                    baseline,
                    working,
                    error: next_error.to_string(),
                });
                None
            }
        }
    }

    /// Abandon the failed change and return the exact last persisted baseline.
    pub fn cancel(&mut self) -> Option<T> {
        self.failed.take().map(|failure| failure.baseline)
    }
}

#[cfg(test)]
mod tests {
    use super::SaveRecovery;

    #[test]
    fn retry_keeps_snapshots_on_failure_then_promotes_working_on_success() {
        let mut recovery = SaveRecovery::new();
        recovery.fail(vec!["baseline"], vec!["working"], "first failure");

        assert!(recovery.retry(|_| Err("second failure")).is_none());
        assert_eq!(recovery.baseline(), Some(&vec!["baseline"]));
        assert_eq!(recovery.working(), Some(&vec!["working"]));
        assert_eq!(recovery.error(), Some("second failure"));

        assert_eq!(
            recovery.retry(|_| Ok::<(), &str>(())).unwrap(),
            vec!["working"]
        );
        assert!(!recovery.is_pending());
    }
}

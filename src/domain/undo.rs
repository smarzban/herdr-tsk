//! Undo stack for soft-delete and complete.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainError, DomainState};

/// One reversible user action on the undo stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoEntry {
    SoftDelete { id: Uuid, expected_revision: Uuid },
    Complete { id: Uuid, expected_revision: Uuid },
}

impl UndoEntry {
    fn target(&self) -> (Uuid, Uuid) {
        match *self {
            UndoEntry::SoftDelete {
                id,
                expected_revision,
            }
            | UndoEntry::Complete {
                id,
                expected_revision,
            } => (id, expected_revision),
        }
    }
}

impl DomainState {
    /// Apply the inverse of the last undo entry when its target revision still matches.
    ///
    /// Empty stack is a documented no-op. Stale entries are retained so a refused Undo never
    /// changes durable state or exposes an older entry accidentally.
    pub fn undo(&mut self) -> Result<(), DomainError> {
        let Some(entry) = self.last_undo().cloned() else {
            return Ok(());
        };
        let (id, expected_revision) = entry.target();
        let current_revision = self.get(id).ok_or(DomainError::UnknownId(id))?.revision;
        if current_revision != expected_revision {
            return Err(DomainError::StaleUndo(id));
        }

        let entry = self
            .pop_undo()
            .expect("undo entry remains present after revision check");
        match entry {
            UndoEntry::SoftDelete { id, .. } => self.restore(id),
            UndoEntry::Complete { id, .. } => self.reopen(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{HumanStatus, ProvenanceOrigin, TaskScope};

    fn create_sample(state: &mut DomainState) -> Uuid {
        state
            .create(
                "Fix flake",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("valid title creates a task")
    }

    #[test]
    fn soft_delete_then_undo_clears_soft_deleted() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.soft_delete(id).expect("soft_delete");
        assert!(state.get(id).expect("task exists").soft_deleted);

        state.undo().expect("undo after soft_delete");

        let task = state.get(id).expect("task still in store");
        assert!(!task.soft_deleted);
        assert_eq!(task.status, HumanStatus::Ready);
        assert_eq!(task.title, "Fix flake");
    }

    #[test]
    fn complete_then_undo_returns_status_ready() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.complete(id).expect("complete");
        assert_eq!(
            state.get(id).expect("task exists").status,
            HumanStatus::Done
        );

        state.undo().expect("undo after complete");

        assert_eq!(
            state.get(id).expect("task exists").status,
            HumanStatus::Ready
        );
    }

    #[test]
    fn undo_with_empty_stack_is_noop() {
        let mut state = DomainState::new();
        // Documented no-op: empty stack must not panic and must succeed.
        state.undo().expect("empty undo is Ok");
        assert!(state.tasks().is_empty());
    }
}

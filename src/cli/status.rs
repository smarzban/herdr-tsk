//! Headless human-status changes.

use std::path::PathBuf;

use crate::cli::parser::TaskAddress;
use crate::domain::{DomainError, DomainState, HumanStatus};
use crate::store::{default_state_dir, TaskStore};
use uuid::Uuid;

/// A successful status change, including an idempotent repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusResult {
    pub number: u64,
    pub title: String,
    pub status: HumanStatus,
}

/// A status failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusError {
    UnknownTask,
    SoftDeletedTask,
    Store(String),
}

impl StatusError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownTask => "unknown-task",
            Self::SoftDeletedTask => "soft-deleted-task",
            Self::Store(_) => "store-error",
        }
    }
}

/// Set one task's human status. Repeating the same status is idempotent.
///
/// Soft-deleted and unknown tasks refuse the same way as archive.
pub fn run(
    target: TaskAddress,
    status: HumanStatus,
    state_dir: Option<PathBuf>,
) -> Result<StatusResult, StatusError> {
    let store = TaskStore::new(state_dir.unwrap_or_else(default_state_dir));
    store
        .locked_transition_if_changed(|state: &mut DomainState| {
            let found = state
                .tasks()
                .iter()
                .find(|task| target.matches(task))
                .map(|task| Found {
                    id: task.id,
                    number: task.number,
                    title: task.title.clone(),
                    current: task.status,
                    soft_deleted: task.soft_deleted,
                });
            Ok(apply(state, found, status))
        })
        .map_err(StatusError::Store)?
}

struct Found {
    id: Uuid,
    number: Option<u64>,
    title: String,
    current: HumanStatus,
    soft_deleted: bool,
}

fn apply(
    state: &mut DomainState,
    found: Option<Found>,
    status: HumanStatus,
) -> (Result<StatusResult, StatusError>, bool) {
    let Some(found) = found else {
        return (Err(StatusError::UnknownTask), false);
    };
    let Some(number) = found.number else {
        return (Err(StatusError::UnknownTask), false);
    };
    if found.soft_deleted {
        return (Err(StatusError::SoftDeletedTask), false);
    }
    let result = StatusResult {
        number,
        title: found.title,
        status,
    };
    if found.current == status {
        return (Ok(result), false);
    }
    match state.set_status(found.id, status) {
        Ok(()) => (Ok(result), true),
        Err(DomainError::UnknownId(_)) => (Err(StatusError::UnknownTask), false),
        Err(other) => (Err(StatusError::Store(other.to_string())), false),
    }
}

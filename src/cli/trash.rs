//! Headless trash restore.

use std::path::PathBuf;

use crate::cli::parser::TaskAddress;
use crate::store::{default_state_dir, TaskStore, TrashError, TrashTarget};

/// A successful trash restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashRestoreResult {
    pub number: u64,
    pub title: String,
}

/// A trash failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashCliError {
    /// Well-formed address with no matching trash line, or the task is live.
    NotInTrash(String),
    Store(String),
}

/// Restore one task from `trash.jsonl` into the live store.
///
/// Refusals are resolved under the store lock, so a concurrent restore or live
/// task cannot race the decision.
pub fn run_restore(
    target: TaskAddress,
    state_dir: Option<PathBuf>,
) -> Result<TrashRestoreResult, TrashCliError> {
    let store = TaskStore::new(state_dir.unwrap_or_else(default_state_dir));
    let (target, display) = match target {
        TaskAddress::Number(number) => (TrashTarget::Number(number), format!("T{number}")),
        TaskAddress::Id(id) => (TrashTarget::Id(id), id.to_string()),
    };
    match store.restore_from_trash(target) {
        Ok(line) => Ok(TrashRestoreResult {
            number: line
                .task
                .number
                .expect("trashed tasks were numbered before removal"),
            title: line.task.title,
        }),
        Err(TrashError::NotInTrash) => Err(TrashCliError::NotInTrash(format!(
            "{display} is not in trash"
        ))),
        Err(TrashError::Store(error)) => Err(TrashCliError::Store(error.to_string())),
    }
}

//! Headless task archive / unarchive.

use std::path::PathBuf;

use crate::cli::parser::TaskAddress;
use crate::domain::{DomainError, DomainState};
use crate::store::{default_state_dir, TaskStore};

/// A successful archive/unarchive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResult {
    pub number: u64,
    pub title: String,
    pub archived: bool,
}

/// An archive failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveCliError {
    /// Well-formed address with no matching task.
    UnknownTask(String),
    /// The addressed task is soft-deleted.
    SoftDeleted(String),
    Store(String),
}

fn display_for(target: &TaskAddress) -> String {
    match target {
        TaskAddress::Number(number) => format!("T{number}"),
        TaskAddress::Id(id) => id.to_string(),
    }
}

/// Archive or unarchive one task by address.
///
/// The decision is resolved under the store lock; the domain verb's own bool is the
/// `changed` flag, so repeating an archive on an already-archived task reports the same
/// output and writes nothing.
pub fn run_task(
    target: TaskAddress,
    archive: bool,
    state_dir: Option<PathBuf>,
) -> Result<ArchiveResult, ArchiveCliError> {
    let store = TaskStore::new(state_dir.unwrap_or_else(default_state_dir));
    let display = display_for(&target);
    store
        .locked_transition_if_changed(|state: &mut DomainState| {
            let found = state
                .tasks()
                .iter()
                .find(|task| target.matches(task))
                .map(|task| (task.id, task.number, task.title.clone(), task.soft_deleted));
            let (outcome, changed) = match found {
                None => (
                    Err(ArchiveCliError::UnknownTask(format!(
                        "{display} is not on the board"
                    ))),
                    false,
                ),
                Some((id, number, title, soft_deleted)) => {
                    if soft_deleted {
                        (
                            Err(ArchiveCliError::SoftDeleted(format!(
                                "{display} is deleted"
                            ))),
                            false,
                        )
                    } else {
                        let Some(number) = number else {
                            return Ok((
                                Err(ArchiveCliError::UnknownTask(format!(
                                    "{display} is not on the board"
                                ))),
                                false,
                            ));
                        };
                        let result = if archive {
                            state.archive_task(id)
                        } else {
                            state.unarchive_task(id)
                        };
                        match result {
                            Ok(changed) => (
                                Ok(ArchiveResult {
                                    number,
                                    title,
                                    archived: archive,
                                }),
                                changed,
                            ),
                            Err(DomainError::SoftDeleted(_)) => (
                                Err(ArchiveCliError::SoftDeleted(format!(
                                    "{display} is deleted"
                                ))),
                                false,
                            ),
                            Err(_) => (
                                Err(ArchiveCliError::UnknownTask(format!(
                                    "{display} is not on the board"
                                ))),
                                false,
                            ),
                        }
                    }
                }
            };
            Ok((outcome, changed))
        })
        .map_err(ArchiveCliError::Store)?
}

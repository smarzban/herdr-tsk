//! Read-only headless task listing.

use std::path::PathBuf;

use serde::Serialize;
use uuid::Uuid;

use crate::domain::{HumanStatus, TaskScope};
use crate::store::{default_state_dir, TaskStore};

/// Parsed `list` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListInput {
    pub json: bool,
    pub state_dir: Option<PathBuf>,
}

/// A list failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListError {
    Store(String),
}

/// One task visible to the list command.
#[derive(Debug, Serialize)]
pub(crate) struct ListRow {
    pub(crate) id: Uuid,
    pub(crate) title: String,
    pub(crate) status: HumanStatus,
    pub(crate) project: Option<String>,
}

/// Read-only result for the list command.
#[derive(Debug, Serialize)]
pub struct ListResult {
    pub(crate) rows: Vec<ListRow>,
}

/// Parse `herdr-tasks list` arguments, including argv0 and the `list` subcommand.
pub fn parse(args: &[String]) -> Result<ListInput, String> {
    if args.get(1).map(String::as_str) != Some("list") {
        return Err("expected list command".into());
    }

    let mut input = ListInput {
        json: false,
        state_dir: None,
    };
    let mut index = 2;
    while let Some(flag) = args.get(index).map(String::as_str) {
        match flag {
            "--json" => {
                input.json = true;
                index += 1;
            }
            "--state-dir" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --state-dir".to_string())?;
                input.state_dir = Some(PathBuf::from(value));
                index += 2;
            }
            _ => return Err(format!("unknown list argument {flag}")),
        }
    }
    Ok(input)
}

/// Load every task that is not soft-deleted, including done tasks.
pub fn run(input: ListInput) -> Result<ListResult, ListError> {
    let store = TaskStore::new(input.state_dir.unwrap_or_else(default_state_dir));
    let domain = store
        .load()
        .map_err(|error| ListError::Store(error.to_string()))?;
    let rows = domain
        .tasks()
        .iter()
        .filter(|task| !task.soft_deleted)
        .map(|task| ListRow {
            id: task.id,
            title: task.title.clone(),
            status: task.status,
            project: match &task.scope {
                TaskScope::Global => None,
                TaskScope::Project { path } => Some(path.clone()),
            },
        })
        .collect();
    Ok(ListResult { rows })
}

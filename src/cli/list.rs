//! Read-only headless task listing.

use std::path::PathBuf;

use serde::Serialize;
use uuid::Uuid;

use crate::context::snapshot_from_env;
use crate::domain::{HumanStatus, TaskScope};
use crate::scope::resolve_flag_scope;
use crate::store::{default_state_dir, TaskStore};

/// Parsed `list` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListInput {
    pub json: bool,
    pub project: Option<String>,
    pub global: bool,
    pub all: bool,
    pub done: bool,
    pub deleted: bool,
    /// One task addressed by id: single-task listing with checklist lines.
    pub task: Option<Uuid>,
    pub state_dir: Option<PathBuf>,
    pub help: bool,
}

/// The mutually exclusive task set requested by `list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListView {
    Open,
    Done,
    Deleted,
}

/// A list failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListError {
    Store(String),
    /// A well-formed task id that addresses no task in the store.
    UnknownTask(Uuid),
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
#[derive(Debug)]
pub struct ListResult {
    pub(crate) rows: Vec<ListRow>,
    pub(crate) view: ListView,
    pub(crate) include_scope: bool,
    /// Checklist lines for single-task listing; empty for every other listing.
    pub(crate) checklist: Vec<crate::cli::check::ChecklistLine>,
}

/// Parse `herdr-tasks list` arguments, including argv0 and the `list` subcommand.
pub fn parse(args: &[String]) -> Result<ListInput, String> {
    if args.get(1).map(String::as_str) != Some("list") {
        return Err("expected list command".into());
    }

    let mut input = ListInput {
        json: false,
        project: None,
        global: false,
        all: false,
        done: false,
        deleted: false,
        task: None,
        state_dir: None,
        help: false,
    };
    let mut index = 2;
    while let Some(flag) = args.get(index).map(String::as_str) {
        let value = |name: &str| match args.get(index + 1) {
            Some(value) if !value.starts_with('-') => Ok(value.clone()),
            _ => Err(format!("missing value for {name}")),
        };
        match flag {
            flag if flag.starts_with("--project=") => {
                input.project = Some(flag["--project=".len()..].to_owned());
                index += 1;
            }
            "--json" => {
                input.json = true;
                index += 1;
            }
            "-p" | "--project" => {
                input.project = Some(value(flag)?);
                index += 2;
            }
            "--global" => {
                input.global = true;
                index += 1;
            }
            "--all" => {
                input.all = true;
                index += 1;
            }
            "--done" => {
                input.done = true;
                index += 1;
            }
            "--deleted" => {
                input.deleted = true;
                index += 1;
            }
            "--help" => {
                input.help = true;
                index += 1;
            }
            flag if flag.starts_with("--state-dir=") => {
                input.state_dir = Some(PathBuf::from(flag["--state-dir=".len()..].to_owned()));
                index += 1;
            }
            "--state-dir" => {
                input.state_dir = Some(PathBuf::from(value(flag)?));
                index += 2;
            }
            flag if !flag.starts_with('-') => {
                if input.task.is_some() {
                    return Err(format!("unknown list argument {flag}"));
                }
                input.task =
                    Some(Uuid::parse_str(flag).map_err(|_| format!("invalid task id {flag}"))?);
                index += 1;
            }
            _ => return Err(format!("unknown list argument {flag}")),
        }
    }

    if input.task.is_some() && (input.all || input.global || input.project.is_some()) {
        return Err("task id cannot be used with --project, --global, or --all".into());
    }
    if input.task.is_some() && (input.done || input.deleted) {
        return Err("task id cannot be used with --done or --deleted".into());
    }
    if input.all && (input.global || input.project.is_some()) {
        return Err("--all cannot be used with --project or --global".into());
    }
    if input.global && input.project.is_some() {
        return Err("--global cannot be used with --project".into());
    }
    if input.done && input.deleted {
        return Err("--done cannot be used with --deleted".into());
    }
    Ok(input)
}

/// Load tasks in the requested scope and view, ordered by their displayed status groups.
pub fn run(input: ListInput) -> Result<ListResult, ListError> {
    let store = TaskStore::new(input.state_dir.unwrap_or_else(default_state_dir));
    let domain = store
        .load()
        .map_err(|error| ListError::Store(error.to_string()))?;
    if let Some(task_id) = input.task {
        // Single-task listing: id addressing, no scope or filter, checklist lines included.
        let task = domain
            .tasks()
            .iter()
            .find(|task| task.id == task_id)
            .ok_or(ListError::UnknownTask(task_id))?;
        let view = if task.soft_deleted {
            ListView::Deleted
        } else if task.status == HumanStatus::Done {
            ListView::Done
        } else {
            ListView::Open
        };
        return Ok(ListResult {
            rows: vec![row_for(task)],
            view,
            include_scope: false,
            checklist: crate::cli::check::checklist_lines(&task.checklist),
        });
    }
    let scope = (!input.all).then(|| {
        resolve_flag_scope(
            input.project.as_deref(),
            input.global,
            &domain,
            &snapshot_from_env(),
        )
    });
    let view = if input.deleted {
        ListView::Deleted
    } else if input.done {
        ListView::Done
    } else {
        ListView::Open
    };
    let mut rows = domain
        .tasks()
        .iter()
        .filter(|task| scope.as_ref().is_none_or(|scope| task.scope == *scope))
        .filter(|task| match view {
            ListView::Open => !task.soft_deleted && is_open(task.status),
            ListView::Done => !task.soft_deleted && task.status == HumanStatus::Done,
            ListView::Deleted => task.soft_deleted,
        })
        .map(row_for)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| status_group_rank(row.status));
    Ok(ListResult {
        rows,
        view,
        include_scope: input.all,
        checklist: Vec::new(),
    })
}

fn row_for(task: &crate::domain::Task) -> ListRow {
    ListRow {
        id: task.id,
        title: task.title.clone(),
        status: task.status,
        project: match &task.scope {
            TaskScope::Global => None,
            TaskScope::Project { path } => Some(path.clone()),
        },
    }
}

fn is_open(status: HumanStatus) -> bool {
    matches!(
        status,
        HumanStatus::Ready | HumanStatus::Started | HumanStatus::Blocked | HumanStatus::Review
    )
}

fn status_group_rank(status: HumanStatus) -> u8 {
    match status {
        HumanStatus::Started => 0,
        HumanStatus::Ready => 1,
        HumanStatus::Blocked => 2,
        HumanStatus::Review => 3,
        HumanStatus::Done => 4,
    }
}

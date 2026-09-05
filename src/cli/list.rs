//! Read-only headless task listing.

use std::path::PathBuf;

use serde::Serialize;
use uuid::Uuid;

use crate::cli::parser::{parse_task_address, TaskAddress};
use crate::context::snapshot_from_env;
use crate::domain::{normalize_thread, HumanStatus, TaskScope};
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
    pub archived: bool,
    /// Normalized at the argv boundary so filtering only compares valid names.
    pub thread: Option<String>,
    /// One task addressed by UUID or human number: single-task listing with step lines.
    pub task: Option<TaskAddress>,
    pub state_dir: Option<PathBuf>,
    pub help: bool,
}

/// The mutually exclusive task set requested by `list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListView {
    Open,
    Done,
    Deleted,
    Archived,
}

/// A list failure after parsing and before presenting output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListError {
    Store(String),
    /// A well-formed task address that addresses no task in the store.
    UnknownTask,
}

/// One task visible to the list command.
#[derive(Debug, Serialize)]
pub(crate) struct ListRow {
    pub(crate) id: Uuid,
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) status: HumanStatus,
    pub(crate) project: Option<String>,
    pub(crate) thread: Option<String>,
    /// `archived` / `project archived` mark, set only in the archived view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) archived: Option<&'static str>,
}

/// Read-only result for the list command.
#[derive(Debug)]
pub struct ListResult {
    pub(crate) rows: Vec<ListRow>,
    pub(crate) view: ListView,
    pub(crate) include_scope: bool,
    /// Step lines for single-task listing; empty for every other listing.
    pub(crate) steps: Vec<crate::cli::steps::StepLine>,
}

/// Parse `tsk list` arguments, including argv0 and the `list` subcommand.
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
        archived: false,
        thread: None,
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
            flag if flag.starts_with("--thread=") => {
                input.thread = Some(
                    normalize_thread(&flag["--thread=".len()..])
                        .map_err(|_| "invalid thread name".to_owned())?,
                );
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
            "--thread" => {
                input.thread = Some(
                    normalize_thread(&value(flag)?)
                        .map_err(|_| "invalid thread name".to_owned())?,
                );
                index += 2;
            }
            "--desk" => {
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
            "--archived" => {
                input.archived = true;
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
                input.task = Some(parse_task_address(flag)?);
                index += 1;
            }
            _ => return Err(format!("unknown list argument {flag}")),
        }
    }

    if input.task.is_some()
        && (input.all || input.global || input.project.is_some() || input.thread.is_some())
    {
        return Err(
            "task operand cannot be used with --project, --desk, --all, or --thread".into(),
        );
    }
    if input.task.is_some() && (input.done || input.deleted) {
        return Err("task operand cannot be used with --done or --deleted".into());
    }
    if input.all && (input.global || input.project.is_some()) {
        return Err("--all cannot be used with --project or --desk".into());
    }
    if input.global && input.project.is_some() {
        return Err("--desk cannot be used with --project".into());
    }
    if input.task.is_some() && (input.done || input.deleted || input.archived) {
        return Err("task operand cannot be used with --done, --deleted, or --archived".into());
    }
    if input.done && input.deleted {
        return Err("--done cannot be used with --deleted".into());
    }
    if input.archived && (input.done || input.deleted) {
        return Err("--archived cannot be used with --done or --deleted".into());
    }
    Ok(input)
}

/// Load tasks in the requested scope and view, ordered by their displayed status groups.
pub fn run(input: ListInput) -> Result<ListResult, ListError> {
    let store = TaskStore::new(input.state_dir.unwrap_or_else(default_state_dir));
    let domain = store
        .load()
        .map_err(|error| ListError::Store(error.to_string()))?;
    if let Some(task_address) = input.task {
        // Single-task listing ignores cwd and filters, and includes step lines.
        let task = domain
            .tasks()
            .iter()
            .find(|task| task_address.matches(task))
            .ok_or(ListError::UnknownTask)?;
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
            steps: crate::cli::steps::step_lines(&task.steps),
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
    } else if input.archived {
        ListView::Archived
    } else {
        ListView::Open
    };
    if view == ListView::Deleted {
        return deleted_rows(&store, &domain, &scope, input.thread.as_deref(), input.all);
    }
    let mut rows = domain
        .tasks()
        .iter()
        .filter(|task| scope.as_ref().is_none_or(|scope| task.scope == *scope))
        .filter(|task| {
            input
                .thread
                .as_deref()
                .is_none_or(|thread| task.thread.as_deref() == Some(thread))
        })
        .filter(|task| match view {
            ListView::Open => !task.soft_deleted && !domain.is_hidden(task) && is_open(task.status),
            ListView::Done => {
                !task.soft_deleted && !domain.is_hidden(task) && task.status == HumanStatus::Done
            }
            ListView::Deleted => task.soft_deleted,
            ListView::Archived => {
                !task.soft_deleted
                    && (task.archived
                        || domain.is_project_archived(match &task.scope {
                            TaskScope::Project { path } => path,
                            TaskScope::Global => "",
                        }))
            }
        })
        .map(|task| {
            let mut row = row_for(task);
            if view == ListView::Archived {
                // "archived" wins over "project archived".
                row.archived = Some(if task.archived {
                    "archived"
                } else {
                    "project archived"
                });
            }
            row
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| status_group_rank(row.status));
    Ok(ListResult {
        rows,
        view,
        include_scope: input.all,
        steps: Vec::new(),
    })
}

/// `--deleted` listing: live soft-deleted tasks plus trash entries, deduped by id
/// with the live copy winning, ordered by `deleted_at` descending. Trash entries
/// render exactly like live rows and keep their `T<n>` number.
fn deleted_rows(
    store: &TaskStore,
    domain: &crate::domain::DomainState,
    scope: &Option<TaskScope>,
    thread: Option<&str>,
    include_scope: bool,
) -> Result<ListResult, ListError> {
    let trash = store
        .load_trash()
        .map_err(|error| ListError::Store(error.to_string()))?;
    let in_scope = |task: &crate::domain::Task| {
        scope.as_ref().is_none_or(|scope| task.scope == *scope)
            && thread.is_none_or(|thread| task.thread.as_deref() == Some(thread))
    };
    let mut dated: Vec<(std::time::SystemTime, ListRow)> = domain
        .tasks()
        .iter()
        .filter(|task| task.soft_deleted && in_scope(task))
        .map(|task| {
            (
                task.soft_deleted_at().unwrap_or(task.updated_at),
                row_for(task),
            )
        })
        .collect();
    let live_ids: std::collections::BTreeSet<Uuid> =
        domain.tasks().iter().map(|task| task.id).collect();
    for line in trash {
        // A task in both places is listed once, from the live copy.
        if live_ids.contains(&line.task.id) || !in_scope(&line.task) {
            continue;
        }
        dated.push((line.deleted_at, row_for(&line.task)));
    }
    dated.sort_by(|(left, _), (right, _)| right.cmp(left));
    Ok(ListResult {
        rows: dated.into_iter().map(|(_, row)| row).collect(),
        view: ListView::Deleted,
        include_scope,
        steps: Vec::new(),
    })
}

fn row_for(task: &crate::domain::Task) -> ListRow {
    ListRow {
        id: task.id,
        number: task
            .number
            .expect("loaded tasks receive a number before CLI presentation"),
        title: task.title.clone(),
        status: task.status,
        project: match &task.scope {
            TaskScope::Global => None,
            TaskScope::Project { path } => Some(path.clone()),
        },
        thread: task.thread.clone(),
        archived: None,
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

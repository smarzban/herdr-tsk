//! Headless task creation for flag and JSON-plan input.

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::cli::parser::FlagAdd;
use crate::context::snapshot_from_env;
use crate::domain::{
    normalize_thread, thread_refusal_message, DomainState, ProvenanceOrigin, TaskScope,
};
use crate::scope::{resolve_flag_scope, resolve_project_path};
use crate::store::{default_state_dir, TaskStore};

/// A flag-add failure after parsing and before presenting an output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    EmptyTitle,
    InvalidTitle,
    /// The resolved scope is an archived project (cwd default or explicit `-p`).
    ProjectArchived(String),
    Store(String),
}

impl AddError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyTitle => "empty-title",
            Self::InvalidTitle => "invalid-title",
            Self::ProjectArchived(_) => "project-archived",
            Self::Store(_) => "store-error",
        }
    }
}

/// A machine-readable result from a JSON plan.
#[derive(Debug, Serialize)]
pub struct PlanResult {
    created: Vec<Created>,
    existing: Vec<Existing>,
    failed: Vec<Failed>,
}

impl PlanResult {
    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }
}

#[derive(Debug, Serialize)]
struct Created {
    i: usize,
    id: Uuid,
    number: u64,
    title: String,
}

/// An accepted item that already has a matching non-soft-deleted task.
#[derive(Debug, Serialize)]
struct Existing {
    i: usize,
    id: Uuid,
    number: u64,
    title: String,
}

#[derive(Debug, Serialize)]
struct Failed {
    i: usize,
    title: Option<String>,
    code: &'static str,
    error: String,
}

struct PlanItem {
    i: usize,
    title: String,
    notes: Option<String>,
    /// `None` is omitted, `Some(None)` is JSON null/global.
    project: Option<Option<String>>,
    /// Missing and JSON null both leave the item unthreaded.
    thread: Option<String>,
}

struct ResolvedPlanItem {
    i: usize,
    title: String,
    notes: Option<String>,
    scope: TaskScope,
    thread: Option<String>,
}

/// Result of one accepted flag add.
#[derive(Debug)]
pub enum FlagAddResult {
    Created {
        id: Uuid,
        number: u64,
        title: String,
        project: Option<String>,
    },
    Existing {
        id: Uuid,
        number: u64,
        title: String,
        project: Option<String>,
    },
}

/// Create one headless task, or report a non-soft-deleted match without changing it.
pub fn run(input: FlagAdd) -> Result<FlagAddResult, AddError> {
    let title = input.title.ok_or(AddError::EmptyTitle)?;
    // AC-21 rejects any C0 control, including one that end trimming would remove.
    if has_c0_control(&title) {
        return Err(AddError::InvalidTitle);
    }
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(AddError::EmptyTitle);
    }

    let store = TaskStore::new(input.state_dir.unwrap_or_else(default_state_dir));
    let snapshot = snapshot_from_env();
    let project = input.project;
    let global = input.global;
    let notes = input.notes.filter(|notes| !notes.trim().is_empty());
    let thread = input.thread;
    store
        .locked_transition_if_changed(|domain| {
            let scope = resolve_flag_scope(project.as_deref(), global, domain, &snapshot);
            if let TaskScope::Project { path } = &scope {
                if domain.is_project_archived(path) {
                    let short = crate::ui::render::short_project(path).to_string();
                    return Ok((Err(AddError::ProjectArchived(short)), false));
                }
            }
            let outcome = if let Some(task) =
                existing_task(domain, &title, &scope, thread.as_deref())
            {
                Ok(FlagAddResult::Existing {
                    id: task.id,
                    number: task
                        .number
                        .expect("loaded tasks receive a number before CLI presentation"),
                    title: task.title.clone(),
                    project: scope_project(&task.scope),
                })
            } else {
                let project = scope_project(&scope);
                let id =
                    match domain.create(&title, notes, scope, ProvenanceOrigin::Capture, thread) {
                        Ok(id) => id,
                        Err(error) => {
                            return Ok((Err(AddError::Store(error.to_string())), false));
                        }
                    };
                domain.assign_numbers_for_persistence();
                let number = domain
                    .get(id)
                    .and_then(|task| task.number)
                    .expect("new tasks receive a number while the store lock is held");
                Ok(FlagAddResult::Created {
                    id,
                    number,
                    title,
                    project,
                })
            };
            let changed = matches!(outcome, Ok(FlagAddResult::Created { .. }));
            Ok((outcome, changed))
        })
        .map_err(AddError::Store)
        .and_then(|outcome| outcome)
}

/// Create every valid item from a parsed JSON plan in one durable write.
pub fn run_plan(
    values: Vec<Value>,
    state_dir: Option<std::path::PathBuf>,
) -> Result<PlanResult, AddError> {
    let mut valid = Vec::new();
    let mut failed = Vec::new();
    for (i, value) in values.into_iter().enumerate() {
        match parse_plan_item(i, value) {
            Ok(item) => valid.push(item),
            Err(item_failure) => failed.push(item_failure),
        }
    }

    if valid.is_empty() {
        return Ok(PlanResult {
            created: Vec::new(),
            existing: Vec::new(),
            failed,
        });
    }

    let store = TaskStore::new(state_dir.unwrap_or_else(default_state_dir));
    let snapshot = snapshot_from_env();
    store
        .locked_transition_if_changed(|domain| {
            let resolved = resolve_plan_items(valid, domain, &snapshot);
            let mut created = Vec::with_capacity(resolved.len());
            let mut existing = Vec::new();
            for item in resolved {
                if let TaskScope::Project { path } = &item.scope {
                    if domain.is_project_archived(path) {
                        failed.push(fail_item(
                            item.i,
                            Some(item.title),
                            "project-archived",
                            "project is archived: use --desk, -p, or tsk project unarchive",
                        ));
                        continue;
                    }
                }
                if let Some(task) =
                    existing_task(domain, &item.title, &item.scope, item.thread.as_deref())
                {
                    existing.push(Existing {
                        i: item.i,
                        id: task.id,
                        number: task
                            .number
                            .expect("loaded tasks receive a number before CLI presentation"),
                        title: item.title,
                    });
                    continue;
                }
                let id = domain
                    .create(
                        &item.title,
                        item.notes,
                        item.scope,
                        ProvenanceOrigin::Capture,
                        item.thread,
                    )
                    .expect("plan item titles and threads are validated before domain creation");
                domain.assign_numbers_for_persistence();
                let number = domain
                    .get(id)
                    .and_then(|task| task.number)
                    .expect("new tasks receive a number while the store lock is held");
                created.push(Created {
                    i: item.i,
                    id,
                    number,
                    title: item.title,
                });
            }
            let changed = !created.is_empty();
            // Archived-project refusals join the parse failures inside the transaction, so
            // restore item order before reporting.
            failed.sort_by_key(|failure| failure.i);
            Ok((
                PlanResult {
                    created,
                    existing,
                    failed,
                },
                changed,
            ))
        })
        .map_err(AddError::Store)
}

fn resolve_plan_items(
    items: Vec<PlanItem>,
    domain: &DomainState,
    snapshot: &crate::context::InvocationSnapshot,
) -> Vec<ResolvedPlanItem> {
    items
        .into_iter()
        .map(|item| ResolvedPlanItem {
            i: item.i,
            title: item.title,
            notes: item.notes,
            scope: match item.project {
                Some(None) => TaskScope::Global,
                Some(Some(project)) => TaskScope::Project {
                    path: resolve_project_path(&project, domain, Some(snapshot)),
                },
                None => snapshot.default_scope.clone(),
            },
            thread: item.thread,
        })
        .collect()
}

fn scope_project(scope: &TaskScope) -> Option<String> {
    match scope {
        TaskScope::Global => None,
        TaskScope::Project { path } => Some(path.clone()),
    }
}

fn existing_task<'a>(
    domain: &'a DomainState,
    title: &str,
    scope: &TaskScope,
    thread: Option<&str>,
) -> Option<&'a crate::domain::Task> {
    // Notice rows are board-only (no CLI address reaches them, not even their UUID) and
    // deliberately carry no T number, so a title collision with a seeded guide or the
    // announcement must never resolve as existing: the add creates an ordinary task.
    domain.tasks().iter().find(|task| {
        !task.soft_deleted
            && !task.is_notice()
            && task.title == title
            && &task.scope == scope
            && task.thread.as_deref() == thread
    })
}

fn parse_plan_item(i: usize, value: Value) -> Result<PlanItem, Failed> {
    let Some(object) = value.as_object() else {
        return Err(fail_item(i, None, "invalid-item", "item must be an object"));
    };
    let title = match object.get("title") {
        None => return Err(fail_item(i, None, "empty-title", "title is required")),
        Some(Value::String(title)) => title,
        Some(_) => return Err(fail_item(i, None, "invalid-item", "title must be a string")),
    };
    let trimmed_title = title.trim().to_string();
    if has_c0_control(title) {
        return Err(fail_item(
            i,
            Some(trimmed_title),
            "invalid-title",
            "title contains a control character",
        ));
    }
    if trimmed_title.is_empty() {
        return Err(fail_item(
            i,
            Some(trimmed_title),
            "empty-title",
            "title is empty",
        ));
    }

    let notes = match object.get("notes") {
        None | Some(Value::Null) => None,
        Some(Value::String(notes)) if notes.trim().is_empty() => None,
        Some(Value::String(notes)) => Some(notes.clone()),
        Some(_) => {
            return Err(fail_item(
                i,
                Some(trimmed_title),
                "invalid-item",
                "notes must be a string or null",
            ));
        }
    };
    let project = match object.get("project") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(project)) => Some(Some(project.clone())),
        Some(_) => {
            return Err(fail_item(
                i,
                Some(trimmed_title),
                "invalid-item",
                "project must be a string or null",
            ));
        }
    };
    let thread = match object.get("thread") {
        None | Some(Value::Null) => None,
        Some(Value::String(thread)) => normalize_thread(thread).map(Some).map_err(|error| {
            fail_item(
                i,
                Some(trimmed_title.clone()),
                "invalid-thread",
                thread_refusal_message(error),
            )
        })?,
        Some(_) => {
            return Err(fail_item(
                i,
                Some(trimmed_title),
                "invalid-thread",
                "thread must be a string or null",
            ));
        }
    };

    Ok(PlanItem {
        i,
        title: trimmed_title,
        notes,
        project,
        thread,
    })
}

fn fail_item(
    i: usize,
    title: Option<String>,
    code: &'static str,
    error: impl Into<String>,
) -> Failed {
    Failed {
        i,
        title,
        code,
        error: error.into(),
    }
}

pub(crate) fn has_c0_control(value: &str) -> bool {
    value.chars().any(|character| character <= '\u{001f}')
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::cli::parser::FlagAdd;

    #[test]
    fn title_controls_are_rejected_before_end_trimming() {
        let input = FlagAdd {
            title: Some("hello\n".into()),
            notes: None,
            project: None,
            thread: None,
            global: false,
            json: false,
            state_dir: None,
            file: None,
            has_item_flags: true,
            help: false,
        };
        assert_eq!(run(input).unwrap_err().code(), "invalid-title");
    }
}

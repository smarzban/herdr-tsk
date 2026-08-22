//! Headless task creation for flag and JSON-plan input.

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::cli::parser::FlagAdd;
use crate::context::snapshot_from_env;
use crate::domain::{ProvenanceOrigin, TaskScope};
use crate::scope::resolve_project_path;
use crate::store::{default_state_dir, TaskStore};

/// A flag-add failure after parsing and before presenting an output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    EmptyTitle,
    InvalidTitle,
    Store(String),
}

impl AddError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyTitle => "empty-title",
            Self::InvalidTitle => "invalid-title",
            Self::Store(_) => "store-error",
        }
    }
}

/// A machine-readable result from a JSON plan.
#[derive(Debug, Serialize)]
pub struct PlanResult {
    created: Vec<Created>,
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
    title: String,
}

#[derive(Debug, Serialize)]
struct Failed {
    i: usize,
    title: Option<String>,
    code: &'static str,
    error: &'static str,
}

struct PlanItem {
    i: usize,
    title: String,
    notes: Option<String>,
    /// `None` is omitted, `Some(None)` is JSON null/global.
    project: Option<Option<String>>,
}

struct ResolvedPlanItem {
    i: usize,
    title: String,
    notes: Option<String>,
    scope: TaskScope,
}

/// Create one headless task and return its normalized title.
pub fn run(input: FlagAdd) -> Result<String, AddError> {
    let title = input.title.ok_or(AddError::EmptyTitle)?;
    // AC-21 rejects any C0 control, including one that end trimming would remove.
    if has_c0_control(&title) {
        return Err(AddError::InvalidTitle);
    }
    let title = title.trim();
    if title.is_empty() {
        return Err(AddError::EmptyTitle);
    }

    let store = TaskStore::new(input.state_dir.unwrap_or_else(default_state_dir));
    let mut domain = store
        .load()
        .map_err(|error| AddError::Store(error.to_string()))?;
    let snapshot = snapshot_from_env();
    let scope = match input.project {
        Some(project) => TaskScope::Project {
            path: resolve_project_path(&project, &domain, Some(&snapshot)),
        },
        None if input.global => TaskScope::Global,
        None => snapshot.default_scope,
    };
    let notes = input.notes.filter(|notes| !notes.trim().is_empty());

    domain
        .create(title, notes, scope, None, None, ProvenanceOrigin::Capture)
        .map_err(|error| AddError::Store(error.to_string()))?;
    store
        .reload_merge_save(&mut domain)
        .map_err(|error| AddError::Store(error.to_string()))?;
    Ok(title.into())
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
            failed,
        });
    }

    let store = TaskStore::new(state_dir.unwrap_or_else(default_state_dir));
    let mut domain = store
        .load()
        .map_err(|error| AddError::Store(error.to_string()))?;
    let snapshot = snapshot_from_env();
    let resolved = valid
        .into_iter()
        .map(|item| ResolvedPlanItem {
            i: item.i,
            title: item.title,
            notes: item.notes,
            scope: match item.project {
                Some(None) => TaskScope::Global,
                Some(Some(project)) => TaskScope::Project {
                    path: resolve_project_path(&project, &domain, Some(&snapshot)),
                },
                None => snapshot.default_scope.clone(),
            },
        })
        .collect::<Vec<_>>();

    let mut created = Vec::with_capacity(resolved.len());
    for item in resolved {
        let id = domain
            .create(
                &item.title,
                item.notes,
                item.scope,
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .expect("plan item titles are validated before domain creation");
        created.push(Created {
            i: item.i,
            id,
            title: item.title,
        });
    }
    store
        .reload_merge_save(&mut domain)
        .map_err(|error| AddError::Store(error.to_string()))?;

    Ok(PlanResult { created, failed })
}

fn parse_plan_item(i: usize, value: Value) -> Result<PlanItem, Failed> {
    let Some(object) = value.as_object() else {
        return Err(failed(i, None, "invalid-item", "item must be an object"));
    };
    let title = match object.get("title") {
        None => return Err(failed(i, None, "empty-title", "title is required")),
        Some(Value::String(title)) => title,
        Some(_) => return Err(failed(i, None, "invalid-item", "title must be a string")),
    };
    let trimmed_title = title.trim().to_string();
    if has_c0_control(title) {
        return Err(failed(
            i,
            Some(trimmed_title),
            "invalid-title",
            "title contains a control character",
        ));
    }
    if trimmed_title.is_empty() {
        return Err(failed(
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
            return Err(failed(
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
            return Err(failed(
                i,
                Some(trimmed_title),
                "invalid-item",
                "project must be a string or null",
            ));
        }
    };

    Ok(PlanItem {
        i,
        title: trimmed_title,
        notes,
        project,
    })
}

fn failed(i: usize, title: Option<String>, code: &'static str, error: &'static str) -> Failed {
    Failed {
        i,
        title,
        code,
        error,
    }
}

fn has_c0_control(value: &str) -> bool {
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
            global: false,
            state_dir: None,
            file: None,
            has_item_flags: true,
        };
        assert_eq!(run(input).unwrap_err().code(), "invalid-title");
    }
}

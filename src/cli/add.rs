//! Flag-form task creation.

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

/// Create one headless task and return its normalized title.
pub fn run(input: FlagAdd) -> Result<String, AddError> {
    let title = input.title.ok_or(AddError::EmptyTitle)?;
    // AC-21 rejects any C0 control, including one that end trimming would remove.
    if title.chars().any(|character| character <= '\u{001f}') {
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
            has_item_flags: true,
        };
        assert_eq!(run(input).unwrap_err().code(), "invalid-title");
    }
}

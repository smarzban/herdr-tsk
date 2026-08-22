//! Shared project-path resolution for board and headless capture.

use std::collections::BTreeSet;

use crate::context::InvocationSnapshot;
use crate::domain::{DomainState, TaskScope};

/// Resolve command-line project/global scope flags with the same default and basename
/// rules used by headless add.
pub fn resolve_flag_scope(
    project: Option<&str>,
    global: bool,
    domain: &DomainState,
    snapshot: &InvocationSnapshot,
) -> TaskScope {
    match project {
        Some(project) => TaskScope::Project {
            path: resolve_project_path(project, domain, Some(snapshot)),
        },
        None if global => TaskScope::Global,
        None => snapshot.default_scope.clone(),
    }
}

/// Resolve a project token against task and invocation project paths.
///
/// A token containing a slash remains verbatim. Otherwise, a unique ASCII-case-insensitive
/// basename match wins; missing or ambiguous matches remain verbatim.
pub fn resolve_project_path(
    token: &str,
    domain: &DomainState,
    snapshot: Option<&InvocationSnapshot>,
) -> String {
    if token.contains('/') {
        return token.to_string();
    }

    let mut candidates = BTreeSet::new();
    for task in domain.tasks() {
        if let TaskScope::Project { path } = &task.scope {
            candidates.insert(path.clone());
        }
    }
    if let Some(snapshot) = snapshot {
        if let TaskScope::Project { path } = &snapshot.default_scope {
            candidates.insert(path.clone());
        }
        if let Some(this_repo) = snapshot.this_repo.as_deref() {
            candidates.insert(this_repo.to_string_lossy().into_owned());
        }
    }

    let mut matches = candidates.into_iter().filter(|path| {
        path.trim_end_matches('/')
            .rsplit('/')
            .find(|component| !component.is_empty())
            .is_some_and(|basename| basename.eq_ignore_ascii_case(token))
    });
    match (matches.next(), matches.next()) {
        (Some(path), None) => path,
        _ => token.to_string(),
    }
}

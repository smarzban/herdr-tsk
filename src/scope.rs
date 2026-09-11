//! Shared project-path resolution for board and headless capture.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::context::InvocationSnapshot;
use crate::domain::{DomainState, TaskScope};

/// Whether two project-path spellings name the same directory.
///
/// Comparison only: stored scope identity is never rewritten through this. When both
/// sides exist on disk they are compared canonically, so the same repository reached
/// as `/tmp/repo` and `/private/tmp/repo` (macOS) still matches. A missing or
/// nonexistent side falls back to lexical equality, so a stored path whose directory
/// has not been created yet behaves exactly as before.
pub fn paths_equivalent(a: &str, b: &str) -> bool {
    if trim(a) == trim(b) {
        return true;
    }
    match (
        std::fs::canonicalize(Path::new(a)),
        std::fs::canonicalize(Path::new(b)),
    ) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Whether a stored project-path set contains an equivalent spelling.
pub fn archived_path_contains(paths: &BTreeSet<String>, path: &str) -> bool {
    paths.iter().any(|stored| paths_equivalent(stored, path))
}

/// Bounded path identity memo used by one board query. It is intentionally owned by
/// the operation, never global, so aliases are refreshed on the next query and a
/// missing path cannot become permanently stale.
#[derive(Debug, Default)]
pub(crate) struct PathIdentityCache {
    canonical: RefCell<BTreeMap<String, Option<PathBuf>>>,
}

impl PathIdentityCache {
    pub(crate) fn equivalent(&self, a: &str, b: &str) -> bool {
        if trim(a) == trim(b) {
            return true;
        }
        self.canonical_path(a) == self.canonical_path(b) && self.canonical_path(a).is_some()
    }

    fn canonical_path(&self, path: &str) -> Option<PathBuf> {
        let key = trim(path).to_string();
        if let Some(value) = self.canonical.borrow().get(&key) {
            return value.clone();
        }
        let value = std::fs::canonicalize(Path::new(&key)).ok();
        self.canonical.borrow_mut().insert(key, value.clone());
        value
    }

    pub(crate) fn contains(&self, paths: &BTreeSet<String>, path: &str) -> bool {
        paths.iter().any(|stored| self.equivalent(stored, path))
    }
}

fn trim(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() && path.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn path_identity_cache_preserves_root_and_refreshes_per_query() {
        let root = std::env::temp_dir().join(format!(
            "tsk-path-identities-{}-{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let first = root.join("first");
        let second = root.join("second");
        let alias = root.join("alias");
        fs::create_dir_all(&first).expect("first directory");
        fs::create_dir(&second).expect("second directory");
        let root_alias = root.join("root-alias");
        symlink("/", &root_alias).expect("root alias");
        symlink(&first, &alias).expect("first alias");

        let cache = PathIdentityCache::default();
        assert!(cache.equivalent("/", &root_alias.to_string_lossy()));
        assert!(!cache.equivalent(
            &root.join("missing").to_string_lossy(),
            &first.to_string_lossy()
        ));
        assert!(cache.equivalent(&alias.to_string_lossy(), &first.to_string_lossy()));

        fs::remove_file(&alias).expect("remove old alias");
        symlink(&second, &alias).expect("second alias");
        assert!(
            !cache.equivalent(&alias.to_string_lossy(), &second.to_string_lossy()),
            "one query keeps its original identity snapshot"
        );
        assert!(
            PathIdentityCache::default()
                .equivalent(&alias.to_string_lossy(), &second.to_string_lossy()),
            "a fresh query must observe the new alias target"
        );
        let _ = fs::remove_dir_all(root);
    }
}

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
    // Archived projects keep resolvable names even when every task of theirs is
    // hidden, so `!p name` can address them for refusals and unarchive flows.
    for path in domain.projects().keys() {
        candidates.insert(path.clone());
    }
    if let Some(snapshot) = snapshot {
        // An outside-Git candidate fills board slot 2 but is not a basename alias. Adding
        // it here could make an existing stored project ambiguous and persist the bare token.
        // Its exact path remains addressable because slash-containing tokens stay verbatim.
        if let TaskScope::Project { path } = &snapshot.default_scope {
            candidates.insert(path.clone());
            if let Some(this_repo) = snapshot.this_repo.as_deref() {
                candidates.insert(this_repo.to_string_lossy().into_owned());
            }
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

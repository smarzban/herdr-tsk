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

    #[test]
    fn bare_project_names_resolve_once_or_refuse_unknown_and_ambiguous() {
        let mut domain = DomainState::new();
        for path in ["/a/Atlas", "/b/atlas"] {
            domain
                .create(
                    "fixture",
                    None,
                    TaskScope::Project { path: path.into() },
                    crate::domain::ProvenanceOrigin::Manual,
                    None,
                )
                .expect("create project fixture");
        }

        assert_eq!(
            resolve_project_path("missing", &domain, None),
            Err(ProjectResolveError::Unknown)
        );
        assert_eq!(
            resolve_project_path("ATLAS", &domain, None),
            Err(ProjectResolveError::Ambiguous(vec![
                "/a/Atlas".into(),
                "/b/atlas".into()
            ]))
        );
    }

    #[test]
    fn path_tokens_require_an_absolute_existing_directory() {
        let root = std::env::temp_dir().join(format!(
            "tsk-project-resolution-{}-{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("project");
        fs::create_dir_all(&project).expect("project directory");
        let missing = root.join("missing").to_string_lossy().into_owned();
        let domain = DomainState::new();

        assert_eq!(
            resolve_project_path("relative/project", &domain, None),
            Err(ProjectResolveError::NoDirectory("relative/project".into()))
        );
        assert_eq!(
            resolve_project_path(&missing, &domain, None),
            Err(ProjectResolveError::NoDirectory(missing))
        );
        assert_eq!(
            resolve_project_path(&project.to_string_lossy(), &domain, None),
            Ok(project.to_string_lossy().into_owned())
        );
        assert_eq!(
            expand_home_from("~/repo", Some(std::ffi::OsStr::new("/home/example"))),
            "/home/example/repo"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn equivalent_absolute_path_keeps_the_stored_project_spelling() {
        let root = std::env::temp_dir().join(format!(
            "tsk-project-alias-{}-{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("project");
        let alias = root.join("alias");
        fs::create_dir_all(&project).expect("project directory");
        symlink(&project, &alias).expect("project alias");
        let stored = alias.to_string_lossy().into_owned();
        let mut domain = DomainState::new();
        domain
            .create(
                "fixture",
                None,
                TaskScope::Project {
                    path: stored.clone(),
                },
                crate::domain::ProvenanceOrigin::Manual,
                None,
            )
            .expect("create project fixture");

        assert_eq!(
            resolve_project_path(&project.to_string_lossy(), &domain, None),
            Ok(stored)
        );
        let _ = fs::remove_dir_all(root);
    }
}

/// Why a user-supplied project token cannot identify a capture destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResolveError {
    /// A bare basename matches no known project.
    Unknown,
    /// A bare basename matches more than one known project, in sorted path order.
    Ambiguous(Vec<String>),
    /// A path token is relative or does not name an existing directory.
    NoDirectory(String),
}

impl ProjectResolveError {
    /// Human-facing refusal text. CLI callers add the stable `unknown-project` code.
    pub fn message(&self, token: &str) -> String {
        match self {
            Self::Unknown => format!("project {token} is not on the board"),
            Self::Ambiguous(paths) => {
                format!("project {token} is ambiguous: {}", paths.join(", "))
            }
            Self::NoDirectory(path) => format!("no directory at {path}"),
        }
    }
}

/// Resolve command-line project/global scope flags with the same default and basename
/// rules used by headless add.
pub fn resolve_flag_scope(
    project: Option<&str>,
    global: bool,
    domain: &DomainState,
    snapshot: &InvocationSnapshot,
) -> Result<TaskScope, ProjectResolveError> {
    match project {
        Some(project) => resolve_project_path(project, domain, Some(snapshot))
            .map(|path| TaskScope::Project { path }),
        None if global => Ok(TaskScope::Global),
        None => Ok(snapshot.default_scope.clone()),
    }
}

/// Resolve a project token against task, registered-project, and invocation paths.
///
/// Bare tokens must have one ASCII-case-insensitive basename match. Path tokens must be
/// absolute existing directories; `~/…` expands through `HOME`. An equivalent stored path
/// wins over a new spelling so capture never rewrites an existing project identity.
pub fn resolve_project_path(
    token: &str,
    domain: &DomainState,
    snapshot: Option<&InvocationSnapshot>,
) -> Result<String, ProjectResolveError> {
    let candidates = project_candidates(domain, snapshot);
    if token.contains('/') || token == "~" {
        let expanded = expand_home(token);
        let path = Path::new(&expanded);
        if !path.is_absolute() || !path.is_dir() {
            return Err(ProjectResolveError::NoDirectory(expanded));
        }
        return Ok(candidates
            .into_iter()
            .find(|candidate| paths_equivalent(candidate, &expanded))
            .unwrap_or(expanded));
    }

    let matches = candidates
        .into_iter()
        .filter(|path| {
            path.trim_end_matches('/')
                .rsplit('/')
                .find(|component| !component.is_empty())
                .is_some_and(|basename| basename.eq_ignore_ascii_case(token))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(ProjectResolveError::Unknown),
        _ => Err(ProjectResolveError::Ambiguous(matches)),
    }
}

fn project_candidates(
    domain: &DomainState,
    snapshot: Option<&InvocationSnapshot>,
) -> BTreeSet<String> {
    let mut candidates = BTreeSet::new();
    for task in domain.tasks() {
        if let TaskScope::Project { path } = &task.scope {
            candidates.insert(path.clone());
        }
    }
    // Archived projects keep resolvable names even when every task of theirs is hidden.
    candidates.extend(domain.projects().keys().cloned());
    if let Some(snapshot) = snapshot {
        if let TaskScope::Project { path } = &snapshot.default_scope {
            candidates.insert(path.clone());
        }
        if let Some(this_repo) = snapshot.this_repo.as_deref() {
            candidates.insert(this_repo.to_string_lossy().into_owned());
        }
    }
    candidates
}

fn expand_home(token: &str) -> String {
    let home = std::env::var_os("HOME");
    expand_home_from(token, home.as_deref())
}

fn expand_home_from(token: &str, home: Option<&std::ffi::OsStr>) -> String {
    let remainder = if token == "~" {
        Some("")
    } else {
        token.strip_prefix("~/")
    };
    let Some(remainder) = remainder else {
        return token.to_string();
    };
    let Some(home) = home else {
        return token.to_string();
    };
    let mut expanded = PathBuf::from(home);
    if !remainder.is_empty() {
        expanded.push(remainder);
    }
    expanded.to_string_lossy().into_owned()
}

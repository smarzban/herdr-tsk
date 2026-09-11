//! Context Resolver: host context → invocation snapshot.
//!
//! Parses `HERDR_PLUGIN_CONTEXT_JSON`, finds the nearest `.git` root, and keeps the
//! current directory as a project candidate outside Git.

use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::{ProvenanceOrigin, TaskScope};
use crate::text::non_empty;

/// Env var herdr injects with invocation context JSON.
pub const CONTEXT_JSON_ENV: &str = "HERDR_PLUGIN_CONTEXT_JSON";

/// Raw host-provided context. Every field optional; unknown JSON keys ignored.
///
/// Field names align with the subset of herdr's `PluginInvocationContext` used by capture.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct RawHostContext {
    #[serde(default)]
    pub focused_pane_cwd: Option<String>,
    #[serde(default)]
    pub workspace_cwd: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub selected_text: Option<String>,
}

/// Normalized Context Resolver output for one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationSnapshot {
    /// Default capture scope before user override.
    pub default_scope: TaskScope,
    /// Project candidate (Git root or current directory); also used as board slot 2.
    pub this_repo: Option<PathBuf>,
    /// Title pre-fill when selected text is present.
    pub title_prefill: Option<String>,
    /// Provenance label for create: Selection when selected text is used, else Capture.
    pub provenance: ProvenanceOrigin,
}

impl RawHostContext {
    /// Parse raw context from `HERDR_PLUGIN_CONTEXT_JSON`, or empty defaults if missing/malformed.
    pub fn from_env() -> Self {
        let json = env::var(CONTEXT_JSON_ENV).ok();
        Self::from_json(json.as_deref())
    }

    /// Pure parser: missing or invalid JSON yields [`RawHostContext::default`].
    pub fn from_json(json: Option<&str>) -> Self {
        json.and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// Effective cwd: focused pane, then workspace, then explicit cwd (non-empty only).
    pub fn effective_cwd(&self) -> Option<PathBuf> {
        non_empty(self.focused_pane_cwd.as_deref())
            .or_else(|| non_empty(self.workspace_cwd.as_deref()))
            .or_else(|| non_empty(self.cwd.as_deref()))
            .map(PathBuf::from)
    }
}

/// Walk `cwd` and its ancestors for a `.git` entry (file or directory). No git library.
///
/// Returns the directory that contains `.git`, or `None` if none found.
pub fn resolve_repo_root(cwd: &Path) -> Option<PathBuf> {
    if cwd.as_os_str().is_empty() {
        return None;
    }
    let mut current = cwd.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Build an [`InvocationSnapshot`] from raw host context and a process-cwd fallback.
///
/// Outside Git, Desk remains the default while the invocation directory is available as a project.
pub fn build_snapshot(raw: &RawHostContext, fallback_cwd: impl AsRef<Path>) -> InvocationSnapshot {
    let fallback = fallback_cwd.as_ref();
    let cwd = raw
        .effective_cwd()
        .unwrap_or_else(|| fallback.to_path_buf());

    let repo_root = resolve_repo_root(&cwd);
    let this_repo = repo_root
        .clone()
        .or_else(|| (!cwd.as_os_str().is_empty()).then_some(cwd));
    let default_scope = repo_root.map_or(TaskScope::Global, |root| TaskScope::Project {
        path: root.to_string_lossy().into_owned(),
    });

    let selected = non_empty(raw.selected_text.as_deref()).map(str::to_string);
    let (title_prefill, provenance) = match &selected {
        Some(text) => (Some(text.clone()), ProvenanceOrigin::Selection),
        None => (None, ProvenanceOrigin::Capture),
    };

    InvocationSnapshot {
        default_scope,
        this_repo,
        title_prefill,
        provenance,
    }
}

/// Convenience: parse env JSON and build a snapshot with process cwd as fallback.
pub fn snapshot_from_env() -> InvocationSnapshot {
    let raw = RawHostContext::from_env();
    let fallback = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    build_snapshot(&raw, fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("tsk-t8-{label}-{nanos}-{seq}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn git_init(dir: &Path) {
        let status = Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .status()
            .expect("spawn git init");
        assert!(status.success(), "git init failed in {}", dir.display());
        assert!(
            dir.join(".git").exists(),
            ".git missing after git init in {}",
            dir.display()
        );
    }

    #[test]
    fn cwd_inside_temp_git_repo_defaults_to_project_scope() {
        let root = temp_dir("repo");
        let _guard = TempDirGuard(root.clone());
        git_init(&root);

        let nested = root.join("src").join("deep");
        fs::create_dir_all(&nested).expect("nested");

        let raw = RawHostContext {
            cwd: Some(nested.to_string_lossy().into_owned()),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/fallback-unused"));

        assert_eq!(
            snap.default_scope,
            TaskScope::Project {
                path: root.to_string_lossy().into_owned(),
            }
        );
        assert_eq!(snap.this_repo.as_deref(), Some(root.as_path()));
    }

    #[test]
    fn cwd_with_no_git_ancestors_defaults_to_desk_with_a_directory_project_candidate() {
        let root = temp_dir("nongit");
        let _guard = TempDirGuard(root.clone());
        // No git init.
        let nested = root.join("work");
        fs::create_dir_all(&nested).expect("nested");

        let raw = RawHostContext {
            focused_pane_cwd: Some(nested.to_string_lossy().into_owned()),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/fallback-unused"));

        assert_eq!(snap.default_scope, TaskScope::Global);
        assert_eq!(snap.this_repo.as_deref(), Some(nested.as_path()));
        assert_eq!(snap.provenance, ProvenanceOrigin::Capture);
        assert!(snap.title_prefill.is_none());

        // Standalone launch without Herdr context uses the same directory default.
        let standalone = build_snapshot(&RawHostContext::default(), &nested);
        assert_eq!(standalone.default_scope, snap.default_scope);
        assert_eq!(standalone.this_repo, snap.this_repo);
    }

    #[test]
    fn selected_text_prefills_title_and_sets_selection_origin() {
        let raw = RawHostContext {
            cwd: Some("/tmp/no-git-here-hopefully".into()),
            selected_text: Some("hi".into()),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/no-git-here-hopefully"));

        assert_eq!(snap.title_prefill.as_deref(), Some("hi"));
        assert_eq!(snap.provenance, ProvenanceOrigin::Selection);
    }

    #[test]
    fn raw_from_json_parses_host_shape_and_ignores_unknown() {
        let json = r#"{
            "focused_pane_cwd": "/proj/src",
            "selected_text": "todo item",
            "unknown_host_field": 123
        }"#;
        let raw = RawHostContext::from_json(Some(json));
        assert_eq!(raw.focused_pane_cwd.as_deref(), Some("/proj/src"));
        assert_eq!(raw.selected_text.as_deref(), Some("todo item"));
    }

    #[test]
    fn raw_from_json_malformed_or_missing_is_default() {
        assert_eq!(RawHostContext::from_json(None), RawHostContext::default());
        assert_eq!(
            RawHostContext::from_json(Some("not-json")),
            RawHostContext::default()
        );
    }

    #[test]
    fn resolve_repo_root_none_for_empty_path() {
        assert_eq!(resolve_repo_root(Path::new("")), None);
    }
}

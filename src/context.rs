//! Context Resolver: host context → invocation snapshot.
//!
//! Parses `HERDR_PLUGIN_CONTEXT_JSON` (and related host fields), resolves project scope by
//! walking ancestors for `.git`, and builds a capsule / passive agent meta without inventing
//! missing values. Agent lifecycle is never a human-status authority.

use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::{AgentMeta, AgentSessionIdentity, ContextCapsule, ProvenanceOrigin, TaskScope};
use crate::text::non_empty;

/// Env var herdr injects with invocation context JSON.
pub const CONTEXT_JSON_ENV: &str = "HERDR_PLUGIN_CONTEXT_JSON";

/// Raw host-provided context. Every field optional; unknown JSON keys ignored.
///
/// Field names align with herdr's `PluginInvocationContext` plus optional file/line and
/// branch hints when present.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct RawHostContext {
    #[serde(default)]
    pub focused_pane_cwd: Option<String>,
    #[serde(default)]
    pub workspace_cwd: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub worktree: Option<RawWorktree>,
    #[serde(default)]
    pub focused_pane_id: Option<String>,
    #[serde(default)]
    pub focused_pane_agent: Option<String>,
    #[serde(default)]
    pub focused_pane_agent_session: Option<AgentSessionIdentity>,
    #[serde(default)]
    pub selected_text: Option<String>,
    /// Optional branch hint when the host provides one (not invented from git).
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
}

/// Subset of herdr `WorkspaceWorktreeInfo` used for capsule fields.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct RawWorktree {
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub checkout_path: Option<String>,
    #[serde(default)]
    pub repo_key: Option<String>,
    #[serde(default)]
    pub repo_name: Option<String>,
    #[serde(default)]
    pub is_linked_worktree: Option<bool>,
}

/// Normalized Context Resolver output for one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationSnapshot {
    /// Default capture scope before user override.
    pub default_scope: TaskScope,
    /// Resolved project root when scope is project; also used as board "this repo".
    pub this_repo: Option<PathBuf>,
    /// Title pre-fill when selected text is present.
    pub title_prefill: Option<String>,
    /// Provenance label for create: Selection when selected text is used, else Capture.
    pub provenance: ProvenanceOrigin,
    /// Capsule with only known fields set. `None` when completely empty.
    pub capsule: Option<ContextCapsule>,
    /// Passive agent/pane ids only. `None` when neither is known.
    pub agent_meta: Option<AgentMeta>,
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
/// Never invents capsule/agent fields that the host did not provide.
/// Unresolved repo → [`TaskScope::Global`].
pub fn build_snapshot(raw: &RawHostContext, fallback_cwd: impl AsRef<Path>) -> InvocationSnapshot {
    let fallback = fallback_cwd.as_ref();
    let cwd = raw
        .effective_cwd()
        .unwrap_or_else(|| fallback.to_path_buf());

    let this_repo = resolve_repo_root(&cwd);
    let default_scope = match &this_repo {
        Some(root) => TaskScope::Project {
            path: root.to_string_lossy().into_owned(),
        },
        None => TaskScope::Global,
    };

    let selected = non_empty(raw.selected_text.as_deref()).map(str::to_string);
    let (title_prefill, provenance) = match &selected {
        Some(text) => (Some(text.clone()), ProvenanceOrigin::Selection),
        None => (None, ProvenanceOrigin::Capture),
    };

    let repo_path = this_repo
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| {
            raw.worktree
                .as_ref()
                .and_then(|w| non_empty(w.repo_root.as_deref()).map(str::to_string))
        });

    let worktree_path = raw
        .worktree
        .as_ref()
        .and_then(|w| non_empty(w.checkout_path.as_deref()).map(str::to_string));

    let capsule = ContextCapsule {
        repo_path,
        worktree_path,
        branch: non_empty(raw.branch.as_deref()).map(str::to_string),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        source_pane_id: non_empty(raw.focused_pane_id.as_deref()).map(str::to_string),
        selected_text: selected,
        file: non_empty(raw.file.as_deref()).map(str::to_string),
        line: raw.line,
    };
    let capsule = if capsule.is_empty() {
        None
    } else {
        Some(capsule)
    };

    let agent_meta = AgentMeta {
        agent_id: non_empty(raw.focused_pane_agent.as_deref()).map(str::to_string),
        pane_id: non_empty(raw.focused_pane_id.as_deref()).map(str::to_string),
        agent_session: raw.focused_pane_agent_session.clone(),
    };
    let agent_meta = if agent_meta.is_empty() {
        None
    } else {
        Some(agent_meta)
    };

    InvocationSnapshot {
        default_scope,
        this_repo,
        title_prefill,
        provenance,
        capsule,
        agent_meta,
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
        let capsule = snap.capsule.expect("cwd known → capsule present");
        assert_eq!(
            capsule.repo_path.as_deref(),
            Some(root.to_string_lossy().as_ref())
        );
        assert_eq!(
            capsule.cwd.as_deref(),
            Some(nested.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn cwd_with_no_git_ancestors_defaults_to_global() {
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
        assert_eq!(snap.this_repo, None);
        assert_eq!(snap.provenance, ProvenanceOrigin::Capture);
        assert!(snap.title_prefill.is_none());
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
        let capsule = snap.capsule.expect("capsule");
        assert_eq!(capsule.selected_text.as_deref(), Some("hi"));
    }

    #[test]
    fn capsule_omits_missing_optional_fields() {
        let raw = RawHostContext {
            cwd: Some("/tmp/only-cwd".into()),
            // deliberately omit worktree, branch, pane, selection, file/line
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/only-cwd"));
        let capsule = snap.capsule.expect("cwd present");

        assert!(capsule.worktree_path.is_none());
        assert!(capsule.branch.is_none());
        assert!(capsule.source_pane_id.is_none());
        assert!(capsule.selected_text.is_none());
        assert!(capsule.file.is_none());
        assert!(capsule.line.is_none());
        // No inventing a repo path when none resolved and host did not supply one.
        assert!(capsule.repo_path.is_none());
        assert_eq!(capsule.cwd.as_deref(), Some("/tmp/only-cwd"));

        // Wire form also omits None fields.
        let json = serde_json::to_value(&capsule).expect("serialize capsule");
        let obj = json.as_object().expect("object");
        assert!(obj.contains_key("cwd"));
        assert!(!obj.contains_key("worktree_path"));
        assert!(!obj.contains_key("branch"));
        assert!(!obj.contains_key("source_pane_id"));
        assert!(!obj.contains_key("selected_text"));
        assert!(!obj.contains_key("file"));
        assert!(!obj.contains_key("line"));
        assert!(!obj.contains_key("repo_path"));
    }

    #[test]
    fn agent_and_pane_ids_stored_as_option_metadata_only() {
        let raw = RawHostContext {
            cwd: Some("/tmp/agent-meta".into()),
            focused_pane_id: Some("w1:p2".into()),
            focused_pane_agent: Some("agent-abc".into()),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/agent-meta"));

        let meta = snap.agent_meta.as_ref().expect("ids present");
        assert_eq!(meta.agent_id.as_deref(), Some("agent-abc"));
        assert_eq!(meta.pane_id.as_deref(), Some("w1:p2"));
        // Capsule source pane mirrors pane id when known; still data only.
        assert_eq!(
            snap.capsule
                .as_ref()
                .and_then(|c| c.source_pane_id.as_deref()),
            Some("w1:p2")
        );

        // Domain create accepts passive meta; there is no agent-status → human-status API.
        let mut state = crate::domain::DomainState::new();
        let id = state
            .create(
                "from agent pane",
                None,
                snap.default_scope.clone(),
                snap.capsule.clone(),
                snap.agent_meta.clone(),
                snap.provenance,
            )
            .expect("create");
        let task = state.get(id).expect("task");
        assert_eq!(
            task.agent_meta.as_ref().and_then(|m| m.agent_id.as_deref()),
            Some("agent-abc")
        );
        assert_eq!(task.status, crate::domain::HumanStatus::Ready);
        // Human status only via domain human commands (set_status / complete / …), not agent meta.
        state
            .set_status(id, crate::domain::HumanStatus::Started)
            .expect("human set_status");
        assert_eq!(
            state.get(id).expect("task").status,
            crate::domain::HumanStatus::Started
        );
    }

    #[test]
    fn missing_agent_and_pane_yields_no_agent_meta() {
        let raw = RawHostContext {
            cwd: Some("/tmp/no-agent".into()),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/no-agent"));
        assert!(snap.agent_meta.is_none());
    }

    #[test]
    fn raw_from_json_parses_host_shape_and_ignores_unknown() {
        let json = r#"{
            "focused_pane_cwd": "/proj/src",
            "focused_pane_id": "w0:p1",
            "focused_pane_agent": "codex",
            "selected_text": "todo item",
            "worktree": {
                "repo_key": "proj",
                "repo_name": "proj",
                "repo_root": "/proj",
                "checkout_path": "/proj",
                "is_linked_worktree": false
            },
            "unknown_host_field": 123
        }"#;
        let raw = RawHostContext::from_json(Some(json));
        assert_eq!(raw.focused_pane_cwd.as_deref(), Some("/proj/src"));
        assert_eq!(raw.focused_pane_id.as_deref(), Some("w0:p1"));
        assert_eq!(raw.focused_pane_agent.as_deref(), Some("codex"));
        assert_eq!(raw.selected_text.as_deref(), Some("todo item"));
        assert_eq!(
            raw.worktree.as_ref().and_then(|w| w.repo_root.as_deref()),
            Some("/proj")
        );
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

    #[test]
    fn host_worktree_and_file_line_fill_capsule_without_invention() {
        let raw = RawHostContext {
            cwd: Some("/tmp/x".into()),
            branch: Some("main".into()),
            file: Some("src/lib.rs".into()),
            line: Some(42),
            worktree: Some(RawWorktree {
                repo_root: Some("/repos/app".into()),
                checkout_path: Some("/repos/app-wt".into()),
                ..RawWorktree::default()
            }),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/x"));
        let c = snap.capsule.expect("capsule");
        // No .git under /tmp/x in this fixture: repo_path falls back to host worktree.repo_root.
        assert_eq!(c.repo_path.as_deref(), Some("/repos/app"));
        assert_eq!(c.worktree_path.as_deref(), Some("/repos/app-wt"));
        assert_eq!(c.branch.as_deref(), Some("main"));
        assert_eq!(c.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(c.line, Some(42));
    }
}

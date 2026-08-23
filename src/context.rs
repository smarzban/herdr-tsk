//! Context Resolver: host context → invocation snapshot.
//!
//! Parses `HERDR_PLUGIN_CONTEXT_JSON` (and related host fields), resolves project scope by
//! walking ancestors for `.git`, and builds a capsule / passive agent meta without inventing
//! missing values. Agent lifecycle is never a human-status authority.

use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::{
    AgentMeta, AgentSessionIdentity, ContextCapsule, ProvenanceOrigin, Task, TaskScope,
};
use crate::host::{board_workspace_id, is_tasks_pane, select_work_pane, HostPorts, PaneInfo};
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

/// Build [`RawHostContext`] from a live work pane (park path).
///
/// Maps pane cwd/foreground_cwd, pane id, and agent identity. A pane working directory is
/// capsule context, not a host-issued worktree root, so it never invents checkout_path.
pub fn raw_from_pane(pane: &PaneInfo) -> RawHostContext {
    let cwd = non_empty(pane.cwd.as_deref()).map(str::to_string);
    let focused_cwd = pane
        .effective_cwd()
        .map(str::to_string)
        .or_else(|| cwd.clone());
    RawHostContext {
        focused_pane_cwd: focused_cwd.clone(),
        workspace_cwd: focused_cwd,
        cwd,
        worktree: None,
        focused_pane_id: non_empty(Some(pane.pane_id.as_str())).map(str::to_string),
        focused_pane_agent: non_empty(pane.agent.as_deref()).map(str::to_string),
        focused_pane_agent_session: pane.agent_session.clone(),
        ..RawHostContext::default()
    }
}

/// An immutable live Park source discovered for one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParkCandidate {
    pub pane_id: String,
    pub cwd: Option<String>,
    pub snapshot: InvocationSnapshot,
}

/// Typed Park discovery result. Park never substitutes frozen board-open environment data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParkDiscovery {
    One(Box<ParkCandidate>),
    Many(Vec<ParkCandidate>),
    NoMatch,
    HostFailure(String),
}

impl ParkDiscovery {
    pub fn candidates(&self) -> &[ParkCandidate] {
        match self {
            Self::One(candidate) => std::slice::from_ref(candidate.as_ref()),
            Self::Many(candidates) => candidates,
            Self::NoMatch | Self::HostFailure(_) => &[],
        }
    }
}

/// Discover live, in-workspace Park candidates for `task`.
///
/// The board's environment is frozen at board-open, so it is deliberately never a
/// fallback. A task worktree takes precedence over its project scope. Path comparison
/// is lexical and component-aware, so `/repo-two` cannot match `/repo`.
pub fn discover_park_sources(task: &Task, host: &(impl HostPorts + ?Sized)) -> ParkDiscovery {
    let panes = match host.list_panes() {
        Ok(panes) => panes,
        Err(error) => return ParkDiscovery::HostFailure(error),
    };
    let current = match host.current_pane() {
        Ok(current) => current,
        Err(error) => return ParkDiscovery::HostFailure(error),
    };
    let Some(workspace_id) = board_workspace_id(&panes, current.as_ref()) else {
        return ParkDiscovery::NoMatch;
    };
    let Some(workspace_id) = non_empty(Some(workspace_id.as_str())) else {
        return ParkDiscovery::NoMatch;
    };
    let scope_root = park_scope_root(task);
    let fallback = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let candidates: Vec<ParkCandidate> = panes
        .iter()
        .filter(|pane| !is_tasks_pane(pane) && pane.workspace_id.as_deref() == Some(workspace_id))
        .filter(|pane| {
            scope_root.as_ref().is_none_or(|root| {
                pane.effective_cwd()
                    .and_then(lexically_normalized_path)
                    .is_some_and(|cwd| cwd == *root || cwd.starts_with(root))
            })
        })
        .map(|pane| ParkCandidate {
            pane_id: pane.pane_id.clone(),
            cwd: pane.effective_cwd().map(str::to_string),
            snapshot: build_snapshot(&raw_from_pane(pane), &fallback),
        })
        .collect();
    match candidates.len() {
        0 => ParkDiscovery::NoMatch,
        1 => ParkDiscovery::One(Box::new(
            candidates.into_iter().next().expect("one candidate"),
        )),
        _ => ParkDiscovery::Many(candidates),
    }
}

fn park_scope_root(task: &Task) -> Option<PathBuf> {
    let worktree = task
        .capsule
        .as_ref()
        .and_then(|capsule| non_empty(capsule.worktree_path.as_deref()));
    let project = match &task.scope {
        TaskScope::Project { path } => non_empty(Some(path.as_str())),
        // Global scope intentionally has no path filter, even when the capture capsule
        // retains ambient repository context.
        TaskScope::Global => None,
    };
    worktree.or(project).and_then(lexically_normalized_path)
}

fn lexically_normalized_path(path: &str) -> Option<PathBuf> {
    let path = non_empty(Some(path))?;
    let source = Path::new(path);
    let mut normalized = PathBuf::new();
    for component in source.components() {
        use std::path::Component;
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() && !source.is_absolute() {
                    normalized.push(component.as_os_str());
                }
            }
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

/// Snapshot for explicit linking: prefer a live work pane over frozen board context.
///
/// Unlike Park, linking has no task scope to resolve. It retains the historic single-source
/// behavior and is not used by the Park path.
pub fn load_snapshot_for_link(host: &(impl HostPorts + ?Sized)) -> InvocationSnapshot {
    let panes = host.list_panes().unwrap_or_default();
    let current = host.current_pane().ok().flatten();
    let board_ws = board_workspace_id(&panes, current.as_ref());
    let work = select_work_pane(&panes, current.as_ref(), board_ws.as_deref());
    let raw = match work {
        Some(pane) => raw_from_pane(&pane),
        None => RawHostContext::from_env(),
    };
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

    use crate::host::{parse_pane_current, parse_pane_list, select_work_pane};

    /// Fixture host returning fixed pane list/current JSON (no process spawn).
    struct FixturePanesHost {
        list_json: &'static str,
        current_json: &'static str,
    }

    impl HostPorts for FixturePanesHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            Ok(parse_pane_list(self.list_json)
                .into_iter()
                .map(|p| p.pane_id)
                .collect())
        }

        fn list_panes(&self) -> Result<Vec<crate::host::PaneInfo>, String> {
            Ok(parse_pane_list(self.list_json))
        }

        fn current_pane(&self) -> Result<Option<crate::host::PaneInfo>, String> {
            Ok(parse_pane_current(self.current_json))
        }

        fn focus_pane(&self, _pane_id: &str) -> Result<(), String> {
            Ok(())
        }

        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn link_snapshot_uses_work_pane_not_tasks_from_fixture_json() {
        // Tasks is focused (board open); work pane is sibling in same workspace.
        let list = r#"{
            "result": {
                "panes": [
                    {
                        "pane_id": "w0:p1",
                        "label": "Grok",
                        "cwd": "/work/repo",
                        "foreground_cwd": "/work/repo/src",
                        "agent": "grok",
                        "agent_session": {
                            "source": "herdr:grok",
                            "value": "session-park"
                        },
                        "focused": false,
                        "workspace_id": "w0"
                    },
                    {
                        "pane_id": "w0:p2",
                        "label": "tsk",
                        "terminal_title_stripped": "tsk",
                        "cwd": "/plugin/board-cwd",
                        "foreground_cwd": "/plugin/board-cwd",
                        "focused": true,
                        "workspace_id": "w0"
                    }
                ]
            }
        }"#;
        let current = r#"{
            "result": {
                "pane": {
                    "pane_id": "w0:p2",
                    "label": "tsk",
                    "cwd": "/plugin/board-cwd",
                    "workspace_id": "w0"
                }
            }
        }"#;
        let panes = parse_pane_list(list);
        let cur = parse_pane_current(current);
        let work = select_work_pane(&panes, cur.as_ref(), Some("w0")).expect("work pane");
        assert_eq!(work.pane_id, "w0:p1");

        let host = FixturePanesHost {
            list_json: list,
            current_json: current,
        };
        let snap = load_snapshot_for_link(&host);
        let capsule = snap.capsule.expect("capsule from work pane");
        assert_eq!(capsule.source_pane_id.as_deref(), Some("w0:p1"));
        assert_eq!(capsule.cwd.as_deref(), Some("/work/repo/src"));
        assert_ne!(
            capsule.cwd.as_deref(),
            Some("/plugin/board-cwd"),
            "must not park Tasks board cwd"
        );
        assert_eq!(
            snap.agent_meta.as_ref().and_then(|m| m.agent_id.as_deref()),
            Some("grok")
        );
        assert_eq!(
            snap.agent_meta.as_ref().and_then(|m| m.pane_id.as_deref()),
            Some("w0:p1")
        );
        assert_eq!(
            snap.agent_meta
                .as_ref()
                .and_then(|m| m.agent_session.as_ref())
                .map(|s| (s.source.as_str(), s.value.as_str())),
            Some(("herdr:grok", "session-park"))
        );
    }

    struct ResolverHost {
        panes: Result<Vec<PaneInfo>, String>,
        current: Result<Option<PaneInfo>, String>,
    }

    impl HostPorts for ResolverHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            self.list_panes()
                .map(|panes| panes.into_iter().map(|pane| pane.pane_id).collect())
        }

        fn list_panes(&self) -> Result<Vec<PaneInfo>, String> {
            self.panes.clone()
        }

        fn current_pane(&self) -> Result<Option<PaneInfo>, String> {
            self.current.clone()
        }

        fn focus_pane(&self, _pane_id: &str) -> Result<(), String> {
            Ok(())
        }

        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn resolver_task(scope: TaskScope, capsule: Option<ContextCapsule>) -> Task {
        let mut domain = crate::domain::DomainState::new();
        let id = domain
            .create(
                "Park me",
                None,
                scope,
                capsule,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");
        domain.get(id).expect("task").clone()
    }

    fn pane(id: &str, workspace: &str, cwd: &str) -> PaneInfo {
        PaneInfo {
            pane_id: id.into(),
            workspace_id: Some(workspace.into()),
            cwd: Some(cwd.into()),
            ..PaneInfo::default()
        }
    }

    fn tasks_pane(workspace: &str) -> PaneInfo {
        PaneInfo {
            pane_id: format!("{workspace}:tasks"),
            workspace_id: Some(workspace.into()),
            label: Some("tsk".into()),
            ..PaneInfo::default()
        }
    }

    #[test]
    fn park_discovery_excludes_other_workspaces_and_tasks_panes() {
        let task = resolver_task(
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            None,
        );
        let host = ResolverHost {
            panes: Ok(vec![
                tasks_pane("w0"),
                pane("w1:outside", "w1", "/repos/app"),
                pane("w0:match", "w0", "/repos/app/src"),
            ]),
            current: Ok(None),
        };

        let ParkDiscovery::One(candidate) = discover_park_sources(&task, &host) else {
            panic!("only the in-workspace non-Tasks pane may be eligible");
        };
        assert_eq!(candidate.pane_id, "w0:match");
    }

    #[test]
    fn park_discovery_global_scope_ignores_capsule_repo_path() {
        let task = resolver_task(
            TaskScope::Global,
            Some(ContextCapsule {
                repo_path: Some("/repos/captured-context".into()),
                ..ContextCapsule::default()
            }),
        );
        let host = ResolverHost {
            panes: Ok(vec![
                tasks_pane("w0"),
                pane("w0:elsewhere", "w0", "/repos/other-project"),
            ]),
            current: Ok(None),
        };

        let ParkDiscovery::One(candidate) = discover_park_sources(&task, &host) else {
            panic!("Global scope must not filter candidates by its capsule repo path");
        };
        assert_eq!(candidate.pane_id, "w0:elsewhere");
    }

    #[test]
    fn park_discovery_uses_lexical_boundaries_and_worktree_before_project() {
        let task = resolver_task(
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            Some(ContextCapsule {
                worktree_path: Some("/repos/app/.worktrees/feature".into()),
                ..ContextCapsule::default()
            }),
        );
        let host = ResolverHost {
            panes: Ok(vec![
                tasks_pane("w0"),
                pane("w0:project-only", "w0", "/repos/app/src"),
                pane("w0:prefix", "w0", "/repos/app-two"),
                pane("w0:worktree", "w0", "/repos/app/.worktrees/feature/./src"),
            ]),
            current: Ok(None),
        };

        let ParkDiscovery::One(candidate) = discover_park_sources(&task, &host) else {
            panic!("worktree descendant must win over project-only and prefix paths");
        };
        assert_eq!(candidate.pane_id, "w0:worktree");
    }

    #[test]
    fn park_discovery_distinguishes_one_many_zero_and_host_failure() {
        let task = resolver_task(
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            None,
        );
        let one = ResolverHost {
            panes: Ok(vec![tasks_pane("w0"), pane("w0:one", "w0", "/repos/app")]),
            current: Ok(None),
        };
        assert!(matches!(
            discover_park_sources(&task, &one),
            ParkDiscovery::One(_)
        ));

        let many = ResolverHost {
            panes: Ok(vec![
                tasks_pane("w0"),
                pane("w0:one", "w0", "/repos/app/a"),
                pane("w0:two", "w0", "/repos/app/b"),
            ]),
            current: Ok(None),
        };
        assert_eq!(discover_park_sources(&task, &many).candidates().len(), 2);
        assert!(matches!(
            discover_park_sources(&task, &many),
            ParkDiscovery::Many(_)
        ));

        let zero = ResolverHost {
            panes: Ok(vec![
                tasks_pane("w0"),
                pane("w0:other", "w0", "/repos/other"),
            ]),
            current: Ok(None),
        };
        assert!(matches!(
            discover_park_sources(&task, &zero),
            ParkDiscovery::NoMatch
        ));

        let failed = ResolverHost {
            panes: Err("host down".into()),
            current: Ok(None),
        };
        assert!(matches!(
            discover_park_sources(&task, &failed),
            ParkDiscovery::HostFailure(message) if message == "host down"
        ));
    }

    #[test]
    fn park_discovery_current_pane_failure_is_visible_host_failure() {
        let task = resolver_task(TaskScope::Global, None);
        let host = ResolverHost {
            panes: Ok(vec![tasks_pane("w0"), pane("w0:work", "w0", "/repos/app")]),
            current: Err("pane current failed".into()),
        };
        assert!(matches!(
            discover_park_sources(&task, &host),
            ParkDiscovery::HostFailure(message) if message == "pane current failed"
        ));
    }

    #[test]
    fn park_scope_normalizes_parent_components_before_boundary_matching() {
        let task = resolver_task(
            TaskScope::Project {
                path: "/repos/app".into(),
            },
            None,
        );
        let host = ResolverHost {
            panes: Ok(vec![
                tasks_pane("w0"),
                pane("w0:escape", "w0", "/repos/app/../other-project"),
            ]),
            current: Ok(None),
        };
        assert!(matches!(
            discover_park_sources(&task, &host),
            ParkDiscovery::NoMatch
        ));
        assert_eq!(
            lexically_normalized_path("../sibling"),
            Some(PathBuf::from("../sibling"))
        );
    }

    #[test]
    fn raw_from_pane_keeps_pane_cwd_out_of_worktree_root() {
        let pane = crate::host::PaneInfo {
            pane_id: "w1:p9".into(),
            cwd: Some("/repos/app".into()),
            foreground_cwd: Some("/repos/app/src".into()),
            agent: Some("codex".into()),
            workspace_id: Some("w1".into()),
            ..crate::host::PaneInfo::default()
        };
        let raw = raw_from_pane(&pane);
        assert_eq!(raw.focused_pane_id.as_deref(), Some("w1:p9"));
        assert_eq!(raw.focused_pane_agent.as_deref(), Some("codex"));
        assert_eq!(raw.focused_pane_cwd.as_deref(), Some("/repos/app/src"));
        assert_eq!(raw.cwd.as_deref(), Some("/repos/app"));
        assert!(
            raw.worktree.is_none(),
            "a pane cwd is not evidence of a checkout root and must not narrow later Park scope"
        );
    }
}

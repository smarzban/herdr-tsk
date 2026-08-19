//! Capture pipeline: snapshot + user fields → domain create.
//!
//! Application path used by Capture UI. Creates only through Task Domain;
//! optional Task Store save when a store is provided.

use uuid::Uuid;

use crate::context::InvocationSnapshot;
use crate::domain::{DomainError, DomainState, TaskScope};
use crate::store::{StoreError, TaskStore};

/// Task id returned by a successful capture (domain `Uuid`).
pub type TaskId = Uuid;

/// Failures from capture create and optional persist.
#[derive(Debug)]
pub enum CaptureError {
    Domain(DomainError),
    Store(StoreError),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::Domain(e) => write!(f, "{e}"),
            CaptureError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CaptureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CaptureError::Domain(e) => Some(e),
            CaptureError::Store(e) => Some(e),
        }
    }
}

impl From<DomainError> for CaptureError {
    fn from(value: DomainError) -> Self {
        CaptureError::Domain(value)
    }
}

impl From<StoreError> for CaptureError {
    fn from(value: StoreError) -> Self {
        CaptureError::Store(value)
    }
}

/// Create one task from a capture form submission.
///
/// - Scope: `scope_override` when set, else `snapshot.default_scope`.
/// - Capsule, agent meta, and provenance origin come from the snapshot.
/// - Empty/whitespace title is rejected by the domain; no task is created.
/// - When `store` is `Some`, persists domain state after a successful create.
pub fn capture_save(
    state: &mut DomainState,
    store: Option<&TaskStore>,
    snapshot: &InvocationSnapshot,
    title: impl AsRef<str>,
    notes: Option<String>,
    scope_override: Option<TaskScope>,
) -> Result<TaskId, CaptureError> {
    let scope = scope_override.unwrap_or_else(|| snapshot.default_scope.clone());
    let id = state.create(
        title,
        notes,
        scope,
        snapshot.capsule.clone(),
        snapshot.agent_meta.clone(),
        snapshot.provenance,
    )?;
    if let Some(store) = store {
        // Merge with any concurrent writer before persist (board + capture).
        store.reload_merge_save(state)?;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{build_snapshot, RawHostContext};
    use crate::domain::{AgentMeta, ContextCapsule, HumanStatus, ProvenanceOrigin};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("herdr-tasks-t9-{label}-{nanos}-{seq}"))
    }

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn project_snapshot(path: &str) -> InvocationSnapshot {
        InvocationSnapshot {
            default_scope: TaskScope::Project {
                path: path.to_string(),
            },
            this_repo: Some(PathBuf::from(path)),
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: Some(ContextCapsule {
                repo_path: Some(path.to_string()),
                cwd: Some(format!("{path}/src")),
                ..ContextCapsule::default()
            }),
            agent_meta: Some(AgentMeta {
                agent_id: Some("agent-1".into()),
                pane_id: Some("w0:p1".into()),
                agent_session: None,
            }),
        }
    }

    fn global_snapshot() -> InvocationSnapshot {
        InvocationSnapshot {
            default_scope: TaskScope::Global,
            this_repo: None,
            title_prefill: None,
            provenance: ProvenanceOrigin::Capture,
            capsule: Some(ContextCapsule {
                cwd: Some("/tmp/nongit".into()),
                ..ContextCapsule::default()
            }),
            agent_meta: None,
        }
    }

    #[test]
    fn default_create_uses_snapshot_project_scope() {
        let snap = project_snapshot("/repos/app");
        let mut state = DomainState::new();
        let id = capture_save(&mut state, None, &snap, "Ship capture", None, None).expect("create");

        let task = state.get(id).expect("task");
        assert_eq!(
            task.scope,
            TaskScope::Project {
                path: "/repos/app".into(),
            }
        );
        assert_eq!(task.title, "Ship capture");
        assert_eq!(task.status, HumanStatus::Ready);
        assert_eq!(task.provenance, ProvenanceOrigin::Capture);
        assert_eq!(
            task.capsule.as_ref().and_then(|c| c.repo_path.as_deref()),
            Some("/repos/app")
        );
        assert_eq!(
            task.agent_meta.as_ref().and_then(|m| m.agent_id.as_deref()),
            Some("agent-1")
        );
        assert_eq!(state.tasks().len(), 1);
    }

    #[test]
    fn default_create_uses_snapshot_global_when_no_repo() {
        let snap = global_snapshot();
        let mut state = DomainState::new();
        let id = capture_save(&mut state, None, &snap, "Loose note", None, None).expect("create");
        assert_eq!(state.get(id).expect("task").scope, TaskScope::Global);
    }

    #[test]
    fn override_to_global_saves_global() {
        let snap = project_snapshot("/repos/app");
        let mut state = DomainState::new();
        let id = capture_save(
            &mut state,
            None,
            &snap,
            "Global instead",
            Some("notes".into()),
            Some(TaskScope::Global),
        )
        .expect("create");

        let task = state.get(id).expect("task");
        assert_eq!(task.scope, TaskScope::Global);
        assert_eq!(task.notes.as_deref(), Some("notes"));
        // Capsule still from snapshot even when scope overridden.
        assert_eq!(
            task.capsule.as_ref().and_then(|c| c.repo_path.as_deref()),
            Some("/repos/app")
        );
    }

    #[test]
    fn override_to_other_project_path_saves_that_path() {
        let snap = project_snapshot("/repos/app");
        let mut state = DomainState::new();
        let other = TaskScope::Project {
            path: "/repos/other".into(),
        };
        let id = capture_save(
            &mut state,
            None,
            &snap,
            "Other project",
            None,
            Some(other.clone()),
        )
        .expect("create");

        assert_eq!(state.get(id).expect("task").scope, other);
    }

    #[test]
    fn empty_title_rejected_at_domain_boundary() {
        let snap = project_snapshot("/repos/app");
        let mut state = DomainState::new();
        let err = capture_save(&mut state, None, &snap, "   \t  ", None, None)
            .expect_err("empty title must fail");
        match err {
            CaptureError::Domain(DomainError::EmptyTitle) => {}
            other => panic!("expected EmptyTitle, got {other:?}"),
        }
        assert!(state.tasks().is_empty());
    }

    #[test]
    fn selection_provenance_and_title_from_snapshot_fields() {
        let raw = RawHostContext {
            cwd: Some("/tmp/nongit-selection".into()),
            selected_text: Some("from selection".into()),
            ..RawHostContext::default()
        };
        let snap = build_snapshot(&raw, PathBuf::from("/tmp/nongit-selection"));
        assert_eq!(snap.provenance, ProvenanceOrigin::Selection);

        let mut state = DomainState::new();
        // Title may still be user-edited; prefill is UI concern. Provenance stays Selection.
        let id =
            capture_save(&mut state, None, &snap, "from selection", None, None).expect("create");
        let task = state.get(id).expect("task");
        assert_eq!(task.provenance, ProvenanceOrigin::Selection);
        assert_eq!(
            task.capsule
                .as_ref()
                .and_then(|c| c.selected_text.as_deref()),
            Some("from selection")
        );
    }

    #[test]
    fn capture_save_persists_when_store_provided() {
        let dir = temp_dir("persist");
        fs::create_dir_all(&dir).expect("mkdir");
        let _guard = TempDirGuard(dir.clone());
        let store = TaskStore::new(&dir);

        let snap = project_snapshot("/repos/app");
        let mut state = DomainState::new();
        let id = capture_save(&mut state, Some(&store), &snap, "Persisted", None, None)
            .expect("create+save");

        let reloaded = store.load().expect("load");
        let task = reloaded.get(id).expect("task on disk");
        assert_eq!(task.title, "Persisted");
        assert_eq!(
            task.scope,
            TaskScope::Project {
                path: "/repos/app".into(),
            }
        );
    }
}

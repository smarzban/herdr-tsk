//! F6 dispatch recovery orchestration.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use herdr_tasks::dispatch::{
    cleanup_dispatch_attempt, resume_dispatch_attempt, start_dispatch_attempt, DispatchMode,
    DispatchRecoveryAction, DispatchRecoveryResult,
};
use herdr_tasks::domain::{
    AgentReceipt, AgentSessionIdentity, DispatchAttemptPhase, DomainState, PaneReceipt,
    ProvenanceOrigin, TaskEventKind, TaskScope, WorktreeReceipt,
};
use herdr_tasks::host::HostPorts;
use herdr_tasks::store::{StoreError, TaskStateStore, TaskStore};
use herdr_tasks::ui::board::{apply_dispatch_recovery_result, BoardInputMode, BoardModel};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("herdr-tasks-f6-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Create,
    Pane,
    Agent,
    Prompt,
}

#[derive(Default)]
struct RecoveryHost {
    fail_before: Option<Step>,
    confirmed: bool,
    fail_remove_worktree: bool,
    confirm_error: bool,
    record_recovery_checks: bool,
    calls: Arc<Mutex<Vec<String>>>,
}

impl RecoveryHost {
    fn call(&self, call: &str) {
        self.calls.lock().unwrap().push(call.into());
    }

    fn fail(&self, step: Step) -> Result<(), String> {
        if self.fail_before == Some(step) {
            Err(format!("injected {step:?} failure"))
        } else {
            Ok(())
        }
    }
}

impl HostPorts for RecoveryHost {
    fn list_pane_ids(&self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
    fn focus_pane(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn open_path(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn create_worktree_receipt(&self, _: &str, _: &str) -> Result<WorktreeReceipt, String> {
        self.call("create");
        self.fail(Step::Create)?;
        Ok(WorktreeReceipt {
            path: "/checkout".into(),
            workspace_id: "workspace-owned".into(),
        })
    }
    fn open_shell_at_cwd_receipt(&self, cwd: &str) -> Result<PaneReceipt, String> {
        self.call(&format!("open:{cwd}"));
        self.fail(Step::Pane)?;
        Ok(PaneReceipt {
            pane_id: "pane-owned".into(),
        })
    }
    fn start_agent_receipt(&self, _: &str, _: &str, pane: &str) -> Result<AgentReceipt, String> {
        self.call(&format!("start:{pane}"));
        self.fail(Step::Agent)?;
        Ok(AgentReceipt {
            pane_id: pane.into(),
            display_name: Some("host-normalized-agent".into()),
            agent_session: Some(AgentSessionIdentity {
                source: "herdr:grok".into(),
                value: "agent-owned".into(),
            }),
        })
    }
    fn agent_prompt(&self, pane: &str, _: &str) -> Result<(), String> {
        self.call(&format!("prompt:{pane}"));
        self.fail(Step::Prompt)
    }
    fn confirm_worktree_receipt(&self, _: &WorktreeReceipt) -> Result<bool, String> {
        if self.record_recovery_checks {
            self.call("confirm-worktree");
        }
        if self.confirm_error {
            Err("confirmation unavailable".into())
        } else {
            Ok(self.confirmed)
        }
    }
    fn confirm_pane_receipt(&self, _: &PaneReceipt) -> Result<bool, String> {
        if self.record_recovery_checks {
            self.call("confirm-pane");
        }
        if self.confirm_error {
            Err("confirmation unavailable".into())
        } else {
            Ok(self.confirmed)
        }
    }
    fn confirm_agent_receipt(&self, _: &AgentReceipt) -> Result<bool, String> {
        if self.record_recovery_checks {
            self.call("confirm-agent");
        }
        if self.confirm_error {
            Err("confirmation unavailable".into())
        } else {
            Ok(self.confirmed)
        }
    }
    fn remove_pane_receipt(&self, receipt: &PaneReceipt) -> Result<(), String> {
        self.call(&format!("remove-pane:{}", receipt.pane_id));
        Ok(())
    }
    fn remove_worktree_receipt(&self, receipt: &WorktreeReceipt) -> Result<(), String> {
        self.call(&format!("remove-worktree:{}", receipt.workspace_id));
        if self.fail_remove_worktree {
            Err("worktree removal failed".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
struct FailingSaveStore {
    inner: TaskStore,
    fail_on_save: usize,
    saves: Arc<Mutex<usize>>,
    saved_attempt_phases: Arc<Mutex<Vec<Option<DispatchAttemptPhase>>>>,
}

impl FailingSaveStore {
    fn new(inner: TaskStore, fail_on_save: usize) -> Self {
        Self {
            inner,
            fail_on_save,
            saves: Arc::new(Mutex::new(0)),
            saved_attempt_phases: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn saved_attempt_phases(&self) -> Vec<Option<DispatchAttemptPhase>> {
        self.saved_attempt_phases.lock().unwrap().clone()
    }
}

impl TaskStateStore for FailingSaveStore {
    fn load(&self) -> Result<DomainState, StoreError> {
        self.inner.load()
    }

    fn save(&self, state: &DomainState) -> Result<(), StoreError> {
        self.saved_attempt_phases.lock().unwrap().push(
            state
                .active_attempts()
                .first()
                .map(|attempt| attempt.phase()),
        );
        let mut saves = self.saves.lock().unwrap();
        *saves += 1;
        if *saves == self.fail_on_save {
            return Err(StoreError::Io(std::io::Error::other(
                "injected save failure",
            )));
        }
        self.inner.save(state)
    }

    fn locked_transition<T>(
        &self,
        transition: impl FnOnce(&mut DomainState) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self.load().map_err(|error| error.to_string())?;
        let result = transition(&mut state)?;
        self.save(&state).map_err(|error| error.to_string())?;
        Ok(result)
    }
}

#[derive(Clone)]
struct ConcurrentBoardMutationStore {
    inner: TaskStore,
    task_id: uuid::Uuid,
    transition_calls: Arc<Mutex<usize>>,
    injected: Arc<Mutex<bool>>,
}

impl ConcurrentBoardMutationStore {
    fn inject_board_edit(&self) {
        let mut injected = self.injected.lock().unwrap();
        if *injected {
            return;
        }
        self.inner
            .locked_transition(|state| {
                state
                    .edit(
                        self.task_id,
                        "edited by main board while dispatch runs",
                        None,
                        TaskScope::Global,
                        None,
                    )
                    .map_err(|error| error.to_string())
            })
            .expect("concurrent board edit persists");
        *injected = true;
    }
}

impl TaskStateStore for ConcurrentBoardMutationStore {
    fn load(&self) -> Result<DomainState, StoreError> {
        self.inner.load()
    }

    fn save(&self, state: &DomainState) -> Result<(), StoreError> {
        // The former worker path persisted its stale snapshot here. Injecting immediately
        // before that write proves the new implementation must not call save for a transition.
        self.inject_board_edit();
        self.inner.save(state)
    }

    fn locked_transition<T>(
        &self,
        transition: impl FnOnce(&mut DomainState) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut calls = self.transition_calls.lock().unwrap();
        *calls += 1;
        let inject_before_transition = *calls > 1;
        drop(calls);
        if inject_before_transition {
            // This is the deterministic interleaving: the worker loaded before host I/O,
            // then the main Board commits a mutation before the worker's next persistence.
            self.inject_board_edit();
        }
        self.inner.locked_transition(transition)
    }
}

fn durable_dispatching_attempt(store: &TaskStore, task_id: uuid::Uuid) -> uuid::Uuid {
    let mut state = store.load().unwrap();
    let attempt_id = state
        .start_dispatch_attempt(
            task_id,
            herdr_tasks::domain::DispatchAttemptMode::Here,
            "grok",
        )
        .unwrap();
    let revision = state.active_attempt(attempt_id).unwrap().revision();
    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            herdr_tasks::domain::DispatchAttemptTransition::RecordReceipt(
                herdr_tasks::domain::OwnedResourceReceipt::Pane(PaneReceipt {
                    pane_id: "pane-owned".into(),
                }),
            ),
        )
        .unwrap();
    store.save(&state).unwrap();
    attempt_id
}

fn seeded(store: &TaskStore) -> (uuid::Uuid, TempDir) {
    let project = temp_dir("project");
    let guard = TempDir(project.clone());
    let mut state = DomainState::new();
    let task_id = state
        .create(
            "Recover me",
            None,
            TaskScope::Project {
                path: project.to_string_lossy().into(),
            },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();
    store.save(&state).unwrap();
    (task_id, guard)
}

/// Type a palette query one key at a time, exactly as the board loop does.
#[test]
fn failure_injection_before_every_host_step_reloads_exact_durable_progress() {
    for (step, receipts) in [
        (Step::Create, 0),
        (Step::Pane, 1),
        (Step::Agent, 2),
        (Step::Prompt, 3),
    ] {
        let dir = temp_dir(&format!("before-{step:?}"));
        let _guard = TempDir(dir.clone());
        let store = TaskStore::new(&dir);
        let (task_id, _project) = seeded(&store);
        let host = RecoveryHost {
            fail_before: Some(step),
            ..RecoveryHost::default()
        };

        let result = start_dispatch_attempt(
            store.clone(),
            task_id,
            DispatchMode::NewWorktree,
            "grok",
            &host,
        );
        let loaded = store.load().unwrap();
        if step == Step::Prompt {
            assert!(
                matches!(
                    result,
                    DispatchRecoveryResult::Dispatched {
                        prompt_warning: Some(_),
                        ..
                    }
                ),
                "{result:?}"
            );
            assert!(loaded.active_attempts().is_empty());
            assert_eq!(
                loaded
                    .get(task_id)
                    .unwrap()
                    .history
                    .iter()
                    .filter(|event| event.kind == TaskEventKind::Dispatched)
                    .count(),
                1
            );
        } else {
            assert!(
                matches!(result, DispatchRecoveryResult::Failed { .. }),
                "{result:?}"
            );
            let attempt = loaded.active_attempts().first().expect("durable attempt");
            assert_eq!(attempt.phase(), DispatchAttemptPhase::Failed);
            assert_eq!(attempt.owned_resources().len(), receipts, "step {step:?}");
            assert_eq!(
                attempt.steps().iter().filter(|s| s.completed).count(),
                receipts,
                "step {step:?}"
            );
        }
    }
}

#[test]
fn dispatch_transition_keeps_concurrent_main_board_mutation_and_uses_fresh_attempt_revision() {
    let dir = temp_dir("concurrent-board-mutation");
    let _guard = TempDir(dir.clone());
    let inner = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&inner);
    let store = ConcurrentBoardMutationStore {
        inner: inner.clone(),
        task_id,
        transition_calls: Arc::new(Mutex::new(0)),
        injected: Arc::new(Mutex::new(false)),
    };
    let host = RecoveryHost {
        fail_before: Some(Step::Agent),
        ..RecoveryHost::default()
    };

    let result = start_dispatch_attempt(store, task_id, DispatchMode::Here, "grok", &host);

    assert!(
        matches!(result, DispatchRecoveryResult::Failed { .. }),
        "{result:?}"
    );
    let loaded = inner.load().unwrap();
    assert_eq!(
        loaded.get(task_id).unwrap().title,
        "edited by main board while dispatch runs",
        "dispatch persistence must merge with, never overwrite, Board changes"
    );
    assert!(loaded.active_attempt_for_task(task_id).is_some());
}

#[test]
fn receipt_save_failure_stops_before_another_resource_creation_and_reloads_last_progress() {
    let dir = temp_dir("receipt-save-failure");
    let _guard = TempDir(dir.clone());
    let inner = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&inner);
    // The first orchestrator save persists the empty attempt. Fail saving the first receipt.
    let store = FailingSaveStore::new(inner.clone(), 2);
    let host = RecoveryHost::default();

    let result = start_dispatch_attempt(store, task_id, DispatchMode::NewWorktree, "grok", &host);

    assert!(
        matches!(result, DispatchRecoveryResult::PersistenceUncertain { .. }),
        "{result:?}"
    );
    assert_eq!(*host.calls.lock().unwrap(), ["create"]);
    let loaded = inner.load().unwrap();
    let attempt = loaded
        .active_attempts()
        .first()
        .expect("durable empty attempt");
    assert_eq!(attempt.owned_resources().len(), 0);
    assert_eq!(
        attempt.steps().iter().filter(|step| step.completed).count(),
        0
    );
}

#[test]
fn new_worktree_dispatch_success_clears_attempt_and_records_dispatch_once() {
    let dir = temp_dir("new-worktree-success");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);

    assert!(matches!(
        start_dispatch_attempt(
            store.clone(),
            task_id,
            DispatchMode::NewWorktree,
            "grok",
            &RecoveryHost::default(),
        ),
        DispatchRecoveryResult::Dispatched { .. }
    ));

    let loaded = store.load().unwrap();
    assert!(loaded.active_attempts().is_empty());
    assert_eq!(
        loaded
            .get(task_id)
            .unwrap()
            .history
            .iter()
            .filter(|event| event.kind == TaskEventKind::Dispatched)
            .count(),
        1
    );
}

#[test]
fn second_start_for_task_returns_existing_recovery_direction_without_host_work() {
    let dir = temp_dir("second-start");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);
    let failed_start = RecoveryHost {
        fail_before: Some(Step::Agent),
        ..RecoveryHost::default()
    };
    let attempt_id = match start_dispatch_attempt(
        store.clone(),
        task_id,
        DispatchMode::Here,
        "grok",
        &failed_start,
    ) {
        DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
        other => panic!("expected failed start: {other:?}"),
    };

    let duplicate_host = RecoveryHost::default();
    let result = start_dispatch_attempt(
        store.clone(),
        task_id,
        DispatchMode::NewWorktree,
        "codex",
        &duplicate_host,
    );

    assert_eq!(
        result,
        DispatchRecoveryResult::RecoveryRequired {
            attempt_id,
            actions: vec![
                DispatchRecoveryAction::Resume,
                DispatchRecoveryAction::Cleanup
            ],
        }
    );
    assert!(duplicate_host.calls.lock().unwrap().is_empty());
    let loaded = store.load().unwrap();
    assert_eq!(loaded.active_attempts().len(), 1);
    assert_eq!(loaded.active_attempts()[0].id(), attempt_id);
}

#[test]
fn prompt_failure_after_agent_creation_preserves_link_and_dispatch_once_without_attempt() {
    let dir = temp_dir("prompt-failure");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);
    let host = RecoveryHost {
        fail_before: Some(Step::Prompt),
        ..RecoveryHost::default()
    };

    let result = start_dispatch_attempt(store.clone(), task_id, DispatchMode::Here, "grok", &host);

    assert!(matches!(
        result,
        DispatchRecoveryResult::Dispatched {
            prompt_warning: Some(ref warning),
            ..
        } if warning.contains("prompt")
    ));
    assert_eq!(
        *host.calls.lock().unwrap(),
        vec![
            "open:".to_string() + _project.0.to_string_lossy().as_ref(),
            "start:pane-owned".into(),
            "prompt:pane-owned".into()
        ]
    );
    let loaded = store.load().unwrap();
    assert!(loaded.active_attempts().is_empty());
    let task = loaded.get(task_id).unwrap();
    assert!(task.agent_meta.is_some());
    assert_eq!(
        task.agent_meta.as_ref().unwrap().pane_id.as_deref(),
        Some("pane-owned")
    );
    assert_eq!(
        task.agent_meta.as_ref().unwrap().agent_id.as_deref(),
        Some("host-normalized-agent"),
        "recovery dispatch retains the host-reported display name"
    );
    assert_eq!(
        task.history
            .iter()
            .filter(|event| event.kind == TaskEventKind::Dispatched)
            .count(),
        1
    );
}

#[test]
fn resume_skips_confirmed_receipts_and_restarts_first_unconfirmed_step() {
    let dir = temp_dir("resume");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);
    let first = RecoveryHost {
        fail_before: Some(Step::Agent),
        confirmed: true,
        ..RecoveryHost::default()
    };
    let attempt_id =
        match start_dispatch_attempt(store.clone(), task_id, DispatchMode::Here, "grok", &first) {
            DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
            other => panic!("expected failed start: {other:?}"),
        };

    let resumed = RecoveryHost {
        confirmed: true,
        ..RecoveryHost::default()
    };
    assert!(matches!(
        resume_dispatch_attempt(store.clone(), attempt_id, &resumed),
        DispatchRecoveryResult::Dispatched { .. }
    ));
    assert_eq!(
        *resumed.calls.lock().unwrap(),
        vec!["start:pane-owned", "prompt:pane-owned"]
    );
    let reloaded = store.load().unwrap();
    assert!(reloaded.active_attempt(attempt_id).is_none());
    let task = reloaded.get(task_id).unwrap();
    assert_eq!(
        task.history
            .iter()
            .filter(|e| e.kind == TaskEventKind::Dispatched)
            .count(),
        1
    );

    let second_dir = temp_dir("resume-unconfirmed");
    let _second_guard = TempDir(second_dir.clone());
    let second_store = TaskStore::new(&second_dir);
    let (second_task, _second_project) = seeded(&second_store);
    let failed = RecoveryHost {
        fail_before: Some(Step::Agent),
        ..RecoveryHost::default()
    };
    let second_attempt = match start_dispatch_attempt(
        second_store.clone(),
        second_task,
        DispatchMode::Here,
        "grok",
        &failed,
    ) {
        DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
        other => panic!("expected failed start: {other:?}"),
    };
    let restart = RecoveryHost {
        confirmed: false,
        ..RecoveryHost::default()
    };
    assert!(matches!(
        resume_dispatch_attempt(second_store.clone(), second_attempt, &restart),
        DispatchRecoveryResult::Dispatched { .. }
    ));
    let restart_calls = restart.calls.lock().unwrap().clone();
    assert!(restart_calls[0].starts_with("open:"), "{restart_calls:?}");
    assert_eq!(
        &restart_calls[1..],
        ["start:pane-owned", "prompt:pane-owned"]
    );
}

#[test]
fn interrupted_dispatching_attempt_persists_failed_before_resume_or_cleanup() {
    for (operation, expected_calls) in [
        (
            "resume",
            vec!["confirm-pane", "start:pane-owned", "prompt:pane-owned"],
        ),
        ("cleanup", vec!["confirm-pane", "remove-pane:pane-owned"]),
    ] {
        let dir = temp_dir(&format!("interrupted-{operation}"));
        let _guard = TempDir(dir.clone());
        let inner = TaskStore::new(&dir);
        let (task_id, _project) = seeded(&inner);
        let attempt_id = durable_dispatching_attempt(&inner, task_id);
        let store = FailingSaveStore::new(inner.clone(), usize::MAX);
        let host = RecoveryHost {
            confirmed: true,
            record_recovery_checks: true,
            ..RecoveryHost::default()
        };

        let result = match operation {
            "resume" => resume_dispatch_attempt(store.clone(), attempt_id, &host),
            "cleanup" => cleanup_dispatch_attempt(store.clone(), attempt_id, &host),
            _ => unreachable!(),
        };

        assert!(
            matches!(
                result,
                DispatchRecoveryResult::Dispatched { .. } | DispatchRecoveryResult::Cleaned { .. }
            ),
            "{operation}: {result:?}"
        );
        assert_eq!(
            store.saved_attempt_phases().first(),
            Some(&Some(DispatchAttemptPhase::Failed)),
            "{operation} must durably fail an interrupted attempt before recovery host work"
        );
        assert_eq!(*host.calls.lock().unwrap(), expected_calls, "{operation}");
    }

    for operation in ["resume", "cleanup"] {
        let dir = temp_dir(&format!("interrupted-save-failure-{operation}"));
        let _guard = TempDir(dir.clone());
        let inner = TaskStore::new(&dir);
        let (task_id, _project) = seeded(&inner);
        let attempt_id = durable_dispatching_attempt(&inner, task_id);
        let store = FailingSaveStore::new(inner.clone(), 1);
        let host = RecoveryHost {
            confirmed: true,
            record_recovery_checks: true,
            ..RecoveryHost::default()
        };

        let result = match operation {
            "resume" => resume_dispatch_attempt(store.clone(), attempt_id, &host),
            "cleanup" => cleanup_dispatch_attempt(store.clone(), attempt_id, &host),
            _ => unreachable!(),
        };

        assert!(
            matches!(result, DispatchRecoveryResult::PersistenceUncertain { .. }),
            "{operation}: {result:?}"
        );
        assert!(host.calls.lock().unwrap().is_empty(), "{operation}");
        let reloaded = inner.load().unwrap();
        assert_eq!(
            reloaded.active_attempt(attempt_id).unwrap().phase(),
            DispatchAttemptPhase::Dispatching,
            "{operation} retains its durable recovery record after a failed transition"
        );
    }
}

#[test]
fn cleanup_persistence_failure_after_pane_removal_stops_before_later_resources() {
    let dir = temp_dir("cleanup-receipt-save-failure");
    let _guard = TempDir(dir.clone());
    let inner = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&inner);
    let start = RecoveryHost {
        fail_before: Some(Step::Agent),
        confirmed: true,
        ..RecoveryHost::default()
    };
    let attempt_id = match start_dispatch_attempt(
        inner.clone(),
        task_id,
        DispatchMode::NewWorktree,
        "grok",
        &start,
    ) {
        DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
        result => panic!("expected failed start: {result:?}"),
    };
    // This wrapper begins counting at cleanup: BeginCleanup persists first, then the receipt
    // removal save fails immediately after the host closes the pane.
    let store = FailingSaveStore::new(inner.clone(), 2);
    let host = RecoveryHost {
        confirmed: true,
        ..RecoveryHost::default()
    };

    let result = cleanup_dispatch_attempt(store, attempt_id, &host);

    assert!(
        matches!(result, DispatchRecoveryResult::PersistenceUncertain { .. }),
        "{result:?}"
    );
    assert_eq!(*host.calls.lock().unwrap(), ["remove-pane:pane-owned"]);
    let durable = inner.load().unwrap();
    let attempt = durable.active_attempt(attempt_id).unwrap();
    assert!(attempt
        .owned_resources()
        .iter()
        .any(|receipt| matches!(receipt, herdr_tasks::domain::OwnedResourceReceipt::Pane(_))));
    assert!(attempt.owned_resources().iter().any(|receipt| matches!(
        receipt,
        herdr_tasks::domain::OwnedResourceReceipt::Worktree(_)
    )));
}

#[test]
fn cleanup_retries_a_cleaning_attempt_after_receipt_removal_save_failure() {
    let dir = temp_dir("cleanup-retry-after-receipt-save-failure");
    let _guard = TempDir(dir.clone());
    let inner = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&inner);
    let start = RecoveryHost {
        fail_before: Some(Step::Agent),
        confirmed: true,
        ..RecoveryHost::default()
    };
    let attempt_id = match start_dispatch_attempt(
        inner.clone(),
        task_id,
        DispatchMode::NewWorktree,
        "grok",
        &start,
    ) {
        DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
        result => panic!("expected failed start: {result:?}"),
    };

    let failing_store = FailingSaveStore::new(inner.clone(), 2);
    assert!(matches!(
        cleanup_dispatch_attempt(
            failing_store,
            attempt_id,
            &RecoveryHost {
                confirmed: true,
                ..RecoveryHost::default()
            },
        ),
        DispatchRecoveryResult::PersistenceUncertain { .. }
    ));
    assert_eq!(
        inner
            .load()
            .expect("reload after failed receipt persistence")
            .active_attempt(attempt_id)
            .expect("recoverable attempt")
            .phase(),
        DispatchAttemptPhase::Cleaning
    );

    let retry_host = RecoveryHost {
        confirmed: false,
        record_recovery_checks: true,
        ..RecoveryHost::default()
    };
    assert!(matches!(
        cleanup_dispatch_attempt(inner.clone(), attempt_id, &retry_host),
        DispatchRecoveryResult::Cleaned { .. }
    ));
    assert_eq!(
        *retry_host.calls.lock().unwrap(),
        ["confirm-pane", "confirm-worktree"],
        "retry must resume narrowing the retained receipts rather than reject Cleaning"
    );
    assert!(inner
        .load()
        .expect("reload cleaned state")
        .active_attempt(attempt_id)
        .is_none());
}

#[test]
fn resume_worktree_uses_confirmed_receipt_path_without_recreating_worktree() {
    let dir = temp_dir("resume-worktree-receipt");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);
    let failed = RecoveryHost {
        fail_before: Some(Step::Pane),
        ..RecoveryHost::default()
    };
    let attempt_id = match start_dispatch_attempt(
        store.clone(),
        task_id,
        DispatchMode::NewWorktree,
        "grok",
        &failed,
    ) {
        DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
        result => panic!("expected failed start: {result:?}"),
    };
    let resumed = RecoveryHost {
        confirmed: true,
        ..RecoveryHost::default()
    };

    assert!(matches!(
        resume_dispatch_attempt(store, attempt_id, &resumed),
        DispatchRecoveryResult::Dispatched { .. }
    ));
    assert_eq!(
        *resumed.calls.lock().unwrap(),
        ["open:/checkout", "start:pane-owned", "prompt:pane-owned"]
    );
}

#[test]
fn resume_confirmation_error_stops_without_recreating_or_removing_resources() {
    let dir = temp_dir("resume-confirmation-error");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);
    let failed = RecoveryHost {
        fail_before: Some(Step::Agent),
        ..RecoveryHost::default()
    };
    let attempt_id =
        match start_dispatch_attempt(store.clone(), task_id, DispatchMode::Here, "grok", &failed) {
            DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
            result => panic!("expected failed start: {result:?}"),
        };
    let host = RecoveryHost {
        confirm_error: true,
        record_recovery_checks: true,
        ..RecoveryHost::default()
    };

    let result = resume_dispatch_attempt(store, attempt_id, &host);

    assert!(
        matches!(result, DispatchRecoveryResult::Error { ref message } if message.contains("could not confirm")),
        "{result:?}"
    );
    assert_eq!(*host.calls.lock().unwrap(), ["confirm-pane"]);
}

#[test]
fn cleanup_uses_only_owned_receipts_in_reverse_order_and_retains_failure() {
    let dir = temp_dir("cleanup");
    let _guard = TempDir(dir.clone());
    let store = TaskStore::new(&dir);
    let (task_id, _project) = seeded(&store);
    let start = RecoveryHost {
        fail_before: Some(Step::Agent),
        confirmed: true,
        ..RecoveryHost::default()
    };
    let attempt_id = match start_dispatch_attempt(
        store.clone(),
        task_id,
        DispatchMode::NewWorktree,
        "grok",
        &start,
    ) {
        DispatchRecoveryResult::Failed { attempt_id, .. } => attempt_id,
        other => panic!("expected failed start: {other:?}"),
    };

    let partial = RecoveryHost {
        confirmed: true,
        fail_remove_worktree: true,
        ..RecoveryHost::default()
    };
    assert!(matches!(
        cleanup_dispatch_attempt(store.clone(), attempt_id, &partial),
        DispatchRecoveryResult::Failed { .. }
    ));
    assert_eq!(
        *partial.calls.lock().unwrap(),
        vec!["remove-pane:pane-owned", "remove-worktree:workspace-owned"]
    );
    let loaded = store.load().unwrap();
    let attempt = loaded
        .active_attempt(attempt_id)
        .expect("remaining attempt");
    assert_eq!(attempt.phase(), DispatchAttemptPhase::Failed);
    assert_eq!(attempt.owned_resources().len(), 1);
    assert!(attempt.last_error().unwrap().contains("worktree"));

    let clean = RecoveryHost {
        confirmed: true,
        ..RecoveryHost::default()
    };
    assert!(matches!(
        cleanup_dispatch_attempt(store.clone(), attempt_id, &clean),
        DispatchRecoveryResult::Cleaned { .. }
    ));
    assert!(store.load().unwrap().active_attempt(attempt_id).is_none());
}

#[test]
fn persisted_dispatch_attempt_resurfaces_recovery_after_worker_error() {
    let mut domain = DomainState::new();
    let task_id = domain
        .create(
            "recover me",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let attempt_id = domain
        .start_dispatch_attempt(
            task_id,
            herdr_tasks::domain::DispatchAttemptMode::Here,
            "grok",
        )
        .expect("start durable attempt");
    let mut model = BoardModel::from_domain(&domain, None);
    assert_eq!(model.input_mode(), BoardInputMode::Recovery);

    model.close_popup();
    assert_ne!(model.input_mode(), BoardInputMode::Recovery);
    model.sync_from_domain(&domain);
    assert_ne!(
        model.input_mode(),
        BoardInputMode::Recovery,
        "idle revalidation must not reopen recovery the user dismissed"
    );

    apply_dispatch_recovery_result(
        &domain,
        &mut model,
        DispatchRecoveryResult::Error {
            message: "receipt confirmation failed".into(),
        },
    );

    assert_eq!(model.input_mode(), BoardInputMode::Recovery);
    assert_eq!(model.selected_id(), Some(task_id));
    assert!(model.message().is_some_and(|message| {
        message.contains("dispatch recovery failed") && message.contains("confirmation failed")
    }));
    assert!(domain.active_attempt(attempt_id).is_some());
}

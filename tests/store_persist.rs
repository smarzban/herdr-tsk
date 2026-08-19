//! Task Store: load/save survives reload.
//! Uses temp dirs only; never writes real plugin state.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use herdr_tasks::dispatch::DispatchRecoveryResult;
use herdr_tasks::domain::{
    AgentMeta, AgentReceipt, AgentSessionIdentity, ContextCapsule, DispatchAttemptError,
    DispatchAttemptMode, DispatchAttemptPhase, DispatchAttemptStep, DispatchAttemptStepState,
    DispatchAttemptTransition, DomainError, DomainState, HumanStatus, ObservedStatus,
    OwnedResourceReceipt, PaneReceipt, ProvenanceOrigin, TaskEvent, TaskEventKind, TaskScope,
    WorktreeReceipt,
};
use herdr_tasks::store::TaskStore;
use herdr_tasks::ui::board::{apply_dispatch_recovery_result, BoardInputMode, BoardModel};
use uuid::Uuid;

fn temp_state_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("herdr-tasks-store-persist-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("create temp state dir");
    dir
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_pre_stabilize_fixture_payload(state: &DomainState, task_id: Uuid, attempt_id: Uuid) {
    let task = state.get(task_id).expect("fixture task survives");
    assert_eq!(task.title, "Prepare V1 baseline");
    assert_eq!(
        task.notes.as_deref(),
        Some("Keep the interrupted dispatch recoverable.")
    );
    assert_eq!(
        task.scope,
        TaskScope::Project {
            path: "/work/herdr-tasks".into()
        }
    );
    assert_eq!(
        task.capsule,
        Some(ContextCapsule {
            repo_path: Some("/work/herdr-tasks".into()),
            worktree_path: Some("/work/herdr-tasks/.worktrees/baseline".into()),
            branch: Some("feat/v1-stabilize".into()),
            cwd: Some("/work/herdr-tasks/.worktrees/baseline".into()),
            source_pane_id: Some("workspace-1:pane-4".into()),
            selected_text: None,
            file: Some("docs/specs/v1-stabilize.md".into()),
            line: Some(243),
        })
    );
    assert_eq!(
        task.agent_meta,
        Some(AgentMeta {
            agent_id: Some("grok-baseline".into()),
            pane_id: Some("workspace-1:pane-4".into()),
            agent_session: Some(AgentSessionIdentity {
                source: "herdr:grok".into(),
                value: "baseline-session".into(),
            }),
        })
    );
    assert_eq!(task.last_observed, Some(ObservedStatus::Working));
    assert_eq!(task.provenance, ProvenanceOrigin::Capture);
    let expected_history = [
        TaskEvent {
            kind: TaskEventKind::Created,
            at: UNIX_EPOCH + Duration::from_secs(1_723_852_800),
        },
        TaskEvent {
            kind: TaskEventKind::Parked,
            at: UNIX_EPOCH + Duration::from_secs(1_723_852_860),
        },
    ];
    assert!(
        task.history.len() >= expected_history.len(),
        "fixture task must retain both history events"
    );
    assert_eq!(&task.history[..expected_history.len()], expected_history);

    let attempt = state
        .active_attempt(attempt_id)
        .expect("active dispatch attempt survives");
    assert_eq!(attempt.task_id(), task_id);
    assert_eq!(attempt.mode(), DispatchAttemptMode::NewWorktree);
    assert_eq!(attempt.kind(), "grok");
    assert_eq!(attempt.phase(), DispatchAttemptPhase::Failed);
    assert_eq!(
        attempt.steps(),
        [
            DispatchAttemptStepState {
                step: DispatchAttemptStep::CreateWorktree,
                completed: true,
            },
            DispatchAttemptStepState {
                step: DispatchAttemptStep::OpenPane,
                completed: true,
            },
            DispatchAttemptStepState {
                step: DispatchAttemptStep::StartAgent,
                completed: true,
            },
            DispatchAttemptStepState {
                step: DispatchAttemptStep::SendPrompt,
                completed: false,
            },
        ]
    );
    assert_eq!(
        attempt.owned_resources(),
        [
            OwnedResourceReceipt::Worktree(WorktreeReceipt {
                path: "/work/herdr-tasks/.worktrees/baseline".into(),
                workspace_id: "workspace-3".into(),
            }),
            OwnedResourceReceipt::Pane(PaneReceipt {
                pane_id: "workspace-3:pane-9".into(),
            }),
            OwnedResourceReceipt::Agent(AgentReceipt {
                pane_id: "workspace-3:pane-9".into(),
                display_name: Some("grok-baseline".into()),
                agent_session: Some(AgentSessionIdentity {
                    source: "herdr:grok".into(),
                    value: "baseline-session".into(),
                }),
            }),
        ]
    );
    assert_eq!(
        attempt.last_error(),
        Some("agent exited before the initial prompt")
    );
}

#[test]
fn pre_stabilize_fixture_survives_mutation_undo_and_dispatch_recovery_resurface() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pre_stabilize_tasks.json"),
        dir.join("tasks.json"),
    )
    .expect("install pre-stabilize fixture");
    let store = TaskStore::new(&dir);
    let task_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").expect("fixture task id");
    let attempt_id =
        Uuid::parse_str("33333333-3333-4333-8333-333333333333").expect("fixture attempt id");

    let mut state = store.load().expect("load pre-stabilize store");
    assert_pre_stabilize_fixture_payload(&state, task_id, attempt_id);
    assert_eq!(
        state.get(task_id).expect("fixture task").history.len(),
        2,
        "fixture starts with exactly its two durable history events"
    );

    state.complete(task_id).expect("mutate fixture task");
    store
        .reload_merge_save(&mut state)
        .expect("save fixture mutation");
    let mut reloaded = store.load().expect("reload saved fixture mutation");
    assert_pre_stabilize_fixture_payload(&reloaded, task_id, attempt_id);
    let reloaded_task = reloaded.get(task_id).expect("fixture task");
    assert_eq!(reloaded_task.status, HumanStatus::Done);
    assert_eq!(
        reloaded_task
            .history
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![
            TaskEventKind::Created,
            TaskEventKind::Parked,
            TaskEventKind::Completed,
        ]
    );

    reloaded.undo().expect("undo saved fixture mutation");
    store
        .reload_merge_save(&mut reloaded)
        .expect("save undone fixture mutation");
    let recovered_state = store.load().expect("reload undone fixture mutation");
    assert_pre_stabilize_fixture_payload(&recovered_state, task_id, attempt_id);
    let recovered_task = recovered_state.get(task_id).expect("fixture task");
    assert_eq!(recovered_task.status, HumanStatus::Ready);
    assert_eq!(
        recovered_task
            .history
            .iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![
            TaskEventKind::Created,
            TaskEventKind::Parked,
            TaskEventKind::Completed,
            TaskEventKind::Reopened,
        ]
    );

    let mut model = BoardModel::from_domain(&recovered_state, None);
    assert_eq!(model.input_mode(), BoardInputMode::Recovery);
    assert_eq!(model.selected_id(), Some(task_id));
    model.close_popup();
    model.sync_from_domain(&recovered_state);
    assert_ne!(
        model.input_mode(),
        BoardInputMode::Recovery,
        "an idle revalidation does not reopen a recovery the user dismissed"
    );
    apply_dispatch_recovery_result(
        &recovered_state,
        &mut model,
        DispatchRecoveryResult::Error {
            message: "host confirmation unavailable".into(),
        },
    );
    assert_eq!(model.input_mode(), BoardInputMode::Recovery);
    assert_pre_stabilize_fixture_payload(&recovered_state, task_id, attempt_id);
}

#[test]
fn save_then_load_preserves_task_id_title_status_and_events() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());

    let mut state = DomainState::new();
    let id = state
        .create(
            "Fix flake",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");

    {
        let store = TaskStore::new(&dir);
        store.save(&state).expect("save domain state");
    } // drop store

    let store = TaskStore::new(&dir);
    let loaded = store.load().expect("load after drop");
    let task = loaded.get(id).expect("task present after reload");
    assert_eq!(task.id, id);
    assert_eq!(task.title, "Fix flake");
    assert_eq!(task.status, HumanStatus::Ready);
    assert!(
        task.history
            .iter()
            .any(|e| e.kind == TaskEventKind::Created),
        "Created event must survive reload"
    );
}

#[test]
fn two_writers_merge_save_do_not_lose_tasks() {
    // Simulate: store has task A; second writer with B-only local merge-saves; both remain.
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut writer_a = DomainState::new();
    let id_a = writer_a
        .create(
            "Task A",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create A");
    store.save(&writer_a).expect("writer A save");

    let mut writer_b = DomainState::new();
    let id_b = writer_b
        .create(
            "Task B",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Capture,
        )
        .expect("create B");
    store
        .reload_merge_save(&mut writer_b)
        .expect("writer B merge-save");

    assert!(writer_b.get(id_a).is_some(), "A must survive merge");
    assert!(writer_b.get(id_b).is_some(), "B must survive merge");

    let loaded = store.load().expect("final load");
    assert_eq!(loaded.tasks().len(), 2);
    assert!(loaded.get(id_a).is_some());
    assert!(loaded.get(id_b).is_some());
}

#[test]
fn stale_undo_refuses_newer_task_revision_without_popping_newer_state() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut stale_writer = DomainState::new();
    let id = stale_writer
        .create(
            "Concurrent task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    stale_writer.complete(id).expect("complete task");
    store.save(&stale_writer).expect("save undoable action");
    // A successful durable save clears the transient merge intent on reload. The stale
    // writer now represents an unchanged board snapshot carrying its guarded Undo entry.
    stale_writer = store.load().expect("reload persisted undoable action");

    let mut newer_writer = store.load().expect("load concurrent writer");
    std::thread::sleep(std::time::Duration::from_millis(5));
    newer_writer
        .edit(
            id,
            "Changed concurrently",
            Some("newer notes".into()),
            TaskScope::Global,
        )
        .expect("newer semantic mutation");
    store.save(&newer_writer).expect("save newer task revision");

    store
        .reload_merge_save(&mut stale_writer)
        .expect("merge newer durable state");
    let newer_task = stale_writer.get(id).expect("merged task").clone();

    let error = stale_writer.undo().expect_err("stale undo must be refused");
    assert!(error.to_string().contains("changed since"));
    assert_eq!(stale_writer.get(id), Some(&newer_task));

    let repeated = stale_writer
        .undo()
        .expect_err("refused undo entry remains guarded");
    assert_eq!(repeated, error);
    assert_eq!(stale_writer.get(id), Some(&newer_task));
}

#[test]
fn legacy_serialized_tasks_and_undo_entries_decode_safely() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut state = DomainState::new();
    let id = state
        .create(
            "Legacy task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    state.complete(id).expect("create legacy undo entry");

    let mut legacy = serde_json::to_value(&state).expect("serialize state");
    legacy["tasks"][0]
        .as_object_mut()
        .expect("task object")
        .remove("revision");
    let undo = legacy["undo_stack"][0]
        .as_object_mut()
        .expect("undo enum object")
        .values_mut()
        .next()
        .and_then(serde_json::Value::as_object_mut)
        .expect("undo fields");
    undo.remove("expected_revision");
    fs::write(
        dir.join("tasks.json"),
        serde_json::to_string_pretty(&legacy).expect("encode legacy state"),
    )
    .expect("write legacy state");

    let mut loaded = store.load().expect("legacy record decodes");
    let before = loaded.get(id).expect("legacy task present").clone();
    assert_eq!(before.revision, None);
    let error = loaded
        .undo()
        .expect_err("legacy undo without revision is refused safely");
    assert!(error.to_string().contains("changed since"));
    assert_eq!(loaded.get(id), Some(&before));
}

#[test]
fn legacy_serialized_domain_without_active_attempts_round_trips() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut state = DomainState::new();
    state
        .create(
            "Legacy dispatch record",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let mut legacy = serde_json::to_value(&state).expect("serialize legacy state");
    legacy
        .as_object_mut()
        .expect("domain object")
        .remove("active_attempts");
    fs::write(
        dir.join("tasks.json"),
        serde_json::to_string_pretty(&legacy).expect("encode legacy state"),
    )
    .expect("write legacy state");

    let loaded = store.load().expect("legacy state decodes");
    assert_eq!(
        serde_json::to_value(&loaded).expect("reserialize loaded state")["active_attempts"],
        serde_json::json!([]),
        "legacy records gain an empty active-attempt collection"
    );
}

#[test]
fn active_attempt_with_worktree_and_pane_receipts_round_trips() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut state = DomainState::new();
    let task_id = state
        .create(
            "Recover dispatch",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let attempt_id = state
        .start_dispatch_attempt(task_id, DispatchAttemptMode::NewWorktree, "codex")
        .expect("start durable attempt");
    let revision = state
        .active_attempt(attempt_id)
        .expect("attempt")
        .revision();
    let worktree = OwnedResourceReceipt::Worktree(WorktreeReceipt {
        path: "/repos/app/.worktrees/recover".into(),
        workspace_id: "workspace-17".into(),
    });
    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::RecordReceipt(worktree.clone()),
        )
        .expect("record exact worktree receipt");
    let revision = state
        .active_attempt(attempt_id)
        .expect("attempt")
        .revision();
    let pane = OwnedResourceReceipt::Pane(PaneReceipt {
        pane_id: "w0:p17".into(),
    });
    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::RecordReceipt(pane.clone()),
        )
        .expect("record exact pane receipt");

    store.save(&state).expect("save active attempt");
    let loaded = store.load().expect("reload active attempt");
    let attempt = loaded.active_attempt(attempt_id).expect("attempt survives");
    assert_eq!(attempt.task_id(), task_id);
    assert_eq!(attempt.mode(), DispatchAttemptMode::NewWorktree);
    assert_eq!(attempt.kind(), "codex");
    assert_eq!(attempt.phase(), DispatchAttemptPhase::Dispatching);
    assert_eq!(attempt.owned_resources(), &[worktree, pane]);
}

#[test]
fn dispatch_attempt_refuses_out_of_order_and_stale_transitions() {
    let mut state = DomainState::new();
    let task_id = state
        .create(
            "Order dispatch",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let attempt_id = state
        .start_dispatch_attempt(task_id, DispatchAttemptMode::NewWorktree, "grok")
        .expect("start attempt");
    let before = state.active_attempt(attempt_id).expect("attempt").clone();
    let revision = before.revision();
    let pane = OwnedResourceReceipt::Pane(PaneReceipt {
        pane_id: "w0:p17".into(),
    });

    let out_of_order = state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::RecordReceipt(pane),
        )
        .expect_err("pane cannot precede required worktree receipt");
    assert!(matches!(
        out_of_order,
        DomainError::DispatchAttempt(DispatchAttemptError::OutOfOrder { .. })
    ));
    assert_eq!(state.active_attempt(attempt_id), Some(&before));

    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::RecordReceipt(OwnedResourceReceipt::Worktree(
                WorktreeReceipt {
                    path: "/repos/app/.worktrees/recover".into(),
                    workspace_id: "workspace-17".into(),
                },
            )),
        )
        .expect("current transition succeeds");
    let after_current = state.active_attempt(attempt_id).expect("attempt").clone();
    let stale = state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::Fail {
                message: "late writer".into(),
            },
        )
        .expect_err("old revision must be refused");
    assert!(matches!(
        stale,
        DomainError::DispatchAttempt(DispatchAttemptError::StaleRevision { .. })
    ));
    assert_eq!(state.active_attempt(attempt_id), Some(&after_current));

    state
        .transition_dispatch_attempt(
            attempt_id,
            after_current.revision(),
            DispatchAttemptTransition::Fail {
                message: "first failure".into(),
            },
        )
        .expect("first failure is recorded");
    let failed = state.active_attempt(attempt_id).expect("attempt").clone();
    let duplicate = state
        .transition_dispatch_attempt(
            attempt_id,
            failed.revision(),
            DispatchAttemptTransition::Fail {
                message: "duplicate failure".into(),
            },
        )
        .expect_err("duplicate transition is refused");
    assert!(matches!(
        duplicate,
        DomainError::DispatchAttempt(DispatchAttemptError::InvalidPhase { .. })
    ));
    assert_eq!(state.active_attempt(attempt_id), Some(&failed));
}

#[test]
fn cleanup_refuses_to_remove_an_unowned_receipt() {
    let mut state = DomainState::new();
    let task_id = state
        .create(
            "Owned only",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let attempt_id = state
        .start_dispatch_attempt(task_id, DispatchAttemptMode::Here, "grok")
        .expect("start attempt");
    let revision = state
        .active_attempt(attempt_id)
        .expect("attempt")
        .revision();
    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::RecordReceipt(OwnedResourceReceipt::Pane(PaneReceipt {
                pane_id: "w0:p-owned".into(),
            })),
        )
        .expect("record owned pane");
    let revision = state
        .active_attempt(attempt_id)
        .expect("attempt")
        .revision();
    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::Fail {
                message: "agent failed".into(),
            },
        )
        .expect("record failure");
    let revision = state
        .active_attempt(attempt_id)
        .expect("attempt")
        .revision();
    state
        .transition_dispatch_attempt(
            attempt_id,
            revision,
            DispatchAttemptTransition::BeginCleanup,
        )
        .expect("begin explicit cleanup");
    let before = state.active_attempt(attempt_id).expect("attempt").clone();

    let error = state
        .transition_dispatch_attempt(
            attempt_id,
            before.revision(),
            DispatchAttemptTransition::RemoveOwnedReceipt(OwnedResourceReceipt::Pane(
                PaneReceipt {
                    pane_id: "w0:p-not-owned".into(),
                },
            )),
        )
        .expect_err("cleanup cannot invent ownership");
    assert!(matches!(
        error,
        DomainError::DispatchAttempt(DispatchAttemptError::UnownedReceipt)
    ));
    assert_eq!(state.active_attempt(attempt_id), Some(&before));
}

#[test]
fn reload_merge_save_merges_attempt_revisions_without_dropping_unrelated_attempts() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut base = DomainState::new();
    let task_a = base
        .create(
            "Attempt A",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create A");
    let task_b = base
        .create(
            "Attempt B",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create B");
    let attempt_a = base
        .start_dispatch_attempt(task_a, DispatchAttemptMode::Here, "grok")
        .expect("start A");
    store.save(&base).expect("seed attempt A");

    let mut local = store.load().expect("stale local load");
    let attempt_b = local
        .start_dispatch_attempt(task_b, DispatchAttemptMode::Here, "codex")
        .expect("local start B");
    // Attempt creation always passes through the durable locked transition in production. Seed
    // that persisted sibling before exercising the later stale board merge-save.
    store.save(&local).expect("persist attempt B");

    let mut disk = store.load().expect("disk writer load");
    let revision = disk
        .active_attempt(attempt_a)
        .expect("attempt A")
        .revision();
    std::thread::sleep(std::time::Duration::from_millis(5));
    disk.transition_dispatch_attempt(
        attempt_a,
        revision,
        DispatchAttemptTransition::Fail {
            message: "disk-side failure".into(),
        },
    )
    .expect("advance disk attempt A");
    store.save(&disk).expect("save newer attempt A");

    store.reload_merge_save(&mut local).expect("merge attempts");
    assert_eq!(
        local
            .active_attempt(attempt_a)
            .expect("merged A")
            .last_error(),
        Some("disk-side failure")
    );
    assert!(
        local.active_attempt(attempt_b).is_some(),
        "unrelated local attempt must survive merge"
    );
    let reloaded = store.load().expect("reload merge result");
    assert!(reloaded.active_attempt(attempt_a).is_some());
    assert!(reloaded.active_attempt(attempt_b).is_some());
}

#[test]
fn reload_merge_save_does_not_resurrect_attempt_removed_from_disk() {
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut seed = DomainState::new();
    let task_id = seed
        .create(
            "Attempt owner",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let attempt_id = seed
        .start_dispatch_attempt(task_id, DispatchAttemptMode::Here, "grok")
        .expect("create attempt");
    store.save(&seed).expect("seed");

    let mut stale_local = store.load().expect("stale board state");
    stale_local
        .set_status(task_id, HumanStatus::Started)
        .expect("stage unrelated board mutation");
    let mut disk = store.load().expect("dispatch worker state");
    let revision = disk.active_attempt(attempt_id).expect("attempt").revision();
    disk.remove_dispatch_attempt(attempt_id, revision)
        .expect("worker completed cleanup");
    store.save(&disk).expect("persist removed attempt");

    store
        .reload_merge_save(&mut stale_local)
        .expect("merge unrelated board mutation");

    assert!(
        stale_local.active_attempt(attempt_id).is_none(),
        "the stale board copy must discard an attempt removed from disk"
    );
    let reloaded = store.load().expect("reload");
    assert!(
        reloaded.active_attempt(attempt_id).is_none(),
        "merge-save must not write the removed attempt back"
    );
    assert_eq!(
        reloaded.get(task_id).expect("task").status,
        HumanStatus::Started
    );
}

#[test]
fn park_then_reload_merge_save_preserves_capsule_and_agent_meta() {
    // successful park is observed after load from the same state location.
    let dir = temp_state_dir();
    let _guard = TempDirGuard(dir.clone());
    let store = TaskStore::new(&dir);

    let mut state = DomainState::new();
    let id = state
        .create(
            "Park me",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    state
        .set_status(id, HumanStatus::Started)
        .expect("status before park");

    let capsule = ContextCapsule {
        repo_path: Some("/repos/app".into()),
        worktree_path: Some("/repos/app/.worktrees/f2".into()),
        branch: Some("feat/park-resume".into()),
        cwd: Some("/repos/app/.worktrees/f2".into()),
        source_pane_id: Some("w0:p7".into()),
        selected_text: None,
        file: Some("src/domain/task.rs".into()),
        line: Some(100),
    };
    let meta = AgentMeta {
        agent_id: Some("agent-park".into()),
        pane_id: Some("w0:p7".into()),
        agent_session: Some(AgentSessionIdentity {
            source: "herdr:pi".into(),
            value: "session-store".into(),
        }),
    };
    state
        .park(id, Some(capsule.clone()), Some(meta.clone()))
        .expect("park");

    store
        .reload_merge_save(&mut state)
        .expect("persist park via merge-save");

    let loaded = store.load().expect("load after park");
    let task = loaded.get(id).expect("task present");
    assert_eq!(
        task.status,
        HumanStatus::Started,
        "park must not change status"
    );
    assert_eq!(task.capsule.as_ref(), Some(&capsule));
    assert_eq!(task.agent_meta.as_ref(), Some(&meta));
    assert!(
        task.history.iter().any(|e| e.kind == TaskEventKind::Parked),
        "Parked event must survive reload"
    );
}

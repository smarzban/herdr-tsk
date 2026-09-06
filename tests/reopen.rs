use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsk_tui::context::InvocationSnapshot;
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::reopen::{ReopenRequest, ReopenWatch};
use tsk_tui::store::TaskStore;
use tsk_tui::ui::board::{apply_intent, BoardModel};
use tsk_tui::ui::input::BoardIntent;

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn state_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsk-reopen-test-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("state dir");
    dir
}

#[test]
fn reopen_watch_ignores_stale_request_seeded_before_board_start() {
    let dir = state_dir();
    let store = TaskStore::new(&dir);
    ReopenRequest::new(Some(PathBuf::from("/repo/old")))
        .write(&dir)
        .expect("write request");
    let mut watch = ReopenWatch::seeded(&store);
    assert_eq!(watch.poll(), None);
    assert!(
        !dir.join("reopen.json").exists(),
        "stale request is retired"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn reopen_watch_reads_one_atomic_request_and_acknowledges_it_once() {
    let dir = state_dir();
    let store = TaskStore::new(&dir);
    let mut watch = ReopenWatch::seeded(&store);
    ReopenRequest::new(Some(PathBuf::from("/repo/new")))
        .write(&dir)
        .expect("write request");
    let request = watch.poll().expect("request");
    assert_eq!(
        request.project.as_deref(),
        Some(std::path::Path::new("/repo/new"))
    );
    assert_eq!(watch.poll(), Some(request.clone()));
    watch.acknowledge();
    assert_eq!(watch.poll(), None);
    assert!(!dir.join("reopen.json").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn resolve_context_rejects_malformed_json_without_publishing_a_request() {
    let dir = state_dir();
    let binary = env!("CARGO_BIN_EXE_tsk");
    let mut child = Command::new(binary)
        .arg("--resolve-context")
        .env("TSK_STATE_DIR", &dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("resolve context");
    use std::io::Write;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"not json")
        .expect("context");
    let output = child.wait_with_output().expect("output");
    assert!(!output.status.success());
    assert!(!dir.join("reopen.json").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn resolve_context_publishes_project_and_desk_requests_without_using_process_cwd() {
    let dir = state_dir();
    let repo = dir.join("repo");
    fs::create_dir_all(repo.join(".git")).expect("repo");
    let binary = env!("CARGO_BIN_EXE_tsk");
    let mut child = Command::new(binary)
        .arg("--resolve-context")
        .env("TSK_STATE_DIR", &dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("resolve context");
    use std::io::Write;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(
            format!(
                r#"{{"focused_pane_cwd":{}}}"#,
                serde_json::to_string(&repo).unwrap()
            )
            .as_bytes(),
        )
        .expect("context");
    let output = child.wait_with_output().expect("output");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        repo.display().to_string()
    );
    let request = fs::read_to_string(dir.join("reopen.json")).expect("request");
    assert!(request.contains(&repo.display().to_string()));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn board_reopen_updates_quick_add_defaults_for_project_and_desk() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repo/a")));
    let snapshot = InvocationSnapshot {
        default_scope: TaskScope::Project {
            path: "/repo/a".into(),
        },
        this_repo: Some(PathBuf::from("/repo/a")),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
    };
    assert!(model.apply_reopen_project(Some(PathBuf::from("/repo/b"))));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snapshot),
    )
    .expect("open add");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddInsertText("project b".into()),
        Some(&snapshot),
    )
    .expect("type");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddSave,
        Some(&snapshot),
    )
    .expect("save");
    model.sync_from_domain(&domain);
    assert_eq!(
        domain.tasks()[0].scope,
        TaskScope::Project {
            path: "/repo/b".into()
        }
    );

    assert!(model.apply_reopen_project(None));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snapshot),
    )
    .expect("open add");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddInsertText("desk after reopen".into()),
        Some(&snapshot),
    )
    .expect("type");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddSave,
        Some(&snapshot),
    )
    .expect("save");
    model.sync_from_domain(&domain);
    assert_eq!(domain.tasks()[1].scope, TaskScope::Global);
}

#[test]
fn reopen_keeps_an_earlier_archived_launch_override_on_desk_capture() {
    let mut domain = DomainState::new();
    domain
        .create(
            "archived task",
            None,
            TaskScope::Project {
                path: "/repo/a".into(),
            },
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("task");
    domain.archive_project("/repo/a").expect("archive");
    let snapshot = InvocationSnapshot {
        default_scope: TaskScope::Project {
            path: "/repo/a".into(),
        },
        this_repo: Some(PathBuf::from("/repo/a")),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
    };
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repo/a")));
    assert!(model.offer_launch_card(&domain, &snapshot));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::LaunchKeepArchived,
        Some(&snapshot),
    )
    .expect("keep archived");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCapture,
        Some(&snapshot),
    )
    .expect("open add");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddInsertText("desk task".into()),
        Some(&snapshot),
    )
    .expect("type");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddSave,
        Some(&snapshot),
    )
    .expect("save");
    model.sync_from_domain(&domain);
    assert_eq!(
        domain.tasks().last().expect("desk task").scope,
        TaskScope::Global
    );
}

#[test]
fn board_reopen_switches_project_or_desk_and_clears_local_filter() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "project task",
            None,
            TaskScope::Project {
                path: "/repo/a".into(),
            },
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("task");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repo/a")));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenThreadFilterPicker,
        None,
    )
    .expect("thread picker");
    apply_intent(&mut domain, &mut model, BoardIntent::ListPickerNext, None).expect("thread");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmListPicker,
        None,
    )
    .expect("filter");
    assert!(model.apply_reopen_project(Some(PathBuf::from("/repo/b"))));
    assert_eq!(
        model.selected_project(),
        Some(std::path::Path::new("/repo/b"))
    );
    assert_eq!(
        model.thread_filter(),
        &tsk_tui::ui::queue::ThreadFilter::All
    );
    assert_ne!(
        model.selected_id(),
        Some(id),
        "selection cannot pin an invisible task"
    );
    assert!(model.apply_reopen_project(None));
    assert!(model.selected_project().is_none());
}

#[test]
fn board_reopen_dismisses_clean_page_picker_search_and_wide_stage() {
    let mut domain = DomainState::new();
    domain
        .create(
            "task",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("page");
    for _ in 0..3 {
        apply_intent(&mut domain, &mut model, BoardIntent::StageRight, None).expect("stage");
    }
    assert_ne!(model.wide_stage(), tsk_tui::ui::tier::WideStage::FullBoard);
    assert!(model.apply_reopen_project(Some(PathBuf::from("/repo/new"))));
    assert_eq!(
        model.input_mode(),
        tsk_tui::ui::board::BoardInputMode::Normal
    );
    assert_eq!(model.wide_stage(), tsk_tui::ui::tier::WideStage::FullBoard);
    assert!(model.selected_project().is_some());
}

#[test]
fn board_reopen_defers_while_dirty_editor_is_open() {
    let mut domain = DomainState::new();
    domain
        .create(
            "task",
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditTitle, None).expect("edit");
    apply_intent(&mut domain, &mut model, BoardIntent::EditInsert('x'), None).expect("type");
    assert!(model.has_unsaved_work());
    assert!(!model.apply_reopen_project(Some(PathBuf::from("/repo/new"))));
    assert!(model.selected_project().is_none());
    assert!(model
        .message()
        .is_some_and(|message| message.contains("save or cancel")));
}

#[test]
fn reopen_request_rejects_oversized_or_symlinked_state_payload() {
    let dir = state_dir();
    let too_long = "x".repeat(5000);
    assert!(ReopenRequest::new(Some(PathBuf::from(too_long)))
        .write(&dir)
        .is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let target = dir.join("target");
        fs::write(&target, b"safe").expect("target");
        symlink(&target, dir.join("reopen.json")).expect("symlink");
        assert!(ReopenRequest::new(Some(PathBuf::from("/repo/new")))
            .write(&dir)
            .is_ok());
        assert_eq!(fs::read(&target).expect("target bytes"), b"safe");
    }
    let _ = fs::remove_dir_all(dir);
}

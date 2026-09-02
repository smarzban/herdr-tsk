//! BoardModel cutover onto queue query, selection anchor, and session-only state.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tsk_tui::domain::{
    DomainState, HumanStatus, ProvenanceOrigin, Task, TaskEvent, TaskEventKind, TaskScope,
};
use tsk_tui::store::TaskStore;
use tsk_tui::ui::board::{apply_intent, draw_board, BoardModel, ProjectScopeOption};
use tsk_tui::ui::input::BoardIntent;
use tsk_tui::ui::queue::{query_lens, BoardLens, BoardTab, SectionKind};
use uuid::Uuid;

const THIS_REPO: &str = "/repos/app";

fn epoch_plus(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn task(id: u128, title: &str, status: HumanStatus, scope: TaskScope, updated_secs: u64) -> Task {
    let at = epoch_plus(updated_secs);
    Task {
        id: Uuid::from_u128(id),
        number: None,
        revision: Uuid::from_u128(id),
        merge_base_revision: None,
        title: title.into(),
        notes: None,
        thread: None,
        status,
        scope,
        provenance: ProvenanceOrigin::Manual,
        history: vec![TaskEvent {
            kind: TaskEventKind::Created,
            at,
        }],
        steps: Vec::new(),
        soft_deleted: false,
        created_at: at,
        updated_at: at,
    }
}

fn project(path: &str) -> TaskScope {
    TaskScope::Project {
        path: path.to_string(),
    }
}

fn threaded_task(
    id: u128,
    title: &str,
    status: HumanStatus,
    scope: TaskScope,
    updated_secs: u64,
    thread: &str,
) -> Task {
    let mut task = task(id, title, status, scope, updated_secs);
    task.thread = Some(thread.to_string());
    task
}

fn on_deck(view: &tsk_tui::ui::queue::QueueView) -> &tsk_tui::ui::queue::QueueSection {
    view.sections
        .iter()
        .find(|section| section.kind == SectionKind::OnDeck)
        .expect("ON DECK section")
}

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-queue-board-model-{tag}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

struct TempGuard(PathBuf);
impl Drop for TempGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn list_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// First open: pin selection on the first IN MOTION row, else the first ON DECK row.
#[test]
fn from_domain_seeds_selection_on_first_in_motion_else_first_deck_row() {
    // Case A: IN MOTION present → first motion id (updated_at desc).
    let motion_newer = task(
        1,
        "motion-new",
        HumanStatus::Started,
        project(THIS_REPO),
        100,
    );
    let motion_older = task(
        2,
        "motion-old",
        HumanStatus::Started,
        project(THIS_REPO),
        50,
    );
    let deck = task(3, "deck", HumanStatus::Ready, project(THIS_REPO), 200);
    let tasks = vec![motion_newer.clone(), motion_older.clone(), deck.clone()];
    let model = BoardModel::from_tasks(tasks.clone(), Some(PathBuf::from(THIS_REPO)));
    let view = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Home(BoardTab::Desk),
        false,
    );
    let first_motion = view
        .sections
        .iter()
        .find(|s| s.kind == SectionKind::InMotion)
        .and_then(|s| s.task_ids.first().copied());
    assert_eq!(first_motion, Some(Uuid::from_u128(1)));
    assert_eq!(
        model.selected_id(),
        first_motion,
        "open seeds the first IN MOTION row"
    );

    // from_domain shares the same seed rule.
    let mut domain = DomainState::new();
    let id_new = domain
        .create(
            "motion-new",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    domain.set_status(id_new, HumanStatus::Started).unwrap();
    let id_old = domain
        .create(
            "motion-old",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    domain.set_status(id_old, HumanStatus::Started).unwrap();
    let _deck = domain
        .create(
            "deck",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    // Domain create stamps wall-clock times; seed still lands on some IN MOTION id.
    let from_domain = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    let selected = from_domain.selected_id().expect("selection");
    assert_eq!(
        domain.get(selected).unwrap().status,
        HumanStatus::Started,
        "from_domain seeds an IN MOTION task when any exist"
    );

    // Case B: desk tab with only project ON DECK rows opens on Projects instead.
    let deck_a = task(10, "a", HumanStatus::Ready, project("/repos/a"), 30);
    let deck_b = task(11, "b", HumanStatus::Ready, project(THIS_REPO), 40);
    let deck_tasks = vec![deck_a.clone(), deck_b.clone()];
    let model = BoardModel::from_tasks(deck_tasks.clone(), Some(PathBuf::from(THIS_REPO)));
    assert_eq!(model.home_tab(), BoardTab::Projects);
    assert_eq!(
        model.selected_id(),
        Some(Uuid::from_u128(11)),
        "project-only boards open on Projects and seed the first ON DECK row"
    );
    let view = query_lens(
        &deck_tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Home(BoardTab::Projects),
        false,
    );
    let first_deck = view
        .sections
        .iter()
        .filter(|s| s.kind == SectionKind::OnDeck)
        .flat_map(|s| s.task_ids.iter().copied())
        .next();
    assert_eq!(first_deck, Some(Uuid::from_u128(11)));

    // Case C: empty → no selection.
    let empty = BoardModel::from_tasks(vec![], Some(PathBuf::from(THIS_REPO)));
    assert_eq!(empty.selected_id(), None);
}

/// Confirming a project selector choice narrows ON DECK to that project.
#[test]
fn confirming_project_choice_changes_visible_queue_sections() {
    let mut domain = DomainState::new();
    let app_id = domain
        .create(
            "app task",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    let other_id = domain
        .create(
            "other task",
            None,
            project("/repos/other"),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    assert_eq!(model.visible_ids(), vec![app_id, other_id]);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
    )
    .unwrap();
    // Picker highlights the current deck scope (All at open); step to /repos/other.
    let other_idx = model
        .project_options()
        .iter()
        .position(|opt| opt == &ProjectScopeOption::Project(PathBuf::from("/repos/other")))
        .expect("/repos/other option");
    for _ in 0..model.project_options().len() {
        if model.project_picker_index() == Some(other_idx) {
            break;
        }
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::ProjectPickerNext,
            None,
        )
        .unwrap();
    }
    assert_eq!(model.project_picker_index(), Some(other_idx));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ConfirmProjectChoice,
        None,
    )
    .unwrap();

    assert_eq!(model.selected_project(), Some(Path::new("/repos/other")));
    assert_eq!(model.visible_ids(), vec![other_id]);
}

/// After a domain sync, selection stays on the same id when it remains visible.
#[test]
fn sync_from_domain_reanchors_by_id() {
    let mut domain = DomainState::new();
    let id_doing = domain
        .create(
            "doing",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    domain.set_status(id_doing, HumanStatus::Started).unwrap();
    let id_todo = domain
        .create(
            "todo",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();

    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectHomeTab(BoardTab::Projects),
        None,
    )
    .unwrap();
    let visible = model.visible_ids();
    assert!(visible.contains(&id_doing));
    assert!(visible.contains(&id_todo));
    let todo_idx = visible.iter().position(|&id| id == id_todo).unwrap();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(todo_idx),
        None,
    )
    .unwrap();
    assert_eq!(model.selected_id(), Some(id_todo));

    // Sync with the same snapshot: selection stays on todo by id.
    model.sync_from_domain(&domain);
    assert_eq!(
        model.selected_id(),
        Some(id_todo),
        "reanchor keeps the same id when still visible"
    );

    // Soft-delete the selected task; reanchor must leave todo and land on a survivor.
    domain.soft_delete(id_todo).unwrap();
    model.sync_from_domain(&domain);
    assert_eq!(
        model.selected_id(),
        Some(id_doing),
        "when the pinned id leaves the visible set, reanchor lands on a survivor"
    );
    assert!(!model.visible_ids().contains(&id_todo));
}

#[test]
fn deck_group_emits_thread_blocks_with_open_counts_iff_open_tasks() {
    let tasks = vec![
        threaded_task(
            1,
            "open",
            HumanStatus::Ready,
            project(THIS_REPO),
            20,
            "Release",
        ),
        threaded_task(
            2,
            "done",
            HumanStatus::Done,
            project(THIS_REPO),
            30,
            "release",
        ),
        threaded_task(
            3,
            "doing",
            HumanStatus::Started,
            project(THIS_REPO),
            40,
            "release",
        ),
    ];

    let view = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Project(Path::new(THIS_REPO)),
        true,
    );
    let deck = on_deck(&view);

    assert_eq!(deck.thread_blocks.len(), 1);
    assert_eq!(deck.thread_blocks[0].name, "release");
    assert_eq!(deck.thread_blocks[0].open_count, 1);
    assert_eq!(deck.thread_blocks[0].task_ids, vec![Uuid::from_u128(1)]);
}

#[test]
fn unthreaded_tasks_list_after_thread_blocks() {
    let tasks = vec![
        task(
            1,
            "loose newest",
            HumanStatus::Ready,
            project(THIS_REPO),
            30,
        ),
        threaded_task(
            2,
            "threaded",
            HumanStatus::Ready,
            project(THIS_REPO),
            20,
            "release",
        ),
        task(
            3,
            "loose older",
            HumanStatus::Blocked,
            project(THIS_REPO),
            10,
        ),
    ];

    let view = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Project(Path::new(THIS_REPO)),
        false,
    );
    let deck = on_deck(&view);

    assert_eq!(deck.thread_blocks[0].task_ids, vec![Uuid::from_u128(2)]);
    assert_eq!(
        deck.loose_task_ids,
        vec![Uuid::from_u128(1), Uuid::from_u128(3)]
    );
    assert_eq!(
        deck.task_ids,
        vec![Uuid::from_u128(2), Uuid::from_u128(1), Uuid::from_u128(3)]
    );
}

#[test]
fn thread_blocks_order_by_recency_and_tasks_within_by_updated_desc() {
    let tasks = vec![
        threaded_task(
            1,
            "alpha older",
            HumanStatus::Ready,
            project(THIS_REPO),
            20,
            "alpha",
        ),
        threaded_task(
            2,
            "alpha newer",
            HumanStatus::Blocked,
            project(THIS_REPO),
            50,
            "alpha",
        ),
        threaded_task(
            3,
            "beta",
            HumanStatus::Review,
            project(THIS_REPO),
            40,
            "beta",
        ),
    ];

    let view = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Project(Path::new(THIS_REPO)),
        false,
    );
    let deck = on_deck(&view);

    assert_eq!(
        deck.thread_blocks
            .iter()
            .map(|block| block.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(
        deck.thread_blocks[0].task_ids,
        vec![Uuid::from_u128(2), Uuid::from_u128(1)]
    );
}

#[test]
fn flat_task_ids_equal_block_then_loose_concatenation() {
    let tasks = vec![
        threaded_task(
            1,
            "alpha",
            HumanStatus::Ready,
            project(THIS_REPO),
            10,
            "alpha",
        ),
        threaded_task(
            2,
            "beta",
            HumanStatus::Ready,
            project(THIS_REPO),
            30,
            "beta",
        ),
        task(3, "loose", HumanStatus::Ready, project(THIS_REPO), 40),
    ];

    let view = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Project(Path::new(THIS_REPO)),
        false,
    );
    let deck = on_deck(&view);
    let expected: Vec<_> = deck
        .thread_blocks
        .iter()
        .flat_map(|block| block.task_ids.iter().copied())
        .chain(deck.loose_task_ids.iter().copied())
        .collect();

    assert_eq!(deck.task_ids, expected);
}

#[test]
fn same_thread_name_in_two_scopes_forms_independent_groups() {
    let tasks = vec![
        threaded_task(
            1,
            "project",
            HumanStatus::Ready,
            project(THIS_REPO),
            20,
            "release",
        ),
        threaded_task(
            2,
            "global",
            HumanStatus::Ready,
            TaskScope::Global,
            30,
            "release",
        ),
    ];

    let project_view = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Project(Path::new(THIS_REPO)),
        false,
    );
    let global_view = query_lens(&tasks, None, BoardLens::Home(BoardTab::Desk), false);
    let project_deck = on_deck(&project_view);
    let global_deck = on_deck(&global_view);

    assert_eq!(
        project_deck.thread_blocks[0].task_ids,
        vec![Uuid::from_u128(1)]
    );
    assert_eq!(
        global_deck.thread_blocks[0].task_ids,
        vec![Uuid::from_u128(2)]
    );
}

#[test]
fn all_scope_in_motion_and_drawer_emit_no_blocks() {
    let tasks = vec![
        threaded_task(
            1,
            "deck",
            HumanStatus::Ready,
            project(THIS_REPO),
            10,
            "release",
        ),
        threaded_task(
            2,
            "motion",
            HumanStatus::Started,
            project(THIS_REPO),
            20,
            "release",
        ),
        threaded_task(
            3,
            "done",
            HumanStatus::Done,
            project(THIS_REPO),
            30,
            "release",
        ),
    ];

    let home_projects = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Home(BoardTab::Projects),
        true,
    );
    assert!(home_projects
        .sections
        .iter()
        .all(|section| section.thread_blocks.is_empty() && section.loose_task_ids.is_empty()));

    let scoped = query_lens(
        &tasks,
        Some(Path::new(THIS_REPO)),
        BoardLens::Project(Path::new(THIS_REPO)),
        true,
    );
    assert!(scoped
        .sections
        .iter()
        .filter(|section| section.kind != SectionKind::OnDeck)
        .all(|section| section.thread_blocks.is_empty() && section.loose_task_ids.is_empty()));
}

#[test]
fn selection_stays_on_task_id_across_thread_block_reorder() {
    let mut domain = DomainState::new();
    let alpha = domain
        .create(
            "alpha",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            Some("alpha".into()),
        )
        .unwrap();
    let beta = domain
        .create(
            "beta",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            Some("beta".into()),
        )
        .unwrap();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    model.set_selected_project(Some(PathBuf::from(THIS_REPO)));

    let before = model.queue_view();
    let before_deck = on_deck(&before);
    assert_eq!(before_deck.thread_blocks.len(), 2);
    assert_eq!(
        before_deck
            .thread_blocks
            .iter()
            .map(|block| block.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta", "alpha"]
    );
    assert_eq!(before_deck.task_ids, vec![beta, alpha]);

    let beta_index = model
        .visible_ids()
        .iter()
        .position(|id| *id == beta)
        .expect("beta is visible");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(beta_index),
        None,
    )
    .unwrap();

    domain
        .edit(
            alpha,
            "alpha changed",
            None,
            project(THIS_REPO),
            Some("alpha".into()),
        )
        .unwrap();
    model.sync_from_domain(&domain);

    let after = model.queue_view();
    let after_deck = on_deck(&after);
    assert_eq!(after_deck.thread_blocks.len(), 2);
    assert_eq!(
        after_deck
            .thread_blocks
            .iter()
            .map(|block| block.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(after_deck.task_ids, vec![alpha, beta]);
    assert_eq!(model.selected_id(), Some(beta));
    assert_eq!(model.visible_ids(), after_deck.task_ids);
    assert_eq!(model.selected_index(), Some(1));
    assert_eq!(after_deck.task_ids[model.selected_index().unwrap()], beta);
}

/// Two fresh models from the same store share no UI state and write no UI-state files.
fn board_rows(model: &BoardModel, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, model);
        })
        .expect("draw board");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn arrow_navigation_crosses_painted_header_task_to_task() {
    let mut domain = DomainState::new();
    domain
        .create(
            "alpha task",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            Some("alpha".to_string()),
        )
        .expect("create alpha");
    domain
        .create(
            "beta task",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            Some("beta".to_string()),
        )
        .expect("create beta");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    model.set_selected_project(Some(PathBuf::from(THIS_REPO)));
    let ids = model.visible_ids();
    assert_eq!(ids.len(), 2, "two threaded tasks are visible");
    apply_intent(&mut domain, &mut model, BoardIntent::SelectIndex(0), None)
        .expect("select first task");

    let rows = board_rows(&model, 80, 24);
    let first_title = domain.get(ids[0]).expect("first task").title.clone();
    let second = domain.get(ids[1]).expect("second task");
    let first_y = rows
        .iter()
        .position(|row| row.contains(&first_title))
        .expect("first task paints");
    let header_y = rows
        .iter()
        .position(|row| row.contains(&format!("#{}", second.thread.as_deref().unwrap())))
        .expect("second block header paints");
    let second_y = rows
        .iter()
        .position(|row| row.contains(&second.title))
        .expect("second task paints");
    assert!(
        first_y < header_y && header_y < second_y,
        "a painted header must physically sit between the tasks:\n{}",
        rows.join("\n")
    );

    apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
        .expect("arrow navigation moves to next task");
    assert_eq!(
        model.selected_id(),
        Some(ids[1]),
        "selection must skip decorative headers and land on the next task"
    );
}

#[test]
fn two_fresh_models_from_same_store_share_no_ui_state_and_no_ui_writes_under_state_or_config_dirs()
{
    let state_dir = temp_dir("state");
    let config_dir = temp_dir("config");
    let _g1 = TempGuard(state_dir.clone());
    let _g2 = TempGuard(config_dir.clone());

    let store = TaskStore::new(&state_dir);
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "shared",
            None,
            project(THIS_REPO),
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    store.save(&domain).expect("save domain");

    let before_state = list_files_recursive(&state_dir);
    let before_config = list_files_recursive(&config_dir);

    let mut a = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    let b = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    assert_eq!(a.selected_id(), Some(id));
    assert_eq!(b.selected_id(), Some(id));

    // Mutate session-only UI on A; a fresh B must not inherit it.
    a.set_message("only on A");
    let _ = apply_intent(&mut domain, &mut a, BoardIntent::ToggleDoneDrawer, None);

    let b_fresh = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    assert_eq!(
        b_fresh.message(),
        None,
        "fresh model has no carried message"
    );
    assert_eq!(
        b_fresh.selected_id(),
        Some(id),
        "fresh model re-seeds; does not inherit A's session UI"
    );
    assert_eq!(a.message(), Some("only on A"));
    assert!(a.drawer_open(), "A's drawer mutation stays in this session");
    assert!(
        !b_fresh.drawer_open(),
        "fresh model starts with drawer closed"
    );
    assert_eq!(
        b_fresh.selected_project(),
        None,
        "fresh model starts with the all-projects deck scope"
    );
    assert_eq!(
        b_fresh.visible_ids(),
        vec![id],
        "fresh model starts with the all-projects deck scope"
    );

    let after_state = list_files_recursive(&state_dir);
    let after_config = list_files_recursive(&config_dir);
    assert_eq!(
        before_state, after_state,
        "BoardModel must not write UI-state files under the task store dir"
    );
    assert_eq!(
        before_config, after_config,
        "BoardModel must not write UI-state files under the config dir"
    );
}

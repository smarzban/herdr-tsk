//! BoardModel cutover onto queue query, selection anchor, and session-only state.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use herdr_tasks::domain::{
    DomainState, HumanStatus, ProvenanceOrigin, Task, TaskEvent, TaskEventKind, TaskScope,
};
use herdr_tasks::store::TaskStore;
use herdr_tasks::ui::board::{apply_intent, BoardModel, ProjectScopeOption};
use herdr_tasks::ui::input::BoardIntent;
use herdr_tasks::ui::queue::{query, DeckScope, SectionKind};
use uuid::Uuid;

const THIS_REPO: &str = "/repos/app";

fn epoch_plus(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn task(id: u128, title: &str, status: HumanStatus, scope: TaskScope, updated_secs: u64) -> Task {
    let at = epoch_plus(updated_secs);
    Task {
        id: Uuid::from_u128(id),
        revision: Some(Uuid::from_u128(id)),
        merge_base_revision: None,
        title: title.into(),
        notes: None,
        status,
        scope,
        capsule: None,
        agent_meta: None,
        last_observed: None,
        provenance: ProvenanceOrigin::Manual,
        history: vec![TaskEvent {
            kind: TaskEventKind::Created,
            at,
        }],
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

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("herdr-tasks-queue-board-model-{tag}-{nanos}-{seq}"));
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
    let view = query(&tasks, Some(Path::new(THIS_REPO)), DeckScope::All, false);
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
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();
    domain.set_status(id_new, HumanStatus::Started).unwrap();
    let id_old = domain
        .create(
            "motion-old",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();
    domain.set_status(id_old, HumanStatus::Started).unwrap();
    let _deck = domain
        .create(
            "deck",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
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

    // Case B: no motion → first ON DECK row in query order (current repo first).
    let deck_a = task(10, "a", HumanStatus::Ready, project("/repos/a"), 30);
    let deck_b = task(11, "b", HumanStatus::Ready, project(THIS_REPO), 40);
    let deck_tasks = vec![deck_a.clone(), deck_b.clone()];
    let model = BoardModel::from_tasks(deck_tasks.clone(), Some(PathBuf::from(THIS_REPO)));
    let view = query(
        &deck_tasks,
        Some(Path::new(THIS_REPO)),
        DeckScope::All,
        false,
    );
    let first_deck = view
        .sections
        .iter()
        .filter(|s| s.kind == SectionKind::OnDeck)
        .flat_map(|s| s.task_ids.iter().copied())
        .next();
    assert_eq!(first_deck, Some(Uuid::from_u128(11)));
    assert_eq!(
        model.selected_id(),
        first_deck,
        "open without motion seeds the first deck row"
    );

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
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();
    let other_id = domain
        .create(
            "other task",
            None,
            project("/repos/other"),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    assert_eq!(model.visible_ids(), vec![app_id, other_id]);

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
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
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();
    domain.set_status(id_doing, HumanStatus::Started).unwrap();
    let id_todo = domain
        .create(
            "todo",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .unwrap();

    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    let visible = model.visible_ids();
    assert!(visible.contains(&id_doing));
    assert!(visible.contains(&id_todo));
    let todo_idx = visible.iter().position(|&id| id == id_todo).unwrap();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(todo_idx),
        None,
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

/// Two fresh models from the same store share no UI state and write no UI-state files.
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
            None,
            None,
            ProvenanceOrigin::Manual,
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
    let _ = apply_intent(
        &mut domain,
        &mut a,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    );

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

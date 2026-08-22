//! mouse hit-map parity for every the control.
//!
//! The hit-map comes from [`board_hit_map`], the same painter [`draw_board`] uses (via
//! `draw_queue_frame`), so these tests exercise the *real* renderer geometry rather than a
//! hand-maintained shadow of it. Each control is proved by dispatching both the keyboard
//! path and the mouse path and comparing what actually happened, never by asserting a
//! mapping table.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use herdr_tasks::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use herdr_tasks::ui::board::{
    apply_intent, board_hit_map, board_verb_items, draw_board, resolve_board_command,
    BoardInputMode, BoardModel,
};
use herdr_tasks::ui::capture::CaptureField;
use herdr_tasks::ui::input::{map_board_form_key, map_key, BoardIntent, PRIMARY_CAPTURE_ACTIONS};
use herdr_tasks::ui::mouse::{
    capture_layout, capture_mouse_paths_complete, left_click, map_board_mouse, map_capture_mouse,
    primary_capture_action_sample_mouse,
};
use herdr_tasks::ui::render::{QueueHit, QueueHitMap, QueueHitTarget};

const THIS_REPO: &str = "/repos/app";
const OTHER_REPO: &str = "/repos/other";
const STANDARD: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn project(path: &str) -> TaskScope {
    TaskScope::Project {
        path: path.to_string(),
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

fn mapped_key(code: KeyCode) -> KeyEvent {
    match code {
        KeyCode::Char(' ' | 'd' | 'o' | 'b' | 'a' | 'e' | 'x' | 'u' | 'q') | KeyCode::Delete => {
            alt(code)
        }
        _ => press(code),
    }
}

fn wheel_up(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn wheel_down(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// One task, this-repo scoped, at the given status, selected.
fn board_with_task(title: &str, status: HumanStatus) -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            title,
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    if status != HumanStatus::Ready {
        domain.set_status(id, status).expect("set status");
    }
    let model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    (domain, model, id)
}

/// Two Todo tasks in different projects, so the project selector offers more than one
/// concrete project option (All, Global, this repo, the other repo).
fn scoped_board() -> (DomainState, BoardModel, uuid::Uuid) {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "here",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create here");
    domain
        .create(
            "elsewhere",
            None,
            project(OTHER_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create elsewhere");
    let model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    (domain, model, id)
}

/// `n` Todo tasks in the same project, enough to overflow the standard 80x24 viewport (20
/// rows) and put the palette's command panel, and the scope dropdown's later options, over
/// real task rows underneath -- the deck size C1 needs to exist at all.
fn deck_of(n: usize) -> (DomainState, BoardModel) {
    let mut domain = DomainState::new();
    for i in 0..n {
        domain
            .create(
                format!("task {i}"),
                None,
                project(THIS_REPO),
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create task");
    }
    let model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    (domain, model)
}

/// A hit region of `target_kind` whose row (`area.y`) coincides with a `Task` region's row
/// -- i.e. an overlay row genuinely painted over the base list, the only geometry C1's
/// z-order bug could ever surface on. `None` when the fixture is not actually long enough
/// to produce the overlap the caller wants to prove something about.
fn overlay_row_over_a_task(
    hits: &QueueHitMap,
    target_kind: impl Fn(&QueueHitTarget) -> bool,
) -> Option<&QueueHit> {
    hits.regions.iter().find(|hit| {
        target_kind(&hit.target)
            && hits.regions.iter().any(|other| {
                matches!(other.target, QueueHitTarget::Task(_)) && other.area.y == hit.area.y
            })
    })
}

fn verb_hit_for_chord<'a>(model: &BoardModel, hits: &'a QueueHitMap, chord: &str) -> &'a QueueHit {
    let entries = board_verb_items(model);
    let index = entries
        .iter()
        .position(|entry| entry.key == chord)
        .unwrap_or_else(|| panic!("no verb entry for chord {chord:?}: {entries:?}"));
    hits.regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Verb(i) if i == index))
        .unwrap_or_else(|| panic!("no hit region for verb chord {chord:?} at index {index}"))
}

fn click(region: &QueueHit, model: &BoardModel, hits: &QueueHitMap) -> Option<BoardIntent> {
    map_board_mouse(model, hits, left_click(region.area.x, region.area.y))
}

#[test]
fn hit_map_covers_selection_rows_verbs_drawer_selector_chip_dropdown_palette_rows_help() {
    // Base list: selection row, verbs, selector chip.
    let (_domain, model, id) = board_with_task("Fix flake", HumanStatus::Ready);
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::Task(t) if t == id)),
        "no selection row hit region: {hits:?}"
    );
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::Verb(_))),
        "no verb hit region: {hits:?}"
    );
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::ProjectChip)),
        "no selector chip hit region: {hits:?}"
    );

    // Drawer: only painted while the done drawer is open.
    let (mut domain, mut model, _id) = board_with_task("archived", HumanStatus::Done);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::ToggleDoneDrawer,
        None,
        None,
    )
    .expect("open drawer");
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::Drawer)),
        "no drawer hit region while open: {hits:?}"
    );

    // Dropdown: the project-scope selector open.
    let (mut domain, mut model, _id) = scoped_board();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
        None,
    )
    .expect("open selector");
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::ProjectOption(_))),
        "no dropdown option hit region: {hits:?}"
    );

    // Palette rows.
    let (mut domain, mut model, _id) = board_with_task("palette", HumanStatus::Ready);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("open palette");
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::Command(0))),
        "no palette row hit region: {hits:?}"
    );

    // Help card.
    let (mut domain, mut model, _id) = board_with_task("help", HumanStatus::Ready);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenHelp, None, None).expect("open help");
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        hits.regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::HelpDismiss)),
        "no help dismiss hit region: {hits:?}"
    );
}

#[test]
fn all_projects_group_header_double_click_scopes_the_board_to_that_project() {
    // All-projects view (the default): each ON DECK project group's header is that
    // project's own scope control, resolved through the same queue view the frame
    // painted rather than by searching rendered text.
    let (mut domain, mut model, _id) = scoped_board();
    assert_eq!(model.selected_project(), None, "fixture starts unscoped");
    let hits = board_hit_map(STANDARD, &model);
    let view = model.queue_view();
    let header = hits
        .regions
        .iter()
        .find(|hit| match hit.target {
            QueueHitTarget::SectionProject(index) => {
                view.sections
                    .get(index)
                    .and_then(|section| section.project_label.as_deref())
                    == Some(OTHER_REPO)
            }
            _ => false,
        })
        .unwrap_or_else(|| panic!("no group header hit region for {OTHER_REPO}: {hits:?}"));
    let intent = click(header, &model, &hits).expect("header click maps to an intent");
    apply_intent(&mut domain, &mut model, intent.clone(), None, None)
        .expect("apply first header click");
    assert_eq!(
        model.selected_project(),
        None,
        "one header click must leave the all-projects scope unchanged"
    );
    apply_intent(&mut domain, &mut model, intent, None, None).expect("apply second header click");
    assert_eq!(
        model.selected_project(),
        Some(Path::new(OTHER_REPO)),
        "header double-click narrows the session deck scope to the clicked project"
    );

    // Scoped to one project the header reads plain ON DECK, so it stops being a scope
    // control: no SectionProject target may be pushed at all.
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        !hits
            .regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::SectionProject(_))),
        "scoped view must not offer group-header scope controls: {hits:?}"
    );
}

#[test]
fn scoped_project_named_all_projects_has_no_group_header_hit_target() {
    const COLLIDING_REPO: &str = "/repos/all projects";
    let mut domain = DomainState::new();
    domain
        .create(
            "name collision",
            None,
            project(COLLIDING_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(COLLIDING_REPO)));
    model.set_selected_project(Some(PathBuf::from(COLLIDING_REPO)));

    let hits = board_hit_map(STANDARD, &model);
    assert!(
        !hits
            .regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::SectionProject(_))),
        "actual scoped state, not its colliding display label, must keep ON DECK inert: {hits:?}"
    );
}

/// The selected task owns one standard form even below the list fold. Its field hits are
/// topmost, the dropdown click equals keyboard navigation plus Enter, and inactive board
/// chrome never escapes the modal form.
#[test]
fn task_form_mouse_fields_dropdown_and_verbs_match_keyboard_while_scrolled() {
    let mut domain = DomainState::new();
    let target = domain
        .create(
            "Selected form target",
            Some("first\nsecond".to_string()),
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create selected task");
    domain
        .create(
            "other project",
            None,
            project(OTHER_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create scope option");
    for index in 0..30 {
        domain
            .create(
                format!("padding task {index}"),
                None,
                project(THIS_REPO),
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create padding task");
    }
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    let target_index = model
        .visible_ids()
        .iter()
        .position(|id| *id == target)
        .expect("target visible");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(target_index),
        None,
        None,
    )
    .expect("select target");
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::BeginEditTitle,
        None,
        None,
    )
    .expect("open task form");

    let mut hits = board_hit_map(STANDARD, &model);
    let intent_for = |target| {
        let hit = hits
            .regions
            .iter()
            .find(|hit| hit.target == target)
            .unwrap_or_else(|| panic!("missing task-form hit region for {target:?}"));
        click(hit, &model, &hits)
    };
    assert_eq!(
        intent_for(QueueHitTarget::Verb(0)),
        map_board_form_key(
            CaptureField::Title,
            false,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ),
        "task-form Save verb must match Title's Enter route"
    );
    assert_eq!(
        intent_for(QueueHitTarget::Verb(2)),
        map_board_form_key(
            CaptureField::Title,
            false,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ),
        "task-form Cancel verb must match Esc"
    );

    // This form was opened with `BeginEditTitle`, so edit mode is ALREADY open: a field click
    // must move focus. The
    // separate view-state rule -- that a click must not ENTER edit mode -- is exercised in
    // `page_field_clicks_stay_inert_and_the_scope_footer_opens_its_dropdown`, which asserts
    // `input_mode() == TaskPage` first. Conflating the two is what previously let this test
    // assert inertness while sitting in edit mode.
    for (target, field) in [
        (QueueHitTarget::FormTitle, CaptureField::Title),
        (QueueHitTarget::FormNotes(0), CaptureField::Notes),
        (QueueHitTarget::FormNotes(1), CaptureField::Notes),
    ] {
        let hit = hits
            .regions
            .iter()
            .find(|hit| hit.target == target)
            .unwrap_or_else(|| panic!("missing task-form field hit {target:?}"));
        assert_eq!(
            click(hit, &model, &hits),
            Some(BoardIntent::FocusFormField(field)),
            "clicking task-form field {target:?} in edit mode must focus it"
        );
    }

    let scope_hit = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::FormScope)
        .expect("task-form scope hit");
    let open = click(scope_hit, &model, &hits).expect("scope click intent");
    assert_eq!(open, BoardIntent::OpenFormScopeDropdown);
    apply_intent(&mut domain, &mut model, open, None, None).expect("open form dropdown");
    assert_eq!(model.input_mode(), BoardInputMode::FormScopeDropdown);
    assert_eq!(
        map_board_mouse(&model, &board_hit_map(STANDARD, &model), left_click(0, 0)),
        None,
        "base controls remain inert while the dropdown owns input"
    );

    let mut keyboard_domain = domain.clone();
    let mut keyboard_model = model.clone();
    while keyboard_model.form_scope_dropdown_choice() != Some(&TaskScope::Global) {
        let next = map_board_form_key(
            CaptureField::Scope,
            true,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        )
        .expect("Down selects the next scope option");
        apply_intent(&mut keyboard_domain, &mut keyboard_model, next, None, None)
            .expect("move keyboard choice");
    }
    let confirm = map_board_form_key(
        CaptureField::Scope,
        true,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    )
    .expect("Enter confirms the highlighted scope");
    apply_intent(
        &mut keyboard_domain,
        &mut keyboard_model,
        confirm,
        None,
        None,
    )
    .expect("confirm keyboard scope");

    let global_index = model
        .form_scope_options()
        .iter()
        .position(|scope| *scope == TaskScope::Global)
        .expect("Global form option");
    hits = board_hit_map(STANDARD, &model);
    let option = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::FormScopeOption(global_index))
        .expect("Global dropdown option is painted above the scrolled task list");
    let choose = click(option, &model, &hits).expect("dropdown option click");
    assert_eq!(choose, BoardIntent::SelectFormScopeOption(global_index));
    apply_intent(&mut domain, &mut model, choose, None, None).expect("choose form scope");
    assert_eq!(model.form_scope(), keyboard_model.form_scope());
    assert_eq!(model.input_mode(), BoardInputMode::EditScope);
    assert_eq!(
        domain.get(target).expect("target still exists").scope,
        project(THIS_REPO),
        "dropdown selection changes only the draft before Save"
    );

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenFormScopeDropdown,
        None,
        None,
    )
    .expect("reopen dropdown");
    let esc = map_board_form_key(
        CaptureField::Scope,
        true,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
    )
    .expect("Esc cancels only the dropdown");
    apply_intent(&mut domain, &mut model, esc, None, None).expect("cancel dropdown");
    assert_eq!(model.input_mode(), BoardInputMode::EditScope);
    assert_eq!(model.form_scope(), Some(&TaskScope::Global));
}

/// One verb-bar chord: dispatching the keyboard's key and clicking the matching hit region
/// must resolve to the same `BoardIntent`, and applying each to its own independent, fresh
/// board must land on the same domain/model effect.
fn assert_verb_parity(title: &str, status: HumanStatus, chord: &str, key: KeyCode) {
    let (mut domain_key, mut model_key, id_key) = board_with_task(title, status);
    let (mut domain_mouse, mut model_mouse, id_mouse) = board_with_task(title, status);
    // A Done task lives in the closed done drawer and is not selected by default; open it
    // and re-select the task if the reanchor moved off it, so the verb bar shows the
    // reopen chord for a real selection (mirrors `space_on_done_reopens` in
    // tests/queue_board_verbs.rs).
    if status == HumanStatus::Done {
        for (domain, model, id) in [
            (&mut domain_key, &mut model_key, id_key),
            (&mut domain_mouse, &mut model_mouse, id_mouse),
        ] {
            apply_intent(domain, model, BoardIntent::ToggleDoneDrawer, None, None)
                .expect("open drawer");
            if model.selected_id() != Some(id) {
                let idx = model
                    .visible_ids()
                    .iter()
                    .position(|&row| row == id)
                    .expect("done row visible with drawer open");
                apply_intent(domain, model, BoardIntent::SelectIndex(idx), None, None)
                    .expect("select done");
            }
        }
    }

    let keyboard_intent = map_key(BoardInputMode::Normal, mapped_key(key))
        .unwrap_or_else(|| panic!("no key for {chord}"));
    apply_intent(
        &mut domain_key,
        &mut model_key,
        keyboard_intent.clone(),
        None,
        None,
    )
    .expect("keyboard apply");

    let hits = board_hit_map(STANDARD, &model_mouse);
    let region = verb_hit_for_chord(&model_mouse, &hits, chord);
    let mouse_intent =
        click(region, &model_mouse, &hits).unwrap_or_else(|| panic!("no mouse intent for {chord}"));
    assert_eq!(
        mouse_intent, keyboard_intent,
        "chord {chord:?}: mouse and key must agree"
    );
    apply_intent(
        &mut domain_mouse,
        &mut model_mouse,
        mouse_intent,
        None,
        None,
    )
    .expect("mouse apply");

    assert_eq!(
        domain_key.get(id_key).unwrap().status,
        domain_mouse.get(id_mouse).unwrap().status,
        "chord {chord:?}: status diverged between the two routes"
    );
    assert_eq!(
        model_key.detail_open().is_some(),
        model_mouse.detail_open().is_some(),
        "chord {chord:?}: detail-open diverged between the two routes"
    );
    assert_eq!(
        model_key.command_surface(),
        model_mouse.command_surface(),
        "chord {chord:?}: command surface diverged between the two routes"
    );
    assert_eq!(
        model_key.input_mode(),
        model_mouse.input_mode(),
        "chord {chord:?}: input mode diverged between the two routes"
    );
}

#[test]
fn click_and_wheel_match_keyboard_effects_for_each_control() {
    // Verb bar: every chord a Todo task shows, plus the Done-only reopen chord.
    assert_verb_parity("space", HumanStatus::Ready, "space", KeyCode::Char(' '));
    assert_verb_parity("enter", HumanStatus::Ready, "enter", KeyCode::Enter);
    assert_verb_parity("d", HumanStatus::Ready, "d", KeyCode::Char('d'));
    assert_verb_parity("b", HumanStatus::Ready, "b", KeyCode::Char('b'));
    assert_verb_parity("colon", HumanStatus::Ready, ":", KeyCode::Char(':'));
    assert_verb_parity("question", HumanStatus::Ready, "?", KeyCode::Char('?'));
    assert_verb_parity("capture", HumanStatus::Started, "+", KeyCode::Char('+'));
    assert_verb_parity("reopen", HumanStatus::Done, "o", KeyCode::Char('o'));

    // Drawer toggle: open it by keyboard on both boards first (a shared start state), then
    // close it by keyboard on one and by clicking the DONE header on the other.
    let (mut domain_key, mut model_key, _id) = board_with_task("drawer key", HumanStatus::Done);
    let (mut domain_mouse, mut model_mouse, _id2) =
        board_with_task("drawer mouse", HumanStatus::Done);
    for (domain, model) in [
        (&mut domain_key, &mut model_key),
        (&mut domain_mouse, &mut model_mouse),
    ] {
        apply_intent(domain, model, BoardIntent::ToggleDoneDrawer, None, None)
            .expect("open drawer");
        assert!(model.drawer_open());
    }
    let keyboard_intent =
        map_key(BoardInputMode::Normal, press(KeyCode::Char('z'))).expect("z key");
    apply_intent(
        &mut domain_key,
        &mut model_key,
        keyboard_intent.clone(),
        None,
        None,
    )
    .expect("keyboard close drawer");
    assert!(!model_key.drawer_open());

    let hits = board_hit_map(STANDARD, &model_mouse);
    let drawer_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Drawer))
        .expect("drawer hit region");
    let mouse_intent = click(drawer_hit, &model_mouse, &hits).expect("drawer click intent");
    assert_eq!(mouse_intent, keyboard_intent);
    apply_intent(
        &mut domain_mouse,
        &mut model_mouse,
        mouse_intent,
        None,
        None,
    )
    .expect("mouse close drawer");
    assert!(!model_mouse.drawer_open());

    // Wheel: a step is the keyboard's own relative selection step.
    let mut seed = DomainState::new();
    seed.create(
        "first",
        None,
        project(THIS_REPO),
        None,
        None,
        ProvenanceOrigin::Manual,
    )
    .expect("first");
    seed.create(
        "second",
        None,
        project(THIS_REPO),
        None,
        None,
        ProvenanceOrigin::Manual,
    )
    .expect("second");
    let mut domain_key = seed.clone();
    let mut domain_mouse = seed.clone();
    let mut model_key = BoardModel::from_domain(&domain_key, Some(PathBuf::from(THIS_REPO)));
    let mut model_mouse = BoardModel::from_domain(&domain_mouse, Some(PathBuf::from(THIS_REPO)));
    let ids = model_key.visible_ids();
    assert_eq!(ids.len(), 2, "both tasks must be visible on the deck");
    assert_eq!(model_key.selected_id(), Some(ids[0]));
    let keyboard_next = map_key(BoardInputMode::Normal, press(KeyCode::Down)).expect("down key");
    apply_intent(
        &mut domain_key,
        &mut model_key,
        keyboard_next.clone(),
        None,
        None,
    )
    .expect("keyboard next");
    let hits = board_hit_map(STANDARD, &model_mouse);
    let mouse_next = map_board_mouse(&model_mouse, &hits, wheel_down(0, 0)).expect("wheel next");
    assert_eq!(mouse_next, keyboard_next);
    apply_intent(&mut domain_mouse, &mut model_mouse, mouse_next, None, None).expect("mouse next");
    assert_eq!(model_key.selected_id(), Some(ids[1]));
    assert_eq!(model_mouse.selected_id(), Some(ids[1]));
    // the last row is a hard stop for the wheel, unlike the
    // keyboard's own wrap: no further step down, but a step back up still moves the
    // selection. This is a deliberate, signed-off divergence from's literal wording,
    // not an inherited accident -- see the comment on `wheel_step_matches_the_keyboard_
    // selection_step` in `src/ui/mouse.rs` for why a wheel step does not wrap.
    let hits = board_hit_map(STANDARD, &model_mouse);
    assert_eq!(map_board_mouse(&model_mouse, &hits, wheel_down(0, 0)), None);
    assert_eq!(
        map_board_mouse(&model_mouse, &hits, wheel_up(0, 0)),
        Some(BoardIntent::SelectPrev)
    );

    // Project selector chip: no the key binds it (mouse-only, like `SelectIndex`), so its
    // effect is proved against the same `OpenProjectSelector` the reducer would apply for
    // any future key bound to it, on an independently built, otherwise-identical board.
    let (mut domain_direct, mut model_direct, _id3) = board_with_task("chip a", HumanStatus::Ready);
    let (mut domain_mouse, mut model_mouse, _id4) = board_with_task("chip b", HumanStatus::Ready);
    apply_intent(
        &mut domain_direct,
        &mut model_direct,
        BoardIntent::OpenProjectSelector,
        None,
        None,
    )
    .expect("direct open");
    let hits = board_hit_map(STANDARD, &model_mouse);
    let chip_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::ProjectChip))
        .expect("selector chip hit region");
    let mouse_intent = click(chip_hit, &model_mouse, &hits).expect("chip click intent");
    assert_eq!(mouse_intent, BoardIntent::OpenProjectSelector);
    apply_intent(
        &mut domain_mouse,
        &mut model_mouse,
        mouse_intent,
        None,
        None,
    )
    .expect("mouse open");
    assert_eq!(
        model_direct.project_picker_index(),
        model_mouse.project_picker_index()
    );
    assert_eq!(model_direct.message(), model_mouse.message());

    // Dropdown option: a single click on a non-selected row must land exactly where
    // stepping the keyboard's `ProjectPickerNext` there and pressing Enter would.
    let (mut domain_key, mut model_key, _id5) = scoped_board();
    let (mut domain_mouse, mut model_mouse, _id6) = scoped_board();
    for (domain, model) in [
        (&mut domain_key, &mut model_key),
        (&mut domain_mouse, &mut model_mouse),
    ] {
        apply_intent(domain, model, BoardIntent::OpenProjectSelector, None, None)
            .expect("open selector");
    }
    let target_index = model_key
        .project_options()
        .iter()
        .position(|option| {
            *option
                == herdr_tasks::ui::board::ProjectScopeOption::Project(PathBuf::from(OTHER_REPO))
        })
        .expect("the other repo is an offered option");
    assert!(
        target_index > 0,
        "the test needs a non-selected option to jump to"
    );
    let next_key = map_key(BoardInputMode::ProjectPicker, press(KeyCode::Down))
        .expect("project picker down key");
    for _ in 0..target_index {
        apply_intent(
            &mut domain_key,
            &mut model_key,
            next_key.clone(),
            None,
            None,
        )
        .expect("step to option");
    }
    let confirm_key = map_key(BoardInputMode::ProjectPicker, press(KeyCode::Enter))
        .expect("project picker enter key");
    apply_intent(&mut domain_key, &mut model_key, confirm_key, None, None).expect("confirm choice");

    let hits = board_hit_map(STANDARD, &model_mouse);
    let option_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::ProjectOption(i) if i == target_index))
        .expect("dropdown option hit region");
    let mouse_intent = click(option_hit, &model_mouse, &hits).expect("dropdown click intent");
    assert_eq!(mouse_intent, BoardIntent::SelectProjectOption(target_index));
    apply_intent(
        &mut domain_mouse,
        &mut model_mouse,
        mouse_intent,
        None,
        None,
    )
    .expect("mouse choice");

    assert!(
        model_key.project_picker_index().is_none(),
        "keyboard route must close the picker"
    );
    assert!(
        model_mouse.project_picker_index().is_none(),
        "mouse route must close the picker"
    );
    assert_eq!(
        model_key.message(),
        model_mouse.message(),
        "the two routes must scope the deck to the same project"
    );
    // Each board created its own tasks (different uuids), so compare the *shape* of what
    // is now visible -- one ON DECK task, the one scoped to the chosen project -- rather
    // than exact ids.
    assert_eq!(model_key.visible_ids().len(), 1);
    assert_eq!(model_mouse.visible_ids().len(), 1);
}

/// non-regression: the standalone quick-capture popup's mouse paths are untouched by
/// this rewrite (its `CaptureLayout`/`map_capture_mouse` route is separate from the board's
/// hit-map, per the task's implementation boundary).
#[test]
fn capture_popup_mouse_paths_unchanged() {
    let layout = capture_layout(Rect::new(0, 0, 80, 16));
    assert!(
        capture_mouse_paths_complete(&layout),
        "every primary capture action must still have a resolvable mouse path"
    );
    for &action in PRIMARY_CAPTURE_ACTIONS {
        let mouse = primary_capture_action_sample_mouse(action, &layout);
        assert!(
            map_capture_mouse(&layout, mouse).is_some(),
            "{action:?}: capture mouse route regressed"
        );
    }

    // The narrow capture width still keeps every control clickable.
    let narrow = capture_layout(Rect::new(0, 0, 40, 16));
    assert!(capture_mouse_paths_complete(&narrow));
}

/// C1: on a deck long enough for the palette's command panel to actually
/// overlap the base list underneath it, a click on a command row must run that command --
/// not fall through to whatever `Task` region the hit-map's old paint-order search found
/// first, which was always `CloseCommandSurface`'s territory once the overlap existed.
/// Every prior parity fixture was 1-2 tasks, too short to ever reach this geometry, which
/// is why the palette's own regression net never caught it. Confirmed to fail before the
/// `hit_at` z-order fix (reverse search, topmost wins): with paint-order search, this
/// clicked a `Task` row and dispatched `SelectIndex`, never the command.
#[test]
fn a_palette_row_over_a_full_deck_still_runs_its_own_command_not_the_task_row_under_it() {
    let (mut domain, mut model) = deck_of(20);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("open palette");
    let hits = board_hit_map(STANDARD, &model);
    let command_hit = overlay_row_over_a_task(&hits, |t| matches!(t, QueueHitTarget::Command(_)))
        .unwrap_or_else(|| {
            panic!("fixture is not long enough to overlap a command row with a task row: {hits:?}")
        });
    let QueueHitTarget::Command(index) = command_hit.target else {
        unreachable!()
    };
    let expected = model
        .visible_commands()
        .get(index)
        .expect("command index in range")
        .intent
        .clone();
    assert_ne!(
        expected,
        BoardIntent::CloseCommandSurface,
        "the overlapping row must be a real command, not the dismiss case itself"
    );
    // a command row click is `SelectCommand(index)`, not the row's own intent directly
    // (the same mouse-only shape as `SelectIndex`/`SelectProjectOption`); resolve it on a
    // clone to check it still names this exact command without perturbing `model`.
    let clicked = click(command_hit, &model, &hits).expect("mouse command intent");
    let mut resolved_model = model.clone();
    assert_eq!(
        resolve_board_command(&mut resolved_model, clicked),
        Some(expected)
    );
}

/// C1: the scope dropdown's own regression net. `scoped_board` offers four
/// project options starting at the same row the task list starts on, so later options sit
/// directly over real task rows; a click on one of those rows must select that option, not
/// cancel the picker the way the region underneath it used to win. Confirmed to fail before
/// the `hit_at` fix the same way the palette case does.
#[test]
fn a_dropdown_option_over_a_task_row_still_selects_that_option_not_the_task_under_it() {
    let (mut domain, mut model, _id) = scoped_board();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenProjectSelector,
        None,
        None,
    )
    .expect("open selector");
    let hits = board_hit_map(STANDARD, &model);
    let option_hit =
        overlay_row_over_a_task(&hits, |t| matches!(t, QueueHitTarget::ProjectOption(_)))
            .unwrap_or_else(|| {
                panic!(
                    "fixture is not long enough to overlap an option row with a task row: {hits:?}"
                )
            });
    let QueueHitTarget::ProjectOption(index) = option_hit.target else {
        unreachable!()
    };
    assert_eq!(
        click(option_hit, &model, &hits),
        Some(BoardIntent::SelectProjectOption(index))
    );
}

/// `ViewBoard` is the one view-segment click that actually dispatches
/// something (`ViewQueue` is the already-active no-op the parity test above covers). Prove
/// the segment that does something, not just the one that does not.
///
/// `v` is the keyboard's identical route to the same intent, so the mouse and
/// keyboard paths are compared here rather than asserted separately.
#[test]
fn click_a_task_row_selects_its_index_on_a_scrolled_list() {
    let (mut domain, mut model) = deck_of(40);
    let visible = model.visible_ids();
    let last = visible.len() - 1;
    let neighbor = last - 1;

    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(last),
        None,
        None,
    )
    .expect("select last task");
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None, None)
        .expect("open peek on the last task");
    assert_eq!(model.detail_open(), Some(visible[last]));

    let hits = board_hit_map(STANDARD, &model);
    let geo = herdr_tasks::ui::tier::resolve(STANDARD.width, STANDARD.height);
    assert!(
        hits.regions
            .iter()
            .all(|hit| !matches!(hit.target, QueueHitTarget::Task(id) if id == visible[0])),
        "the deck must actually be long enough to scroll task 0 out of the viewport: {hits:?} \
         (geo={geo:?})"
    );

    let region = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Task(id) if id == visible[neighbor]))
        .expect("the task just above the open accordion must still carry its own hit region");
    assert_eq!(
        click(region, &model, &hits),
        Some(BoardIntent::SelectIndex(neighbor)),
        "a click on a scrolled-into-view row must resolve to its own index, not the \
         unscrolled row position"
    );
}

/// G-2 (gate round 1, PR #11): the follow-selection scroll above only proves the click's
/// row->index mapping once the *accordion's* anchor has forced a scroll. This is the
/// keyboard-navigation form of the same defect the gate named: with nothing open at all
/// (no capture, no accordion), stepping the plain selection past the fold with `j`
/// (`SelectNext`) must keep it painted at every step -- the mutating verbs (`space`/`d`/`x`)
/// act on whatever is selected, so an off-screen selection would silently mutate a row the
/// user cannot see.
#[test]
fn stepping_selection_past_the_fold_with_select_next_keeps_the_selected_row_painted() {
    let (mut domain, mut model) = deck_of(40);
    let visible = model.visible_ids();
    let last = visible.len() - 1;

    for step in 0..last {
        apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None, None)
            .expect("select next");
        let selected = model
            .selected_id()
            .expect("a task stays selected while stepping through the deck");
        let hits = board_hit_map(STANDARD, &model);
        assert!(
            hits.regions
                .iter()
                .any(|hit| matches!(hit.target, QueueHitTarget::Task(id) if id == selected)),
            "step {step}: selected task {selected} must still carry a hit region, i.e. still \
             be painted, with nothing open: {hits:?}"
        );
    }
    assert_eq!(
        model.selected_id(),
        Some(visible[last]),
        "SelectNext must have walked all the way to the last task"
    );
}

/// a click on the palette's own chrome -- the `command` header row or
/// the `:` query row -- must not dismiss it. The old code returned `None` there; this
/// rewrite's `_ => Some(CloseCommandSurface)` fallthrough turned that into a dismissal
/// (and, combined with C1, a destructive one on any overlapping deck). Bound the
/// fallthrough: only a click genuinely outside the whole surface closes it.
#[test]
fn a_click_on_the_palettes_own_chrome_does_not_dismiss_it() {
    let (mut domain, mut model, _id) = board_with_task("palette chrome", HumanStatus::Ready);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("open palette");
    let hits = board_hit_map(STANDARD, &model);
    let chrome_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::CommandChrome))
        .expect("the palette must paint chrome (header/query row) outside its command rows");
    assert_eq!(
        click(chrome_hit, &model, &hits),
        None,
        "a click on the palette's own chrome must be inert, not a dismissal"
    );
    // A click genuinely outside the whole surface still closes it -- the far corner of the
    // frame paints no palette chrome at all on a short command list.
    assert_eq!(
        map_board_mouse(
            &model,
            &hits,
            left_click(STANDARD.width - 1, STANDARD.height - 1)
        ),
        Some(BoardIntent::CloseCommandSurface)
    );
}

/// R-2: the standard-tier command surface windows to 6
/// rows at 80x24 while 13 commands exist, and the painted `▲▼` marker is inert
/// `CommandChrome`, so a mouse-only user could not reach the 7 commands outside the
/// initial window (`quit`, the last one, among them). The wheel now moves the command
/// selection the same `CommandNext`/`CommandPrev` the keyboard's `j`/`k` dispatch, which
/// scrolls the painted window because it derives `scroll` from the selected index
/// (`paint_palette_overlay`) -- no new scroll state, no new hit target. Confirmed to fail
/// before the fix: `wheel_board_intent` returned `None` for any non-`Normal` mode, so the
/// wheel did nothing over an open palette.
#[test]
fn wheel_scrolls_the_open_command_surface_so_every_command_becomes_reachable() {
    let (mut domain, mut model, _id) = board_with_task("palette wheel", HumanStatus::Ready);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
        None,
    )
    .expect("open palette");

    let commands = model.visible_commands();
    assert_eq!(
        commands.len(),
        13,
        "this ready fixture must expose all 13 M1 palette commands (reopen needs a done \
         selection, AC-17): {commands:?}"
    );
    let last = commands.len() - 1;
    assert_eq!(commands[last].label, "quit");
    assert_eq!(model.command_selected(), Some(0));

    // The initial 6-row window does not paint `quit`'s row at all: no hit region for it.
    let hits = board_hit_map(STANDARD, &model);
    assert!(
        !hits
            .regions
            .iter()
            .any(|hit| matches!(hit.target, QueueHitTarget::Command(i) if i == last)),
        "quit must start outside the painted window: {hits:?}"
    );

    // Drive ScrollDown through the real `map_board_mouse` path, one step at a time (the
    // same path the app's own event loop uses), until the selection reaches the last
    // command.
    let mut steps = 0;
    while model.command_selected() != Some(last) {
        let hits = board_hit_map(STANDARD, &model);
        let intent = map_board_mouse(&model, &hits, wheel_down(0, 0)).unwrap_or_else(|| {
            panic!(
                "wheel must keep advancing the selection (step {steps}, at {:?})",
                model.command_selected()
            )
        });
        apply_intent(&mut domain, &mut model, intent, None, None).expect("apply wheel step");
        steps += 1;
        assert!(
            steps <= last,
            "wheel must reach the last command within {last} steps"
        );
    }

    // Now reachable: the repainted window carries a hit region for `quit`, and the wheel
    // clamps rather than wraps past the last command (the signed-off Normal-mode wheel
    // convention at `wheel_board_intent`, `:1474-1476`, kept consistent here).
    let hits = board_hit_map(STANDARD, &model);
    let quit_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Command(i) if i == last))
        .unwrap_or_else(|| {
            panic!("quit must be reachable once the wheel has scrolled to it: {hits:?}")
        });
    // the click is `SelectCommand(last)`, not `Quit` directly; resolve on a clone so
    // this check does not itself close the surface used by the wheel-clamp check below.
    let quit_clicked = click(quit_hit, &model, &hits).expect("mouse command intent");
    let mut resolved_model = model.clone();
    assert_eq!(
        resolve_board_command(&mut resolved_model, quit_clicked),
        Some(BoardIntent::Quit),
        "clicking the reachable `quit` region must resolve to BoardIntent::Quit"
    );
    assert_eq!(
        map_board_mouse(&model, &hits, wheel_down(0, 0)),
        None,
        "the wheel must clamp at the last command, not wrap to the first"
    );

    // And clamps at the other end too: drive ScrollUp back to the first command, then
    // confirm one more step yields no intent rather than wrapping to the last.
    while model.command_selected() != Some(0) {
        let hits = board_hit_map(STANDARD, &model);
        let intent = map_board_mouse(&model, &hits, wheel_up(0, 0))
            .expect("wheel must retreat the selection");
        apply_intent(&mut domain, &mut model, intent, None, None).expect("apply wheel step");
    }
    let hits = board_hit_map(STANDARD, &model);
    assert_eq!(
        map_board_mouse(&model, &hits, wheel_up(0, 0)),
        None,
        "the wheel must clamp at the first command, not wrap to the last"
    );
}

/// `DELETE_NOTICE_UNDO` ("u Undo") is painted on the status line while a
/// delete-recovery notice is armed (`draw_queue_frame`'s status row), but had no hit region
/// and no `map_board_mouse` arm -- a painted affordance with no mouse route, restoring the
/// coverage `the_delete_notice_undo_control_is_clickable_in_every_mode_that_shows_it`
/// (deleted in this rewrite) used to guard. Matches the keyboard's own `u` -> `Undo`.
#[test]
fn delete_notice_undo_control_is_clickable_and_matches_the_keyboard() {
    let (mut domain, mut model, id) = board_with_task("doomed", HumanStatus::Ready);
    apply_intent(&mut domain, &mut model, BoardIntent::SoftDelete, None, None)
        .expect("soft delete");
    assert!(
        model.delete_notice().is_some(),
        "a fresh soft delete must arm the undo notice"
    );
    let hits = board_hit_map(STANDARD, &model);
    let undo_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::DeleteNoticeUndo))
        .expect("no hit region for the painted Undo control");
    let keyboard_intent = map_key(BoardInputMode::Normal, alt(KeyCode::Char('u'))).expect("u key");
    assert_eq!(keyboard_intent, BoardIntent::Undo);
    assert_eq!(click(undo_hit, &model, &hits), Some(BoardIntent::Undo));

    let undo_intent = click(undo_hit, &model, &hits).unwrap();
    apply_intent(&mut domain, &mut model, undo_intent, None, None).expect("mouse undo");
    assert!(
        !domain.get(id).unwrap().soft_deleted,
        "the click must restore the task"
    );
}

/// Minor 1: the Undo hit region used to be located by `find`ing the
/// literal `u Undo` text over the whole composed status row (`Deleted "<title>" · u Undo`),
/// which is partly user text. A task titled with that literal steals the region: the real
/// control (painted at the end of the notice, after the *first* deleted-title occurrence)
/// goes unreachable at its own coordinates, while a click on the earlier occurrence inside
/// the title fires `Undo` it never painted there. The region must land only on the control
/// the notice actually painted, and a click on the title's own occurrence of the words must
/// not fire `Undo`.
#[test]
fn delete_notice_undo_region_survives_a_title_containing_the_literal_u_undo() {
    let (mut domain, mut model, id) = board_with_task("u Undo now", HumanStatus::Ready);
    apply_intent(&mut domain, &mut model, BoardIntent::SoftDelete, None, None)
        .expect("soft delete");
    assert!(
        model.delete_notice().is_some(),
        "a fresh soft delete must arm the undo notice"
    );

    let hits = board_hit_map(STANDARD, &model);
    let undo_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::DeleteNoticeUndo))
        .expect("no hit region for the painted Undo control");

    // The region must dispatch Undo when clicked, exactly as the plain-title case does.
    let keyboard_intent = map_key(BoardInputMode::Normal, alt(KeyCode::Char('u'))).expect("u key");
    assert_eq!(keyboard_intent, BoardIntent::Undo);
    assert_eq!(click(undo_hit, &model, &hits), Some(BoardIntent::Undo));

    // A click on the title's own (earlier) occurrence of the literal words must not fire
    // `Undo`: it must resolve through whatever the row actually paints there (the notice
    // text is not a control), never through the region a naive `find` would have located.
    // The status row is painted from column 0 (`paint_status_line`'s `put_line`), leading
    // with one space, then `Deleted "`: the title's own `u Undo` inside `u Undo now` starts
    // right after that 10-column prefix, well left of the real control near the row's end.
    let title_occurrence_x: u16 = 10;
    assert!(
        title_occurrence_x + 6 <= undo_hit.area.x,
        "the title's own occurrence must sit left of the real control: {undo_hit:?}"
    );
    assert_eq!(
        map_board_mouse(
            &model,
            &hits,
            left_click(title_occurrence_x, undo_hit.area.y)
        ),
        None,
        "a click on the notice's own text must not fire Undo"
    );

    let undo_intent = click(undo_hit, &model, &hits).unwrap();
    apply_intent(&mut domain, &mut model, undo_intent, None, None).expect("mouse undo");
    assert!(
        !domain.get(id).unwrap().soft_deleted,
        "the click on the real control must still restore the task"
    );
}

/// `non_left_click_ignored`'s guard was deleted along with the rest
/// of the classic `mouse.rs` suite; the behavior it covered survives at the top of
/// `map_board_mouse` (every `mouse.kind` other than `Down(Left)`/`ScrollUp`/`ScrollDown`
/// returns `None` before any hit-map lookup) but was left unguarded. A right or middle
/// click over a live control (the first verb-bar entry) must not dispatch anything.
#[test]
fn non_left_clicks_over_a_live_control_are_ignored() {
    let (_domain, model, _id) = board_with_task("click kinds", HumanStatus::Ready);
    let hits = board_hit_map(STANDARD, &model);
    let verb_hit = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::Verb(0)))
        .expect("verb hit region");
    for kind in [
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::Drag(MouseButton::Left),
    ] {
        let mouse = MouseEvent {
            kind,
            column: verb_hit.area.x,
            row: verb_hit.area.y,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(
            map_board_mouse(&model, &hits, mouse),
            None,
            "{kind:?} over a live control must not dispatch"
        );
    }
}

// ---------------------------------------------------------------------------
// Task page mouse parity: click peeks, double-click opens the page, page field
// clicks focus their fields, the scope footer opens its dropdown, wheel scrolls.
// ---------------------------------------------------------------------------

#[test]
fn header_line_registers_no_hit_target() {
    let mut domain = DomainState::new();
    domain
        .create_with_thread(
            "threaded row",
            None,
            project(THIS_REPO),
            None,
            None,
            ProvenanceOrigin::Manual,
            Some("release".to_string()),
        )
        .expect("create threaded task");
    let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(THIS_REPO)));
    model.set_selected_project(Some(PathBuf::from(THIS_REPO)));
    let rows = page_rows(&model);
    let header_y = rows
        .iter()
        .position(|row| row.contains("#release"))
        .expect("thread header paints");
    let hits = board_hit_map(STANDARD, &model);

    assert!(
        hits.regions.iter().all(|hit| hit.area.y != header_y as u16),
        "decorative thread header must register no hit target: {hits:?}"
    );
    assert_eq!(
        map_board_mouse(&model, &hits, left_click(0, header_y as u16)),
        None,
        "clicking the decorative header must be inert"
    );
}

#[test]
fn a_row_click_selects_and_peeks_and_a_second_click_opens_the_task_page() {
    let (mut domain, mut model) = deck_of(3);
    let visible = model.visible_ids();
    let target = visible[1];

    // First click selects the row and expands its peek.
    let hits = board_hit_map(STANDARD, &model);
    let area = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::Task(target))
        .expect("the target row paints a hit region")
        .area;
    let intent =
        map_board_mouse(&model, &hits, left_click(area.x + 1, area.y)).expect("first click");
    apply_intent(&mut domain, &mut model, intent, None, None).expect("apply first click");
    assert_eq!(model.selected_id(), Some(target));
    assert_eq!(
        model.detail_open(),
        Some(target),
        "a single row click expands that row's peek"
    );
    assert_eq!(model.input_mode(), BoardInputMode::Normal);

    // A second click on the same row inside the double-click window opens the page.
    let hits = board_hit_map(STANDARD, &model);
    let area = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::Task(target))
        .expect("the peeked row still paints a hit region")
        .area;
    let intent =
        map_board_mouse(&model, &hits, left_click(area.x + 1, area.y)).expect("second click");
    apply_intent(&mut domain, &mut model, intent, None, None).expect("apply second click");
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    assert_eq!(model.detail_open(), None, "the page replaces the peek");
}

#[test]
fn page_field_clicks_stay_inert_including_the_scope_footer() {
    let (mut domain, mut model) = deck_of(1);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open the page");
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // Title and notes regions are inert: only `e`/`n`/Tab enter edit mode on the page.
    let hits = board_hit_map(STANDARD, &model);
    let title_area = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::FormTitle)
        .expect("the page header paints a title hit")
        .area;
    assert_eq!(
        map_board_mouse(&model, &hits, left_click(title_area.x + 3, title_area.y)),
        None,
        "clicking the title must not enter edit mode"
    );
    let notes_area = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::FormNotes(_)))
        .expect("the page body paints notes hits")
        .area;
    assert_eq!(
        map_board_mouse(&model, &hits, left_click(notes_area.x + 3, notes_area.y)),
        None,
        "clicking the notes must not enter edit mode"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // The meta footer is inert in view mode. Scope changes only after an edit starts.
    let scope_area = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::FormScope)
        .expect("the page footer paints a scope hit")
        .area;
    assert_eq!(
        map_board_mouse(&model, &hits, left_click(scope_area.x + 3, scope_area.y)),
        None,
        "clicking the scope footer must not open the dropdown in view mode"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
}

#[test]
fn the_wheel_scrolls_the_page_notes_not_the_board_list() {
    let (mut domain, mut model) = deck_of(3);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open the page");
    let selection_before = model.selected_id();

    let hits = board_hit_map(STANDARD, &model);
    let down = map_board_mouse(&model, &hits, wheel_down(5, 5)).expect("wheel down");
    assert_eq!(down, BoardIntent::PageWheelScrollDown);
    apply_intent(&mut domain, &mut model, down, None, None).expect("scroll down");
    let up = map_board_mouse(&model, &hits, wheel_up(5, 5)).expect("wheel up");
    assert_eq!(up, BoardIntent::PageWheelScrollUp);
    apply_intent(&mut domain, &mut model, up, None, None).expect("scroll up");

    assert_eq!(
        model.selected_id(),
        selection_before,
        "the page's wheel never moves the board's selection"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
}

#[test]
fn page_verb_clicks_resolve_through_the_page_legend() {
    let (mut domain, mut model) = deck_of(1);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open the page");
    let id = model.selected_id().expect("one task");

    let hits = board_hit_map(STANDARD, &model);
    // The page legend's `d done` entry: find its verb index from the painted legend.
    let verbs = board_verb_items(&model);
    let done_index = verbs
        .iter()
        .position(|entry| entry.key == "d")
        .expect("the page legend shows d done");
    let verb_area = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::Verb(done_index))
        .expect("the d verb paints a hit region")
        .area;
    let intent = map_board_mouse(&model, &hits, left_click(verb_area.x + 1, verb_area.y))
        .expect("verb click");
    apply_intent(&mut domain, &mut model, intent, None, None).expect("complete via click");
    assert_eq!(
        domain.get(id).expect("task").status,
        HumanStatus::Done,
        "clicking the page's done verb completes its task"
    );
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
}

#[test]
fn page_step_add_footer_chip_routes_to_begin_add_step() {
    let (mut domain, mut model) = deck_of(1);
    let id = model.selected_id().expect("task");
    domain.add_step(id, "existing step").expect("add step");
    model = BoardModel::from_domain(&domain, None);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open");
    let verbs = board_verb_items(&model);
    let step_index = verbs
        .iter()
        .position(|entry| entry.key == "a")
        .expect("visible a step chip");
    let hits = board_hit_map(STANDARD, &model);
    let area = hits
        .regions
        .iter()
        .find(|hit| hit.target == QueueHitTarget::Verb(step_index))
        .expect("step chip hit")
        .area;
    assert_eq!(
        map_board_mouse(&model, &hits, left_click(area.x + 1, area.y)),
        Some(BoardIntent::BeginAddStep)
    );
}

/// The task page painted row by row at the standard board size, so a test can
/// click the coordinates a row actually painted at.
fn page_rows(model: &BoardModel) -> Vec<String> {
    let mut terminal =
        Terminal::new(TestBackend::new(STANDARD.width, STANDARD.height)).expect("test terminal");
    terminal
        .draw(|frame| draw_board(frame, model))
        .expect("draw page");
    let buffer = terminal.backend().buffer();
    (0..STANDARD.height)
        .map(|y| {
            (0..STANDARD.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// AC-21: a single click on a step row moves the step cursor onto
/// that step — a click selects, it never toggles, never opens the editor,
/// never arms a delete mark — and a click on the notes half changes nothing
/// about the cursor.
#[test]
fn clicking_a_step_row_selects_it() {
    let (mut domain, mut model, id) = board_with_task("Click target", HumanStatus::Ready);
    for text in ["alpha step", "bravo step", "charlie step"] {
        domain.add_step(id, text).expect("add step");
    }
    model.sync_from_domain(&domain);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open the page");
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    // The click lands on step 3's painted row, located from the same frame the
    // hit map was recorded beside.
    let hits = board_hit_map(STANDARD, &model);
    let rows = page_rows(&model);
    let step_y = rows
        .iter()
        .position(|row| row.contains("charlie step"))
        .expect("step 3 paints a row");
    let intent = map_board_mouse(&model, &hits, left_click(3, step_y as u16))
        .expect("a step-row click must dispatch a select intent");
    let revision_before = domain.get(id).expect("task").revision;
    apply_intent(&mut domain, &mut model, intent, None, None).expect("apply the click");

    let selected = page_rows(&model);
    assert!(
        selected.iter().any(|row| row.contains("▸ ▪ charlie step")),
        "the cursor must sit on the clicked step:\n{}",
        selected.join("\n")
    );
    assert_eq!(
        model.input_mode(),
        BoardInputMode::TaskPage,
        "a click never opens the editor"
    );
    let task = domain.get(id).expect("task");
    assert_eq!(
        task.steps.iter().map(|step| step.done).collect::<Vec<_>>(),
        vec![false, false, false],
        "a click never toggles a step"
    );
    assert_eq!(task.revision, revision_before, "a click persists nothing");
    assert!(
        !selected.iter().any(|row| row.contains("✗")),
        "a click never arms a delete mark:\n{}",
        selected.join("\n")
    );

    // A click on the notes half changes nothing about the cursor.
    let notes_area = hits
        .regions
        .iter()
        .find(|hit| matches!(hit.target, QueueHitTarget::FormNotes(_)))
        .expect("the page body paints notes hits")
        .area;
    assert_eq!(
        map_board_mouse(&model, &hits, left_click(notes_area.x + 3, notes_area.y)),
        None,
        "a notes-half click dispatches nothing"
    );
    let after = page_rows(&model);
    assert!(
        after.iter().any(|row| row.contains("▸ ▪ charlie step")),
        "the cursor stays on the clicked step after a notes-half click:\n{}",
        after.join("\n")
    );
}

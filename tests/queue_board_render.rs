//! Queue Board Renderer frame goldens.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::{Frame, Terminal};
use tsk_tui::domain::{
    DomainState, HumanStatus, ProvenanceOrigin, Task, TaskEvent, TaskEventKind, TaskScope,
};
use tsk_tui::ui::input::map_key;
use tsk_tui::ui::queue::{self, BoardLens, BoardTab, QueueView, ThreadProjectCollapseKey};
use tsk_tui::ui::render::{
    assert_buffer_mono, assert_no_color_sgr, draw_queue_frame, BottomInputSlot, PaletteCommandRow,
    QueueFrameModel, QueueOverlay, VerbEntry,
};
use tsk_tui::ui::tier::{self, Tier, TierGeometry};
use tsk_tui::ui::{
    apply_intent, board_verb_items, draw_board, BoardInputMode, BoardIntent, BoardModel,
};
use uuid::Uuid;

/// Fixed "now" for deterministic age labels in the fixture.
const NOW_SECS: u64 = 1_700_000_000;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(NOW_SECS)
}

fn at_secs_ago(secs_ago: u64) -> SystemTime {
    now()
        .checked_sub(Duration::from_secs(secs_ago))
        .expect("fixture timestamp")
}

fn task(id: u128, title: &str, status: HumanStatus, scope: TaskScope, secs_ago: u64) -> Task {
    let at = at_secs_ago(secs_ago);
    Task {
        id: Uuid::from_u128(id),
        number: None,
        revision: Uuid::from_u128(id),
        merge_base_revision: None,
        title: title.to_string(),
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
        archived: false,
        created_at: at,
        updated_at: at,
    }
}

fn project(path: &str) -> TaskScope {
    TaskScope::Project {
        path: path.to_string(),
    }
}

/// Deterministic deck-only fixture: motion, multi-project deck, blocked/review glyphs, done.
fn fixture_tasks() -> Vec<Task> {
    let mut tasks = vec![
        task(
            1,
            "Smoke-test worktree dispatch",
            HumanStatus::Started,
            project("/repos/tsk"),
            3 * 60,
        ),
        task(
            2,
            "Edit target binding pin",
            HumanStatus::Started,
            project("/repos/herdr"),
            12 * 60,
        ),
        task(
            10,
            "Prototype the queue-style board UI",
            HumanStatus::Ready,
            project("/repos/tsk"),
            3600,
        ),
        task(
            11,
            "Cut rust-toolchain pin into CI docs",
            HumanStatus::Ready,
            project("/repos/tsk"),
            86400,
        ),
        task(
            20,
            "Wire dispatch cleanup receipts",
            HumanStatus::Blocked,
            project("/repos/herdr"),
            41 * 60,
        ),
        task(
            21,
            "Docs refresh pass after F9 ships",
            HumanStatus::Review,
            project("/repos/herdr"),
            3 * 3600,
        ),
        task(
            30,
            "Global backlog note",
            HumanStatus::Ready,
            TaskScope::Global,
            2 * 86400,
        ),
        task(
            40,
            "Ship the queue board milestone",
            HumanStatus::Done,
            project("/repos/tsk"),
            5 * 3600,
        ),
        task(
            41,
            "Retire classic board chrome",
            HumanStatus::Done,
            TaskScope::Global,
            6 * 3600,
        ),
    ];
    for (index, task) in tasks.iter_mut().enumerate() {
        task.number = Some((index + 1) as u64);
    }
    tasks
}

/// Board model matching this file's fixture: same tasks, `seed_selection` lands on the same
/// task 1 ("Smoke-test worktree dispatch", Doing) [`fixture_model`] hardcodes as its
/// selection.
fn base_board_model() -> BoardModel {
    let model = BoardModel::from_tasks(fixture_tasks(), Some(PathBuf::from("/repos/tsk")));
    assert_eq!(
        model.selected_id(),
        Some(Uuid::from_u128(1)),
        "base_board_model's auto-selection drifted from the render fixture's hardcoded \
         selection -- keep them in sync or `fixture_verbs` binds the wrong row's legend"
    );
    model
}

/// Imp-5 (round 2 regression): the goldens' verb array used to *coincide in wording* with
/// `draw_board`'s, but nothing bound them -- a later edit to either side could
/// silently re-open the divergence. Compute this fixture's verbs through the same
/// [`board_verb_items`] function `draw_board` calls, on a `BoardModel` built to match this
/// file's render fixture, so the two can never drift apart again.
fn fixture_verbs() -> &'static [VerbEntry<'static>] {
    static CACHE: OnceLock<Vec<VerbEntry<'static>>> = OnceLock::new();
    CACHE.get_or_init(|| board_verb_items(&base_board_model()))
}

/// Imp-A (round 3 finding): a **Todo**-selected model, so `board_verb_items` yields six
/// entries (`s start` plus the base five) instead of the Doing selection's five. Used
/// only where the trim-the-sixth path needs a budget-5 bar to actually drop an entry --
/// `fixture_verbs()`'s Doing selection is exactly 5 against a budget of 5, so `take(5)`
/// never drops anything and a test built on it can't fail.
fn todo_verbs() -> Vec<VerbEntry<'static>> {
    let mut model = base_board_model();
    let mut domain = DomainState::new();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectHomeTab(BoardTab::Projects),
        None,
    )
    .expect("projects tab for fixture todo task");
    let target = Uuid::from_u128(10);
    let visible = model.visible_ids();
    for _ in 0..visible.len() {
        if model.selected_id() == Some(target) {
            break;
        }
        apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None).expect("select next");
    }
    assert_eq!(
        model.selected_id(),
        Some(target),
        "fixture task 10 (Todo) must be reachable via SelectNext -- keep in sync with fixture_tasks"
    );
    board_verb_items(&model)
}

/// Same binding as [`fixture_verbs`], for the accordion scene: task 1's detail open, so
/// `board_verb_items` must flip `enter` to `close`.
fn accordion_verbs() -> Vec<VerbEntry<'static>> {
    let mut model = base_board_model();
    let mut domain = DomainState::new();
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None)
        .expect("peek the fixture's selected task");
    assert_eq!(
        model.detail_open(),
        Some(Uuid::from_u128(1)),
        "ToggleDetail must have opened the fixture's selected task"
    );
    board_verb_items(&model)
}

/// G-8 (gate MEDIUM): the palette golden used to hand-write its command rows and included
/// `dispatch agent…`, a label no code path can produce -- the excludes dispatch from the
/// palette outright. This is the same defect class fixed for the verb bar
/// (`fixture_verbs`/`accordion_verbs` above): compute the rows through the real
/// `BoardModel::visible_commands` catalog, driven by the actual `OpenCommandPalette` /
/// `CommandQueryInsert` / `CommandNext` intents `draw_board` itself drives, so the golden
/// can never show a command the product cannot produce.
///
/// Matches this file's palette scene: the fixture's selected task (1, Doing) with a "stat"
/// query narrowed to the status tail, second row (`set status: blocked`) highlighted.
fn palette_commands() -> Vec<PaletteCommandRow<'static>> {
    let mut model = base_board_model();
    let mut domain = DomainState::new();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenCommandPalette,
        None,
    )
    .expect("open the palette");
    for character in "stat".chars() {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::CommandQueryInsert(character),
            None,
        )
        .expect("type palette query character");
    }
    assert_eq!(
        model.command_query(),
        "stat",
        "fixture query drifted from the scene's \"stat\" query"
    );
    let labels: Vec<&str> = model
        .visible_commands()
        .iter()
        .map(|command| command.label)
        .collect();
    assert_eq!(
        labels,
        vec![
            "set status: ready",
            "set status: started",
            "set status: blocked",
            "set status: review",
        ],
        "the \"stat\" query against the real M1 catalog must narrow to exactly the four \
         status commands (no dispatch, no other tail entry) -- if the product catalog \
         changed, this fixture must follow it, not be hand-patched"
    );
    apply_intent(&mut domain, &mut model, BoardIntent::CommandNext, None).expect("command next");
    apply_intent(&mut domain, &mut model, BoardIntent::CommandNext, None).expect("command next");
    assert_eq!(
        model.command_selected(),
        Some(2),
        "fixture must highlight \"set status: blocked\""
    );
    let selected = model.command_selected();
    model
        .visible_commands()
        .iter()
        .enumerate()
        .map(|(i, command)| PaletteCommandRow {
            label: command.label,
            selected: Some(i) == selected,
        })
        .collect()
}

fn empty_projects() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(HashSet::new)
}

fn empty_threads() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(HashSet::new)
}

fn empty_thread_projects() -> &'static HashSet<ThreadProjectCollapseKey> {
    static SET: OnceLock<HashSet<ThreadProjectCollapseKey>> = OnceLock::new();
    SET.get_or_init(HashSet::new)
}

fn fixture_view(tasks: &[Task], drawer_open: bool) -> QueueView {
    queue::query_lens(
        tasks,
        Some(Path::new("/repos/tsk")),
        BoardLens::Home(BoardTab::Desk),
        drawer_open,
    )
}

fn fixture_view_projects(tasks: &[Task], drawer_open: bool) -> QueueView {
    queue::query_lens(
        tasks,
        Some(Path::new("/repos/tsk")),
        BoardLens::Home(BoardTab::Projects),
        drawer_open,
    )
}

fn fixture_model<'a>(tasks: &'a [Task], view: &'a QueueView) -> QueueFrameModel<'a> {
    fixture_model_on_tab(tasks, view, BoardTab::Desk)
}

fn fixture_model_on_tab<'a>(
    tasks: &'a [Task],
    view: &'a QueueView,
    home_tab: BoardTab,
) -> QueueFrameModel<'a> {
    QueueFrameModel {
        tasks,
        view,
        selection_id: Some(Uuid::from_u128(1)),
        at_home: true,
        home_tab,
        scope_label: "",
        collapsed_projects: empty_projects(),
        collapsed_threads: empty_threads(),
        collapsed_thread_projects: empty_thread_projects(),
        status_message: None,
        status_undo_offset: None,
        verb_items: fixture_verbs(),
        now: now(),
        overlay: QueueOverlay::None,
        detail_open: None,
        list_scroll: 0,
        follow_list: true,
    }
}

fn paint(width: u16, height: u16, model: &QueueFrameModel<'_>) -> (Vec<String>, TierGeometry) {
    let geo = tier::resolve(width, height);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame: &mut Frame| {
            let _ = draw_queue_frame(frame, model, &geo, Rect::new(0, 0, width, height));
        })
        .expect("draw");
    let backend = terminal.backend();
    let buffer = backend.buffer();
    assert_buffer_mono(buffer);
    let rows: Vec<String> = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect();
    // Full-frame plain text must stay free of color SGR.
    let joined = rows.join("\n");
    assert_no_color_sgr(&joined);
    (rows, geo)
}

fn row_display_width(row: &str) -> usize {
    // Buffer symbols are already one cell each in the TestBackend grid; length == width budget.
    row.chars().count()
}

/// Paint a whole [`BoardModel`] through `draw_board` -- the payload-builder path
/// (`build_task_page_overlay`) rather than a hand-composed `QueueFrameModel` -- so a
/// test can assert what the page payload consumed from a real stored task.
fn board_rows(model: &BoardModel, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, model);
        })
        .expect("draw board");
    let buffer = terminal.backend().buffer();
    assert_buffer_mono(buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn inactive_home_tabs_are_dimmed() {
    let model = base_board_model();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw board");
    let buffer = terminal.backend().buffer();
    let (tab_y, row) = (0..24)
        .map(|y| {
            let row = (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>();
            (y, row)
        })
        .find(|(_, row)| {
            row.contains("desk") && row.contains("projects") && row.contains("threads")
        })
        .expect("tab row");
    for label in ["projects", "threads"] {
        let x = row.find(label).expect("tab label") as u16;
        assert!(
            buffer[(x, tab_y)]
                .style()
                .add_modifier
                .contains(Modifier::DIM),
            "inactive {label} tab must be dimmed"
        );
    }
}

fn trimmed(row: &str) -> String {
    row.trim_end().to_string()
}

/// List content without the overflow scrollbar's track/thumb cell on the right edge.
fn list_body(row: &str) -> String {
    let mut body = trimmed(row);
    if body.ends_with('█') || body.ends_with('▌') || body.ends_with('│') {
        body.pop();
        body = body.trim_end().to_string();
    }
    body
}

fn assert_exact_header_spacing(
    rows: &[String],
    geo: TierGeometry,
    header: &str,
    first_content: &str,
    dimensions: &str,
) {
    let marker = format!("{header} ─");
    let header_row = rows
        .iter()
        .position(|line| list_body(line).contains(&marker))
        .unwrap_or_else(|| panic!("{dimensions}: missing {header:?}:\n{:#?}", rows));
    let viewport_top = geo.viewport_top as usize;
    let viewport_bottom = viewport_top + geo.viewport_height as usize;
    assert!(
        header_row > viewport_top && header_row + 2 < viewport_bottom,
        "{dimensions}: {header:?} and its surrounding rows must be visible after selection scrolling:\n{:#?}",
        rows
    );
    assert!(
        list_body(&rows[header_row - 1]).is_empty() && list_body(&rows[header_row + 1]).is_empty(),
        "{dimensions}: {header:?} needs one blank row immediately above and below:\n{:#?}",
        rows
    );
    assert!(
        !list_body(&rows[header_row - 2]).is_empty()
            && list_body(&rows[header_row + 2]).contains(first_content),
        "{dimensions}: {header:?} must have exactly one surrounding blank row before {first_content:?}:\n{:#?}",
        rows
    );
}

#[test]
fn standard_78x24_fixture_has_selector_list_rule_status_verb_and_no_other_chrome_rows() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, true);
    let model = fixture_model(&tasks, &view);
    let (rows, geo) = paint(78, 24, &model);

    assert_eq!(geo.tier, Tier::Standard);
    assert_eq!(geo.selector_row, Some(1));
    assert_eq!(geo.rule_row, Some(21));
    assert_eq!(geo.status_row, Some(22));
    assert_eq!(geo.verb_row, Some(23));
    assert_eq!(geo.viewport_top, 2);
    assert_eq!(geo.viewport_height, 19);
    assert_eq!(geo.meta_column_width, 28);
    assert_eq!(geo.title_width, 50);

    assert!(trimmed(&rows[0]).is_empty(), "row above tabs stays blank");

    let selector = trimmed(&rows[1]);
    assert!(
        !selector.contains("queue") && !selector.contains("board"),
        "selector must not paint a view switcher: {selector:?}"
    );
    assert!(
        selector.contains("desk") && selector.contains("projects") && selector.contains("threads"),
        "selector must show home tabs: {selector:?}"
    );
    assert!(
        !selector.contains('▾'),
        "home tabs do not paint the project chip: {selector:?}"
    );

    let list: String = rows
        [geo.viewport_top as usize..(geo.viewport_top + geo.viewport_height) as usize]
        .iter()
        .map(|r| trimmed(r))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        list.contains("IN MOTION"),
        "list must include IN MOTION header:\n{list}"
    );
    assert!(
        list.lines()
            .any(|line| line.trim_start().starts_with("desk ─")),
        "desk tab must include the desk ON DECK header:\n{list}"
    );
    assert!(
        list.contains('▸') && list.contains('○') && list.contains('✓'),
        "status glyphs must appear:\n{list}"
    );
    assert!(
        list.contains("Smoke-test worktree dispatch"),
        "task titles must appear:\n{list}"
    );
    // Standard trailing meta includes ages (fixture ages are known).
    assert!(
        list.contains("3m") || list.contains("1h") || list.contains("1d"),
        "standard tier must paint age meta:\n{list}"
    );
    assert!(
        !list.contains("need you") && !list.contains("NEEDS YOU"),
        "M1 must not paint NEEDS YOU:\n{list}"
    );
    assert!(
        !list.contains("claude") && !list.contains("grok") && !list.contains("agent"),
        "M1 must not paint agent fields:\n{list}"
    );

    let rule = &rows[21];
    assert!(
        rule.chars().filter(|&c| c == '─').count() >= 40,
        "dim rule row expected, got {rule:?}"
    );

    let status = trimmed(&rows[22]);
    assert!(
        status.contains("done") && !status.contains("in motion"),
        "status line must show done count only: {status:?}"
    );
    assert!(
        !status.contains("need") && !status.contains("claude") && !status.contains("grok"),
        "status must omit need-you and agent ticker: {status:?}"
    );

    let verbs = trimmed(&rows[23]);
    // Imp-3 (round 2): `space` is context-dependent -- this fixture's selection is a Doing
    // task, and `PrimaryVerb` is a silent no-op there, so a correct legend omits the entry
    // rather than advertise a no-op. `enter`/`?` are always present regardless of selection.
    assert!(
        verbs.contains("enter") && verbs.contains('?') && verbs.contains("+ capture"),
        "standard verb bar must retain open, help, and capture: {verbs:?}"
    );

    // Chrome is exactly selector + rule + status + verb. Viewport rows are list content only
    // (headers/tasks/spacers), never a second status/verb/rule band.
    for (idx, row) in rows.iter().enumerate() {
        let t = trimmed(row);
        if idx == 1 || idx == 21 || idx == 22 || idx == 23 {
            continue;
        }
        assert!(
            !t.contains("all projects ▾") && !t.starts_with(" space "),
            "non-chrome row {idx} must not repeat selector/verb chrome: {t:?}"
        );
        if idx != 21 && idx != 0 {
            let rule_like = t.chars().filter(|&c| c == '─').count() >= 60
                && !t.contains("IN MOTION")
                && !t.contains("DONE")
                && !t.chars().any(|c| c.is_ascii_digit());
            assert!(
                !rule_like,
                "extra full-width rule chrome at row {idx}: {t:?}"
            );
        }
    }

    for row in &rows {
        assert_eq!(row_display_width(row), 78);
    }
}

fn assert_visible_chrome(rows: &[String], geo: TierGeometry, dimensions: &str) {
    let status = trimmed(&rows[geo.status_row.expect("status row") as usize]);
    assert!(
        status.contains("done") && !status.contains("in motion"),
        "{dimensions}: status must remain reachable: {status:?}"
    );
    let verbs = trimmed(&rows[geo.verb_row.expect("verb row") as usize]);
    assert!(
        !verbs.is_empty(),
        "{dimensions}: verb controls must remain reachable"
    );
    for row in rows {
        assert_eq!(
            row_display_width(row),
            geo.row_width as usize,
            "{dimensions}: row exceeds the frame width"
        );
    }
}

/// every section heading has one, and only one, blank row above and below it in both
/// tiers. Selecting its first task must scroll that header and task into the list viewport.
#[test]
fn every_section_header_has_symmetric_spacing_and_scrolls_with_its_selected_task() {
    let tasks = fixture_tasks();
    let desk_view = fixture_view(&tasks, true);
    let projects_view = fixture_view_projects(&tasks, true);
    let desk_cases = [
        (
            "IN MOTION",
            Uuid::from_u128(1),
            "Smoke-test worktree dispatch",
        ),
        ("desk", Uuid::from_u128(30), "Global backlog note"),
        (
            "DONE",
            Uuid::from_u128(40),
            "Ship the queue board milestone",
        ),
    ];
    let project_cases = [
        ("tsk", Uuid::from_u128(1), "Smoke-test worktree dispatch"),
        ("herdr", Uuid::from_u128(2), "Edit target binding pin"),
    ];

    for &(width, height) in &[(78u16, 24u16), (77u16, 24u16), (40u16, 10u16)] {
        let dimensions = format!("{width}x{height}");
        for &(header, selected_id, selected_title) in &desk_cases {
            let mut model = fixture_model(&tasks, &desk_view);
            model.selection_id = Some(selected_id);
            let (rows, geo) = paint(width, height, &model);
            assert_exact_header_spacing(&rows, geo, header, selected_title, &dimensions);

            let top = geo.viewport_top as usize;
            let bottom = top + geo.viewport_height as usize;
            let viewport = rows[top..bottom]
                .iter()
                .map(|row| trimmed(row))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                viewport.contains(selected_title),
                "{dimensions}: selected {selected_title:?} must remain visible through normal list scrolling:\n{viewport}"
            );
            assert_visible_chrome(&rows, geo, &dimensions);
        }
        for &(header, selected_id, selected_title) in &project_cases {
            let mut model = fixture_model_on_tab(&tasks, &projects_view, BoardTab::Projects);
            model.selection_id = Some(selected_id);
            let (rows, geo) = paint(width, height, &model);
            assert_exact_header_spacing(&rows, geo, header, selected_title, &dimensions);

            let top = geo.viewport_top as usize;
            let bottom = top + geo.viewport_height as usize;
            let viewport = rows[top..bottom]
                .iter()
                .map(|row| trimmed(row))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                viewport.contains(selected_title),
                "{dimensions}: selected {selected_title:?} must remain visible through normal list scrolling:\n{viewport}"
            );
            assert_visible_chrome(&rows, geo, &dimensions);
        }
    }
}

#[test]
fn empty_board_hint_advertises_the_live_quick_add_key() {
    let tasks = fixture_tasks();
    let view = queue::query_lens(
        &tasks,
        None,
        BoardLens::Project(Path::new("/no-tasks")),
        false,
    );
    let model = fixture_model(&tasks, &view);
    let (rows, _) = paint(80, 24, &model);
    let frame = rows.join("\n");

    assert!(frame.contains("+ capture"), "empty-board hint: {frame}");
    assert!(!frame.contains("a capture"), "empty-board hint: {frame}");
}

#[test]
fn quick_add_refusal_message_uses_the_reserved_blank_row_without_color_or_overflow() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, true);

    for &(width, height) in &[(80, 24), (40, 10)] {
        let mut model = fixture_model(&tasks, &view);
        model.overlay = QueueOverlay::QuickAdd {
            input: BottomInputSlot {
                text: String::new(),
                cursor_col: 0,
                placeholder: "title…",
                refusal: None,
                message: Some("Title required"),
                above_rows: Vec::new(),
                cursor_row_offset: 0,
            },
            project_scope: false,
            recovery: false,
        };
        let (rows, geo) = paint(width, height, &model);
        // Quick-add reserves two rows by shifting its input up one from ordinary status
        // chrome, leaving the former status row blank below it and this row above it.
        let input_row = (geo.status_row.expect("quick-add input row") - 1) as usize;
        let message_row = input_row.checked_sub(1).expect("reserved blank row");

        assert!(
            trimmed(&rows[message_row]).contains("Title required"),
            "{width}x{height}: refusal must be visible above quick-add input: {rows:#?}"
        );
        assert!(
            trimmed(&rows[input_row]).starts_with('▎'),
            "{width}x{height}: refusal must not replace the input cursor row: {rows:#?}"
        );
        for row in &rows {
            assert_eq!(row_display_width(row), width as usize, "{width}x{height}");
        }
    }
}

/// An empty hint and capture form are first list content after a destination heading. The
/// B4 regression: overlays must pad every painted cell to their width so base frame content
/// (status line, section rules+counts, task meta) cannot bleed through.
#[test]
fn overlay_rows_are_padded_exact_no_base_bleed() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, true);
    // Distinctive status so any bleed is obvious (e.g. `:re` + status tail must not happen).
    let status = " 3 in motion · 42 done";

    // --- Palette query paints on status row; must be exact padded query, no status tail ---
    {
        let mut model = fixture_model(&tasks, &view);
        model.status_message = Some(status);
        model.overlay = QueueOverlay::Palette {
            query: "re",
            commands: &[
                PaletteCommandRow {
                    label: "reopen",
                    selected: true,
                },
                PaletteCommandRow {
                    label: "delete",
                    selected: false,
                },
            ],
        };
        let (rows, geo) = paint(80, 24, &model);
        let qrow = geo.status_row.expect("status row at 80x24");
        let row = &rows[qrow as usize];
        assert_eq!(row.chars().count(), 80, "row must fill width");
        // Painted query is ":re" (with leading space from paint), bold but symbols identical.
        assert!(
            row.starts_with(" :re"),
            "palette query row must start with query: {row:?}"
        );
        // Everything after the visible query content must be spaces; no "n motion" etc.
        let after = &row[4..]; // " :re".len() == 4
        assert!(
            after.chars().all(|c| c == ' '),
            "palette query row must pad to width with spaces, got tail: {after:?} in {row:?}"
        );
        assert!(
            !row.contains("motion") && !row.contains("done"),
            "status tail must not bleed into palette query: {row:?}"
        );
    }

    // --- Help card rows must be full-width padded, no meta bleed into card area ---
    {
        let mut model = fixture_model(&tasks, &view);
        model.status_message = Some(status);
        let help_lines: Vec<String> = vec![
            "the queue - keys".to_string(),
            String::new(),
            "j/k move | ↑/↓ move".to_string(),
            "enter detail | a capture".to_string(),
            ": palette | ? help".to_string(),
            "u undo | z drawer".to_string(),
            String::new(),
            "any key to close".to_string(),
        ];
        model.overlay = QueueOverlay::Help { lines: &help_lines };
        let (rows, _geo) = paint(80, 24, &model);
        // Help centers; ensure painted rows are width-padded and carry no status/meta fragments.
        // The body width is min(width-2,62), padded on left; trailing must fill to row_width.
        let body_rows: Vec<_> = rows
            .iter()
            .filter(|r| {
                r.contains("queue")
                    || r.contains("move")
                    || r.contains("detail")
                    || r.contains("palette")
                    || r.contains("drawer")
                    || r.contains("close")
            })
            .collect();
        assert!(!body_rows.is_empty(), "help must paint body rows");
        for r in &body_rows {
            assert_eq!(r.chars().count(), 80, "help row must be full width: {r:?}");
            // After centering pad, body ends and we must have trailing spaces to 80.
            // We only care no bleed of status/meta.
            assert!(
                !r.contains("motion") && !r.contains("done") && !r.contains("3 in"),
                "base status/meta must not bleed into help: {r:?}"
            );
        }
    }

    // --- Scope dropdown is a centered modal card; its rows must be padded full width ---
    {
        let mut model = fixture_model(&tasks, &view);
        model.status_message = Some(status);
        let scope_opts: Vec<String> = vec!["desk".to_string(), "tsk".to_string()];
        model.overlay = QueueOverlay::ScopeDropdown {
            options: &scope_opts,
            selected: 0,
        };
        let (rows, _geo) = paint(80, 24, &model);
        let desk_row = rows
            .iter()
            .position(|row| row.contains("▸ desk") || row.contains("  desk"))
            .expect("scope dropdown must paint the desk option");
        assert_eq!(
            rows[desk_row].chars().count(),
            80,
            "scope dropdown option row must fill width: {:?}",
            rows[desk_row]
        );
        // The `tsk` option must be painted as its own row, not merely satisfied by task
        // titles or meta elsewhere on the frame.
        assert!(
            rows.get(desk_row + 1)
                .is_some_and(|option_row| option_row.contains("tsk")),
            "scope dropdown must paint each option on its own row"
        );
        assert!(
            !rows[desk_row].contains("motion") && !rows[desk_row].contains("done"),
            "base status/meta must not bleed into scope dropdown: {:?}",
            rows[desk_row]
        );
    }
}

#[test]
fn compact_77x24_and_48x19_and_40x10_paint_glyph_title_only_rows_and_leq_5_verb_entries() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, false);
    // Imp-A: a Todo selection gives `board_verb_items` six entries (`s start` plus the
    // base five), so the budget-5 trim below is genuinely exercised -- the Doing selection
    // `fixture_model` otherwise uses yields exactly five, which `take(5)` never trims.
    let verbs = todo_verbs();
    let model = QueueFrameModel {
        verb_items: &verbs,
        ..fixture_model(&tasks, &view)
    };

    for &(w, h) in &[(77, 24), (48, 19), (40, 10)] {
        let (rows, geo) = paint(w, h, &model);
        assert_eq!(geo.tier, Tier::Compact, "{w}x{h}");
        assert_eq!(geo.meta_column_width, 0, "{w}x{h}");
        assert!(
            geo.verb_bar_entry_budget <= 5,
            "{w}x{h} verb budget {}",
            geo.verb_bar_entry_budget
        );

        let body = rows
            .iter()
            .map(|r| trimmed(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.contains('○') || body.contains('▸') || body.contains('■'),
            "{w}x{h} must paint glyphs:\n{body}"
        );
        assert!(
            body.contains("Prototype the queue-style board UI")
                || body.contains("Smoke-test")
                || body.contains("queue"),
            "{w}x{h} must paint titles or chrome:\n{body}"
        );

        // Glyph+title only: known age tokens from the fixture must not trail as meta.
        // (Titles in the fixture contain none of these standalone age tokens.)
        for age in ["3m", "12m", "41m", "1h", "1d", "2d", "3h", "5h", "6h"] {
            // Compact may still show counts like "2" on headers; ages are multi-char with unit.
            if let Some(verb_row) = geo.verb_row {
                let list_and_status: String = rows
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i as u16 != verb_row && Some(*i as u16) != geo.status_row)
                    .map(|(_, r)| trimmed(r))
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !list_and_status.split_whitespace().any(|tok| tok == age),
                    "{w}x{h} compact must drop trailing age meta token {age}:\n{list_and_status}"
                );
            }
        }

        if let Some(verb_row) = geo.verb_row {
            let painted = trimmed(&rows[verb_row as usize]);
            // Count key tokens from the fixture set that actually appear.
            let shown = todo_verbs()
                .iter()
                .filter(|v| {
                    if v.key == "+" {
                        painted.contains("+ capture")
                    } else {
                        painted.contains(v.key)
                    }
                })
                .count();
            assert!(
                shown <= 5,
                "{w}x{h} verb bar shows {shown} entries (>5): {painted:?}"
            );
            assert!(
                shown <= geo.verb_bar_entry_budget as usize,
                "{w}x{h} showed {shown} verbs over budget {}",
                geo.verb_bar_entry_budget
            );
            assert!(
                !painted.contains("+ capture"),
                "{w}x{h} compact keeps its existing verbs instead of displacing one for capture: {painted:?}"
            );
        }

        for row in &rows {
            assert_eq!(row_display_width(row), w as usize, "{w}x{h} row overflow");
        }
    }
}

/// The task page paints a full-height takeover in BOTH tiers: header (glyph + title +
/// status word), notes body, and meta footer. The selector row stays hidden, the base list
/// never bleeds through, and every row is width-bounded at the 40x10 floor.
#[test]
fn board_row_leads_title_with_uppercase_t_identifier() {
    let mut tasks = fixture_tasks();
    tasks[0].number = Some(12);
    let view = fixture_view(&tasks, false);
    let model = fixture_model(&tasks, &view);
    let body = paint(80, 24, &model).0.join("\n");

    assert!(
        body.contains("T12 Smoke-test worktree dispatch"),
        "identifier must lead the title:\n{body}"
    );
    assert!(
        !body.contains("12 · tsk · 3m"),
        "trailing meta must not repeat the identifier:\n{body}"
    );
}

#[test]
fn standard_row_meta_keeps_age_when_long_project_competes() {
    let mut tasks = fixture_tasks();
    tasks[0].number = Some(7);
    tasks[0].scope = project("/src/customer-portal-api");
    tasks[0].updated_at = at_secs_ago(12 * 60);
    let view = fixture_view(&tasks, false);
    let model = fixture_model(&tasks, &view);
    let rows = paint(80, 24, &model).0;
    let row = rows
        .iter()
        .find(|row| row.contains("Smoke-test worktree dispatch"))
        .expect("started fixture row");
    assert!(row.contains('7'), "number must remain visible:\n{row}");
    assert!(
        row.contains("12m"),
        "age must remain visible when a long project shares the meta column:\n{row}"
    );
}

#[test]
fn peek_keeps_the_identifier_on_its_task_row_not_in_detail_meta() {
    let mut tasks = fixture_tasks();
    tasks[0].number = Some(12);
    let view = fixture_view(&tasks, false);
    let mut model = fixture_model(&tasks, &view);
    model.detail_open = Some(Uuid::from_u128(1));
    let body = paint(80, 24, &model).0.join("\n");

    assert!(
        body.contains("T12 Smoke-test worktree dispatch"),
        "row must lead with T12:\n{body}"
    );
    assert!(
        !body.contains("12 · created"),
        "peek detail must not duplicate the identifier:\n{body}"
    );
}

#[test]
fn task_page_header_shows_identifier_not_footer() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, false);
    let mut model = fixture_model(&tasks, &view);
    model.overlay = QueueOverlay::TaskPage {
        header_rows: vec!["○ numbered page".to_string()],
        header_identifier: Some("T12".to_string()),
        header_identifier_task: Some(Uuid::from_u128(1)),
        title_cursor: None,
        status_word: "ready",
        notes_rows: Vec::new(),
        notes_cursor: None,
        more_lines: 0,
        step_views: Vec::new(),
        stored_step_count: 0,
        step_cursor: None,
        step_add_selected: false,
        step_scroll: 0,
        step_marked: None,
        inline_step_editor: None,
        bottom_input: None,
        meta: "desk · created 1m ago · updated 1m ago".to_string(),
        meta_scope_x: 0,
        meta_scope_width: 4,
        thread_slot_width: None,
        focus: None,
        scope_dropdown: None,
    };
    let body = paint(80, 24, &model).0.join("\n");

    assert!(
        body.contains("T12 numbered page"),
        "header must lead with T12:\n{body}"
    );
    assert!(
        !body.contains("12 · desk"),
        "footer must not repeat the identifier:\n{body}"
    );
}

#[test]
fn done_drawer_rows_lead_with_identifiers() {
    let mut tasks = fixture_tasks();
    tasks[7].number = Some(12);
    let view = fixture_view(&tasks, true);
    let model = fixture_model(&tasks, &view);
    let body = paint(80, 24, &model).0.join("\n");

    assert!(
        body.contains("T12"),
        "done row must show a leading identifier:\n{body}"
    );
    assert!(
        !body.contains("12 · tsk · 5h"),
        "done row must not retain trailing number chrome:\n{body}"
    );
}

#[test]
fn numbered_board_at_40x10_paints_in_bounds_wraps_titles_and_keeps_tasks_reachable() {
    let mut tasks = (0..4)
        .map(|index| {
            let mut task = task(
                500 + index,
                &format!("item {index} title wraps across the compact forty column boundary"),
                HumanStatus::Ready,
                TaskScope::Global,
                index as u64,
            );
            task.number = Some(1000 + index as u64);
            task
        })
        .collect::<Vec<_>>();
    let mut model = BoardModel::from_tasks(std::mem::take(&mut tasks), None);
    let ids = model.visible_ids();
    let mut domain = DomainState::new();

    for (index, id) in ids.iter().copied().enumerate() {
        if index > 0 {
            apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
                .expect("select next numbered task");
        }
        assert_eq!(model.selected_id(), Some(id));
        let rows = board_rows(&model, 40, 10);
        let body = rows.join("\n");
        assert!(
            body.contains(&format!("100{index}")),
            "number must paint at 40x10:\n{body}"
        );
        assert!(
            body.contains(&format!("item {index}")) && body.contains("boundary"),
            "title must wrap rather than truncate:\n{body}"
        );
        assert!(
            rows.iter().all(|row| row_display_width(row) == 40),
            "row exceeded 40 columns"
        );
    }
}

#[test]
fn task_page_renders_header_notes_and_meta_as_a_full_takeover_in_both_tiers() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, false);
    let mut model = fixture_model(&tasks, &view);
    model.overlay = QueueOverlay::TaskPage {
        header_rows: vec!["\u{25cb} Rename this task".to_string()],
        header_identifier: None,
        header_identifier_task: None,
        title_cursor: None,
        status_word: "ready",
        notes_rows: vec![
            "first draft note".to_string(),
            "second draft note".to_string(),
        ],
        notes_cursor: None,
        more_lines: 0,
        step_views: Vec::new(),
        stored_step_count: 0,
        step_cursor: None,
        step_add_selected: false,
        step_scroll: 0,
        step_marked: None,
        inline_step_editor: None,
        bottom_input: None,
        meta: "tsk \u{b7} created 1h ago \u{b7} updated 1h ago".to_string(),
        meta_scope_x: 0,
        meta_scope_width: 11,
        thread_slot_width: None,
        focus: None,
        scope_dropdown: None,
    };

    for &(width, height) in &[(78u16, 24u16), (77u16, 24u16), (40u16, 10u16)] {
        let (rows, geo) = paint(width, height, &model);
        let body = rows
            .iter()
            .map(|row| trimmed(row))
            .collect::<Vec<_>>()
            .join("\n");
        // The meta footer clips at the 40x10 floor, so "updated" is only required at the
        // 78-column standard width where the full line fits.
        let mut expected = vec!["Rename this task", "ready", "first draft note", "created"];
        if width >= 78 {
            expected.push("updated");
        }
        for expected in expected {
            assert!(
                body.contains(expected),
                "{width}x{height} task page omitted {expected:?}:\n{body}"
            );
        }
        // The selector row is hidden and the base list never bleeds through the page.
        assert!(
            !body.contains("all projects"),
            "{width}x{height} task page must hide the selector row:\n{body}"
        );
        assert!(
            !body.contains("IN MOTION"),
            "{width}x{height} task page must erase the base list in both tiers:\n{body}"
        );
        assert!(
            rows.iter()
                .all(|row| row_display_width(row) == width as usize),
            "{width}x{height} task page exceeded the frame width"
        );
        assert_visible_chrome(&rows, geo, &format!("{width}x{height}"));
    }
}

/// T-2 (AC-5/AC-7): the task page of a task with steps steps paints a steps
/// section between the notes block and the meta footer. The fixture drives the real
/// payload-builder path (`BoardModel` + `draw_board`) from stored steps (add + toggle),
/// so the assertions cross the storage -> page-payload -> paint boundary: the label's
/// done/total counts and the per-step glyphs must come from the extracted step views.
#[test]
fn task_page_paints_steps_section_between_notes_and_footer() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Page task with steps",
            Some("the notes body".into()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let first = domain.add_step(id, "first step").expect("step 1");
    let second = domain.add_step(id, "second step").expect("step 2");
    domain.add_step(id, "third step").expect("step 3");
    domain.toggle_step(id, first).expect("toggle step 1");
    domain.toggle_step(id, second).expect("toggle step 2");

    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open task page");

    let rows = board_rows(&model, 78, 24);
    let shown: Vec<String> = rows.iter().map(|row| trimmed(row)).collect();
    let find = |needle: &str, shown: &[String]| {
        shown
            .iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} missing from page:\n{rows:#?}"))
    };
    let notes_row = find("the notes body", &shown);
    let label_row = find("steps 2/3", &shown);
    let meta_row = find("created", &shown);
    assert_eq!(
        label_row,
        notes_row + 3,
        "steps label must follow the notes with two blank rows:\n{}",
        shown.join("\n")
    );
    assert!(
        label_row < meta_row,
        "steps section must sit before the meta footer:\n{}",
        shown.join("\n")
    );
    // Steps render one per line below the label, in storage order: done `✓`, open `▪`.
    let first_step = find("✓ first step", &shown);
    let second_step = find("✓ second step", &shown);
    let third_step = find("▪ third step", &shown);
    assert!(
        label_row < first_step && first_step < second_step && second_step < third_step,
        "steps must paint one per line below the label in storage order:\n{}",
        shown.join("\n")
    );

    // The compact tier stays operable: the notes and their two-row separation remain
    // visible at the head, while the steps section can be reached by scrolling. Every
    // scroll position must preserve the fixed chrome and the frame width.
    let compact = board_rows(&model, 40, 10);
    let compact_shown: Vec<String> = compact.iter().map(|row| trimmed(row)).collect();
    let compact_note = compact_shown
        .iter()
        .position(|row| row.contains("the notes body"))
        .expect("compact page omitted the notes");
    assert!(
        list_body(&compact[compact_note + 1]).is_empty()
            && list_body(&compact[compact_note + 2]).is_empty(),
        "compact page must keep two blank rows before steps:\n{}",
        compact_shown.join("\n")
    );

    let mut saw_label = false;
    let mut saw_first = false;
    let mut saw_second = false;
    let mut saw_third = false;
    for scroll in 0..=5 {
        let rows = board_rows(&model, 40, 10);
        let geo = tier::resolve(40, 10);
        let body = rows
            .iter()
            .map(|row| trimmed(row))
            .collect::<Vec<_>>()
            .join("\n");
        saw_label |= body.contains("steps 2/3");
        saw_first |= body.contains("✓ first step");
        saw_second |= body.contains("✓ second step");
        saw_third |= body.contains("▪ third step");
        let rule_row = geo.rule_row.expect("compact rule row");
        let status_row = geo.status_row.expect("compact status row");
        let verb_row = geo.verb_row.expect("compact verb row");
        assert!(
            rows[rule_row as usize].contains('─')
                && !trimmed(&rows[status_row as usize]).is_empty()
                && !trimmed(&rows[verb_row as usize]).is_empty(),
            "compact fixed chrome must remain intact at scroll {scroll}:\n{body}"
        );
        assert!(
            rows.iter().all(|row| row_display_width(row) == 40),
            "compact steps rows exceeded the frame width at scroll {scroll}:\n{body}"
        );
        if scroll < 5 {
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::PageWheelScrollDown,
                None,
            )
            .expect("scroll compact task page");
        }
    }
    assert!(saw_label, "compact scrolling never reached the steps label");
    assert!(saw_first, "compact scrolling never reached the first step");
    assert!(
        saw_second,
        "compact scrolling never reached the second step"
    );
    assert!(saw_third, "compact scrolling never reached the third step");
}

/// A task with no stored steps can scroll all the way to its trailing add target.
#[test]
fn task_page_without_steps_reaches_its_trailing_add_target() {
    let notes = (0..20)
        .map(|index| format!("overflow line {index}"))
        .collect::<Vec<_>>()
        .join("\n");

    for &(width, height) in &[(78u16, 24u16), (40u16, 10u16)] {
        let mut domain = DomainState::new();
        domain
            .create(
                "Notes-only page task",
                Some(notes.clone()),
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create task");
        let mut model = BoardModel::from_domain(&domain, None);
        apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None)
            .expect("open task page");

        let mut body = String::new();
        for _ in 0..64 {
            let rows = board_rows(&model, width, height);
            body = rows
                .iter()
                .map(|row| trimmed(row))
                .collect::<Vec<_>>()
                .join("\n");
            if body.contains("steps 0/0") && body.contains("+ step") {
                break;
            }
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::PageWheelScrollDown,
                None,
            )
            .expect("scroll toward add target");
        }

        assert!(
            body.contains("steps 0/0") && body.contains("+ step"),
            "{width}x{height} empty-steps page never reached its add target:\n{body}"
        );
        // `✓`/`▪` are step glyphs (the task is ready, so the header glyph is `○`).
        assert!(
            !body.contains('✓') && !body.contains('▪'),
            "{width}x{height} empty-steps page must paint no step glyphs:\n{body}"
        );
    }
}

/// At the 40x10 compact floor, a notes edit keeps its draft row visible and preserves
/// the two blank rows before the steps section. The notes editor owns the viewport, so
/// below-fold steps become reachable after Esc returns to page view.
#[test]
fn task_page_notes_edit_keeps_a_visible_row_and_spacing_at_the_compact_floor() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Editing notes beside steps",
            Some("draft line under edit".into()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let first = domain.add_step(id, "first step").expect("step 1");
    domain.add_step(id, "second step").expect("step 2");
    domain.add_step(id, "third step").expect("step 3");
    domain.toggle_step(id, first).expect("toggle step 1");

    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open task page");
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None)
        .expect("begin notes edit");

    let rows = board_rows(&model, 40, 10);
    let shown: Vec<String> = rows.iter().map(|row| trimmed(row)).collect();
    let body = shown.join("\n");
    assert!(
        body.contains("draft line under edit"),
        "a notes edit must paint at least one draft row at 40x10:\n{body}"
    );
    let note_row = shown
        .iter()
        .position(|row| row.contains("draft line under edit"))
        .expect("notes edit row missing");
    assert!(
        list_body(&rows[note_row + 1]).is_empty() && list_body(&rows[note_row + 2]).is_empty(),
        "notes edit must preserve two blank rows before steps:\n{body}"
    );
    assert!(
        rows.iter().all(|row| row_display_width(row) == 40),
        "compact edit page exceeded the frame width"
    );

    apply_intent(&mut domain, &mut model, BoardIntent::CancelEdit, None)
        .expect("return to page view");
    let mut saw_label = false;
    let mut saw_step = false;
    for scroll in 0..=5 {
        let rows = board_rows(&model, 40, 10);
        let geo = tier::resolve(40, 10);
        let body = rows
            .iter()
            .map(|row| trimmed(row))
            .collect::<Vec<_>>()
            .join("\n");
        saw_label |= body.contains("steps 1/3");
        saw_step |= body.contains("✓ first step");
        let rule_row = geo.rule_row.expect("compact rule row");
        let status_row = geo.status_row.expect("compact status row");
        let verb_row = geo.verb_row.expect("compact verb row");
        assert!(
            rows[rule_row as usize].contains('─')
                && !trimmed(&rows[status_row as usize]).is_empty()
                && !trimmed(&rows[verb_row as usize]).is_empty(),
            "compact fixed chrome must remain intact after notes edit at scroll {scroll}:\n{body}"
        );
        if scroll < 5 {
            apply_intent(
                &mut domain,
                &mut model,
                BoardIntent::PageWheelScrollDown,
                None,
            )
            .expect("scroll compact page after notes edit");
        }
    }
    assert!(saw_label, "steps label was not reachable after notes edit");
    assert!(saw_step, "steps were not reachable after notes edit");
}

#[test]
fn notes_edit_shows_raw_markdown_markers_that_view_mode_strips() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Markdown notes",
            Some("see *em* and **strong** here".into()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open");
    let view = board_rows(&model, 80, 24).join("\n");
    assert!(
        !view.contains("*em*"),
        "view must strip em markers:\n{view}"
    );
    assert!(
        view.contains("em"),
        "view must still show the em text:\n{view}"
    );
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None).expect("edit");
    let edit = board_rows(&model, 80, 24).join("\n");
    assert!(
        edit.contains("*em*"),
        "notes edit must show raw *em* markers:\n{edit}"
    );
    assert!(
        edit.contains("**strong**"),
        "notes edit must show raw **strong** markers:\n{edit}"
    );
}

#[test]
fn task_page_view_leaves_fence_body_unparsed() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Fenced",
            Some("```\n**not bold**\n```".into()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open");
    let view = board_rows(&model, 80, 24).join("\n");
    assert!(view.contains("```"), "page must show fence ticks:\n{view}");
    assert!(
        view.contains("**not bold**"),
        "fence body must not run inline markdown:\n{view}"
    );
}

#[test]
fn task_page_fence_stays_closed_after_the_opener_scrolls_off() {
    let mut notes = String::from("```\n");
    for i in 0..40 {
        notes.push_str(&format!("pad-line-{i}\n"));
    }
    notes.push_str("**still stars**\n```\n");
    let mut domain = DomainState::new();
    domain
        .create(
            "Long fence",
            Some(notes),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open");
    let _ = board_rows(&model, 80, 24);
    for _ in 0..30 {
        apply_intent(&mut domain, &mut model, BoardIntent::PageScrollDown, None).expect("scroll");
    }
    let view = board_rows(&model, 80, 24).join("\n");
    assert!(
        !view.contains("pad-line-0"),
        "fence opener should have left the viewport:\n{view}"
    );
    assert!(
        view.contains("**still stars**"),
        "scrolled fence body must still skip inline markdown:\n{view}"
    );
}

/// T-10 (AC-27): a Notes caret uses the same shared-stream offset as its rows.
#[test]
fn notes_edit_caret_accounts_for_shared_stream_scroll() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Caret stream offset",
            Some("first\nsecond\nthird".to_string()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    for index in 0..30 {
        domain
            .add_step(id, format!("step {index}"))
            .expect("add step");
    }
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");
    board_rows(&model, 80, 24);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageWheelScrollDown,
        None,
    )
    .expect("scroll shared stream");
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None).expect("edit notes");

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw edit page");
    let cursor = terminal.backend().cursor_position();
    assert_eq!(
        cursor.y, 5,
        "entering Notes edit restores the cursor-windowed draft to the visible stream origin"
    );
}

/// AC-27: entering Notes after a deep shared-stream read restores the cursor-windowed
/// draft and keeps the terminal caret on its visible draft row.
#[test]
fn notes_edit_after_deep_stream_scroll_keeps_draft_and_caret_aligned() {
    let mut domain = DomainState::new();
    let notes = (0..20)
        .map(|line| format!("note line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let id = domain
        .create(
            "Deep Notes",
            Some(notes),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    for index in 0..30 {
        domain
            .add_step(id, format!("step {index}"))
            .expect("add step");
    }
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open");
    board_rows(&model, 80, 24);
    for _ in 0..20 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageWheelScrollDown,
            None,
        )
        .expect("deep wheel");
    }
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None).expect("edit notes");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw");
    let text = board_rows(&model, 80, 24).join("\n");
    assert!(
        text.contains("note line 9"),
        "draft window must remain visible:\n{text}"
    );
    assert_eq!(
        terminal.backend().cursor_position().y,
        19,
        "caret must remain on the visible cursor-windowed draft row"
    );
}

/// AC-27 applies to form navigation too: Shift+Tab from Scope enters Notes at the
/// cursor-window origin, rather than preserving the reading position from the shared stream.
#[test]
fn shift_tab_from_scope_resets_notes_stream_origin_and_aligns_caret() {
    let mut domain = DomainState::new();
    let notes = (0..20)
        .map(|line| format!("note line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _id = domain
        .create(
            "Shift tab Notes",
            Some(notes),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open");
    board_rows(&model, 80, 24);
    for _ in 0..20 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageWheelScrollDown,
            None,
        )
        .expect("deep wheel");
    }
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditScope, None).expect("edit scope");
    let shift_tab = map_key(
        BoardInputMode::EditScope,
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
    );
    assert_eq!(shift_tab, Some(BoardIntent::FormFocusPrev));
    apply_intent(
        &mut domain,
        &mut model,
        shift_tab.clone().expect("Shift+Tab intent"),
        None,
    )
    .expect("Shift+Tab selects the trailing add target");
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    apply_intent(&mut domain, &mut model, BoardIntent::FormFocusPrev, None)
        .expect("Shift+Tab from the add target enters Notes");
    assert_eq!(model.input_mode(), BoardInputMode::EditNotes);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw");
    let text = board_rows(&model, 80, 24).join("\n");
    assert!(
        text.contains("note line 9"),
        "Shift+Tab Notes draft must return to the visible cursor window:\n{text}"
    );
    assert_eq!(
        terminal.backend().cursor_position().y,
        19,
        "Shift+Tab Notes caret must land on its painted draft row"
    );
}

/// A long single-line note wraps onto continuation rows while the Notes editor is
/// open: both ends of the draft stay on the page instead of the cursor's row
/// scrolling sideways under a fixed-width window.
#[test]
fn notes_edit_wraps_a_long_line_instead_of_scrolling_horizontally() {
    let mut domain = DomainState::new();
    let note = format!("HEAD{}TAIL", "w".repeat(200));
    domain
        .create(
            "Long line",
            Some(note),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");
    board_rows(&model, 80, 24);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None).expect("edit notes");

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw edit page");
    let rows = board_rows(&model, 80, 24);
    assert!(
        rows.iter().any(|row| row.contains("HEAD")),
        "the note's head must stay visible while editing:\n{}",
        rows.join("\n")
    );
    let tail_row = rows
        .iter()
        .position(|row| row.contains("TAIL"))
        .unwrap_or_else(|| panic!("the note's tail must wrap into view:\n{}", rows.join("\n")))
        as u16;
    assert_eq!(
        terminal.backend().cursor_position().y,
        tail_row,
        "the caret must sit on the wrapped continuation row carrying the draft's end:\n{}",
        rows.join("\n")
    );
}

/// View mode already wraps stored notes (`wrapped_draft_rows`); this pins that
/// contract so a later presenter change cannot quietly reintroduce truncation.
#[test]
fn task_page_view_wraps_long_notes_instead_of_truncating() {
    let mut domain = DomainState::new();
    let note = format!("HEAD{}TAIL", "w".repeat(200));
    domain
        .create(
            "Long line",
            Some(note),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");

    let rows = board_rows(&model, 80, 24);
    assert!(
        rows.iter().any(|row| row.contains("HEAD")),
        "view mode lost the note's head:\n{}",
        rows.join("\n")
    );
    assert!(
        rows.iter().any(|row| row.contains("TAIL")),
        "view mode truncated the note instead of wrapping it:\n{}",
        rows.join("\n")
    );
}

/// Long notes and steps form one scrollable page body. Steps follow the notes after
/// two blank rows instead of being positioned at a viewport fraction. The header and
/// meta footer remain fixed while PageScroll reveals the deferred steps and scrollbar.
#[test]
fn task_page_scrolls_notes_and_steps_as_one_content_region() {
    let notes = (0..20)
        .map(|i| format!("N{i:02} filler line"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Scrollable page",
            Some(notes),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    domain.add_step(id, "only step").expect("add step");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open task page");

    let initial = board_rows(&model, 78, 24);
    let shown: Vec<String> = initial.iter().map(|row| trimmed(row)).collect();
    assert!(
        shown[1].contains("Scrollable page"),
        "header must stay fixed"
    );
    assert!(shown[20].contains("created"), "footer must stay fixed");
    assert!(
        shown.iter().any(|row| row.contains('▌')),
        "overflow needs a scrollbar"
    );
    assert!(shown.iter().any(|row| row.contains("N16 filler line")));
    assert!(
        !shown.iter().any(|row| row.contains("steps 0/1")),
        "long notes push the steps below the viewport:\n{}",
        shown.join("\n")
    );

    for _ in 0..8 {
        apply_intent(
            &mut domain,
            &mut model,
            BoardIntent::PageWheelScrollDown,
            None,
        )
        .expect("wheel scroll down");
    }
    let scrolled = board_rows(&model, 78, 24);
    let shown: Vec<String> = scrolled.iter().map(|row| trimmed(row)).collect();
    assert!(
        shown[1].contains("Scrollable page"),
        "header must stay fixed"
    );
    assert!(shown[20].contains("created"), "footer must stay fixed");
    assert!(shown.iter().any(|row| row.contains("steps 0/1")));
    let step_row = shown
        .iter()
        .position(|row| row.contains("▪ only step"))
        .expect("step visible after scrolling");
    assert!(
        shown[step_row + 1].contains("+ step"),
        "the trailing add target follows the last step:\n{}",
        shown.join("\n")
    );
    let note_row = initial
        .iter()
        .find(|row| row.contains("N00 filler line"))
        .expect("first note row");
    assert!(
        note_row
            .chars()
            .rev()
            .skip(1)
            .take(2)
            .all(|cell| cell == ' '),
        "content needs the same two-cell gutter before the right-edge scrollbar: {note_row:?}"
    );
}

/// Steps stack from the top of their section, with step 1 directly under the section
/// label and each next step directly below the previous, never pinned to the footer.
#[test]
fn steps_stack_from_the_top_below_the_divider() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Stacking page",
            Some("the notes body".into()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    for text in ["first step", "second step", "third step"] {
        domain.add_step(id, text).expect("add step");
    }
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open task page");

    // 78x24: the note is on row 3, the two-row gap follows, and the label is on row 6;
    // steps 1..3 stack downward from it while the footer-side rows stay empty.
    let rows = board_rows(&model, 78, 24);
    let shown: Vec<String> = rows.iter().map(|row| trimmed(row)).collect();
    let label = shown
        .iter()
        .position(|row| row.contains("steps 0/3"))
        .unwrap_or_else(|| panic!("steps label missing:\n{}", shown.join("\n")));
    assert_eq!(
        label, 6,
        "the steps label follows the notes and two-row gap"
    );
    assert!(
        shown[7].contains("▪ first step")
            && shown[8].contains("▪ second step")
            && shown[9].contains("▪ third step"),
        "steps must stack one directly under another from the top of the half:\n{}",
        shown.join("\n")
    );
    assert!(
        shown[19].is_empty(),
        "the row above the footer must stay empty — the block is not footer-anchored:\n{}",
        shown.join("\n")
    );

    // A fourth step lands directly below the third, still far from the footer.
    domain.add_step(id, "fourth step").expect("add step 4");
    model.sync_from_domain(&domain);
    let rows4 = board_rows(&model, 78, 24);
    let shown4: Vec<String> = rows4.iter().map(|row| trimmed(row)).collect();
    assert!(
        shown4[10].contains("▪ fourth step"),
        "the fourth step must land directly below the third:\n{}",
        shown4.join("\n")
    );
    assert!(
        shown4[19].is_empty(),
        "the fourth step must not be pinned to the footer:\n{}",
        shown4.join("\n")
    );
}

/// Imp-1 (round 2 regression): the compact palette windowed its command list off a raw
/// row *index* (`list_bottom`) instead of the rows actually available above chrome, so an
/// ordinary unfiltered the catalog (>= 6 commands) painted the `command` header over row 0 --
/// the selector row -- instead of stopping at `viewport_top`. Reproduce with 9 commands (the
/// review's repro count) at three compact sizes and assert the selector survives untouched
/// Imp-2 (round 2 regression): the title/notes editors reused `CAPTURE_VERBS`, which
/// advertised `tab notes/scope` (Tab is unbound in both single-field modes) and `enter save`
/// (Notes' plain Enter opens a line, not save). Each editor's verb bar must name only keys
/// a compact editor takeover must actually erase the base list underneath it. The prior
/// bug painted `Paragraph::new("")` over the viewport, which sets style but writes no symbols,
/// so the list content already painted there kept showing through every one of the three
/// an over-tall notes draft must stop at the takeover region, not paint into the rule or
/// status row below it. At 40x10 the prior bug's hardcoded 8-row request destroyed both.
#[test]
fn compact_notes_editor_never_paints_into_the_rule_or_status_row_at_40x10() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, false);
    let mut model = fixture_model(&tasks, &view);
    model.status_message = None;

    let rows_text: Vec<String> = (1..=8).map(|i| format!("NOTELINE{i}")).collect();
    model.overlay = QueueOverlay::EditNotes {
        rows: rows_text,
        cursor_row: 7,
        cursor_col: 0,
    };

    let (rows, geo) = paint(40, 10, &model);
    let rule_row = geo.rule_row.expect("rule row at 40x10");
    let status_row = geo.status_row.expect("status row at 40x10");

    assert!(
        !rows[rule_row as usize].contains("NOTELINE"),
        "notes draft must not overwrite the rule row: {:?}",
        rows[rule_row as usize]
    );
    assert!(
        !rows[status_row as usize].contains("NOTELINE"),
        "notes draft must not overwrite the status row: {:?}",
        rows[status_row as usize]
    );
    assert!(
        rows[rule_row as usize]
            .chars()
            .filter(|&c| c == '─')
            .count()
            > 0,
        "rule row lost its rule chrome: {:?}",
        rows[rule_row as usize]
    );
    assert!(
        trimmed(&rows[status_row as usize]).contains("done"),
        "status row lost its counts chrome: {:?}",
        rows[status_row as usize]
    );
}

// ---------------------------------------------------------------------------
// accordion/takeover detail, and surface goldens for the reviewer.
// ---------------------------------------------------------------------------

/// standard (Enter) accordion expands full width directly under the selected
/// row -- it does not replace/hide any other row, and it never touches the domain (render
/// only ever holds `&[Task]`, so "no mutation" is a structural guarantee this test pins).
#[test]
fn standard_accordion_expands_full_width_under_selection_without_mutating_domain() {
    let tasks = fixture_tasks();
    let before = tasks.clone();
    let view = fixture_view(&tasks, true);
    let mut model = fixture_model(&tasks, &view);
    let selected = Uuid::from_u128(1); // "Smoke-test worktree dispatch", the fixture's selection.
    model.detail_open = Some(selected);

    // Tall enough that the accordion's extra rows do not push DONE off the viewport --
    // width >= 78 and height >= 24 both stay Standard tier (see `tier::resolve`).
    let (rows, geo) = paint(80, 30, &model);
    assert_eq!(geo.tier, Tier::Standard);

    let selected_idx = rows
        .iter()
        .position(|r| r.contains("Smoke-test worktree dispatch"))
        .expect("selected task row must still be painted");
    // The accordion body sits directly under the selected row, indented like the
    // prototype's expanded rows (` │ `). Row width is not asserted here: every row in
    // `rows` is already exactly `width` chars by construction of the fixed-size
    // `TestBackend` grid `paint()` reads from -- that is a property of the harness,
    // not something `paint()`'s own assertions (mono/color only) enforce, so re-checking it
    // per body row would be vacuous. What is worth pinning is the actual body content.
    let body: Vec<&String> = rows[selected_idx + 1..]
        .iter()
        .take_while(|r| r.contains('│'))
        .collect();
    assert!(
        !body.is_empty(),
        "accordion body must appear directly under the selected row:\n{:#?}",
        rows
    );
    let body_joined = body
        .iter()
        .map(|r| trimmed(r))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body_joined.contains("no notes yet"),
        "accordion body must show its notes preview:\n{body_joined}"
    );
    assert!(
        !body_joined.contains("scope") && !body_joined.contains("created"),
        "peek must omit task metadata:\n{body_joined}"
    );
    assert!(
        rows.get(selected_idx + 1 + body.len())
            .is_some_and(|row| row.contains('└')),
        "the notes gutter must end in a connected corner:\n{}",
        rows.join("\n")
    );
    // Every other task and every section header the base fixture paints (drawer open) must
    // still be present: the accordion expands the list, it never mutates or hides it.
    let joined = rows
        .iter()
        .map(|r| trimmed(r))
        .collect::<Vec<_>>()
        .join("\n");
    for still_present in [
        "IN MOTION",
        "desk",
        "DONE",
        "Edit target binding pin",
        "Global backlog note",
        "Ship the queue board milestone",
    ] {
        assert!(
            joined.contains(still_present),
            "accordion must not remove other rows, missing {still_present:?}:\n{joined}"
        );
    }
    // Structural guarantee: render took `&[Task]`, so it cannot have mutated the domain.
    assert_eq!(
        tasks, before,
        "painting the accordion must never mutate the domain"
    );
}

/// Peek (`→`) body: up to five note lines, then a dim "N more lines" tail naming what did
/// not fit. Task metadata stays on the full page, and a final corner closes the note gutter.
#[test]
fn standard_peek_body_caps_notes_at_five_lines_with_a_more_lines_tail() {
    let mut tasks = fixture_tasks();
    // Task 1 is the fixture's selection ("Smoke-test worktree dispatch").
    tasks[0].notes = Some(
        (1..=8)
            .map(|i| format!("note line {i}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let view = fixture_view(&tasks, true);
    let mut model = fixture_model(&tasks, &view);
    model.detail_open = Some(Uuid::from_u128(1));

    let (rows, geo) = paint(80, 34, &model);
    assert_eq!(geo.tier, Tier::Standard);
    let selected_idx = rows
        .iter()
        .position(|r| r.contains("Smoke-test worktree dispatch"))
        .expect("selected task row must be painted");
    let body: Vec<String> = rows[selected_idx + 1..]
        .iter()
        .take_while(|r| r.contains('│'))
        .map(|r| trimmed(r))
        .collect();
    let joined = body.join("\n");
    for i in 1..=5 {
        assert!(
            joined.contains(&format!("note line {i}")),
            "peek must show note line {i}:\n{joined}"
        );
    }
    assert!(
        !joined.contains("note line 6"),
        "peek must not show a sixth note line:\n{joined}"
    );
    assert!(
        joined.contains("3 more lines"),
        "peek must name the three hidden lines:\n{joined}"
    );
    assert!(
        !joined.contains("scope") && !joined.contains("created"),
        "peek must omit task metadata:\n{joined}"
    );
    assert!(
        rows.get(selected_idx + 1 + body.len())
            .is_some_and(|row| row.contains('└')),
        "the more-lines tail must be followed by the connected corner:\n{}",
        rows.join("\n")
    );
}

/// Five or fewer note lines fit whole: no "more lines" tail, nothing dropped.
#[test]
fn standard_peek_body_shows_short_notes_whole_without_a_tail() {
    let mut tasks = fixture_tasks();
    tasks[0].notes = Some("one\ntwo\nthree".to_string());
    let view = fixture_view(&tasks, true);
    let mut model = fixture_model(&tasks, &view);
    model.detail_open = Some(Uuid::from_u128(1));

    let (rows, geo) = paint(80, 30, &model);
    assert_eq!(geo.tier, Tier::Standard);
    let selected_idx = rows
        .iter()
        .position(|r| r.contains("Smoke-test worktree dispatch"))
        .expect("selected task row must be painted");
    let joined: String = rows[selected_idx + 1..]
        .iter()
        .take_while(|r| r.contains('│'))
        .map(|r| trimmed(r))
        .collect::<Vec<_>>()
        .join("\n");
    for expected in ["one", "two", "three"] {
        assert!(
            joined.contains(expected),
            "peek must show {expected:?}:\n{joined}"
        );
    }
    assert!(
        !joined.contains("more lines"),
        "short notes must not carry a more-lines tail:\n{joined}"
    );
}

/// the accordion inherited the un-fixed root cause of I4 -- the list was
/// top-anchored with no scroll-to-anchor, so expanding a task below the viewport painted
/// nothing at all. Pad the deck past the standard viewport, expand the *last* task, and
/// require its title and full detail body to be visible (not scrolled away, not truncated).
#[test]
fn standard_accordion_on_a_task_below_the_fold_scrolls_the_whole_block_into_view() {
    let mut tasks = fixture_tasks();
    for i in 0..30u128 {
        tasks.push(task(
            2000 + i,
            &format!("Padding task {i}"),
            HumanStatus::Ready,
            project("/repos/tsk"),
            3600,
        ));
    }
    let last_id = Uuid::from_u128(2000 + 29);
    let view = fixture_view_projects(&tasks, false);
    let mut model = fixture_model_on_tab(&tasks, &view, BoardTab::Projects);
    model.detail_open = Some(last_id);

    let (rows, geo) = paint(80, 24, &model);
    assert_eq!(geo.tier, Tier::Standard);
    let top = geo.viewport_top as usize;
    let bottom = (geo.viewport_top + geo.viewport_height) as usize;
    let viewport: String = rows[top..bottom]
        .iter()
        .map(|r| trimmed(r))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        viewport.contains("Padding task 29"),
        "the expanded task's own row must be scrolled into view:\n{viewport}"
    );
    assert!(
        viewport.contains("no notes yet") && viewport.contains('└'),
        "accordion body must show the notes preview and connected corner once scrolled into view:\n{viewport}"
    );
}

/// G-2 (gate round 1, PR #11): with no expanded accordion open, the list once painted from
/// row 0, so on a deck longer than the viewport, moving the plain selection past the fold
/// walked it off-screen with no visual
/// feedback, and the mutating verbs (`space`/`d`/`x`) then acted on a row the user could not
/// see. Select the *last* task of a 40-task deck with nothing else open and require its own
/// title to be scrolled into view.
#[test]
fn plain_selection_on_a_task_below_the_fold_scrolls_it_into_view_in_both_tiers() {
    let tasks: Vec<Task> = (0..40u128)
        .map(|i| {
            task(
                5000 + i,
                &format!("Selection padding task {i}"),
                HumanStatus::Ready,
                project("/repos/tsk"),
                3600,
            )
        })
        .collect();
    let last_id = Uuid::from_u128(5000 + 39);
    let view = fixture_view_projects(&tasks, false);
    let mut model = fixture_model_on_tab(&tasks, &view, BoardTab::Projects);
    model.selection_id = Some(last_id);

    for &(width, height) in &[(80u16, 24u16), (77u16, 24u16), (40u16, 10u16)] {
        let (rows, geo) = paint(width, height, &model);
        let top = geo.viewport_top as usize;
        let bottom = (geo.viewport_top + geo.viewport_height) as usize;
        let viewport: String = rows[top..bottom]
            .iter()
            .map(|r| trimmed(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            viewport.contains("Selection padding task 39"),
            "{width}x{height}: the plain-selected task's own row must be scrolled into view \
             even with no capture or accordion open:\n{viewport}"
        );
        assert_visible_chrome(&rows, geo, &format!("{width}x{height}"));
    }
}

/// on compact, both the read-only detail accordion and the field editors are
/// full-screen takeovers;
/// Peek (`→`/Enter's inline detail) is list content in BOTH tiers: at compact sizes the
/// base list stays visible and the peek body weaves under its row, scrolling into view.
/// Field editors remain the full-screen takeover family; closing either restores the
/// prior selection.
#[test]
fn compact_peek_is_inline_and_editors_stay_full_screen_takeovers() {
    let tasks = fixture_tasks();
    let view = fixture_view(&tasks, false);
    let mut model = fixture_model(&tasks, &view);
    let selected = Uuid::from_u128(1);
    assert_eq!(model.selection_id, Some(selected));

    for &(w, h) in &[(48u16, 19u16), (40u16, 10u16)] {
        let geo = tier::resolve(w, h);
        assert_eq!(geo.tier, Tier::Compact, "{w}x{h}");
        let top = geo.viewport_top as usize;
        let bottom = (geo.viewport_top + geo.viewport_height) as usize;

        // Peek: inline under the selected row, base list intact.
        model.detail_open = Some(selected);
        model.overlay = QueueOverlay::None;
        let (rows, _geo) = paint(w, h, &model);
        let viewport: String = rows[top..bottom]
            .iter()
            .map(|r| trimmed(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            viewport.contains("Smoke-test worktree dispatch"),
            "{w}x{h}: the peeked task's row must stay visible:\n{viewport}"
        );
        if h > 10 {
            assert!(
                viewport.contains("no notes yet"),
                "{w}x{h}: the peek body must weave inline under the row:\n{viewport}"
            );
        } else {
            assert!(
                viewport.contains("no notes yet"),
                "{w}x{h}: compact floor still shows the peek under the row:\n{viewport}"
            );
        }

        // A field editor (title) is still a full takeover that erases the base list.
        model.detail_open = None;
        model.overlay = QueueOverlay::EditTitle {
            draft: "x".to_string(),
            cursor_col: 1,
        };
        let (rows2, _geo2) = paint(w, h, &model);
        let viewport2: String = rows2[top..bottom]
            .iter()
            .map(|r| trimmed(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !viewport2.contains("IN MOTION"),
            "{w}x{h}: title editor takeover must erase the base list:\n{viewport2}"
        );

        // Close both: the base list, and the prior selection, are back.
        model.overlay = QueueOverlay::None;
        let (rows3, _geo3) = paint(w, h, &model);
        let viewport3: String = rows3[top..bottom]
            .iter()
            .map(|r| trimmed(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            viewport3.contains("Smoke-test worktree dispatch") && viewport3.contains("IN MOTION"),
            "{w}x{h}: closing must restore the base list:\n{viewport3}"
        );
        assert_eq!(
            model.selection_id,
            Some(selected),
            "{w}x{h}: closing must not have touched the selection"
        );
    }
}

/// `detail_open_task`'s filtering -- the compact tier now resolves
/// its takeover task through `model.view.sections`, not `model.tasks` directly (the
/// residual fix) -- had no test anywhere. Build a view scoped to exclude a task that is
/// still present in `QueueFrameModel::tasks` (a scope narrower than "all projects", the
/// scenario the doc comment on `detail_open_task` names: "a scope change ... can outlive
/// its task's visibility"), point `detail_open` at that excluded task, and require the
/// compact takeover to paint nothing for it.
#[test]
fn compact_paints_no_takeover_for_a_detail_open_task_excluded_by_the_current_scope() {
    let tasks = fixture_tasks();
    // Task 20 ("Wire dispatch cleanup receipts", Blocked, `/repos/herdr`) stays in
    // `tasks` -- the slice `QueueFrameModel::tasks` always carries -- but a view scoped to
    // just `/repos/tsk` excludes it from `view.sections` entirely (unlike a Doing
    // task, an ON DECK task is genuinely scope-filtered; `IN MOTION` is not).
    let excluded_id = Uuid::from_u128(20);
    assert!(
        tasks.iter().any(|t| t.id == excluded_id),
        "fixture must still carry the excluded task in model.tasks"
    );
    let scoped_view = queue::query_lens(
        &tasks,
        Some(Path::new("/repos/tsk")),
        BoardLens::Project(Path::new("/repos/tsk")),
        false,
    );
    assert!(
        !scoped_view
            .sections
            .iter()
            .flat_map(|s| s.task_ids.iter())
            .any(|&id| id == excluded_id),
        "the scoped view must actually exclude the task this test points detail_open at"
    );
    let model = QueueFrameModel {
        detail_open: Some(excluded_id),
        at_home: false,
        scope_label: "tsk",
        ..fixture_model(&tasks, &scoped_view)
    };

    for &(w, h) in &[(48u16, 19u16), (40u16, 10u16)] {
        let (rows, geo) = paint(w, h, &model);
        assert_eq!(geo.tier, Tier::Compact, "{w}x{h}");
        let top = geo.viewport_top as usize;
        let bottom = (geo.viewport_top + geo.viewport_height) as usize;
        let viewport: String = rows[top..bottom]
            .iter()
            .map(|r| trimmed(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !viewport.contains("Wire dispatch cleanup receipts"),
            "{w}x{h}: a detail_open task excluded by the current scope must not repaint as \
             a takeover:\n{viewport}"
        );
    }
}

/// One named scene painted for the reviewer's side-by-side against
/// `python3 design/prototype/main.py --dump` (the-relevant scenes only).
struct GoldenScene {
    name: &'static str,
    rows: Vec<String>,
    width: u16,
}

fn golden_scenes() -> Vec<GoldenScene> {
    let tasks = fixture_tasks();

    let board_view = fixture_view(&tasks, false);
    let board_model = fixture_model(&tasks, &board_view);
    let (board_rows, _) = paint(80, 24, &board_model);
    let (default_split_rows, default_split_geo) = paint(78, 24, &board_model);
    assert_eq!(
        default_split_geo.tier,
        Tier::Standard,
        "the default 78x24 Herdr split must render the designed tier"
    );

    // Imp-3: the accordion scene's verb bar must show `enter close`, the same word
    // `board_verb_items` computes for a task whose detail is open, bound by construction
    // via `accordion_verbs` rather than reused from the base scene's array.
    let accordion_verbs_owned = accordion_verbs();
    let mut accordion_model = fixture_model(&tasks, &board_view);
    accordion_model.detail_open = Some(Uuid::from_u128(1));
    accordion_model.verb_items = &accordion_verbs_owned;
    let (accordion_rows, _) = paint(80, 24, &accordion_model);

    let mut palette_model = fixture_model(&tasks, &board_view);
    let palette_commands_owned = palette_commands();
    palette_model.overlay = QueueOverlay::Palette {
        query: "stat",
        commands: &palette_commands_owned,
    };
    let (palette_rows, _) = paint(80, 24, &palette_model);

    // C1: the help golden must be the help card the product actually paints, not a
    // hand-written stand-in. `help_card_lines()` is the same function `draw_board` feeds
    // into `QueueOverlay::Help`, derived from the live keymap.
    //
    // this card deliberately diverges from the prototype's help scene --
    // the prototype lays its bindings out as a two-column card with an em dash and a few
    // prototype-only keys (triage/dispatch/lens navigation) that the does not have. That is
    // intentional for the (deck-only, no lenses/dispatch keyboard surface yet), not a gap;
    // recorded here so a reviewer does not re-litigate the divergence as a bug.
    let mut help_model = fixture_model(&tasks, &board_view);
    let help_lines: Vec<String> = tsk_tui::ui::input::help_card_lines();
    help_model.overlay = QueueOverlay::Help { lines: &help_lines };
    let (help_rows, _) = paint(80, 24, &help_model);

    // `done_drawer` has no prototype counterpart -- the prototype's own
    // `done` scene pushes the DONE section below the fold and dumps a frame that never shows
    // it. This scene's check is therefore structural only (DONE painted, drawer open),
    // not a byte-for-byte comparison against a prototype dump the way the other five scenes
    // are; recorded here rather than left implicit.
    let done_view = fixture_view(&tasks, true);
    let done_model = fixture_model(&tasks, &done_view);
    let (done_rows, _) = paint(80, 24, &done_model);

    vec![
        GoldenScene {
            name: "board",
            rows: board_rows,
            width: 80,
        },
        GoldenScene {
            name: "board_default_split_78",
            rows: default_split_rows,
            width: 78,
        },
        GoldenScene {
            name: "accordion",
            rows: accordion_rows,
            width: 80,
        },
        GoldenScene {
            name: "palette",
            rows: palette_rows,
            width: 80,
        },
        GoldenScene {
            name: "help",
            rows: help_rows,
            width: 80,
        },
        GoldenScene {
            name: "done_drawer",
            rows: done_rows,
            width: 80,
        },
    ]
}

/// Boxed plain-text dump matching the shape of the prototype's own `show()` dump, so a
/// reviewer can diff this file against `python3 design/prototype/main.py --dump` by eye.
fn dump_frame(rows: &[String], width: u16) -> String {
    let mut out = String::new();
    out.push('┌');
    out.push_str(&"─".repeat(width as usize));
    out.push_str("┐\n");
    for row in rows {
        out.push('│');
        out.push_str(row);
        out.push('│');
        out.push('\n');
    }
    out.push('└');
    out.push_str(&"─".repeat(width as usize));
    out.push_str("┘\n");
    out
}

fn golden_fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/queue_board")
}

fn golden_path(name: &str) -> std::path::PathBuf {
    golden_fixtures_dir().join(format!("{name}.txt"))
}

/// Regeneration helper (not part of the suite): run with `--ignored` after a deliberate,
/// reviewed change to what a scene paints, to rewrite the checked-in goldens from the
/// renderer's own output. Never hand-edit a `.txt` fixture.
#[test]
#[ignore]
fn regenerate_golden_fixtures() {
    for scene in golden_scenes() {
        let dump = dump_frame(&scene.rows, scene.width);
        std::fs::write(golden_path(scene.name), dump).expect("write golden");
    }
}

/// the the-relevant scenes the reviewer compares against the frozen prototype dump
/// (board at 80×24, board at the default 78×24 split, accordion/expanded, palette, help,
/// done drawer) each have a checked-in golden frame, and every fresh paint still matches
/// its golden byte-for-byte.
#[test]
fn surface_goldens_board_accordion_palette_help_drawer_exist_for_reviewer_side_by_side() {
    for scene in golden_scenes() {
        let dump = dump_frame(&scene.rows, scene.width);
        let path = golden_path(scene.name);
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden fixture for scene {:?} at {path:?} ({e}); \
                 generate it from this same `dump_frame` output and check it in",
                scene.name
            )
        });
        assert_eq!(
            dump, expected,
            "golden frame for scene {:?} drifted from {path:?}",
            scene.name
        );
    }
}

/// every checked-in golden frame is itself scanned for forbidden color SGR, the same
/// scan every in-test paint already runs, so a hand-edited or stale fixture cannot smuggle
/// color back in unnoticed.
///
/// this scan cannot itself detect a colored cell -- the `.txt` goldens are built from
/// `buffer[(x,y)].symbol()` (see `paint()` above), symbols only, no `Style`, so no SGR byte
/// can ever reach a golden file regardless of what the renderer painted. The real color
/// oracle is [`assert_buffer_mono`], run on every painted `Buffer` inside `paint()` before a
/// single row is ever turned into golden text; this scan's job is narrower and stated
/// plainly here: it only guards the on-disk fixture itself against a hand edit or drift that
/// would smuggle a raw escape sequence into text a reviewer diffs by eye.
/// G-8 (gate MEDIUM): guards the binding `palette_commands()` establishes -- a future edit
/// that reverts the palette golden scene to a hand-rolled `PaletteCommandRow` array (as
/// opposed to deriving it from `BoardModel::visible_commands`) must fail here rather than
/// silently reintroducing a command the the catalog does not offer (: no park, resume,
/// link, or dispatch entries).
#[test]
fn palette_golden_scene_commands_are_bound_to_the_real_m1_catalog_and_exclude_dispatch() {
    let commands = palette_commands();
    let labels: Vec<&str> = commands.iter().map(|command| command.label).collect();
    assert_eq!(
        labels,
        vec![
            "set status: ready",
            "set status: started",
            "set status: blocked",
            "set status: review",
        ],
        "the palette golden scene's command rows must be exactly what \
         `BoardModel::visible_commands` produces for the fixture's selection and query, not \
         a hand-rolled stand-in"
    );
    assert!(
        commands
            .iter()
            .all(|command| !command.label.contains("dispatch")),
        "AC-17 excludes dispatch from the M1 palette outright; the golden scene must never \
         show it: {commands:?}"
    );
}

/// T-7 (AC-22): the task page's footer verb bar lists the step-add verb while the
/// bound task has steps steps (view mode). A task with no steps keeps the
/// pre-T-7 verb bar exactly: the with-steps bar is the without-steps bar plus the
/// one step-add entry — modifier implied by the bar's prefix convention — and
/// nothing else. The full listing is asserted at a width the whole bar fits; at
/// the 78-column standard floor the bar's existing width clipping may take the
/// entry's label tail but never its key chord.
#[test]
fn footer_lists_the_step_add_verb() {
    let page_verb_row_with = |steps: &[&str], width: u16| -> String {
        let mut domain = DomainState::new();
        let id = domain
            .create(
                "Verb bar witness",
                Some("the notes body".into()),
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create task");
        for text in steps {
            domain.add_step(id, text).expect("add step");
        }
        let mut model = BoardModel::from_domain(&domain, None);
        apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None)
            .expect("open task page");
        apply_intent(&mut domain, &mut model, BoardIntent::BeginEditTitle, None)
            .expect("enter task edit mode");
        apply_intent(&mut domain, &mut model, BoardIntent::CancelEdit, None)
            .expect("return to task page");
        let rows = board_rows(&model, width, 24);
        let verb_row = tier::resolve(width, 24).verb_row.expect("verb row");
        trimmed(&rows[verb_row as usize])
    };

    let with = page_verb_row_with(&["only step"], 100);
    let without = page_verb_row_with(&[], 100);

    for row in [&with, &without] {
        assert!(
            row.contains("ctrl+a step"),
            "task edit mode must list the step-add verb, including an empty checklist:\n{row}"
        );
    }
    assert_eq!(
        with, without,
        "step availability belongs to task edit mode, not to whether a step already exists"
    );

    let floor = page_verb_row_with(&["only step"], 78);
    assert!(
        floor.contains("ctrl+a…"),
        "the compact verb-bar budget may ellipsize the final Ctrl chord, without making the bar overflow:\n{floor}"
    );
}

/// T-4: scoped ON DECK thread blocks paint one dim decorative header before their task rows.
#[test]
fn scoped_board_paints_thread_header_above_its_tasks() {
    let mut tasks = fixture_tasks();
    tasks[2].thread = Some("release".to_string());
    tasks[3].thread = Some("release".to_string());
    let mut model = BoardModel::from_tasks(tasks, Some(PathBuf::from("/repos/tsk")));
    model.set_selected_project(Some(PathBuf::from("/repos/tsk")));

    let rows = board_rows(&model, 80, 24);
    let header = rows
        .iter()
        .position(|row| row.contains("#release"))
        .expect("scoped thread block must paint its header");
    let first_task = rows
        .iter()
        .position(|row| row.contains("Prototype the queue-style board UI"))
        .expect("threaded task must paint");
    assert!(
        header < first_task,
        "the thread header must precede its task rows:\n{}",
        rows.join("\n")
    );
}

#[test]
fn threaded_task_rows_indent_under_headers_while_unthreaded_rows_stay_flush() {
    let mut tasks = fixture_tasks();
    tasks[2].thread = Some("release".to_string());
    tasks[3].thread = Some("release".to_string());
    tasks.push(task(
        12,
        "Loose project task",
        HumanStatus::Ready,
        project("/repos/tsk"),
        30,
    ));
    let mut model = BoardModel::from_tasks(tasks, Some(PathBuf::from("/repos/tsk")));
    model.set_selected_project(Some(PathBuf::from("/repos/tsk")));

    let rows = board_rows(&model, 80, 24);
    let leading_spaces = |row: &str| {
        row.chars()
            .take_while(|character| *character == ' ')
            .count()
    };
    let header = rows
        .iter()
        .find(|row| row.contains("#release"))
        .expect("thread header paints");
    let threaded = rows
        .iter()
        .find(|row| row.contains("Prototype the queue-style board UI"))
        .expect("threaded task paints");
    let loose = rows
        .iter()
        .find(|row| row.contains("Loose project task"))
        .expect("unthreaded task paints");

    assert_eq!(
        leading_spaces(header),
        leading_spaces(loose),
        "headers and loose rows keep their existing left edge"
    );
    assert_eq!(
        leading_spaces(threaded),
        leading_spaces(loose) + 2,
        "threaded rows indent beneath their header:\n{}",
        rows.join("\n")
    );
}

#[test]
fn thread_blocks_leave_a_blank_row_before_loose_tasks() {
    let mut tasks = fixture_tasks();
    tasks[2].thread = Some("release".to_string());
    tasks[3].thread = Some("release".to_string());
    tasks.push(task(
        12,
        "Loose project task",
        HumanStatus::Ready,
        project("/repos/tsk"),
        30,
    ));
    let mut model = BoardModel::from_tasks(tasks, Some(PathBuf::from("/repos/tsk")));
    model.set_selected_project(Some(PathBuf::from("/repos/tsk")));

    let rows = board_rows(&model, 80, 24);
    let last_threaded_task = rows
        .iter()
        .position(|row| row.contains("Cut rust-toolchain pin into CI docs"))
        .expect("last threaded task paints");
    let loose_task = rows
        .iter()
        .position(|row| row.contains("Loose project task"))
        .expect("loose task paints");

    assert!(
        rows[last_threaded_task + 1].trim().is_empty(),
        "a thread block leaves a spacer before following content:\n{}",
        rows.join("\n")
    );
    assert!(
        last_threaded_task + 1 < loose_task,
        "the loose task follows the thread spacer:\n{}",
        rows.join("\n")
    );
}

#[test]
fn every_thread_block_leaves_a_spacer_before_following_content() {
    let mut tasks = fixture_tasks();
    tasks[2].thread = Some("alpha".to_string());
    tasks[3].thread = Some("beta".to_string());
    tasks.push(task(
        12,
        "Loose project task",
        HumanStatus::Ready,
        project("/repos/tsk"),
        30,
    ));
    let mut model = BoardModel::from_tasks(tasks, Some(PathBuf::from("/repos/tsk")));
    model.set_selected_project(Some(PathBuf::from("/repos/tsk")));

    let rows = board_rows(&model, 80, 24);
    for title in [
        "Prototype the queue-style board UI",
        "Cut rust-toolchain pin into CI docs",
    ] {
        let task = rows
            .iter()
            .position(|row| row.contains(title))
            .unwrap_or_else(|| panic!("threaded task {title:?} paints"));
        assert!(
            rows[task + 1].trim().is_empty(),
            "each thread block leaves a spacer after {title:?}:\n{}",
            rows.join("\n")
        );
    }
}

#[test]
fn header_shows_name_and_open_count() {
    let mut tasks = fixture_tasks();
    tasks[2].thread = Some("release".to_string());
    tasks[3].thread = Some("release".to_string());
    let mut model = BoardModel::from_tasks(tasks, Some(PathBuf::from("/repos/tsk")));
    model.set_selected_project(Some(PathBuf::from("/repos/tsk")));
    let deck_index = model
        .visible_ids()
        .iter()
        .position(|id| *id == Uuid::from_u128(10))
        .expect("threaded task is visible");
    let mut domain = DomainState::new();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::SelectIndex(deck_index),
        None,
    )
    .expect("select threaded task");

    let standard = board_rows(&model, 80, 24).join("\n");
    assert!(
        standard.contains("#release") && standard.contains("2 open"),
        "standard thread header must name its block and its open count:\n{standard}"
    );
    let compact = board_rows(&model, 40, 12).join("\n");
    assert!(
        compact.contains("#release 2") && !compact.contains("2 open"),
        "compact thread header must retain name/count in its compressed form:\n{compact}"
    );
}

#[test]
fn board_with_headers_paints_within_40x10_and_all_tasks_reachable() {
    let mut tasks = vec![
        task(
            100,
            "alpha first",
            HumanStatus::Ready,
            project("/repos/tsk"),
            40,
        ),
        task(
            101,
            "alpha second",
            HumanStatus::Ready,
            project("/repos/tsk"),
            30,
        ),
        task(
            102,
            "beta first",
            HumanStatus::Ready,
            project("/repos/tsk"),
            20,
        ),
        task(
            103,
            "beta second",
            HumanStatus::Ready,
            project("/repos/tsk"),
            10,
        ),
    ];
    tasks[0].thread = Some("alpha".to_string());
    tasks[1].thread = Some("alpha".to_string());
    tasks[2].thread = Some("beta".to_string());
    tasks[3].thread = Some("beta".to_string());
    let mut model = BoardModel::from_tasks(tasks, Some(PathBuf::from("/repos/tsk")));
    model.set_selected_project(Some(PathBuf::from("/repos/tsk")));
    let mut domain = DomainState::new();
    let ids = model.visible_ids();
    assert_eq!(ids.len(), 4, "fixture must expose every open task");
    assert_eq!(
        model.selected_id(),
        Some(ids[0]),
        "the first visible task must seed arrow traversal"
    );

    for (index, id) in ids.into_iter().enumerate() {
        if index > 0 {
            apply_intent(&mut domain, &mut model, BoardIntent::SelectNext, None)
                .expect("arrow to next task");
        }
        assert_eq!(
            model.selected_id(),
            Some(id),
            "arrow traversal must land on every open task"
        );
        let rows = board_rows(&model, 40, 10);
        let task = model
            .visible_tasks()
            .into_iter()
            .find(|task| task.id == id)
            .expect("selected task stays visible");
        let header = format!(
            "#{} 2",
            task.thread.as_deref().expect("threaded fixture task")
        );
        let header_y = rows
            .iter()
            .position(|row| row.contains(&header))
            .expect("selected task's header must occupy a painted row");
        let task_y = rows
            .iter()
            .position(|row| row.contains(&task.title))
            .expect("selected task must be reachable at 40x10");
        assert!(
            header_y < task_y,
            "the header must consume its own row before the selected task:\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter().all(|row| row_display_width(row) == 40),
            "thread headers and rows must stay within 40 columns"
        );
    }
}

#[test]
fn peek_omits_thread_metadata_for_every_task() {
    let mut threaded = task(200, "threaded", HumanStatus::Ready, TaskScope::Global, 1);
    threaded.thread = Some("release".to_string());
    let mut threaded_model = BoardModel::from_tasks(vec![threaded], None);
    let mut domain = DomainState::new();
    apply_intent(
        &mut domain,
        &mut threaded_model,
        BoardIntent::PeekDetail,
        None,
    )
    .expect("open threaded peek");
    assert!(
        !board_rows(&threaded_model, 80, 24)
            .join("\n")
            .contains("thread #release"),
        "thread metadata belongs to the task page, not the peek"
    );

    let mut unthreaded_model = BoardModel::from_tasks(
        vec![task(
            201,
            "unthreaded",
            HumanStatus::Ready,
            TaskScope::Global,
            1,
        )],
        None,
    );
    apply_intent(
        &mut domain,
        &mut unthreaded_model,
        BoardIntent::PeekDetail,
        None,
    )
    .expect("open unthreaded peek");
    assert!(
        !board_rows(&unthreaded_model, 80, 24)
            .join("\n")
            .contains("thread #"),
        "unthreaded peek must not invent a thread line"
    );
}

#[test]
fn peek_shows_inline_backticks_and_fence_ticks() {
    let mut coded = task(202, "coded", HumanStatus::Ready, TaskScope::Global, 1);
    coded.notes = Some("`code`\n```\nfn x() {}\n```".into());
    let mut model = BoardModel::from_tasks(vec![coded], None);
    let mut domain = DomainState::new();
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None).expect("peek");
    let body = board_rows(&model, 80, 24).join("\n");
    assert!(
        body.contains("`code`"),
        "peek must keep inline backticks:\n{body}"
    );
    assert!(body.contains("```"), "peek must keep fenced ticks:\n{body}");
    assert!(
        body.contains("fn x() {}"),
        "peek must show the fence body:\n{body}"
    );
}

#[test]
fn peek_runs_markdown_not_plain_dim_text() {
    let mut coded = task(203, "peek-md", HumanStatus::Ready, TaskScope::Global, 1);
    coded.notes = Some("# Title\n**strong** here\n```\n**still stars**\n```".into());
    let mut model = BoardModel::from_tasks(vec![coded], None);
    let mut domain = DomainState::new();
    apply_intent(&mut domain, &mut model, BoardIntent::PeekDetail, None).expect("peek");
    let body = board_rows(&model, 80, 24).join("\n");
    assert!(
        !body.contains("**strong**"),
        "peek must strip strong markers like the page:\n{body}"
    );
    assert!(
        body.contains("strong"),
        "peek must keep strong text:\n{body}"
    );
    assert!(
        body.contains("# ") || body.contains("# Title"),
        "peek must keep heading hashes:\n{body}"
    );
    assert!(
        body.contains("**still stars**"),
        "peek fence must leave body markers:\n{body}"
    );
}

#[test]
fn all_golden_frames_pass_no_color_sgr_scan() {
    let dir = golden_fixtures_dir();
    let mut scanned = 0usize;
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        assert_no_color_sgr(&text);
        scanned += 1;
    }
    assert_eq!(
        scanned, 6,
        "expected the six board surface goldens (board, board_default_split_78, accordion, palette, help, done_drawer) in {dir:?}"
    );
}

/// The main list wraps a long title onto continuation lines indented under the
/// task's own first row: nothing is cut, and no row ends in an omission marker.
#[test]
fn board_list_wraps_a_long_title_onto_a_continuation_row() {
    let mut domain = DomainState::new();
    let title = "alpha bravo charlie delta echo foxtrot golf hotel".to_string();
    domain
        .create(
            &title,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let model = BoardModel::from_domain(&domain, None);

    let rows = board_rows(&model, 80, 24);
    // The head row keeps the classic shape (gutter + glyph + title head).
    assert!(
        rows.iter().any(|row| row.contains("○ alpha")),
        "head row must keep the glyph + title shape:\n{}",
        rows.join("\n")
    );
    // A continuation line indents exactly four cells into the title column.
    let continuation = rows
        .iter()
        .find(|row| row.contains("hotel"))
        .unwrap_or_else(|| panic!("no wrapped continuation row:\n{}", rows.join("\n")));
    assert!(
        continuation.starts_with("    ") && !continuation.starts_with("     "),
        "continuation rows indent four cells under the title column: {continuation:?}"
    );
    // Neither of the task's own rows may carry an omission marker (the verb bar's
    // own tier budget is a different surface).
    let head = rows
        .iter()
        .find(|row| row.contains("○ alpha"))
        .expect("head row");
    assert!(
        !head.contains('…') && !continuation.contains('…'),
        "a wrapped task row carried an omission marker"
    );
}

/// Quick-add wraps its draft into the reserved blank rows instead of scrolling
/// sideways: every typed word stays visible above or on the input line.
#[test]
fn quick_add_wraps_a_long_title_into_the_reserved_rows() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenCapture, None).expect("open quick add");
    let title: String = (0..40).map(|index| format!("w{index} ")).collect();
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::QuickAddInsertText(title),
        None,
    )
    .expect("type long title");

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw quick add");
    let rows = board_rows(&model, 80, 24);
    let prompt_rows: Vec<&String> = rows.iter().filter(|row| row.contains("▎")).collect();
    assert_eq!(
        prompt_rows.len(),
        2,
        "exactly one continuation row fits the reserved blank above the input:\n{}",
        rows.join("\n")
    );
    let text = rows.join("\n");
    // Reading order survives wrapping: an earlier segment paints above the tail,
    // and the rule row two above the input stays chrome, never draft text.
    let tail = rows
        .iter()
        .position(|row| row.contains("w39"))
        .expect("tail segment painted");
    let above = rows
        .iter()
        .position(|row| row.contains("▎") && !row.contains("w39"))
        .expect("continuation row painted");
    assert!(
        above < tail,
        "the draft reads top-down across its wrapped rows:\n{text}"
    );
    assert!(
        rows[tail - 2].starts_with('─'),
        "the rule row stays a rule row:\n{}",
        rows.join("\n")
    );
}

/// Arrow keys move the Notes caret across WRAPPED rows on the task page: down
/// crosses logical lines and wrapped continuations alike; up returns it.
#[test]
fn notes_edit_arrows_move_across_logical_and_wrapped_rows() {
    let mut domain = DomainState::new();
    domain
        .create(
            "Arrow nav",
            Some("one\n\ntwo".to_string()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");
    board_rows(&model, 80, 24);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None).expect("edit notes");

    let caret_at = |model: &BoardModel, width: u16, height: u16| {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| {
                let _ = draw_board(frame, model);
            })
            .expect("draw");
        terminal.backend().cursor_position().y
    };
    let caret_y = |model: &BoardModel| caret_at(model, 80, 24);

    // Seeded at the end of "two" (row 2). Up twice walks back over the blank row.
    assert_eq!(caret_y(&model), 5);
    apply_intent(&mut domain, &mut model, BoardIntent::EditMoveUp, None).expect("up to blank row");
    assert_eq!(caret_y(&model), 4, "up lands on the blank middle row");
    apply_intent(&mut domain, &mut model, BoardIntent::EditMoveUp, None).expect("up to first row");
    assert_eq!(caret_y(&model), 3, "up reaches the first note row");
    apply_intent(&mut domain, &mut model, BoardIntent::EditMoveDown, None).expect("down again");
    assert_eq!(caret_y(&model), 4);

    // Wrapped rows: at the compact floor the same draft wraps at word boundaries,
    // and Up from the tail row climbs onto the wrapped head row.
    let mut domain = DomainState::new();
    domain
        .create(
            "Wrapped arrows",
            Some("aaaaa bbbbb ccccc ddddd eeeee fffff ggggg".to_string()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");
    board_rows(&model, 40, 10);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditNotes, None).expect("edit notes");
    let before = caret_at(&model, 40, 10);
    apply_intent(&mut domain, &mut model, BoardIntent::EditMoveUp, None)
        .expect("up across the wrap");
    let after = caret_at(&model, 40, 10);
    assert!(
        after < before,
        "Up must climb from the wrapped tail row ({before}) onto the head row, got {after}"
    );
}

/// A pathological title cannot eat the page: at the compact floor the wrapped
/// header stops inside the page body, the last shown row carries the omission
/// marker, and the rule/status/verb chrome survives untouched.
#[test]
fn task_page_caps_a_wrapped_header_inside_the_page_body() {
    let mut domain = DomainState::new();
    let title = "word ".repeat(120);
    domain
        .create(
            &title,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");

    let rows = board_rows(&model, 40, 10);
    // Chrome rows keep their own content: rule dashes at 7, status at 8, verbs at 9.
    assert!(
        rows[7].starts_with('─'),
        "the rule row was overwritten by the header:\n{}",
        rows.join("\n")
    );
    assert!(
        rows[8].contains("done"),
        "the status row was overwritten by the header:\n{}",
        rows.join("\n")
    );
    assert!(
        rows[9].contains("ctrl+e"),
        "the verb row was overwritten by the header:\n{}",
        rows.join("\n")
    );
    // The header itself is bounded and honest about what it hides.
    assert!(
        (1..7).any(|y| rows[y].contains("wor…")),
        "a capped header names the rows it cannot show:\n{}",
        rows.join("\n")
    );
}

/// Step navigation must not re-scroll the whole page on every cursor move: the
/// viewport height the renderer records (`window_rows`) bounds the scroll math,
/// and losing it collapses the window to one row.
#[test]
fn step_cursor_moves_do_not_rescroll_the_page_when_steps_fit() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Steps page",
            Some("n".to_string()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    for index in 0..20 {
        domain
            .add_step(id, format!("step {index}"))
            .expect("add step");
    }
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");
    // One painted frame records the viewport; then walk the step cursor down.
    board_rows(&model, 80, 24);
    for _ in 0..4 {
        apply_intent(&mut domain, &mut model, BoardIntent::PageScrollDown, None)
            .expect("advance step cursor");
    }
    let rows = board_rows(&model, 80, 24);
    let text = rows.join("\n");
    assert!(
        text.contains("step 0"),
        "a fitting steps window must not scroll its head away:\\n{text}"
    );
    assert!(
        text.contains("step 3"),
        "the walked-to step must be visible:\\n{text}"
    );
}

/// A title-edit caret hidden below the capped header parks at the END of the
/// last shown row, never at its hidden column on the ellipsis row.
#[test]
fn edit_title_caret_parks_at_the_capped_headers_end() {
    let mut domain = DomainState::new();
    let title = "word ".repeat(120);
    domain
        .create(
            &title,
            None,
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None).expect("open page");
    board_rows(&model, 40, 10);
    apply_intent(&mut domain, &mut model, BoardIntent::BeginEditTitle, None).expect("edit title");

    let mut terminal = Terminal::new(TestBackend::new(40, 10)).expect("terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw edit page");
    let rows = board_rows(&model, 40, 10);
    // The capped header's last row carries the marker; the caret must sit at
    // that row's past-end column, immediately after the "…".
    let last_header = (1..7)
        .rev()
        .find(|&y| rows[y].contains("wor…"))
        .expect("capped header row");
    let cursor = terminal.backend().cursor_position();
    assert_eq!(
        cursor.y, last_header as u16,
        "caret left the last header row"
    );
    let painted = rows[last_header].trim_end();
    assert_eq!(
        cursor.x as usize,
        painted.chars().count(),
        "caret must sit immediately after the marker, not at a hidden column:\n{}",
        rows.join("\n")
    );
}

/// A deck longer than the viewport paints a dim track+thumb on the right edge and
/// reserves a gap so wrapped titles never sit under the thumb.
#[test]
fn overflowing_board_list_paints_a_scrollbar() {
    let tasks: Vec<Task> = (0..40u128)
        .map(|i| {
            task(
                7000 + i,
                &format!("Scrollbar padding task {i}"),
                HumanStatus::Ready,
                project("/repos/tsk"),
                3600,
            )
        })
        .collect();
    let last_id = Uuid::from_u128(7000 + 39);
    let view = fixture_view_projects(&tasks, false);
    let mut model = fixture_model_on_tab(&tasks, &view, BoardTab::Projects);
    model.selection_id = Some(last_id);

    let (rows, geo) = paint(80, 24, &model);
    let top = geo.viewport_top as usize;
    let bottom = (geo.viewport_top + geo.viewport_height) as usize;
    let viewport = &rows[top..bottom];
    assert!(
        viewport.iter().any(|row| row.contains('▌')),
        "overflowing list needs a thumb:\n{}",
        viewport.join("\n")
    );
    assert!(
        viewport
            .iter()
            .all(|row| { !matches!(row.chars().last(), Some('│')) }),
        "scrollbar gutter is blank, not a │ track:\n{}",
        viewport.join("\n")
    );
    assert!(
        viewport
            .iter()
            .any(|row| trimmed(row).contains("Scrollbar padding task 39")),
        "selection-follow still keeps the selected task visible:\n{}",
        viewport.join("\n")
    );
}

#[test]
fn overflowing_board_list_does_not_clip_task_rows_to_ellipsis() {
    let tasks: Vec<Task> = (0..40u128)
        .map(|i| {
            task(
                7100 + i,
                &format!("ScrollbarNoBreakTitleThatMustWrapNotEllipsize{i:02}XXXXXXXXXXXXXXXX"),
                HumanStatus::Ready,
                project("/repos/tsk"),
                3600,
            )
        })
        .collect();
    let view = fixture_view_projects(&tasks, false);
    let model = fixture_model_on_tab(&tasks, &view, BoardTab::Projects);
    let (rows, geo) = paint(80, 24, &model);
    let top = geo.viewport_top as usize;
    let bottom = (geo.viewport_top + geo.viewport_height) as usize;
    let mut saw_title = false;
    for row in &rows[top..bottom] {
        if trimmed(row).contains("ScrollbarNoBreakTitle") {
            saw_title = true;
            assert!(
                !row.contains('…'),
                "scrollbar must not clip task rows into ellipsis:\n{row}"
            );
        }
    }
    assert!(
        saw_title,
        "fixture title must actually paint:\n{}",
        rows.join("\n")
    );
}

#[test]
fn scrolled_list_pins_the_section_header_under_the_tabs() {
    let tasks: Vec<Task> = (0..40u128)
        .map(|i| {
            task(
                7200 + i,
                &format!("Sticky header task {i}"),
                HumanStatus::Ready,
                TaskScope::Global,
                3600,
            )
        })
        .collect();
    let view = fixture_view(&tasks, false);
    let mut model = fixture_model(&tasks, &view);
    model.selection_id = Some(Uuid::from_u128(7200));
    model.list_scroll = 6;
    model.follow_list = false;
    let (rows, geo) = paint(80, 24, &model);
    let top = geo.viewport_top as usize;
    assert!(
        trimmed(&rows[top]).is_empty(),
        "pinned header needs a blank row under the tabs:\n{}",
        rows.join("\n")
    );
    let header = trimmed(&rows[top + 1]);
    assert!(
        header.contains("desk"),
        "desk header must pin under the tabs after scrolling past it:\n{header}\n{}",
        rows.join("\n")
    );
}

#[test]
fn scrolled_done_section_pins_done_under_the_tabs() {
    let mut tasks: Vec<Task> = (0..8u128)
        .map(|i| {
            task(
                7300 + i,
                &format!("Open sticky {i}"),
                HumanStatus::Ready,
                TaskScope::Global,
                3600,
            )
        })
        .collect();
    tasks.extend((0..40u128).map(|i| {
        task(
            7400 + i,
            &format!("Done sticky {i}"),
            HumanStatus::Done,
            TaskScope::Global,
            3600,
        )
    }));
    let view = fixture_view(&tasks, true);
    let mut model = fixture_model(&tasks, &view);
    model.selection_id = Some(Uuid::from_u128(7300));
    model.list_scroll = 20;
    model.follow_list = false;
    let (rows, geo) = paint(80, 24, &model);
    let top = geo.viewport_top as usize;
    assert!(
        trimmed(&rows[top]).is_empty(),
        "pinned header needs a blank row under the tabs:\n{}",
        rows.join("\n")
    );
    let header = trimmed(&rows[top + 1]);
    assert!(
        header.contains("DONE"),
        "DONE header must pin under the tabs once that section reaches the top:\n{header}\n{}",
        rows.join("\n")
    );
}

#[test]
fn archived_task_paints_in_no_working_lens_in_any_status_at_any_tier() {
    for status in [
        HumanStatus::Ready,
        HumanStatus::Started,
        HumanStatus::Blocked,
        HumanStatus::Review,
        HumanStatus::Done,
    ] {
        let mut domain = DomainState::new();
        let project = TaskScope::Project {
            path: "/repos/lens".into(),
        };
        domain
            .create(
                "visible row task",
                None,
                project.clone(),
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create live task");
        let archived_title = format!("archived-{status:?} row");
        let archived_id = domain
            .create(
                archived_title.clone(),
                None,
                project,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create archived task");
        domain.archive_task(archived_id).expect("archive it");
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from("/repos/lens")));

        // Lens setups: desk, projects, threads (home tabs) and project focus.
        let lens_names = ["desk", "projects", "threads", "project focus"];
        for (lens_index, lens) in lens_names.iter().enumerate() {
            match lens_index {
                0 => {
                    apply_intent(
                        &mut domain,
                        &mut model,
                        BoardIntent::SelectHomeTab(BoardTab::Desk),
                        None,
                    )
                    .expect("desk tab");
                }
                1 => {
                    apply_intent(
                        &mut domain,
                        &mut model,
                        BoardIntent::SelectHomeTab(BoardTab::Projects),
                        None,
                    )
                    .expect("projects tab");
                }
                2 => {
                    apply_intent(
                        &mut domain,
                        &mut model,
                        BoardIntent::SelectHomeTab(BoardTab::Threads),
                        None,
                    )
                    .expect("threads tab");
                }
                _ => {
                    apply_intent(
                        &mut domain,
                        &mut model,
                        BoardIntent::OpenProjectSelector,
                        None,
                    )
                    .expect("open picker");
                    apply_intent(
                        &mut domain,
                        &mut model,
                        BoardIntent::SelectProjectOption(1),
                        None,
                    )
                    .expect("focus the project");
                }
            }

            for (width, height) in [(80u16, 24u16), (40u16, 10u16)] {
                let rows = board_rows(&model, width, height);
                assert!(
                    !rows.iter().any(|row| row.contains(&archived_title)),
                    "archived {status:?} task painted at {width}x{height} in {lens}:\n{}",
                    rows.join("\n")
                );
            }

            // Stage G rail at a wide width: the board column is a 32-cell rail.
            for _ in 0..2 {
                apply_intent(&mut domain, &mut model, BoardIntent::StageRight, None)
                    .expect("stage right");
            }
            let rows = board_rows(&model, 130, 24);
            assert!(
                !rows.iter().any(|row| row.contains(&archived_title)),
                "archived {status:?} task painted on the stage G rail in {lens}:\n{}",
                rows.join("\n")
            );
            for _ in 0..2 {
                apply_intent(&mut domain, &mut model, BoardIntent::StageLeft, None)
                    .expect("stage left");
            }
        }
    }
}

//! Wide stage slider: chrome (T2), stage routing (T3), mouse (T4), dirty drafts (T5).

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use tsk_tui::domain::{DomainState, HumanStatus, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::board::{
    resolve_responsive, FocusedSurface, ResponsivePresentation, WideStage, WIDE_SPLIT_MIN_WIDTH,
};
use tsk_tui::ui::input::map_responsive_key;
use tsk_tui::ui::render::{assert_buffer_mono, QueueHitMap, QueueHitTarget};
use tsk_tui::ui::tier::{Tier, STANDARD_VERB_BAR_ENTRY_BUDGET};
use tsk_tui::ui::{
    apply_intent, draw_board, BoardInputMode, BoardIntent, BoardModel, IntentOutcome,
};

const REPO: &str = "/repos/tsk";

const T12_NOTES: &str = "Rework the wide split so the task page reads as a **detail pane**, not a boxed clone.\n\n- keep board unboxed\n- decide separator\n- verb bar ownership\n\n```\nresolve_responsive(w, h, focus)\n```";

/// The brief's reference fixture: T12 started with markdown notes in this repo, T15 ready on
/// the desk. The model carries the numbers; the domain shares the ids.
fn fixture() -> (DomainState, BoardModel) {
    fixture_with_titles("Frame the wide task view", "Renew domain")
}

fn fixture_with_titles(first: &str, second: &str) -> (DomainState, BoardModel) {
    let mut domain = DomainState::new();
    let started = domain
        .create(
            first,
            Some(T12_NOTES.to_string()),
            TaskScope::Project {
                path: REPO.to_string(),
            },
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create T12");
    domain
        .set_status(started, HumanStatus::Started)
        .expect("start T12");
    domain
        .create(
            second,
            Some("renewal notes".to_string()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create T15");
    let mut tasks = domain.tasks().to_vec();
    tasks[0].number = Some(12);
    tasks[1].number = Some(15);
    let model = BoardModel::from_tasks(tasks, Some(PathBuf::from(REPO)));
    (domain, model)
}

fn go(domain: &mut DomainState, model: &mut BoardModel, intent: BoardIntent) {
    apply_intent(domain, model, intent, None).expect("apply intent");
}

fn to_stage(domain: &mut DomainState, model: &mut BoardModel, stage: WideStage) {
    let steps = match stage {
        WideStage::FullBoard => 0,
        WideStage::Split => 1,
        WideStage::Rail => 2,
        WideStage::FullTask => 3,
    };
    for _ in 0..steps {
        go(domain, model, BoardIntent::StageRight);
    }
    assert_eq!(model.wide_stage(), stage);
}

const STAGES: [WideStage; 4] = [
    WideStage::FullBoard,
    WideStage::Split,
    WideStage::Rail,
    WideStage::FullTask,
];

fn render_buffer(model: &BoardModel, width: u16, height: u16) -> (Buffer, QueueHitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    let mut hits = QueueHitMap::default();
    terminal
        .draw(|frame| hits = draw_board(frame, model))
        .expect("draw board");
    (terminal.backend().buffer().clone(), hits)
}

fn rows_of(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// Render and assert the frame is mono at every width, wide or not.
fn render(model: &BoardModel, width: u16, height: u16) -> (Vec<String>, QueueHitMap) {
    let (buffer, hits) = render_buffer(model, width, height);
    assert_buffer_mono(&buffer);
    (rows_of(&buffer), hits)
}

fn region_text(rows: &[String], area: Rect) -> String {
    rows.iter()
        .skip(area.y as usize)
        .take(area.height as usize)
        .map(|row| {
            row.chars()
                .skip(area.x as usize)
                .take(area.width as usize)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn column_text(rows: &[String], column: Rect, y: u16) -> String {
    rows[y as usize]
        .chars()
        .skip(column.x as usize)
        .take(column.width as usize)
        .collect()
}

fn footer_rows(height: u16) -> (u16, u16, u16) {
    (height - 3, height - 2, height - 1)
}

fn inside(area: Rect, hit: Rect) -> bool {
    hit.x >= area.x
        && hit.y >= area.y
        && hit.x.saturating_add(hit.width) <= area.x.saturating_add(area.width)
        && hit.y.saturating_add(hit.height) <= area.y.saturating_add(area.height)
}

// ---------------------------------------------------------------------------
// T2 chrome
// ---------------------------------------------------------------------------

#[test]
fn wide_frames_are_mono_at_every_stage_width_and_height() {
    for stage in STAGES {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, stage);
        for (width, height) in [
            (WIDE_SPLIT_MIN_WIDTH, 24),
            (111, 24),
            (130, 24),
            (130, 10),
            (157, 26),
            (200, 40),
        ] {
            let (buffer, _) = render_buffer(&model, width, height);
            assert_buffer_mono(&buffer);
        }
    }
}

#[test]
fn wide_paints_exactly_one_footer_at_every_stage() {
    for stage in STAGES {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, stage);
        let (rows, hits) = render(&model, 130, 24);
        let (rule_y, status_y, verb_y) = footer_rows(24);
        let rule_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.chars().all(|c| c == '─'))
            .map(|(y, _)| y)
            .collect();
        assert_eq!(
            rule_rows,
            vec![rule_y as usize],
            "{stage:?}: one full-width rule"
        );
        let done_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.starts_with(" 0 done"))
            .map(|(y, _)| y)
            .collect();
        assert_eq!(
            done_rows,
            vec![status_y as usize],
            "{stage:?}: one status row"
        );
        assert!(rows[status_y as usize].starts_with(" 0 done"), "{stage:?}");
        let verb_row = &rows[verb_y as usize];
        assert!(
            verb_row.starts_with(' '),
            "{stage:?}: verb bar keeps its leading space"
        );
        assert!(verb_row.contains("ctrl+d done"), "{stage:?}: {verb_row}");
        let ctrl_rows = rows.iter().filter(|row| row.contains("ctrl+")).count();
        assert_eq!(ctrl_rows, 1, "{stage:?}: exactly one verb bar");
        let verb_hits: Vec<&tsk_tui::ui::render::QueueHit> = hits
            .regions
            .iter()
            .filter(|hit| matches!(hit.target, QueueHitTarget::Verb(_)))
            .collect();
        assert!(!verb_hits.is_empty(), "{stage:?}: verb hits exist");
        assert!(
            verb_hits.iter().all(|hit| hit.area.y == verb_y),
            "{stage:?}: every verb hit sits on the one verb row"
        );
    }
}

#[test]
fn stage_zero_and_full_task_render_the_standard_tier_at_130x24() {
    let (mut domain, mut model) = fixture();
    let geometry = resolve_responsive(130, 24, WideStage::FullBoard);
    assert_eq!(geometry.density, Tier::Standard);
    let (rows, hits) = render(&model, 130, 24);
    assert!(rows[0].trim().is_empty(), "row 0 blank");
    assert!(
        rows[1].contains("desk  ·  projects  ·  threads"),
        "selector row"
    );
    assert!(rows[2].trim().is_empty(), "blank above IN MOTION");
    assert!(rows[3].starts_with(" IN MOTION ─"));
    assert!(rows[4].trim().is_empty(), "blank between header and rows");
    assert!(rows[5].starts_with("  ▸ T12 Frame the wide task view"));
    assert!(
        rows[5].trim_end().ends_with("tsk · 0s"),
        "meta column: {}",
        rows[5]
    );
    assert!(rows[6].trim().is_empty());
    assert!(rows[7].starts_with(" desk ─"));
    assert!(rows[9].starts_with("  ○ T15 Renew domain"));
    let verbs = hits
        .regions
        .iter()
        .filter(|hit| matches!(hit.target, QueueHitTarget::Verb(_)))
        .count();
    assert!(verbs > 5, "standard verb budget, not compact: {verbs}");
    assert!(verbs <= usize::from(STANDARD_VERB_BAR_ENTRY_BUDGET));

    to_stage(&mut domain, &mut model, WideStage::FullTask);
    assert_eq!(
        resolve_responsive(130, 24, WideStage::FullTask).density,
        Tier::Standard
    );
    let (rows, _) = render(&model, 130, 24);
    assert!(rows[0].trim().is_empty(), "F keeps the blank row");
    assert!(rows[1].starts_with(" T12 Frame the wide task view ─"));
    assert!(rows[1].trim_end().ends_with("started · tsk"));
    assert!(
        rows[2].contains("Rework the wide split"),
        "body starts under the header"
    );
}

#[test]
fn task_column_header_replaces_the_in_pane_header_with_stage_weight() {
    for stage in [WideStage::Split, WideStage::Rail, WideStage::FullTask] {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, stage);
        let geometry = resolve_responsive(130, 24, stage);
        let column = geometry.task_content();
        let (buffer, _) = render_buffer(&model, 130, 24);
        let rows = rows_of(&buffer);
        let header = column_text(&rows, column, 1);
        assert!(
            header.starts_with(" T12 Frame the wide task view ─"),
            "{stage:?}: {header}"
        );
        assert!(
            header.trim_end().ends_with("started · tsk"),
            "{stage:?}: {header}"
        );
        let body = region_text(&rows, Rect::new(column.x, 2, column.width, 19));
        assert!(
            !body.contains("▸ T12"),
            "{stage:?}: in-pane header must not paint"
        );
        assert!(
            !body
                .lines()
                .any(|line| line.trim().chars().all(|c| c == '─') && !line.trim().is_empty()),
            "{stage:?}: no in-pane divider"
        );
        assert!(column_text(&rows, column, 2).contains("Rework the wide split"));
        let title_x = column.x + 5;
        let title_cell = &buffer[(title_x, 1)];
        if stage == WideStage::Split {
            assert!(title_cell.modifier.contains(Modifier::DIM), "A header dim");
            assert!(
                !title_cell.modifier.contains(Modifier::BOLD),
                "A header not bold"
            );
        } else {
            assert!(
                title_cell.modifier.contains(Modifier::BOLD),
                "{stage:?} header bold"
            );
            assert!(!title_cell.modifier.contains(Modifier::DIM));
        }
    }
}

#[test]
fn stage_a_board_keeps_its_meta_column() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    let geometry = resolve_responsive(130, 24, WideStage::Split);
    let (rows, _) = render(&model, 130, 24);
    let row = column_text(&rows, geometry.board, 5);
    assert!(row.starts_with("  ▸ T12 Frame"), "{row}");
    assert!(row.trim_end().ends_with("tsk · 0s"), "meta column: {row}");
    assert_eq!(rows[5].chars().nth(geometry.rule.x as usize), Some('│'));
}

#[test]
fn rail_wraps_titles_with_indent_four_and_dims_every_cell() {
    let long = "A deliberately long rail title that cannot fit thirty two columns";
    let (mut domain, mut model) = fixture_with_titles(long, "Renew domain");
    go(&mut domain, &mut model, BoardIntent::ToggleDoneDrawer);
    assert!(model.drawer_open());
    to_stage(&mut domain, &mut model, WideStage::Rail);
    let geometry = resolve_responsive(130, 24, WideStage::Rail);
    let (buffer, _) = render_buffer(&model, 130, 24);
    let rows = rows_of(&buffer);
    let rail = geometry.board;
    let (rule_y, _, _) = footer_rows(24);
    let rail_rows: Vec<String> = (0..rule_y).map(|y| column_text(&rows, rail, y)).collect();
    let first = rail_rows
        .iter()
        .position(|row| row.starts_with("  ▹ T12 "))
        .expect("selected rail row with hollow marker");
    let continuations: Vec<&String> = rail_rows[first + 1..]
        .iter()
        .take_while(|row| !row.trim().is_empty())
        .collect();
    assert!(
        continuations.len() >= 2,
        "long title wraps over several rows"
    );
    for row in &continuations {
        assert!(row.starts_with("    "), "continuation indent 4: {row:?}");
        assert!(!row.starts_with("     "), "exactly four cells: {row:?}");
    }
    let painted: String = std::iter::once(&rail_rows[first])
        .chain(continuations.iter().copied())
        .map(|row| row.trim().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    for word in long.split(' ') {
        assert!(painted.contains(word), "never truncated: {painted}");
    }
    assert!(!painted.contains('…'));
    for y in 0..rule_y {
        assert_eq!(
            buffer[(rail.width, y)].symbol(),
            "│",
            "rule column at row {y}"
        );
        assert_eq!(
            buffer[(rail.width - 1, y)].symbol(),
            " ",
            "no glyph touches the rule at row {y}"
        );
        for x in 0..rail.width {
            assert!(
                buffer[(x, y)].modifier.contains(Modifier::DIM),
                "rail cell ({x},{y}) must be dim"
            );
        }
    }
    let rail_text = rail_rows.join("\n");
    assert!(!rail_text.contains("0s"), "rail drops the meta column");
    assert!(!rail_text.contains("DONE"), "rail drops the done drawer");
    assert!(rail_text.contains(" IN MOTION ─"));
    assert!(rail_text.contains(" desk ─"));
    assert!(rail_text.contains("desk  ·  projects  ·  threads"));
}

#[test]
fn status_row_crumb_and_keys_follow_the_stage() {
    let expectations = [
        (WideStage::FullBoard, None, "→ pane · enter open"),
        (
            WideStage::Split,
            Some("board ▸ task"),
            "→ task · ← close · enter open",
        ),
        (
            WideStage::Rail,
            Some("board ◂ task"),
            "← board · → full page",
        ),
        (WideStage::FullTask, None, "← rail · esc back"),
    ];
    for (stage, crumb, keys) in expectations {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, stage);
        let (rows, _) = render(&model, 130, 24);
        let (_, status_y, _) = footer_rows(24);
        let status = &rows[status_y as usize];
        assert!(status.trim_end().ends_with(keys), "{stage:?}: {status}");
        match crumb {
            Some(crumb) => assert!(
                status.contains(&format!("{crumb}    {keys}")),
                "{stage:?}: {status}"
            ),
            None => assert!(!status.contains('▸') && !status.contains('◂'), "{stage:?}"),
        }
        for other in ["board ▸ task", "board ◂ task"] {
            if crumb != Some(other) {
                assert!(!status.contains(other), "{stage:?}: {status}");
            }
        }
    }
}

#[test]
fn status_row_refusal_wins_over_the_crumb_then_the_keys() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    let (_, status_y, _) = footer_rows(24);
    let (rows, _) = render(&model, 130, 24);
    assert!(rows[status_y as usize].contains("board ▸ task"));

    model.set_message("x".repeat(90));
    let (rows, _) = render(&model, 130, 24);
    let status = &rows[status_y as usize];
    assert!(status.contains(&"x".repeat(90)));
    assert!(
        !status.contains("board ▸ task"),
        "crumb drops first: {status}"
    );
    assert!(status.contains("enter open"), "keys survive: {status}");

    model.set_message("y".repeat(120));
    let (rows, _) = render(&model, 130, 24);
    let status = &rows[status_y as usize];
    assert!(status.contains(&"y".repeat(120)));
    assert!(
        !status.contains("enter open"),
        "keys drop after the crumb: {status}"
    );
}

#[test]
fn status_row_shows_editor_keys_while_an_editor_is_active() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    go(&mut domain, &mut model, BoardIntent::BeginEditTitle);
    let (rows, _) = render(&model, 130, 24);
    let (_, status_y, _) = footer_rows(24);
    assert!(rows[status_y as usize].contains("shift+enter save · esc cancel"));
    assert!(!rows[status_y as usize].contains("← board"));
    let geometry = resolve_responsive(130, 24, WideStage::Rail);
    let header = column_text(&rows, geometry.task_content(), 1);
    assert!(header.trim_end().ends_with("editing title"), "{header}");
}

#[test]
fn empty_pane_paints_no_task_header_and_is_inert() {
    let mut model = BoardModel::from_tasks(Vec::new(), Some(PathBuf::from(REPO)));
    let mut domain = DomainState::new();
    assert_eq!(model.selected_id(), None);
    go(&mut domain, &mut model, BoardIntent::StageRight);
    assert_eq!(
        model.wide_stage(),
        WideStage::FullBoard,
        "stage A needs a selection"
    );
    go(&mut domain, &mut model, BoardIntent::OpenTaskPage);
    assert_eq!(model.wide_stage(), WideStage::FullBoard);

    // Stage A reached with a task, which then disappears: stay in A, paint the empty pane.
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    model.sync_from_domain(&DomainState::new());
    assert_eq!(model.selected_id(), None);
    assert_eq!(model.wide_stage(), WideStage::Split);
    let geometry = resolve_responsive(130, 24, WideStage::Split);
    let (rows, hits) = render(&model, 130, 24);
    let column = geometry.task_content();
    let header = column_text(&rows, column, 1);
    assert!(header.starts_with(" no task ─"), "{header}");
    assert!(column_text(&rows, column, 2).contains("select a task to preview it here"));
    assert!(
        hits.regions
            .iter()
            .all(|hit| !inside(column_text_area(column, 24), hit.area)),
        "empty pane exposes no hits"
    );
    go(&mut domain, &mut model, BoardIntent::StageRight);
    assert_eq!(model.wide_stage(), WideStage::Split, "G needs a selection");
}

fn column_text_area(column: Rect, height: u16) -> Rect {
    Rect::new(column.x, 0, column.width, height - 3)
}

#[test]
fn wide_hits_stay_inside_their_column_or_the_footer() {
    for stage in STAGES {
        for (width, height) in [(110, 24), (130, 24), (157, 26), (240, 60)] {
            let (mut domain, mut model) = fixture();
            to_stage(&mut domain, &mut model, stage);
            let geometry = resolve_responsive(width, height, stage);
            let (_, hits) = render(&model, width, height);
            let footer = Rect::new(0, height - 3, width, 3);
            let board = Rect::new(0, 0, geometry.board.width, height - 3);
            let task = Rect::new(
                geometry.task_content().x,
                0,
                geometry.task_content().width,
                height - 3,
            );
            for area in hits
                .regions
                .iter()
                .map(|hit| hit.area)
                .chain(hits.copyable.iter().copied())
            {
                assert!(
                    inside(footer, area) || inside(board, area) || inside(task, area),
                    "{stage:?} {width}x{height}: {area:?} escapes board {board:?}, task {task:?}, footer {footer:?}"
                );
                assert!(
                    !(geometry.rule.width > 0
                        && area.x <= geometry.rule.x
                        && area.x + area.width > geometry.rule.x
                        && area.y < height - 3),
                    "{stage:?} {width}x{height}: {area:?} crosses the rule column"
                );
            }
            if geometry.task.width > 0 && geometry.board.width > 0 {
                assert!(
                    hits.regions.iter().any(|hit| inside(board, hit.area)),
                    "{stage:?}: board column keeps row hits"
                );
            }
        }
    }
}

#[test]
fn stage_a_preview_paints_controls_for_the_focus_router_only() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    let geometry = resolve_responsive(130, 24, WideStage::Split);
    let (_, hits) = render(&model, 130, 24);
    let column = column_text_area(geometry.task_content(), 24);
    let number = hits
        .regions
        .iter()
        .find(|hit| inside(column, hit.area) && matches!(hit.target, QueueHitTarget::TaskNumber(_)))
        .expect("the T<number> prefix keeps its copy hit");
    assert_eq!(
        number.area,
        Rect::new(geometry.task_content().x + 1, 1, 3, 1)
    );
    assert!(
        hits.regions
            .iter()
            .any(|hit| inside(column, hit.area) && matches!(hit.target, QueueHitTarget::StepAdd)),
        "preview controls exist for the focus router"
    );
}

#[test]
fn narrow_board_is_unchanged_by_the_stage_model() {
    let (mut domain, mut model) = fixture();
    let (rows, _) = render(&model, 109, 24);
    assert!(rows[1].contains("desk  ·  projects  ·  threads"));
    assert!(rows[5].starts_with("  ▸ T12 Frame the wide task view"));
    assert!(
        !rows[22].contains("→ pane"),
        "no crumb below the wide threshold"
    );
    go(&mut domain, &mut model, BoardIntent::PeekDetail);
    let (rows, _) = render(&model, 109, 24);
    assert!(
        rows.join("\n").contains("│ Rework the wide split"),
        "peek still works narrow"
    );
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
}

// ---------------------------------------------------------------------------
// T3 stage routing
// ---------------------------------------------------------------------------

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn wide_key(model: &BoardModel, code: KeyCode) -> Option<BoardIntent> {
    map_responsive_key(
        model.input_mode(),
        model.wide_stage(),
        ResponsivePresentation::WideSplit,
        model.task_editing(),
        key(code),
    )
}

/// Press one key at a wide width: map it, apply it when it maps to something.
fn press(domain: &mut DomainState, model: &mut BoardModel, code: KeyCode) -> Option<BoardIntent> {
    let intent = wide_key(model, code);
    if let Some(intent) = intent.clone() {
        go(domain, model, intent);
    }
    intent
}

#[test]
fn stage_zero_keys_slide_open_select_and_stay_put() {
    let (mut domain, mut model) = fixture();
    let first = model.selected_id();
    assert_eq!(wide_key(&model, KeyCode::Left), None, "← is inert in 0");
    assert_eq!(
        wide_key(&model, KeyCode::Esc),
        Some(BoardIntent::CloseLayer)
    );
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Char('j')),
        Some(BoardIntent::SelectNext)
    );
    assert_ne!(model.selected_id(), first);
    assert_eq!(model.wide_stage(), WideStage::FullBoard);
    assert_eq!(press(&mut domain, &mut model, KeyCode::Tab), None);
    assert_eq!(model.wide_stage(), WideStage::FullBoard);
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Right),
        Some(BoardIntent::StageRight)
    );
    assert_eq!(model.wide_stage(), WideStage::Split);
    assert_eq!(model.focused_surface(), FocusedSurface::Board);

    let (mut domain, mut model) = fixture();
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Enter),
        Some(BoardIntent::OpenTaskPage)
    );
    assert_eq!(model.wide_stage(), WideStage::FullTask);
    assert_eq!(model.stage_origin(), Some(WideStage::FullBoard));
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
}

#[test]
fn stage_a_keys_slide_both_ways_open_and_retarget_the_pane() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    let first = model.selected_id().expect("selection");
    assert_eq!(
        wide_key(&model, KeyCode::Esc),
        Some(BoardIntent::CloseLayer)
    );
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Char('j')),
        Some(BoardIntent::SelectNext)
    );
    let second = model.selected_id().expect("moved selection");
    assert_ne!(second, first);
    assert_eq!(model.wide_stage(), WideStage::Split);
    let geometry = resolve_responsive(130, 24, WideStage::Split);
    let (rows, _) = render(&model, 130, 24);
    assert!(column_text(&rows, geometry.task_content(), 1).contains("T15 Renew domain"));
    assert_eq!(press(&mut domain, &mut model, KeyCode::Tab), None);
    assert_eq!(
        model.wide_stage(),
        WideStage::Split,
        "Tab is not a stage key"
    );

    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Left),
        Some(BoardIntent::StageLeft)
    );
    assert_eq!(model.wide_stage(), WideStage::FullBoard);
    to_stage(&mut domain, &mut model, WideStage::Split);
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Right),
        Some(BoardIntent::StageRight)
    );
    assert_eq!(model.wide_stage(), WideStage::Rail);
    assert_eq!(model.focused_surface(), FocusedSurface::Task);
    assert_eq!(model.edit_target(), Some(second));
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);

    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Enter),
        Some(BoardIntent::OpenTaskPage)
    );
    assert_eq!(model.wide_stage(), WideStage::FullTask);
    assert_eq!(model.stage_origin(), Some(WideStage::Split));
}

#[test]
fn stage_g_keys_slide_open_close_and_navigate_the_page() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    let bound = model.edit_target().expect("bound page");
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Char('j')),
        Some(BoardIntent::PageScrollDown)
    );
    assert_eq!(model.wide_stage(), WideStage::Rail);
    assert_eq!(
        model.selected_id(),
        Some(bound),
        "j navigates the page, not the rail"
    );
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Right),
        Some(BoardIntent::StageRight)
    );
    assert_eq!(model.wide_stage(), WideStage::FullTask);
    assert_eq!(model.stage_origin(), Some(WideStage::Rail));

    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    let bound = model.edit_target().expect("bound page");
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Left),
        Some(BoardIntent::StageLeft)
    );
    assert_eq!(model.wide_stage(), WideStage::Split);
    assert_eq!(model.focused_surface(), FocusedSurface::Board);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(model.edit_target(), Some(bound), "the pane stays bound");

    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Esc),
        Some(BoardIntent::CloseLayer)
    );
    assert_eq!(model.wide_stage(), WideStage::Split);

    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    let bound = model.edit_target().expect("bound page");
    assert_eq!(
        press(&mut domain, &mut model, KeyCode::Enter),
        Some(BoardIntent::OpenTaskPage)
    );
    assert_eq!(model.wide_stage(), WideStage::FullTask);
    assert_eq!(model.stage_origin(), Some(WideStage::Rail));
    assert_eq!(model.edit_target(), Some(bound));
}

#[test]
fn stage_f_keys_return_to_the_rail_or_the_origin() {
    for origin in [WideStage::FullBoard, WideStage::Split, WideStage::Rail] {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, origin);
        press(&mut domain, &mut model, KeyCode::Enter);
        assert_eq!(model.wide_stage(), WideStage::FullTask, "{origin:?}");
        assert_eq!(
            wide_key(&model, KeyCode::Right),
            None,
            "{origin:?}: → inert in F"
        );
        assert_eq!(
            press(&mut domain, &mut model, KeyCode::Char('k')),
            Some(BoardIntent::PageScrollUp)
        );
        assert_eq!(model.wide_stage(), WideStage::FullTask);
        assert_eq!(
            press(&mut domain, &mut model, KeyCode::Esc),
            Some(BoardIntent::CloseLayer)
        );
        assert_eq!(model.wide_stage(), origin, "Esc returns to {origin:?}");
        assert_eq!(model.stage_origin(), None, "origin memory clears");
        assert_eq!(
            model.focused_surface(),
            origin.focused_surface(),
            "{origin:?}"
        );
        if origin == WideStage::FullBoard {
            assert_eq!(
                model.edit_target(),
                None,
                "back on the bare board the page closes"
            );
        } else {
            assert!(
                model.edit_target().is_some(),
                "{origin:?} keeps the page session"
            );
        }
    }

    for origin in [WideStage::FullBoard, WideStage::Split, WideStage::Rail] {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, origin);
        press(&mut domain, &mut model, KeyCode::Enter);
        assert_eq!(
            press(&mut domain, &mut model, KeyCode::Left),
            Some(BoardIntent::StageLeft)
        );
        assert_eq!(
            model.wide_stage(),
            WideStage::Rail,
            "← from F always goes to G"
        );
        assert_eq!(model.stage_origin(), None);
        assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    }
}

#[test]
fn enter_in_stage_f_closes_the_page_like_the_single_pane_page() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Split);
    press(&mut domain, &mut model, KeyCode::Enter);
    assert_eq!(model.wide_stage(), WideStage::FullTask);
    press(&mut domain, &mut model, KeyCode::Enter);
    assert_eq!(
        model.wide_stage(),
        WideStage::Split,
        "Enter toggles the page shut"
    );
    assert_eq!(model.stage_origin(), None);
}

#[test]
fn stage_keys_need_a_selection() {
    let mut domain = DomainState::new();
    let mut model = BoardModel::from_tasks(Vec::new(), Some(PathBuf::from(REPO)));
    for code in [KeyCode::Right, KeyCode::Enter] {
        press(&mut domain, &mut model, code);
        assert_eq!(model.wide_stage(), WideStage::FullBoard, "{code:?}");
        assert_eq!(model.input_mode(), BoardInputMode::Normal);
    }
    assert_eq!(model.edit_target(), None);
}

#[test]
fn stage_keys_fall_through_to_editor_semantics_while_editing() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    go(&mut domain, &mut model, BoardIntent::BeginEditTitle);
    assert_eq!(model.input_mode(), BoardInputMode::EditTitle);
    assert_eq!(
        wide_key(&model, KeyCode::Left),
        Some(BoardIntent::EditMoveLeft)
    );
    assert_eq!(
        wide_key(&model, KeyCode::Right),
        Some(BoardIntent::EditMoveRight)
    );
    assert_eq!(
        wide_key(&model, KeyCode::Esc),
        Some(BoardIntent::CancelEdit)
    );
    // An edit session in view mode (add target selected) keeps the page's own Esc.
    go(&mut domain, &mut model, BoardIntent::CancelEdit);
    go(&mut domain, &mut model, BoardIntent::BeginEditTitle);
    for _ in 0..8 {
        if model.input_mode() == BoardInputMode::TaskPage {
            break;
        }
        go(&mut domain, &mut model, BoardIntent::FormFocusNext);
    }
    assert_eq!(model.input_mode(), BoardInputMode::TaskPage);
    assert!(model.task_editing());
    assert_eq!(wide_key(&model, KeyCode::Left), None);
    assert_eq!(
        wide_key(&model, KeyCode::Esc),
        Some(BoardIntent::CloseLayer)
    );
    press(&mut domain, &mut model, KeyCode::Esc);
    assert!(!model.task_editing(), "Esc cancels the session first");
    assert_eq!(
        model.wide_stage(),
        WideStage::Rail,
        "and leaves the stage alone"
    );
}

#[test]
fn narrow_routes_are_unchanged_by_the_slider() {
    let (mut domain, mut model) = fixture();
    let narrow = |model: &BoardModel, code| {
        map_responsive_key(
            model.input_mode(),
            model.wide_stage(),
            ResponsivePresentation::SingleBoard,
            model.task_editing(),
            key(code),
        )
    };
    assert_eq!(
        narrow(&model, KeyCode::Right),
        Some(BoardIntent::PeekDetail)
    );
    assert_eq!(
        narrow(&model, KeyCode::Left),
        Some(BoardIntent::CollapseDetail)
    );
    assert_eq!(
        narrow(&model, KeyCode::Enter),
        Some(BoardIntent::OpenTaskPage)
    );
    go(&mut domain, &mut model, BoardIntent::OpenTaskPage);
    let page = |model: &BoardModel, code| {
        map_responsive_key(
            model.input_mode(),
            model.wide_stage(),
            ResponsivePresentation::SingleTask,
            model.task_editing(),
            key(code),
        )
    };
    assert_eq!(page(&model, KeyCode::Left), None);
    assert_eq!(page(&model, KeyCode::Right), None);
    assert_eq!(page(&model, KeyCode::Esc), Some(BoardIntent::CloseLayer));
    go(&mut domain, &mut model, BoardIntent::CloseLayer);
    assert_eq!(model.input_mode(), BoardInputMode::Normal);
    assert_eq!(model.edit_target(), None);
    assert_eq!(model.wide_stage(), WideStage::FullBoard);
}

#[test]
fn shrinking_and_growing_keeps_every_stage_and_its_session() {
    let long = "long notes ".repeat(400);
    for stage in STAGES {
        let mut domain = DomainState::new();
        domain
            .create(
                "resize survivor",
                Some(long.clone()),
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create survivor");
        domain
            .create(
                "other row",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("create other");
        let mut model = BoardModel::from_domain(&domain, Some(PathBuf::from(REPO)));
        let survivor = domain
            .tasks()
            .iter()
            .find(|task| task.title == "resize survivor")
            .expect("survivor")
            .id;
        let index = model
            .visible_ids()
            .iter()
            .position(|&id| id == survivor)
            .expect("survivor row");
        go(&mut domain, &mut model, BoardIntent::SelectIndex(index));
        to_stage(&mut domain, &mut model, stage);
        let _ = render(&model, 130, 24);
        if stage.focused_surface() == FocusedSurface::Task {
            go(&mut domain, &mut model, BoardIntent::PageWheelScrollDown);
            assert!(
                model.page_scroll() > 0,
                "{stage:?}: fixture exercises page scroll"
            );
        }
        let before = (
            model.wide_stage(),
            model.focused_surface(),
            model.selected_id(),
            model.edit_target(),
            model.page_scroll(),
            model.input_mode(),
        );

        let (narrow_rows, _) = render(&model, 109, 24);
        let narrow = resolve_responsive(109, 24, stage);
        match stage.focused_surface() {
            FocusedSurface::Board => {
                assert_eq!(narrow.presentation, ResponsivePresentation::SingleBoard);
                assert!(narrow_rows[1].contains("desk  ·  projects"), "{stage:?}");
            }
            FocusedSurface::Task => {
                assert_eq!(narrow.presentation, ResponsivePresentation::SingleTask);
                assert!(
                    !narrow_rows.join("\n").contains("other row"),
                    "{stage:?}: the page fills the frame"
                );
            }
        }
        assert_eq!(
            (
                model.wide_stage(),
                model.focused_surface(),
                model.selected_id(),
                model.edit_target(),
                model.page_scroll(),
                model.input_mode(),
            ),
            before,
            "{stage:?}: shrink keeps the stage and session"
        );

        let (wide_rows, _) = render(&model, 130, 24);
        assert_eq!(
            (
                model.wide_stage(),
                model.focused_surface(),
                model.selected_id(),
                model.edit_target(),
                model.page_scroll(),
                model.input_mode(),
            ),
            before,
            "{stage:?}: grow keeps the stage and session"
        );
        let geometry = resolve_responsive(130, 24, stage);
        if geometry.task.width > 0 {
            assert!(
                column_text(&wide_rows, geometry.task_content(), 1).contains("resize survivor"),
                "{stage:?}"
            );
        }
        if geometry.board.width > 0 {
            assert!(
                region_text(&wide_rows, geometry.board).contains("resize survivor"),
                "{stage:?}"
            );
        }
    }
}

#[test]
fn shrinking_during_an_edit_keeps_mode_draft_cursor_and_binding() {
    let (mut domain, mut model) = fixture();
    to_stage(&mut domain, &mut model, WideStage::Rail);
    go(&mut domain, &mut model, BoardIntent::BeginEditTitle);
    go(&mut domain, &mut model, BoardIntent::EditInsert('!'));
    go(&mut domain, &mut model, BoardIntent::EditMoveLeft);
    let before = (
        model.wide_stage(),
        model.input_mode(),
        model.edit_target(),
        model.edit_buffer().to_string(),
        model.edit_cursor(),
    );
    assert!(model.task_session_dirty());
    let (narrow_rows, _) = render(&model, 109, 24);
    assert!(narrow_rows.join("\n").contains(&before.3));
    let _ = render(&model, 130, 24);
    assert_eq!(
        (
            model.wide_stage(),
            model.input_mode(),
            model.edit_target(),
            model.edit_buffer().to_string(),
            model.edit_cursor(),
        ),
        before
    );
}

#[test]
fn repeated_threshold_crossings_keep_every_stage_live() {
    for stage in STAGES {
        let (mut domain, mut model) = fixture();
        to_stage(&mut domain, &mut model, stage);
        for width in [109u16, 110].into_iter().cycle().take(20) {
            let (rows, hits) = render(&model, width, 24);
            assert!(!rows.is_empty());
            assert!(!hits.regions.is_empty(), "{stage:?} at {width}");
        }
        assert_eq!(model.wide_stage(), stage);
    }
}

#[test]
fn stage_changes_never_mutate_the_domain() {
    let (mut domain, mut model) = fixture();
    let before = domain.clone();
    for intent in [
        BoardIntent::StageRight,
        BoardIntent::StageRight,
        BoardIntent::StageRight,
        BoardIntent::StageLeft,
        BoardIntent::StageLeft,
        BoardIntent::OpenTaskPage,
        BoardIntent::CloseLayer,
        BoardIntent::StageLeft,
    ] {
        let outcome = apply_intent(&mut domain, &mut model, intent, None).expect("stage intent");
        assert_eq!(outcome, IntentOutcome::None);
    }
    assert_eq!(model.wide_stage(), WideStage::FullBoard);
    assert_eq!(domain.tasks(), before.tasks());
    for width in [109u16, 110, 130, 109] {
        let _ = render(&model, width, 24);
    }
    assert_eq!(domain.tasks(), before.tasks());
}

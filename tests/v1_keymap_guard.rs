//! Guard the documented V1 queue keymap against retired UI returning.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::board::{apply_intent, BoardInputMode, BoardModel};
use tsk_tui::ui::input::{map_key, normal_mode_keymap, BoardIntent};

fn normal(code: KeyCode) -> Option<BoardIntent> {
    map_key(
        BoardInputMode::Normal,
        KeyEvent::new(code, KeyModifiers::NONE),
    )
}

fn alt(code: KeyCode) -> Option<BoardIntent> {
    map_key(
        BoardInputMode::Normal,
        KeyEvent::new(code, KeyModifiers::ALT),
    )
}

#[test]
fn normal_mode_keymap_equals_the_readme_and_queue_board_v1_set() {
    let documented = [
        (KeyCode::Char('q'), BoardIntent::Quit),
        (KeyCode::Esc, BoardIntent::CloseLayer),
        (KeyCode::Char('j'), BoardIntent::SelectNext),
        (KeyCode::Down, BoardIntent::SelectNext),
        (KeyCode::Char('k'), BoardIntent::SelectPrev),
        (KeyCode::Up, BoardIntent::SelectPrev),
        (KeyCode::Char(' '), BoardIntent::PrimaryVerb),
        (KeyCode::Char('d'), BoardIntent::Complete),
        (KeyCode::Char('o'), BoardIntent::Reopen),
        (KeyCode::Char('b'), BoardIntent::ToggleBlock),
        (KeyCode::Enter, BoardIntent::OpenTaskPage),
        (KeyCode::Right, BoardIntent::PeekDetail),
        (KeyCode::Left, BoardIntent::CollapseDetail),
        (KeyCode::Char('+'), BoardIntent::OpenCapture),
        (KeyCode::Char('e'), BoardIntent::BeginEditTitle),
        (KeyCode::Char('x'), BoardIntent::SoftDelete),
        (KeyCode::Delete, BoardIntent::SoftDelete),
        (KeyCode::Char('u'), BoardIntent::Undo),
        (KeyCode::Char('z'), BoardIntent::ToggleDoneDrawer),
        (KeyCode::Char(':'), BoardIntent::OpenCommandPalette),
        (KeyCode::Char('?'), BoardIntent::OpenHelp),
        (KeyCode::Char('P'), BoardIntent::OpenProjectSelector),
    ];
    assert_eq!(
        normal_mode_keymap(),
        documented,
        "the normal-mode table must equal the documented V1 queue keymap"
    );
    let mutating = [
        KeyCode::Char('q'),
        KeyCode::Char(' '),
        KeyCode::Char('d'),
        KeyCode::Char('o'),
        KeyCode::Char('b'),
        KeyCode::Char('e'),
        KeyCode::Char('x'),
        KeyCode::Delete,
        KeyCode::Char('u'),
    ];
    for (key, intent) in documented {
        if mutating.contains(&key) {
            assert_eq!(normal(key), None, "bare mutating key {key:?} must be dead");
            assert_eq!(alt(key), Some(intent), "alt+{key:?}");
        } else {
            assert_eq!(normal(key), Some(intent), "documented key {key:?}");
        }
    }

    for retired in [
        'a', 'v', 'p', 'r', 'l', 'c', 'n', 's', 't', 'i', 'f', '1', '2', '3', '[', ']',
    ] {
        assert_eq!(
            normal(KeyCode::Char(retired)),
            None,
            "retired normal-mode key {retired:?} must stay unbound"
        );
    }
}

#[test]
fn ctrl_c_quits_from_normal_and_task_page_modes() {
    assert_eq!(
        map_key(
            BoardInputMode::Normal,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ),
        Some(BoardIntent::Quit)
    );
    assert_eq!(
        map_key(
            BoardInputMode::TaskPage,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ),
        Some(BoardIntent::Quit)
    );
}

/// T-3 (AC-9): bare `a`/`space`/`x`/`e` on the task page view produce no steps
/// mutation. The guard runs with the step cursor active (the state the modifier-protected
/// verbs would target), so a bare key that slipped past the verb-modifier gate would be
/// caught acting on the highlighted step.
#[test]
fn bare_page_keys_never_mutate_steps() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "Guarded page task",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    domain.add_step(id, "alpha step").expect("step 1");
    domain.add_step(id, "bravo step").expect("step 2");
    let mut model = BoardModel::from_domain(&domain, None);
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::OpenTaskPage,
        None,
        None,
    )
    .expect("open page");
    // Activate the step cursor (the first bare Down on a task with steps).
    apply_intent(
        &mut domain,
        &mut model,
        BoardIntent::PageScrollDown,
        None,
        None,
    )
    .expect("cursor press");

    let before = domain.get(id).expect("task").clone();
    for key in [
        KeyCode::Char('a'),
        KeyCode::Char(' '),
        KeyCode::Char('x'),
        KeyCode::Char('e'),
    ] {
        assert_eq!(
            map_key(
                BoardInputMode::TaskPage,
                KeyEvent::new(key, KeyModifiers::NONE)
            ),
            None,
            "bare {key:?} must be dead on the page view"
        );
    }
    let after = domain.get(id).expect("task");
    assert_eq!(
        after.steps, before.steps,
        "bare page keys must not mutate the steps"
    );
    assert_eq!(after.status, before.status, "status untouched");
    assert_eq!(after.revision, before.revision, "no journaled mutation");
    assert_eq!(
        model.input_mode(),
        BoardInputMode::TaskPage,
        "bare page keys must not open an edit"
    );
}

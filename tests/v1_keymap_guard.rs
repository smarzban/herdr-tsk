//! Guard the documented V1 queue keymap against retired UI returning.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_tasks::ui::board::BoardInputMode;
use herdr_tasks::ui::input::{map_key, normal_mode_keymap, BoardIntent};

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
        (KeyCode::Char('a'), BoardIntent::OpenCapture),
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
        KeyCode::Char('a'),
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
        'v', 'p', 'r', 'l', 'c', 'n', 's', 't', 'i', 'f', '1', '2', '3', '[', ']',
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

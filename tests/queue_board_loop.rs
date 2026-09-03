//! App loop wiring: the Frame Scheduler on the board's input wait, no the attention poll on
//! the board frame path, and a load+draw smoke path.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tsk_tui::app::{
    board_frame, board_poll_duration, load_board_model, take_pending_after_paint, FramePoll,
};
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::ui::scheduler::{next_wait, DEFAULT_BASE_TICK};
use tsk_tui::ui::{apply_intent, draw_board, BoardIntent, BoardModel, IntentOutcome};

/// A directory this test owns alone, removed on drop even if the test panics.
struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Restores an environment variable to whatever it held before the test touched it, so one
/// test's `TSK_STATE_DIR` never leaks into the next.
struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &std::path::Path) -> Self {
        let previous = env::var_os(key);
        env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => env::set_var(self.key, value),
            None => env::remove_var(self.key),
        }
    }
}

fn temp_state_dir(label: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!("tsk-queue-board-loop-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("create temp state dir");
    dir
}

/// The board loop's poll duration is not a second copy of the scheduler's arithmetic: it
/// calls straight through to it, idle and short-tick alike.
#[test]
fn board_frame_poll_duration_uses_scheduler_idle_and_short_ticks() {
    assert_eq!(
        board_poll_duration(false),
        next_wait(false, DEFAULT_BASE_TICK),
        "idle wait must equal the scheduler's own idle answer"
    );
    assert_eq!(
        board_poll_duration(false),
        Duration::from_millis(250),
        "M1 has no animation source, so the loop's idle wait is the 250ms base tick"
    );

    assert_eq!(
        board_poll_duration(true),
        next_wait(true, DEFAULT_BASE_TICK),
        "the animating wait must equal the scheduler's own short-tick answer"
    );
    assert!(
        board_poll_duration(true) < board_poll_duration(false),
        "an active animation must shorten the wait below the idle floor"
    );
    assert!(
        board_poll_duration(true) >= Duration::from_millis(25),
        "the short tick must never drop below the scheduler's 25ms floor"
    );
}

/// Drives [`board_frame`] the way the real board loop does -- settle, paint, wait, repeat --
/// for many idle iterations, and observes the duration `board_frame` *itself* hands the wait
/// closure, so this is a test of the product function's wiring, not a second copy of the
/// scheduler's arithmetic re-asserted through a test-owned duration.
///
/// The wait closure here only records what it is given and never computes a duration of its
/// own -- that is the point: if `board_frame` stopped calling [`board_poll_duration`] (e.g. a
/// regression back to a fixed constant, or to a value under the scheduler's 25ms/250ms
/// floors), this test would observe the wrong duration and fail, because the duration is read
/// off the call, not recomputed by the test.
#[test]
fn instrumented_loop_idle_wait_never_sustained_below_25ms_without_animation() {
    let domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    let mut recorded_waits = Vec::new();
    for _ in 0..50 {
        let poll = board_frame(
            &mut model,
            || panic!("no walkthrough was opened; nothing should report a dismissal to record"),
            |model| {
                // `TestBackend`'s draw error is `Infallible`; `expect` collapses it to the
                // `io::Result<()>` `board_frame` requires without inventing a fake error path.
                terminal
                    .draw(|frame| {
                        let _ = draw_board(frame, model);
                    })
                    .expect("test backend draw");
                Ok(())
            },
            |duration| {
                recorded_waits.push(duration);
                Ok(false)
            },
            false,
        )
        .expect("board frame");
        assert_eq!(
            poll,
            FramePoll::Idle,
            "the wait closure always answers no-event, so every iteration is Idle"
        );
    }

    assert_eq!(
        recorded_waits.len(),
        50,
        "every iteration must record its wait"
    );
    assert!(
        recorded_waits
            .iter()
            .all(|wait| *wait >= Duration::from_millis(25)),
        "an idle loop must never sustain a sub-25ms wait: {recorded_waits:?}"
    );
    assert!(
        recorded_waits
            .iter()
            .all(|wait| *wait >= Duration::from_millis(250)),
        "with no animation ever active the wait must stay at the 250ms idle floor: {recorded_waits:?}"
    );
}

#[test]
fn autoscroll_shortens_the_board_frame_wait() {
    let domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut recorded = None;
    let poll = board_frame(
        &mut model,
        || panic!("no walkthrough"),
        |model| {
            terminal
                .draw(|frame| {
                    let _ = draw_board(frame, model);
                })
                .expect("draw");
            Ok(())
        },
        |duration| {
            recorded = Some(duration);
            Ok(false)
        },
        true,
    )
    .expect("board frame");
    assert_eq!(poll, FramePoll::Idle);
    let wait = recorded.expect("wait recorded");
    assert_eq!(wait, next_wait(true, DEFAULT_BASE_TICK));
    assert!(
        wait < Duration::from_millis(250),
        "armed autoscroll must shorten the wait, got {wait:?}"
    );
    assert!(
        wait >= Duration::from_millis(25),
        "short tick must stay at the 25ms floor, got {wait:?}"
    );
}

/// [`load_board_model`] (the whole the open path: store load + context snapshot + BoardModel
/// construction, no host refresh) feeds straight into [`draw_board`] without panicking at the
/// standard tier's floor size ( smoke;/'s "board loop stops calling attention"
/// leaves this as the one open-path exercise: load, then draw).
#[test]
fn load_board_and_draw_path_smoke_at_80x24() {
    let dir = temp_state_dir("smoke");
    let _dir_guard = TempDirGuard(dir.clone());
    let _env_guard = EnvVarGuard::set("TSK_STATE_DIR", &dir);

    let model = load_board_model().expect("load board model from an empty temp state dir");

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            let _ = draw_board(frame, &model);
        })
        .expect("draw the loaded board without panicking");
}

#[test]
fn pending_resize_event_paints_before_it_is_returned() {
    let key = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let mut pending = Some(key.clone());
    let mut painted = 0usize;
    let out = take_pending_after_paint(&mut pending, || {
        painted += 1;
        Ok::<_, ()>(())
    })
    .expect("paint");
    assert_eq!(
        painted, 1,
        "settled size must paint before the deferred event"
    );
    assert_eq!(out, Some(key));
    assert!(pending.is_none());
}

#[test]
fn no_pending_event_skips_the_settled_paint() {
    let mut pending: Option<Event> = None;
    let mut painted = 0usize;
    let out = take_pending_after_paint(&mut pending, || {
        painted += 1;
        Ok::<_, ()>(())
    })
    .expect("paint");
    assert_eq!(painted, 0);
    assert!(out.is_none());
}

#[test]
fn run_board_resize_arm_drains_through_coalesce_then_paints() {
    let src = include_str!("../src/app.rs");
    assert!(
        src.contains("pending_event = scheduler::coalesce_resizes(event::poll, event::read)?"),
        "run_board Resize arm must call coalesce_resizes"
    );
    assert!(
        src.contains("take_pending_after_paint(&mut pending_event"),
        "run_board must paint the settled size before a deferred event"
    );
}

#[test]
fn repeated_threshold_resizes_keep_board_loop_live() {
    let domain = DomainState::new();
    let mut model = BoardModel::from_domain(&domain, None);

    for width in [109, 110].into_iter().cycle().take(40) {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).expect("test terminal");
        let poll = board_frame(
            &mut model,
            || panic!("no walkthrough"),
            |model| {
                terminal
                    .draw(|frame| {
                        let _ = draw_board(frame, model);
                    })
                    .expect("settled responsive frame");
                Ok(())
            },
            |_| Ok(false),
            false,
        )
        .expect("responsive board frame");
        assert_eq!(poll, FramePoll::Idle);
    }
}

#[test]
fn threshold_crossings_without_task_verbs_leave_domain_unchanged() {
    let mut domain = DomainState::new();
    let id = domain
        .create(
            "immutable through resize",
            Some("domain must not move".to_string()),
            TaskScope::Global,
            ProvenanceOrigin::Manual,
            None,
        )
        .expect("create task");
    let before = domain.get(id).expect("task").clone();
    let mut model = BoardModel::from_domain(&domain, None);

    assert_eq!(
        apply_intent(&mut domain, &mut model, BoardIntent::OpenTaskPage, None)
            .expect("open the full task page"),
        IntentOutcome::None
    );
    for width in [109, 110].into_iter().cycle().take(20) {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).expect("test terminal");
        let poll = board_frame(
            &mut model,
            || panic!("no walkthrough"),
            |model| {
                terminal
                    .draw(|frame| {
                        let _ = draw_board(frame, model);
                    })
                    .expect("draw threshold frame");
                Ok(())
            },
            |_| Ok(false),
            false,
        )
        .expect("threshold frame");
        assert_eq!(poll, FramePoll::Idle);
    }

    assert_eq!(domain.get(id), Some(&before));
}

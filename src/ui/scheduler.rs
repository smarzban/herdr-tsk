//! Frame Scheduler: adaptive input-wait duration.
//!
//! Pure helper only — app loop wiring is.

use std::time::Duration;

/// Default idle poll when no base is supplied by the caller.
pub const DEFAULT_BASE_TICK: Duration = Duration::from_millis(250);

/// Short-tick floor while an animation is active (never the idle path).
const SHORT_TICK_FLOOR: Duration = Duration::from_millis(25);

/// Prototype-like animation frame interval (~30 Hz, within 16–33 ms).
const SHORT_TICK: Duration = Duration::from_millis(33);

/// Quiet window after a terminal resize before the next forced layout paint.
///
/// Dragging a pane edge fires many `Event::Resize` reports; waiting this long
/// after the latest one lets the size settle so the board rebuilds once.
pub const RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);

/// Next input-wait duration for one frame-loop cycle.
///
/// - Idle (`active_animations == false`): at least `base` and
///   [`DEFAULT_BASE_TICK`].
/// - Animating: the shorter of `base` or the short tick, floored at 25 ms.
pub fn next_wait(active_animations: bool, base: Duration) -> Duration {
    if active_animations {
        base.min(SHORT_TICK).max(SHORT_TICK_FLOOR)
    } else {
        base.max(DEFAULT_BASE_TICK)
    }
}

/// Drain a burst of resize events, returning the first non-resize (if any).
///
/// `poll` / `read` are injected so the policy is unit-testable without a tty.
/// Each resize resets the quiet window; when the window elapses with no further
/// event, returns `None` so the caller can paint once at the settled size.
pub fn coalesce_resizes<E>(
    mut poll: impl FnMut(Duration) -> Result<bool, E>,
    mut read: impl FnMut() -> Result<crossterm::event::Event, E>,
    is_resize: impl Fn(&crossterm::event::Event) -> bool,
) -> Result<Option<crossterm::event::Event>, E> {
    let mut deadline = std::time::Instant::now() + RESIZE_DEBOUNCE;
    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        let wait = deadline.saturating_duration_since(now);
        if !poll(wait)? {
            return Ok(None);
        }
        let event = read()?;
        if is_resize(&event) {
            deadline = std::time::Instant::now() + RESIZE_DEBOUNCE;
            continue;
        }
        return Ok(Some(event));
    }
}

#[cfg(test)]
mod tests {
    use super::{coalesce_resizes, next_wait, DEFAULT_BASE_TICK, RESIZE_DEBOUNCE};
    use crossterm::event::Event;
    use std::time::Duration;

    #[test]
    fn idle_wait_is_at_least_250ms_with_no_active_animation() {
        let wait = next_wait(false, DEFAULT_BASE_TICK);
        assert!(
            wait >= Duration::from_millis(250),
            "idle wait must be ≥250 ms, got {wait:?}"
        );
        assert_eq!(wait, DEFAULT_BASE_TICK);

        let sub_floor_base = Duration::from_millis(5);
        assert_eq!(next_wait(false, sub_floor_base), DEFAULT_BASE_TICK);
    }

    #[test]
    fn active_animation_shortens_wait_below_base_tick_and_never_below_25ms_when_idle_returns() {
        let base = DEFAULT_BASE_TICK;
        let animating = next_wait(true, base);
        assert!(
            animating < base,
            "active animation must shorten below base, got {animating:?} vs base {base:?}"
        );
        assert!(
            animating >= Duration::from_millis(25),
            "short tick must not drop below 25 ms floor, got {animating:?}"
        );
        // Prototype-like cadence sits in the ~16–33 ms band after the floor.
        assert!(
            animating <= Duration::from_millis(33),
            "short tick should stay ≤33 ms, got {animating:?}"
        );

        let tiny_base = Duration::from_millis(20);
        assert_eq!(next_wait(true, tiny_base), Duration::from_millis(25));

        let idle_again = next_wait(false, base);
        assert!(
            idle_again >= Duration::from_millis(250),
            "returning to idle must restore ≥250 ms, got {idle_again:?}"
        );
    }

    #[test]
    fn no_sustained_sub_25ms_wait_while_animation_set_empty() {
        // Sweep bases because a sustained-loop property is verified by.
        for base in [
            Duration::from_millis(5),
            Duration::from_millis(25),
            DEFAULT_BASE_TICK,
            Duration::from_millis(500),
        ] {
            let wait = next_wait(false, base);
            assert!(
                wait >= Duration::from_millis(25),
                "empty animation set produced sub-25 ms wait: {wait:?}"
            );
            assert!(
                wait >= Duration::from_millis(250),
                "empty animation set must stay at least 250 ms, got {wait:?}"
            );
            assert!(
                wait >= base,
                "idle wait must preserve base {base:?}, got {wait:?}"
            );
        }
    }

    #[test]
    fn coalesce_resizes_returns_none_when_only_resizes_arrive() {
        use std::cell::Cell;
        let sizes = [(80u16, 24u16), (90, 30), (100, 40)];
        let idx = Cell::new(0usize);
        let out = coalesce_resizes(
            |_| Ok::<_, ()>(idx.get() < sizes.len()),
            || {
                let i = idx.get();
                let (w, h) = sizes[i];
                idx.set(i + 1);
                Ok(Event::Resize(w, h))
            },
            |e| matches!(e, Event::Resize(_, _)),
        )
        .expect("coalesce");
        assert!(out.is_none());
        assert!(RESIZE_DEBOUNCE >= Duration::from_millis(16));
    }

    #[test]
    fn coalesce_resizes_surfaces_the_first_non_resize() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use std::cell::Cell;
        let events = [
            Event::Resize(80, 24),
            Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        ];
        let idx = Cell::new(0usize);
        let out = coalesce_resizes(
            |_| Ok::<_, ()>(idx.get() < events.len()),
            || {
                let i = idx.get();
                let event = events[i].clone();
                idx.set(i + 1);
                Ok(event)
            },
            |e| matches!(e, Event::Resize(_, _)),
        )
        .expect("coalesce");
        assert!(matches!(out, Some(Event::Key(_))));
    }
}

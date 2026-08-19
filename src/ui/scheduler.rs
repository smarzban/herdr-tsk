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

#[cfg(test)]
mod tests {
    use super::{next_wait, DEFAULT_BASE_TICK};
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
}

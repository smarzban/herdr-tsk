//! Tier Layout Resolver: map terminal size to standard/compact frame geometry.

/// Layout tier for the queue board (the: standard + compact; wide is not returned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Standard,
    Compact,
}

/// Pure frame geometry for one terminal size.
///
/// Chrome row indices are `None` when that row does not fit. Viewport height is
/// zero when there is no room between selector and bottom chrome. Renderers can
/// place rows from this struct at any size ≥1×1 without panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierGeometry {
    pub tier: Tier,
    pub width: u16,
    pub height: u16,
    /// Selector row (view segments + project chip). Always row 0 when height ≥ 1.
    pub selector_row: Option<u16>,
    /// First list viewport row (below selector).
    pub viewport_top: u16,
    /// List viewport height in rows.
    pub viewport_height: u16,
    /// Dim rule above the status line.
    pub rule_row: Option<u16>,
    /// Status line (counts).
    pub status_row: Option<u16>,
    /// Verb bar (bottom).
    pub verb_row: Option<u16>,
    /// Full content width for a painted row (terminal width).
    pub row_width: u16,
    /// Title cells available after the meta reserve (and 0-width when width is 0).
    pub title_width: u16,
    /// Trailing meta column budget; 0 in compact.
    pub meta_column_width: u16,
    /// Project chip truncation ceiling (cells).
    pub selector_chip_max: u16,
    /// How many verb-bar entries may be shown.
    pub verb_bar_entry_budget: u16,
}

/// Project chip truncation ceiling shared by both tiers.
pub const SELECTOR_CHIP_MAX_CELLS: u16 = 24;

/// Compact verb bar keeps at most this many entries.
pub const COMPACT_VERB_BAR_ENTRY_BUDGET: u16 = 5;

/// Full standard verb-bar set: space · enter · d · b · : · ? · +.
pub const STANDARD_VERB_BAR_ENTRY_BUDGET: u16 = 7;

/// Minimum width for the standard board tier. This is the default Herdr split amendment.
const STANDARD_MIN_WIDTH: u16 = 78;

/// Reserved trailing meta cells in the standard tier (project · age).
const STANDARD_META_COLUMN_WIDTH: u16 = 28;

/// Map terminal dimensions to a tier and frame geometry.
///
/// Standard when width ≥ 78 and height ≥ 24; otherwise compact. Width ≥ 120 still
/// reports standard in the (wide is not returned). Any size yields a geometry;
/// callers may pass values below 1×1 and still receive a non-panicking result.
pub fn resolve(width: u16, height: u16) -> TierGeometry {
    let tier = if width >= STANDARD_MIN_WIDTH && height >= 24 {
        Tier::Standard
    } else {
        Tier::Compact
    };

    let (selector_row, viewport_top, viewport_height, rule_row, status_row, verb_row) =
        chrome_rows(height);

    let meta_column_width = match tier {
        Tier::Standard => STANDARD_META_COLUMN_WIDTH.min(width),
        Tier::Compact => 0,
    };
    let title_width = width.saturating_sub(meta_column_width);
    let verb_bar_entry_budget = match tier {
        Tier::Standard => STANDARD_VERB_BAR_ENTRY_BUDGET,
        Tier::Compact => COMPACT_VERB_BAR_ENTRY_BUDGET,
    };

    TierGeometry {
        tier,
        width,
        height,
        selector_row,
        viewport_top,
        viewport_height,
        rule_row,
        status_row,
        verb_row,
        row_width: width,
        title_width,
        meta_column_width,
        selector_chip_max: SELECTOR_CHIP_MAX_CELLS,
        verb_bar_entry_budget,
    }
}

/// Place chrome from the outside in so indices never overlap.
///
/// height ≥ 4: blank · selector · viewport · rule · status · verb.
/// height == 3: selector · status · verb.
/// height == 2: selector · verb.
/// height ≤ 1: selector only (or nothing when height == 0).
fn chrome_rows(height: u16) -> (Option<u16>, u16, u16, Option<u16>, Option<u16>, Option<u16>) {
    match height {
        0 => (None, 0, 0, None, None, None),
        1 => (Some(0), 0, 0, None, None, None),
        2 => (Some(0), 0, 0, None, None, Some(1)),
        3 => (Some(0), 0, 0, None, Some(1), Some(2)),
        4 => (Some(0), 1, 0, Some(1), Some(2), Some(3)),
        h => {
            // row 0 blank; row 1 selector; rows 2..h-4 list; h-3 rule; h-2 status; h-1 verbs
            (
                Some(1),
                2,
                h.saturating_sub(5),
                Some(h - 3),
                Some(h - 2),
                Some(h - 1),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_78x24_and_above_until_120_cols_tier_is_standard_with_selector_viewport_rule_status_verb_rows(
    ) {
        for &(w, h) in &[
            (78, 24),
            (78, 30),
            (100, 24),
            (119, 40),
            (120, 24),
            (200, 50),
        ] {
            let g = resolve(w, h);
            assert_eq!(g.tier, Tier::Standard, "{w}x{h}");
            assert_eq!(g.selector_row, Some(1), "{w}x{h}");
            assert_eq!(g.viewport_top, 2, "{w}x{h}");
            assert_eq!(g.viewport_height, h.saturating_sub(5), "{w}x{h}");
            assert_eq!(g.rule_row, Some(h - 3), "{w}x{h}");
            assert_eq!(g.status_row, Some(h - 2), "{w}x{h}");
            assert_eq!(g.verb_row, Some(h - 1), "{w}x{h}");
            assert_eq!(g.row_width, w, "{w}x{h}");
            assert!(g.meta_column_width > 0, "{w}x{h} standard reserves meta");
            assert_eq!(
                g.title_width,
                w.saturating_sub(g.meta_column_width),
                "{w}x{h}"
            );
            assert_eq!(g.selector_chip_max, SELECTOR_CHIP_MAX_CELLS, "{w}x{h}");
            assert_eq!(
                g.verb_bar_entry_budget, STANDARD_VERB_BAR_ENTRY_BUDGET,
                "{w}x{h}"
            );
            if (w, h) == (STANDARD_MIN_WIDTH, 24) {
                assert_eq!(g.meta_column_width, STANDARD_META_COLUMN_WIDTH);
                assert_eq!(g.title_width, 50, "78 columns retain 50 title cells");
            }
            // Width ≥ 120 still reports standard in the (wide deferred).
            if w >= 120 {
                assert_eq!(g.tier, Tier::Standard, "wide not returned in M1 at {w}x{h}");
            }
        }
    }

    #[test]
    fn below_78_or_below_24_tier_is_compact_with_one_line_rows_and_verb_bar_budget_leq_5() {
        for &(w, h) in &[
            (77, 24),
            (78, 23),
            (77, 23),
            (48, 18),
            (40, 10),
            (50, 30),
            (100, 20),
        ] {
            let g = resolve(w, h);
            assert_eq!(g.tier, Tier::Compact, "{w}x{h}");
            assert_eq!(g.meta_column_width, 0, "{w}x{h} one-line rows: no meta");
            assert_eq!(g.title_width, w, "{w}x{h}");
            assert_eq!(g.row_width, w, "{w}x{h}");
            assert!(
                g.verb_bar_entry_budget <= 5,
                "{w}x{h} verb budget {} > 5",
                g.verb_bar_entry_budget
            );
            assert_eq!(
                g.verb_bar_entry_budget, COMPACT_VERB_BAR_ENTRY_BUDGET,
                "{w}x{h}"
            );
            assert_eq!(g.selector_chip_max, SELECTOR_CHIP_MAX_CELLS, "{w}x{h}");
            if h >= 4 {
                assert_eq!(g.selector_row, Some(1), "{w}x{h}");
                assert_eq!(g.viewport_top, 2, "{w}x{h}");
                assert_eq!(g.viewport_height, h.saturating_sub(5), "{w}x{h}");
                assert_eq!(g.rule_row, Some(h - 3), "{w}x{h}");
                assert_eq!(g.status_row, Some(h - 2), "{w}x{h}");
                assert_eq!(g.verb_row, Some(h - 1), "{w}x{h}");
            }
        }
    }

    #[test]
    fn every_size_from_1x1_through_40x10_returns_a_geometry_without_panic() {
        for h in 1u16..=10 {
            for w in 1u16..=40 {
                let g = resolve(w, h);
                assert_eq!(g.width, w);
                assert_eq!(g.height, h);
                assert_eq!(g.row_width, w);
                assert!(
                    g.title_width.saturating_add(g.meta_column_width) <= g.row_width,
                    "{w}x{h}: title+meta exceed row"
                );
                assert_eq!(g.selector_chip_max, SELECTOR_CHIP_MAX_CELLS);
                // Compact at these sizes (all below standard breakpoint).
                assert_eq!(g.tier, Tier::Compact, "{w}x{h}");
                assert_eq!(g.meta_column_width, 0, "{w}x{h}");
                assert!(g.verb_bar_entry_budget <= 5, "{w}x{h}");
                // Chrome indices that exist stay inside the frame and do not collide.
                let mut used = vec![false; h as usize];
                for row in [g.selector_row, g.rule_row, g.status_row, g.verb_row]
                    .into_iter()
                    .flatten()
                {
                    assert!(row < h, "{w}x{h}: chrome row {row} out of bounds");
                    assert!(
                        !used[row as usize],
                        "{w}x{h}: chrome row {row} occupied twice"
                    );
                    used[row as usize] = true;
                }
                if g.viewport_height > 0 {
                    let end = g.viewport_top as u32 + g.viewport_height as u32;
                    assert!(end <= h as u32, "{w}x{h}: viewport overflows");
                    for row in g.viewport_top..g.viewport_top + g.viewport_height {
                        assert!(
                            !used[row as usize],
                            "{w}x{h}: viewport overlaps chrome at {row}"
                        );
                    }
                }
            }
        }
    }
}

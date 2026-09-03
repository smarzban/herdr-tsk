# Board UI

**Responsibility.** Session presentation of the queue: derive sections, map keys and
mouse to `BoardIntent`, reduce intents against domain + `BoardModel`, paint with
ratatui. Persistence is [app](app.md) / [store](store.md).

**Public surface.** Re-exported from `ui`: `BoardModel`, `BoardInputMode`,
`apply_intent`, `draw_board`, `board_hit_map`, `BoardIntent`, `map_key`,
`map_board_mouse`, queue types, render helpers, tier geometry,
scrollbar, markdown painters, text-select. Internal split:
`ui/board/{model,apply,commands,chrome,draw}`.

## How it works

### Queue query

Pure function of the task snapshot + lens + done-drawer flag. Soft-deleted tasks
are dropped before grouping.

| Lens | Sections |
| --- | --- |
| Home **desk** | IN MOTION = every `started` task (any scope), newest `updated_at` first; ON DECK = desk (`Global`) ready/blocked/review, with thread blocks then loose; optional DONE drawer |
| Home **projects** | One group per project path that has open work (or is `this_repo`); inside: started, review, blocked, ready; collapse is session-only |
| Home **threads** | One group per thread name across projects; project sub-groups (desk is `project_path: None`); same status order |
| Project focus | IN MOTION for that path; ON DECK for that path with thread headers; optional DONE |

Status rank inside a group: started → review → blocked → ready → done.
`visible_task_ids` honors collapse sets; DONE on the threads tab stays selectable
even without a thread label.

Thread blocks exist only on scoped ON DECK. Headers are renderer chrome
(ADR-0003): they consume a list row of budget, register no hit target, and are
not in `task_ids`. Invariant: `task_ids` = concat(blocks' ids, `loose_task_ids`)
in paint order.

`StatusCounts.need` is always 0 in this tree (field retained). Done count is the
number of live done tasks (the `drawer_open` branches currently agree).

### Model

`BoardModel` is session-only: location (home tab vs project path), selected id,
wide stage (`WideStage`, with the origin stage `Enter` left), board and page scroll,
peek, collapse sets, input mode, optional `BoardForm` / `QuickAddState`, palette,
help, save-recovery presentation, text selection, ephemeral message +
delete-recovery notice. The focused surface derives from the stage (`FullBoard` and
`Split` are board-owned, `Rail` and `FullTask` task-owned). The stage is not
persisted; the board always opens in `FullBoard`.

`BoardFormBinding` is `Task(id)` XOR `Capture(snapshot)` for the form's lifetime.
Quick-add expansion (`Tab`) stashes title·notes·scope so Esc returns to the line
and a second Tab restores what was typed.

`sync_from_domain` replaces the task vec and reanchors via `ui::selection::reanchor`
(keep id if still visible; else nearest in the *previous* visible order, preceding
on ties). Home tab does not follow foreign creates unless the view was empty.

### Intents and reducer

`BoardIntent` is the single vocabulary from keyboard, mouse, and palette
(`resolve_board_command`). `apply_intent` mutates domain only through
`DomainState`. `board_intent_may_persist` is the one list of mutating actions;
chrome-row lifetime (message vs delete-recovery notice) reads that same list.

Chrome row: notice first, message second, legend last. Mutating actions clear
both, then the new outcome owns the row; a *refused* mutating action puts the
delete notice back (undo was not spent). Command surfaces cover the notice while
open without deleting it.

### Keys

`map_key(mode, key)` remains the source for every surface key table.
`route_responsive_key` adds only the stage slider: at 110 usable columns or wider,
bare `→` / `←` in Normal or TaskPage view mode emit `StageRight` / `StageLeft`
(0 → A → G → F and back; inert at the ends), `Enter` opens the full page and records
the origin stage, `Esc` (`CloseLayer`) from F returns there and from G parks the page
beside the board (A). `Tab` is never a stage key. Active editors continue through
their existing maps; below 110 columns every route is unchanged. Mutating letters
require the fixed Ctrl modifier.
`1`/`2`/`3` select home tabs only in `Normal` at home without ctrl/alt/super.
`map_board_form_key` shares `map_form_edit_key` with capture. Task-page view mode
(`TaskPage`) keeps the board keymap so bare `e` enters edit rather than inserting
into a hidden draft.

### Mouse

Below 110 columns, row click peeks; the same row again closes peek; fast
double-click opens the page. At wide widths a board or rail row click selects in
place (`FocusBoardAndSelectIndex`: stage unchanged, no peek; the pane or page
retargets), a fast double-click opens F with the origin recorded, and any stage A
press inside the task column slides to G first (`wide_mouse_focus_intent`) and then
dispatches against the painted frame's hits, so the control keeps its identity across
the column resize. The shared footer routes to the focused surface's verbs in every
stage. `map_responsive_board_mouse` routes only translated renderer-owned hits inside
the live surface. Scrollbar
track/thumb: `ListScrollTo` / `PageScrollTo` without changing selection. Thread
headers are not in the hit map. Form fields ignore clicks until an edit has
started. Text drag uses `text_select` + autoscroll
(`DEFAULT_BASE_TICK` vs short tick). Copy is OSC 52, capped at 100_000 chars.

### Paint

`ui::tier::resolve`: **standard** when width ≥ 78 **and** height ≥ 24; else
**compact**. `resolve_responsive(width, height, stage)` adds the inclusive 110-column
stage slider: `FullBoard` and `FullTask` use the whole frame; `Split` is board
`floor(w * 0.4)` · rule 1 · task remainder; `Rail` is rail 32 · rule 1 · task
remainder. The task rect's first cell is a pad (`task_content`). Density follows the
frame, so a split at 130×24 keeps the standard rhythm and the stage A board keeps its
meta column. No box, no border, no colour: `assert_buffer_mono` holds at every width.
Below 110, `FullBoard`/`Split` render `SingleBoard` and `Rail`/`FullTask` render
`SingleTask`, keeping the stage. Geometry is defined down to 1×1 without panic;
product floor is 40×10. Standard reserves 28 cells of trailing meta; compact is glyph +
title. Verb-bar budgets: 7 standard, 5 compact. Project chip max 24 cells.

Wide chrome (`draw_wide_board`): both columns share the frame's row rhythm (blank row,
selector row, viewport) through `tier::resolve_column`, a dim `│` rule column sits
between them, and `render::draw_queue_footer` paints exactly one rule, status row and
verb bar across the frame. The status row's right side carries a dim stage crumb and
the keys that apply (`→ pane · enter open`, `board ▸ task    → task · ← close · enter
open`, `board ◂ task    ← board · → full page`, `← rail · esc back`, or `shift+enter
save · esc cancel` while an editor is active); refusal text on the left wins the row,
dropping the crumb first. `render::draw_task_column` paints the task header rule on
the selector row (`T12 title ──── started · tsk`, DIM in A, BOLD in G/F; the state slot
reads `editing <field>` or `unsaved`) and the page body under it with its in-page
header, divider and bottom chrome suppressed; the footer meta is `created … · updated
…` (plus the thread, and the scope while an edit session is active). With no selected
task the column paints `no task ───` and a dim hint and exposes no hits.
`render::draw_rail_frame` is the stage G rail: the list at 32 columns, no meta column,
no done drawer, rows `  <mark> T<n> <title>` wrapped with `edit::wrap_text` and a
four-cell continuation indent, `▹` on the selected row, every cell DIM.

`draw_queue_frame` paints selector, list, rule, status, verb bar, overlays
(palette, help, save recovery, walkthrough, bottom input slot). `present_line`
ellipsis is chrome. Task text wrapping is `edit::wrap_text`.

Notes markdown: [invariants](invariants.md) §27. Peek runs the same painter then
`dim_line`. `paint_task_row` uses `status_glyph`: ready `○`, started `▸`, blocked
`■`, review `▲`, done `✓`.

Surface content uses `MONO_MODIFIERS` only, wide chrome included; there is no colour
anywhere. `assert_buffer_mono` / `strip_color` cover every surface at every width.

### Selection reanchor

```
reanchor(previous, previous_visible, new_visible) -> Option<Uuid>
```

Empty new → `None`. Previous still visible → keep (even if reordered). Else scan
outward from the old index. `previous == None` returns `None` (model seeds on
open).

## Invariants

[Invariants](invariants.md) §§18–28, plus ADR-0002/0003. Selection never sits on a
collapsed group or a header. Quick-add refusals live on the line and clear on
close. Success create has no status message.

## Error paths

Domain refusals → chrome message (or the quick-add line). Persist failures →
save-recovery mode (app). Unpresentable walkthrough → `WalkthroughOutcome::Unpresentable`,
no dismissal write.

## Extension points

New section membership: `ui/queue.rs` first, then renderer row accounting (headers
already consume budget). New keys: `BoardIntent` + mapper + reducer; if it
persists, add it to `board_intent_may_persist`. Keymap/status/tab changes also
need `site/src/content/docs/docs/{keys,board,capture,cli}.md` and
`site/public/board-demo.js`. The web demo uses **bare** verb keys on purpose
(browsers steal control chords); do not “fix” it to match the TUI.

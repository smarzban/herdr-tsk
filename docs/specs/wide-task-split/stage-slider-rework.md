# Wide task split: stage-slider rework

Supersedes the boxed 50/50 design in `wide-task-split.md` for everything about geometry,
chrome, focus signalling, and key/mouse routing. The dirty-edit protection (AC-14 to AC-17),
resize continuity (AC-12, 13, 15, 19, 20) and "no domain mutation from layout" rules stay as
written there.

Branch: `feat/wide-task-split`, this worktree. One commit per task below, conventional prefix
(`feat:` / `fix:` / `test:` / `docs:`). Do not push. `HANDOFF.md` is out of scope; do not create one.

## Why

The merged-candidate design boxes the task page beside an unboxed board, paints two footers
(two rules, two `n done`, two verb bars), repeats the task number three times, splits 50/50, and
signals focus only through a coloured border (the one colour exception in a mono UI). Owner
rejected it on UX/UI. This document is the replacement.

## Model

Wide (usable width ≥ `WIDE_SPLIT_MIN_WIDTH` = 110) is a four-stage slider. Focus and geometry
are the same thing: the board owns focus in stages 0 and A, the task owns it in G and F.

| Stage | Left | Right | Focus |
| --- | --- | --- | --- |
| 0 `FullBoard` | board, full width | none | board |
| A `Split` | board, `floor(w * 0.4)` cols | task page, remainder | board |
| G `Rail` | rail, 32 cols, dim | task page, remainder | task |
| F `FullTask` | none | task page, full width | task |

Between left and right there is exactly one rule column painting `│`, plus one pad column on the
task side. No box, no border ring, no colour anywhere: `assert_buffer_mono` holds at every width.

Below 110 nothing changes from today: `→` peeks, `Enter` opens the task page, the focused surface
fills the frame. Stage is session state on `BoardModel` only, never persisted. The board always
opens in stage 0.

### Keys (Normal / TaskPage modes, no modifiers; edit modes keep editor semantics)

| Stage | `→` | `←` | `Enter` | `Esc` | `j` / `k` |
| --- | --- | --- | --- | --- | --- |
| 0 | → A | nothing | → F | nothing | select |
| A | → G | → 0 | → F | nothing | select, retarget pane |
| G | → F | → A | task-page verb | → A | task-page nav |
| F | nothing | → G | task-page verb | → return stage | task-page nav |

- `Enter` from 0, A or G records the origin stage; `Esc` from F returns there. `←` from F always
  goes to G. Origin memory clears when F is left.
- `Tab` is not a stage key. It keeps its single meaning (task-page edit mode, quick-add expand).
- `→` never peeks at ≥ 110. `PeekDetail` stays suppressed in wide.
- With no selected task, `→` and `Enter` in stage 0 do nothing, and stages A/G/F cannot be entered
  by keyboard. If the selection disappears while in A (e.g. merge from disk), stay in A and paint
  the empty pane.

### Mouse

- Stage 0 / A: single row click selects (and retargets the pane in A), stage unchanged.
- Stage A: any click inside the task column moves to G first, then dispatches the click to the
  task surface (existing focus-before-dispatch rule, AC-11).
- Stage G: a click on the left side takes focus left. A rail row click selects that row and
  lands the board in A; blank rail space, tabs, and section headers slide the same way. A
  retarget refused by a dirty draft keeps the stage (same feedback as the keyboard refusal).
- Any stage: fast double-click on a board/rail row opens F with that task; origin stage recorded.
- Task-page controls in G and F behave exactly as the single-pane task page (AC-9).

### Resize

- Shrink below 110: stages 0 and A render `SingleBoard` (selection, list scroll preserved); G and
  F render `SingleTask` (page scroll, edit session preserved). The stage value is kept.
- Grow back to ≥ 110: the kept stage renders again with the same focus and session.

### Dirty drafts

Existing semantics stay. Additionally:

- `←` from G to A is allowed (no task switch; the pane stays bound to the drafted task).
- `j`/`k` and rail/row clicks that would retarget are refused as today (AC-16/17).
- `→` from A back to G returns to the draft untouched.
- The task header state slot paints `unsaved` while a dirty draft exists and no editor is active.

## Chrome

Both columns share the existing `TierGeometry` row rhythm: row 0 blank, row 1 selector row,
viewport, then one full-width bottom rule, one status row, one verb bar. There is exactly one
footer in wide, spanning the whole frame.

### Task column header (replaces the in-pane `▸ T12 title … status` row and its rule)

Two rows in the board's own idiom: the title row on the selector row of the task column, and a
full-width dash rule on the row under it.

```
 ▸ T12 Frame the wide task view                               started · tsk
 ────────────────────────────────────────────────────────────────────────────
```

- Title row: the task's status glyph (`▸`, `○`, …), `T<number> <title>`, capped like the existing
  task-page header (chrome limit, not task text). Drafts without a number omit the prefix.
- Right of the title row (state slot, always kept): `status · project` normally; `editing <field>`
  while an editor is active; `unsaved` while a dirty draft exists and no editor is active.
- Weight: DIM in stage A (preview), BOLD in G and F. `T<number>` keeps its copy hit region.
- No selected task: `no task` over the same dash rule, then a dim body line
  `  select a task to preview it here`. The pane is inert (no hits).

Task body starts on the row after the rule. The task page's own bottom chrome (rule, done
count, verb bar) is not painted inside the column; the shared footer owns it. Task footer meta
becomes `created … · updated …` (project moves up to the header slot).

### Rail (stage G)

32 columns including the leading space, everything DIM:

```
 desk  ·  projects  ·  threads  │
                                │
 IN MOTION ───────────────────1 │
                                │
  ▹ T12 Frame the wide task     │
    view                        │
                                │
 desk ────────────────────────1 │
                                │
  ○ T15 Renew domain            │
```

- Keeps blank row, tab selector, section rules with counts, thread headers, collapse state.
- Drops the meta column (project · age) and the done drawer.
- Rows are `  <mark> T<n> <title>` wrapped with `edit::wrap_text` to the rail width, continuation
  indent 4. Never truncated.
- Selected row marker is `▹` (hollow). Other rows keep their status glyph.

### Board (stage A)

Today's board renderer at `floor(w * 0.4)` columns, nothing else changes. Selected marker `▸`.

### Shared footer

```
──────────────────────────────────────────────────────────────────────────────────────────────
 0 done                                                                    board ▸ task    → task · enter open
 enter open · ctrl+d done · ctrl+b block · : palette · ? help · + capture
```

- Status row left: today's content (`n done`, refusals, status messages).
- Status row right (dim, wide only): crumb + the stage keys that apply right now.
  Stage 0: `→ pane · enter open`. A: `board ▸ task    → task · ← close · enter open`.
  G: `board ◂ task    ← board · → full page`. F: `← rail · esc back`.
  When an editor is active: `shift+enter save · esc cancel` replaces the keys.
  Refusal text on the left wins the row if they collide; drop the crumb first, then the keys.
- Verb bar follows focus: board verbs in 0/A, task-page verbs in G/F, editor verbs while editing.

### Reference frames (130×24, fixture: T12 started with markdown notes, T15 ready)

Stage A, board focus:

```
 desk  ·  projects  ·  threads                      │  ▸ T12 Frame the wide task view                               started · tsk
                                                    │ ────────────────────────────────────────────────────────────────────────────
 IN MOTION ───────────────────────────────────────1 │   Rework the wide split so the task page reads as a detail pane,
                                                    │   not a boxed clone.
  ▸ T12 Frame the wide                     tsk · 0s │
        task view                                   │   - keep board unboxed
                                                    │   - decide separator
 desk ────────────────────────────────────────────1 │   - verb bar ownership
                                                    │
  ○ T15 Renew domain                             0s │   ```
                                                    │   resolve_responsive(w, h, focus)
                                                    │   ```
                                                    │
                                                    │
                                                    │   steps 0/0
                                                    │    + step
                                                    │
                                                    │
                                                    │
                                                    │   created 0s ago · updated 0s ago
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
 0 done                                                                             board ▸ task    → task · ← close · enter open
 enter open · ctrl+d done · ctrl+b block · : palette · ? help · + capture
```

Stage G, task focus (rail dim, header bold):

```
 desk  ·  projects  ·  threads  │  ▸ T12 Frame the wide task view                                                   started · tsk
                                │ ────────────────────────────────────────────────────────────────────────────────────────────────
 IN MOTION ───────────────────1 │   Rework the wide split so the task page reads as a detail pane, not a boxed clone.
                                │
  ▹ T12 Frame the wide task     │   - keep board unboxed
    view                        │   - decide separator
                                │   - verb bar ownership
 desk ────────────────────────1 │
                                │   ```
  ○ T15 Renew domain            │   resolve_responsive(w, h, focus)
                                │   ```
                                │
                                │
                                │   steps 0/0
                                │    + step
                                │
                                │
                                │
                                │   created 0s ago · updated 0s ago
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
 0 done                                                                                     board ◂ task    ← board · → full page
 ctrl+e edit · ctrl+d done · ctrl+b block · esc close
```

## Tasks

Test-first: write the failing test, watch it fail, then implement. Run
`cargo test --test wide_task_split` plus any touched suites per task; run the full green bar at
the end of T3 and T6.

### T1 geometry (`src/ui/tier.rs`)

- Replace `FocusedSurface`-driven `resolve_responsive` with a `WideStage { FullBoard, Split, Rail,
  FullTask }` input. Output: `board`, `rule` (1-col Rect, may be empty), `task` rects, and
  `density` (Tier of the narrower content, as today). Remove `inset_panel`; task content is the
  task rect minus the 1-col left pad. `FocusedSurface` may remain as a derived accessor.
- Split: board `floor(w*0.4)`, rule 1, task remainder. Rail: 32, rule 1, task remainder.
  0 and F: single full-frame rect, no rule. Below 110: stage 0/A → SingleBoard, G/F → SingleTask.
- Tests: exact rects at 110 and 130 for each stage; sum of widths == w; bounded for all
  110..=250 × 10..=60; below-110 mapping.

### T2 chrome (`src/ui/board/draw.rs`, `src/ui/render.rs`, `src/ui/board/chrome.rs`)

- Paint the unboxed columns, the rule column, the task header rule with state slot and weight,
  the rail renderer, the shared footer with crumb, and the empty-pane state. Remove the border
  ring and every `Color::Cyan` / `Color::DarkGray` use. Task page renderer must be able to
  suppress its own header row and bottom chrome.
- Tests (rewrite in `tests/wide_task_split.rs`): `assert_buffer_mono` at all widths; exactly one
  footer rule row; header text and weight per stage; rail wraps a long title over two rows with
  indent 4 and never truncates; rail has no meta column; crumb text per stage; empty pane inert.
  Regenerate goldens only if the narrow board changed (it should not).

### T3 stage routing (`src/ui/input.rs`, `src/ui/board/apply.rs`, `src/ui/board/model.rs`, `src/app.rs`)

- Intents: `StageRight`, `StageLeft`, `OpenTaskPage` records origin, `CloseLayer`/Esc from F
  restores it. Remove `Tab` from any stage path. Wire the key table above in
  `map_responsive_key`. Keep below-110 routes byte-identical to today (existing tests pin them).
- Resize: stage value survives threshold crossings; focused surface derives from stage.
- Tests: full key table per stage (one test per stage row); Enter/Esc origin memory from 0, A, G;
  `←` from F goes to G; Tab from A does not change stage; no-selection guards; shrink/grow
  continuity for each stage; repeated threshold crossings keep the loop live; no domain mutation
  from stage changes (reuse the existing AC-19/20 tests, adapted).

### T4 mouse (`src/ui/mouse.rs`, `src/app.rs`)

- Stage A task-column click → G then dispatch. Rail row click retargets in place. Double-click
  → F with origin. Board row click in 0/A selects without changing stage.
- Tests: each route; hit maps stay inside their column; G/F task-surface outcomes equal the
  single-pane task page (adapt `focused_wide_task_surface_matches_single_pane_keyboard_and_mouse_outcomes`).

### T5 dirty drafts across stages

- `←` G→A allowed with binding kept; retarget refusals in A and on rail clicks; `unsaved` header
  slot; `→` A→G returns to the draft; refusal clears after save/cancel.
- Tests: adapt the five existing dirty tests to stages and add the header-slot assertion.

### T6 docs, site, demo

- `AGENTS.md` Board section: replace the boxed/colour paragraph with the stage model; delete the
  "sole colour exception" sentence. `docs/technical/{board-ui,app,invariants}.md`.
  `site/src/content/docs/docs/{keys,board,task-page}.md`, `site/public/board-demo.js` (bare keys
  there, as the site rule says), `site/test/site.test.mjs` if it asserts on the old chrome.
- Update `wide-task-split.md` acceptance list: AC-3 (touching allocations) becomes "one rule
  column, no box"; add AC-21 stage table, AC-22 single footer, AC-23 mono at all widths,
  AC-24 rail wraps. Refresh `verification-report.md` with the new test names.

## Acceptance checks from the first attempt (all must hold)

A previous implementation of T2 to T6 was rejected and reset. It failed on every point below;
each one needs a test that fails without the fix.

1. Stage 0 at 130×24 (and F) renders the standard tier, byte-identical to the pre-rework board at
   that size: blank rows between sections present. Density must not drop to compact on a
   full-width surface.
2. Exactly one verb bar and one status row in wide. Nothing paints over them. `ctrl+` prefixes
   are kept. Both rows keep their one-column leading space like the narrow board.
3. The in-pane `▸ T<n> title … status` header row and its `───` rule are not painted in A, G or F.
   The header rule (`T12 title ──── started · tsk`) sits on the selector row of the task column,
   DIM in A and BOLD in G and F, with the state slot present in every case.
4. Stage A board keeps its meta column (`tsk · 0s`) exactly as the narrow board paints it.
5. Rail rows wrap with `edit::wrap_text` at rail width with continuation indent 4; a title longer
   than the rail occupies two rows and no glyph touches the rule column. The selected rail row
   paints `▹`. Every rail cell carries the DIM modifier.
6. `tests/wide_task_split.rs` covers every test listed under T1 to T5. A test named for hits
   inspects the hit map. Aim for coverage comparable to the ~40 tests the file had before the
   rework, not 6.
7. All `src/app.rs` unit tests are green. The three that pinned the focused-surface model
   (`app_keyboard_route_owns_responsive_focus_handoffs`,
   `app_mouse_click_focuses_task_before_dispatching_same_control`,
   `app_task_scrollbar_focuses_refreshes_bound_and_routes_page_scroll`) are rewritten for stages,
   not deleted.
8. Full green bar passes before the report.

Useful self-check: add a temporary ignored test that dumps the buffer as text for each stage at
130×24 and 110×24 and read it against the reference frames. Delete it before the final commit.

## Out of scope

Persisted stage preference, `alt+↑/↓` task switching in G, three-column tiers, any change to
narrow (< 110) behaviour, store or domain changes.

## Report back

Changed files per task, test names added/rewritten with pass counts, full green bar result
(`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`),
and state plainly that live herdr smoke was not run (the owner runs it).

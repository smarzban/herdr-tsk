# Architecture

tsk is a local-first terminal task board. One process paints a queue, another can mutate
the same store from the CLI or a capture overlay. There is no server, no shared database,
and no agent-driven status. Human status is the only progress authority.

This page is the system map. Subsystem pages hold mechanism. [Invariants](invariants.md)
holds the rules a change must not break.

## What this tree is (and is not)

This crate (`tsk-tui`) is the v1 board: capture, human-status verbs, steps, threads, and
headless `add` / `list` / `steps`. It ships as the herdr plugin `herdr-tsk` and as a
standalone `tsk` binary against `~/.tsk`.

Park, resume, attention, linking, and dispatch *execution* are not in this tree. Their
older engines live only on `archive/dark-engine-pre-v1`. Domain types for capsules,
agent meta, observed status, and dispatch attempts remain on the store document so older
files still load. The board does not apply observations to human status and does not
start host dispatch.

## Components

```
argv / TSK_MODE / --find-board-pane
        │
        ▼
 src/main.rs  ── route() ──► Surface { Board, Capture, Add, Steps, List, FindBoardPane, … }
        │
        ├── Board / Capture ──► tsk_tui::run ──► app::{run_board, run_capture}
        │                           │
        │                           ├── TaskStore  (tsk.json under TSK_STATE_DIR | ~/.tsk)
        │                           ├── SettingsRecord / WalkthroughRecord  (~/.tsk)
        │                           ├── InvocationSnapshot  (HERDR_PLUGIN_CONTEXT_JSON)
        │                           ├── DomainState  (in memory)
        │                           └── BoardModel / CaptureModel  (session only)
        │
        ├── add / list / steps ──► cli::run_with ──► store lock → domain → presenter
        └── --find-board-pane ──► board_pane::find_board_pane_from_stdin
```

| Layer | Owns | Does not own |
| --- | --- | --- |
| `src/main.rs` | Process routing, exit codes, stdout/stderr | Domain rules |
| [`cli`](cli.md) | Headless parse / execute / present | TUI paint |
| [`app`](app.md) | TUI loops, save recovery, idle store watch | Queue derivation |
| [`domain`](domain.md) | Task lifecycle, revisions, history, undo | Filesystem |
| [`store`](store.md) | Lock, load, atomic replace, format check | Status verbs |
| [`ui/board`](board-ui.md) | Session model, intents, paint | Persistence |
| [`ui/queue`](board-ui.md#queue-query) | Pure section derivation from a task snapshot | Selection, mouse |
| [`context`](context.md) / [`scope`](context.md) | Invocation default and project tokens | Creating tasks |
| [`capture`](capture.md) | Form/CLI create path into domain | Board chrome |
| [`config`](config.md) | Verb modifier, walkthrough dismissal | Task document |
| [`plugin`](plugin.md) | herdr pane + actions | Store location (deliberately ignored) |
| [`site`](site.md) | Marketing + operator docs | Binary behavior |

Chrome lives under `src/ui/` (`board/` model · apply · commands · chrome · draw, plus
`capture`, `mouse`, `input`, `render`). That split is load-bearing: the reducer mutates
domain + session; the query is a pure function of tasks; paint consumes both.

## Main flows

### Open the board

1. `cli::router::route` sees no subcommand and no `--find-board-pane` / `--help` → `Surface::Board`.
2. `app::run` → `run_board`: `TaskStore::load`, `InvocationSnapshot` from env + cwd, `BoardModel::from_domain`.
3. Settings load the verb modifier (`alt` default). `run_board` does **not** auto-open the walkthrough; `open_walkthrough_for_launch` remains for tests, and the palette can replay it.
4. Frame loop is **settle, paint, wait** (`app::board_frame`). Idle ticks cheaply `stat` `tsk.json` and merge if it changed (`StoreWatch` + `merge_tasks_from_disk`).

### Mutating verb

1. Key or mouse becomes a `BoardIntent` (`ui::input` / `ui::mouse`). Mutating letters need the configured verb modifier.
2. `apply_board_intent_with_save_recovery` loads a persist baseline *before* the reducer when `board_intent_may_persist` is true.
3. `apply_intent` calls `DomainState` only. Outcome `Persist` means the caller writes.
4. `TaskStore::reload_merge_save` takes the exclusive lock, revision-guards the local mutation against disk, then atomic-replaces. Failure enters [`SaveRecovery`](app.md#save-recovery); the form and input mode stay allocated until Retry or Cancel.

### Capture

- Board `+` is a one-line quick-add on the status-row slot, not a form takeover.
  After token parse it creates through `capture_save` (`store: None`); the board
  loop persists.
- `Tab` expands that draft onto the task page (Notes edit, because a draft has nothing to view).
- herdr **Quick capture** launches the same binary with `TSK_MODE=capture` (`scripts/open-capture.sh`) into `AppMode::Capture`.
- All three create through `DomainState::create` / `create_with_thread`. Empty titles never persist.

### Headless add / list / steps

`main` routes those surfaces to `cli::run_with`. Mutations use `TaskStore::locked_transition_if_changed` so a refused command does not rewrite the file. `list` is read-only.

## Constraints that shaped the design

1. **One store everywhere.** herdr injects `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR`. tsk ignores them. Pane, overlay, and CLI all read `TSK_STATE_DIR` else `~/.tsk`. Two boards would otherwise silently diverge.
2. **Human status is truth.** Capsule, agent meta, and `last_observed` are retained data. Completing every step never completes the task.
3. **Local, concurrent writers.** Board + capture overlay + `tsk add` share one JSON file. The lock plus per-task revision (not wall clock) is the concurrency model. Same-task concurrent edits refuse rather than last-write-win.
4. **Terminal as a hostile display.** Task titles and notes are untrusted. C0/C1 never reach the emulator as controls (`ui::terminal_text`). Text wraps; it does not truncate (chrome ellipsis is a different budget).
5. **Operable in a herdr split.** Standard layout at ≥78×24 (typical split width), compact below, no panic to 40×10. Sections are computed, never navigated.
6. **Mono modifiers only.** Bold, dim, underline, reverse. No color theme module. Markdown on notes is a styled subset of those modifiers.
7. **Mutating keys are chording.** Bare letters do nothing on the board so a focused pane cannot complete or delete work. Nav, peek, `Enter`, `P`, `1`/`2`/`3`, `z`, `:`, `?`, `+`, `Esc` stay bare.
8. **v1 cuts fold of dispatch.** Dispatch-attempt types persist for serde compatibility; the board has no host attention poll, park/resume, linking, or dispatch recovery.

## Alternatives considered

Recorded as ADRs; do not restate them here:

- [ADR-0001](../specs/adr/0001-checklist-structured-field.md) — steps are a structured field, not a notes checkbox convention.
- [ADR-0002](../specs/adr/0002-thread-tag-not-parent-task.md) — a thread is a tag on the task, not a parent task.
- [ADR-0003](../specs/adr/0003-thread-headers-are-chrome-not-rows.md) — thread headers are renderer chrome, not selectable queue rows.

Other standing rejections (from code comments and `AGENTS.md`, not a numbered ADR):

- SQLite for search: in-memory filter over `DomainState` until open/save is felt-slow or the file is regularly above ~10 MB.
- Auto-complete from agent observation: forbidden.
- Renaming `TaskScope::Global` or the on-disk `"global"` to `"desk"`: display-only. A rename is a store migration.
- Shared `/tmp` fallback for a missing `HOME`: refused; last resort is a *relative* `.tsk-state` / `.tsk-config` in the process cwd.

## Where a change belongs

| Change | First look |
| --- | --- |
| Status verb, create, edit, steps, undo | `src/domain/` then the board reducer or CLI |
| Persist, lock, format version, backup | `src/store.rs` |
| Section membership, thread blocks, tab lenses | `src/ui/queue.rs` (pure) then `src/ui/render.rs` |
| Key chord, mouse hit, help line | `src/ui/input.rs`, `src/ui/mouse.rs` |
| Frame loop, idle merge, save recovery | `src/app.rs`, `src/save_recovery.rs` |
| `tsk add` / `list` / `steps` flags or JSON | `src/cli/` |
| Invocation default, `!p` / `--project` | `src/context.rs`, `src/scope.rs` |
| Verb modifier, walkthrough | `src/config.rs` |
| herdr pane open/focus | `herdr-plugin.toml`, `scripts/`, `src/board_pane.rs` |
| Operator-facing keys/board/cli text | `site/src/content/docs/docs/` *and* `site/public/board-demo.js` |

# Symbol reference

## Strategy

**Generated rustdoc for every `pub` item. Hand-written pages for why, invariants, and
load-bearing privates.**

The crate is an unpublished library (`publish = false`) plus a binary. Integration
tests and `src/main.rs` consume `pub` modules. There are hundreds of public items
(types, functions, consts). Hand-transcribing signatures would drift; rustdoc will not
write architecture.

Public-symbol rows in [coverage.md](coverage.md) close as **generated (rustdoc)**.

### Generate

From the repo root, with the pinned toolchain (`rust-toolchain.toml` → 1.96.0):

```bash
cargo doc --no-deps --package tsk-tui
```

Output: `target/doc/tsk_tui/index.html` (gitignored). Open with
`cargo doc --no-deps --package tsk-tui --open`.

`--document-private-items` additionally emits `pub(crate)` / private items. The
load-bearing subset of those is listed below so a maintainer does not have to generate
privates to learn the rules.

This command is **not** on the green bar and **not** in CI. Adding it is an
owner-approved change, not a docs default.

Crate-level rustdoc lives on `src/lib.rs` (`//! tsk library root.`) and module `//!`
comments. Prefer deepening those comments when a public item's *signature* docs are
thin; do not fork a second hand-written API book.

## Load-bearing internals

Private or `pub(crate)` items that carry an invariant rustdoc-on-publics would miss.
Mechanism belongs on the subsystem page; this is the index.

| Item | Visibility | Why it is load-bearing | Page |
| --- | --- | --- | --- |
| `ui::terminal_text` | `pub(crate)` | C0/C1 never paint as controls | [security](security-model.md), [board UI](board-ui.md) |
| `ui::split_line_breaks` | `pub(crate)` | One definition of `\r\n` / `\n` / `\r` | [invariants](invariants.md) §26 |
| `ui::present_line` / `present_lines` | `pub(crate)` | Width-bounded paint + overflow marker | [board UI](board-ui.md) |
| `ui::edit::wrap_text` | `pub(crate)` | The one wrap engine (word-boundary, display cells) | [invariants](invariants.md) §25 |
| `ui::edit::EditBuffer` | `pub(crate)` | Cursor in Unicode scalars; shared Title/Notes/Thread/step drafts | [board UI](board-ui.md) |
| `ui::edit::flatten_line_breaks` | `pub(crate)` | Paste flattening uses the same break definition | [board UI](board-ui.md) |
| `ui::edit::escaped_draft_rows` | `pub(crate)` | Single-line editors only; do not reuse for wrapping notes | [invariants](invariants.md) §25 |
| `BoardForm` / `BoardFormBinding` | `pub(super)` | Task id XOR capture snapshot for the form's lifetime | [board UI](board-ui.md) |
| `board_keyboard_intent` form allowlist | private in `app` | Save-recovery `r`/`c`/Esc must outrank an allocated form | [app](app.md) |
| `DomainState::merge_for_save` | `pub(crate)` | Revision guard under the store lock | [domain](domain.md), [store](store.md) |
| `LEGACY_MERGE_BASE_REVISION` | private | Nil UUID base for pre-revision tasks | [invariants](invariants.md) §6 |
| `DomainState::stamp_format_version` / `clear_merge_bases` | `pub(crate)` | Must run on every successful replace | [store](store.md) |
| `time_serde` | private module | Wire times as `[secs, nanos]` | [data model](data-model.md) |
| `text::non_empty` | `pub(crate)` | Trim + reject empty host strings without allocating | [context](context.md) |
| `StoreWatch::{poll,record}` | private | Failed idle load must not mark the watch caught up | [app](app.md) |
| `config::resolve_config_dir` | private | Empty `TSK_CONFIG_DIR` must not resolve to `""` | [config](config.md) |
| `AtomicFilesystem` | private trait | Lets tests fail a write stage without weakening production | [store](store.md) |

### `wrap_text`

```text
ui::edit::wrap_text(value: &str, width: usize) -> Vec<WrappedRow>
```

Splits on `split_line_breaks` first, then wraps each logical line to at most `width`
**display cells** (ratatui `Line::width`), preferring the last whitespace on the row.
A run with no whitespace (or a single character wider than the allocation) hard-breaks
at the cell edge. `width == 0` yields no rows. This is the engine named in
`AGENTS.md`; list rows, notes, peek, capture Notes, and task-page titles all go through
it.

### Form binding

`BoardFormBinding::Task(Uuid)` and `BoardFormBinding::Capture(Box<Option<InvocationSnapshot>>)`
are mutually exclusive by construction. A capture form cannot acquire a selected-task
id; a task form cannot reload invocation snapshot under its drafts. Clearing the form
before `reload_merge_save` returns is the failure mode invariant 19 exists to prevent.

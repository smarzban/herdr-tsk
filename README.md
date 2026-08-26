# tsk

**A task board for your terminal.**

tsk captures work and moves it through human status: ready, started, blocked,
review, done. It ships today as a [herdr](https://herdr.dev) plugin, and the
`tsk` binary also runs standalone against its own state directory.

Park, resume, attention, linking, and dispatch-start are not board actions.
Dispatch recovery can still open if a persisted attempt is already in the store.

## Requirements

- Rust 1.96.0 (pinned in `rust-toolchain.toml`)
- Linux or macOS
- herdr 0.7.5 or newer (plugin mode only)

## Install

Build from source:

```bash
git clone git@github.com:smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
```

The built binary is `target/release/tsk`.

### Standalone

Run `tsk` directly. The store is `~/.tsk`: `tsk.json` and `settings.json`
live side by side. Override the locations with `TSK_STATE_DIR` /
`TSK_CONFIG_DIR`.

```bash
tsk add -t "Draft release notes"
tsk list
```

### As a herdr plugin

The plugin pane runs `./target/release/tsk`, so build before you link:

```bash
herdr plugin link "$PWD"
```

`herdr plugin list` should show `herdr-tsk` enabled against that path. Rebuild
after you pull.

## Open the board

From herdr, pick **Open tsk board**, or:

```bash
herdr plugin action invoke open-board --plugin herdr-tsk
```

It opens a **tsk** split beside the current pane. Invoking it again focuses the
board you already have.

**Quick capture** opens the capture form without the board:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

In plugin mode herdr injects `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR`, but
tsk deliberately ignores them: there is one store (`~/.tsk`) whether the board runs in
herdr, another multiplexer, or a bare terminal. Herdr documents plugin state as
plugin-owned and never touches its contents, so the pane and the CLI edit the same
board safely.

## Pane size

| Pane | What you get |
| --- | --- |
| at least 78×24 | standard board: section headers, row meta, full verb legend |
| smaller | compact: glyph and title only; help, palette, and the task page take the full pane |

The board stays operable down to 40×10. A typical herdr split is 78 columns, which
is the standard board.

## Sections

One urgency-ordered list. Sections are computed, not navigated.

- **IN MOTION**: work you have started
- **Home** (`P` → desk, or the default when you open the board): three tabs —
  **desk** (`1`), **projects** (`2`), **threads** (`3`). Desk shows global in-motion
  work and your desk ON DECK. Projects groups by repo path (collapsible; double-click
  a header to focus that project). Threads groups by thread name across projects.
- When scoped to one project: **ON DECK** for that project, with derived thread
  headers above their open tasks
- **z** opens the done drawer

## Keys

Mutating keys need **Alt** (or **Ctrl**, if you flip it in the palette). Bare
letters do nothing, so typing in a focused board cannot complete or delete work.

| Key | Does |
| --- | --- |
| `j` `k` or `↑` `↓` | move (the wheel does too) |
| `alt+space` | start the selected task, or reopen it if it is done |
| `alt+d` | done |
| `alt+o` | reopen |
| `alt+b` | toggle blocked |
| `Enter` | open the task page |
| `→` `←` | peek notes under the row (up to five lines) |
| `+` | capture |
| `alt+e` | edit title |
| `alt+x` or `alt+Delete` | delete (`alt+u` undoes) |
| `z` | done drawer |
| `:` | command palette |
| `?` | help |
| `P` | project scope (home or a project) |
| `1` `2` `3` | home tabs: desk · projects · threads |
| `Esc` | close the open surface |
| `alt+q` | quit |

Click a row to peek. Click it again to close. A fast double-click opens the page.

`Esc` closes one layer at a time. In the palette, `q` types into the query; leave
with `Esc`.

## The task page

`Enter` opens the selected task full height. It is view-first: nothing is in edit
mode until you ask. `alt+e` edits the title, `alt+n` edits notes, `Tab` moves
between fields. The scope footer does nothing until an edit has started.
`Ctrl+Enter` or `Alt+Enter` saves from any field. Field `Esc` cancels that field.
Page `Esc` closes the page.

## Capture and edit

| Key | Does |
| --- | --- |
| `Tab` / `Shift+Tab` | Title, Notes, Thread, Scope |
| `Enter` in Title | save |
| `Enter` in Notes | new line |
| `Ctrl+Enter` or `Alt+Enter` | save from any field |
| `Esc` | cancel |

Title is required. Scope defaults to the repo the board was opened from, or your
desk if there is no repo.

Notes are multiline, so `Enter` inserts a line. `Ctrl+Enter` saves when the
terminal reports it; `Alt+Enter` saves everywhere else. Both save. Neither inserts
a line.

## Delete and undo

`alt+x` removes the selected task. The status line offers undo:

```
Deleted "Draft the quickstart" · u Undo
```

`alt+u` restores it. Undo also reverses the last done. It is refused if the task
changed in between.

## Verify

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
```

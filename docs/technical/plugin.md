# Plugin / host

**Responsibility.** Ship tsk as herdr plugin `herdr-tsk`: manifest, pane identity,
idempotent open-or-focus, overlay capture. The plugin does **not** own the store
location.

**Public surface.** `herdr-plugin.toml`; `scripts/open-board.sh`,
`open-capture.sh`, `tsk-cli.sh`; `board_pane::{BOARD_PANE_LABEL,
find_board_pane_from_stdin, find_board_pane_id}`.

## How it works

Manifest (`herdr-plugin.toml`):

- `id` / `name`: `herdr-tsk`, version `0.4.0` (same as crate `tsk-tui`)
- `min_herdr_version`: `0.7.5`
- platforms: linux, macos
- build: `cargo build --release`
- pane: id `board`, title `tsk`, placement `split`, command
  `./target/release/tsk`
- actions: `open-board` → `scripts/open-board.sh`; `quick-capture` →
  `scripts/open-capture.sh`

Pane **title** `tsk` is the identity string. It must equal
`board_pane::BOARD_PANE_LABEL` (`"tsk"`). The painted board heading
`BOARD_TITLE` is `"Tasks"` — a different string. Matching on terminal title would
false-positive a standalone `tsk` in some other pane.

### Open-or-focus

`open-board.sh`:

1. `$HERDR_BIN_PATH` else `herdr`; board binary `$TSK_BIN` else
   `scripts/../target/release/tsk` (`open-capture.sh` does not read `TSK_BIN`).
2. `pane list` JSON piped to `tsk --find-board-pane`.
3. If a flag-safe id comes back, `plugin pane focus`; on failure (stale list),
   fall through so the keypress is never a silent no-op.
4. Else `plugin pane open --plugin herdr-tsk --entrypoint board --placement split --focus`.

`--find-board-pane` is Rust so the launcher does not depend on python3. Pane ids
must be non-empty, not start with `-`, and match `[A-Za-z0-9_.:-]+`.

### Quick capture

`open-capture.sh` opens the **same** `board` entrypoint as an **overlay** with
`--env TSK_MODE=capture`. Short-lived; board focus idempotency is not this
script's job.

### `tsk-cli.sh`

Looks up the enabled `herdr-tsk` `plugin_root` from `herdr plugin list --json`
(via python3) and execs `$plugin_root/target/release/tsk` with the remaining
argv. Missing binary → exit 127. Agents get the same `~/.tsk` as the pane with
no extra state plumbing.

### Context injection

herdr injects `HERDR_PLUGIN_CONTEXT_JSON` (used) and `HERDR_PLUGIN_STATE_DIR` /
`HERDR_PLUGIN_CONFIG_DIR` (ignored). One board everywhere: [store](store.md),
[context](context.md).

This tree has no host attention poll, no pane-link verbs, and no dispatch
recovery. `HostPorts` / dispatch modules in stale `target/doc/herdr_tasks/` are
from a previous crate name; they are not in `src/` now.

## Invariants

[Invariants](invariants.md) §34. Rebuild `target/release/tsk` before live smoke;
a running pane keeps the old binary until quit.

## Error paths

- No matching pane: `find_board_pane_id` → `None`; binary `--find-board-pane`
  exits 1 with no stdout. The shell script then opens a new pane.
- Unparseable pane-list JSON: `None` (not a panic).
- Unsafe pane ids skipped until a safe one, or none.
- `tsk-cli.sh` without an enabled plugin: python `SystemExit`.

## Extension points

New actions: a `[[actions]]` row plus a script. New panes: title must match a
label constant if `--find-board-pane` should see them. Do not parse pane-list
JSON in bash.

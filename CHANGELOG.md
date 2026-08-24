# Changelog

## Unreleased

The projectless scope is now **desk**: board header, `tsk list` labels, and the new
`--desk` flag (`--global` still works as an alias). Stored scope values are unchanged,
so no data migration is needed.

Store hardening: `tasks.json` carries `format_version` (currently 1) and a
newer document is refused rather than rewritten; each replace keeps the previous
file as `tasks.json.1`; leftover `.tasks.json.tmp.*` files are swept under the
lock; the exclusive lock uses `std::fs::File::lock` instead of `fs2`.

Rebrand to **tsk** ("a task board for your terminal"). The crate is now
`tsk-tui` building the `tsk` binary. The store unifies at `~/.tsk` (`tasks.json`
and `settings.json` side by side), overridable with `TSK_STATE_DIR` /
`TSK_CONFIG_DIR`; injected host variables like `HERDR_PLUGIN_STATE_DIR` are
ignored so the herdr pane and a bare terminal edit one board. There is no
automatic move from the old locations; a first run creates an empty `~/.tsk`.
The herdr plugin id stays `herdr-tasks`.

## 0.3.0

Tasks can carry an optional normalized thread. Headless add accepts `--thread`
and plan item `thread`; `herdr-tasks list --thread <name>` filters within the
selected scope, JSON rows always include `thread`, and human rows append
` #name` only for threaded tasks.

## 0.2.0

Per-task steps: a flat, ordered list on the task page with a shared notes and
steps scroll region, step cursor, and modifier-protected add, toggle, rename,
and delete verbs. Headless, `herdr-tasks steps <task-id> add <text>` creates
one step and `herdr-tasks steps <task-id> toggle <step-short-id>` flips one
step by unambiguous id prefix; single-task `herdr-tasks list <task-id>` prints
one line per step with its `[x]`/`[ ]` state and step short id. Step progress
never changes task status.

## 0.1.0

Queue board for capture, organization, and human-status verbs inside herdr.

Standard layout at 78×24, compact below. Human status is `ready`, `started`,
`blocked`, `review`, `done`. Mutating keys use Alt (Ctrl from the palette).
Peek, task page, project scope, done drawer, and undo are on the board.

Park, resume, linking, and dispatch-start are not board actions. Dispatch
recovery still opens if a persisted attempt is already in the store.

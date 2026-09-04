---
title: CLI
description: Headless tsk add, list, and steps. The agents' door to the board.
---

The same `~/.tsk` store backs the board, herdr, and these commands. The board
reads a change on its next tick.

```bash
tsk add -t "Draft release notes"
tsk list
```

## For agents

The CLI is how an agent reaches the board. The rules that matter:

- Put work on the board with `tsk add`, one task per call or a JSON plan through
  `--file`. A repeat of the same title, project, and thread returns the existing
  task, so a retried plan is safe.
- Plan with `tsk steps <task> add`. Read back with `tsk list <task>` before a
  toggle, because toggle is not idempotent.
- Prefer `--json` and read the exit code. Exit 1 means retry only the failed
  items. Exit 3 means list before retrying.
- Done is a human verb on the board. The CLI has no verb for it, and no status
  verb yet.

The repo ships the same rules as an agent skill in
[`skills/tsk-cli/SKILL.md`](https://github.com/smarzban/herdr-tsk/blob/main/skills/tsk-cli/SKILL.md).

## Commands

| Command | Does |
| --- | --- |
| `tsk` | opens the board |
| `tsk capture` | opens the capture form (also `TSK_MODE=capture`) |
| `tsk add` · `tsk list` · `tsk steps` | headless; below |
| `tsk --help` | usage, exit 0 |
| `tsk --find-board-pane` | herdr helper: reads `pane list` JSON on stdin, prints the id of the pane labelled `tsk`; exit 1 when none |

Every headless command takes `--state-dir <dir>` to work against another store.

## add

Create one task, or apply a JSON plan. Human output is `added <title>` or
`task already exists`.

```
tsk add -t <title> [-n <notes>] [-p <project> | --desk] [--thread <name>] [--json] [--state-dir <dir>]
tsk add [--file <path|->] [--state-dir <dir>]
```

`--desk` is your desk (stored as scope `global`). `-p` / `--project` uses the
same basename-or-path rules as capture `!p`. `--thread` normalizes like `!t`.

An add whose trimmed title, resolved project, and normalized thread already
exist succeeds without changing the task.

Values that start with `-` need `--title=…`, `--notes=…`, `--project=…`,
`--state-dir=…`, or `--file=…`. Item flags plus `--file` is usage (exit 2,
nothing persists). Piped stdin with item flags is ignored and not read.

Notes that trim to nothing are dropped.

`--json` on a flag add prints one object: `outcome` (`created` or `existing`),
`id`, `number`, `title`, and `project` (or `null`).

Plan JSON is an array:

```json
[{"title": "...", "notes": "...", "project": "...", "thread": "..."}]
```

`project` null means desk; a missing `project` means the invocation default (the
repo you ran from, or desk). `thread` may be `null` or missing. The result is
`{ "created": [...], "existing": [...], "failed": [...] }`, each item carrying its
index `i`; failed items add `code` and `error`. Notes are not echoed.

```bash
tsk add --file plan.json
cat plan.json | tsk add
```

## list

Prints tasks. It does not change them.

```
tsk list [<task>] [-p <project> | --desk | --all] [--thread <name>] [--done | --deleted] [--json] [--state-dir <dir>]
```

Default: ready, started, blocked, and review in the invocation project, or your
desk outside a repository.

- `--desk` selects desk
- `--all` every scope
- `--done` done only
- `--deleted` soft-deleted only
- `--thread` filters within the selected scope

Human output groups rows under `STARTED`, `READY`, `BLOCKED`, `REVIEW`, then
`DONE` or `DELETED` when asked, as `- <number> <title> #thread`. With `--all`, a
trailing scope label (`desk` or `project: <path>`) tells the groups apart.

A store-global task number (`T12`, `t12`, or bare `12`) or a task UUID lists
that one task and its steps (`[x]` / `[ ]` plus the step short id). Direct lookup
ignores cwd. A task operand cannot combine with scope, thread, or status
filters. An address that matches nothing is a usage error (exit 2).

`--json` is a flat array of `id`, `number`, `title`, `status`, `project`, and
`thread`. Single-task JSON also attaches `steps`.

To recover a typo scope: `tsk list --all --json`.

## steps

```
tsk steps <task> add <text> [--state-dir <dir>]
tsk steps <task> toggle <step-short-id> [--state-dir <dir>]
```

Output is `added <short-id> <text>` or `toggled <short-id> [x] <text>`.

See [steps](/docs/steps/) for the board side. `toggle` is not idempotent. Verify
with `tsk list <task>` before a retry.

## Exit contract

| Exit | Meaning |
| --- | --- |
| 0 | listed, or every add item created/existed, or the step applied |
| 1 | one or more item refusals (add/steps). Retry only the failed subset. For toggle, list first. |
| 2 | usage or parse error, including a `list` address that matches nothing. Nothing persisted. |
| 3 | store I/O. Commit is indeterminate. `tsk list` before retrying. |

Add refusal codes: `empty-title`, `invalid-title`, `invalid-thread`, `invalid-item`.
Any C0 control in a title or step text is `invalid-title` / `invalid-step-text`
before trimming.

Steps refusals (exit 1): `empty-step-text`, `invalid-step-text`, `unknown-task`,
`soft-deleted-task`, `unknown-step`, `ambiguous-step`.

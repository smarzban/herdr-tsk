---
title: CLI
description: Headless tsk add, list, and steps.
---

The same `~/.tsk` store backs the board, herdr, and these commands.

```bash
tsk add -t "Draft release notes"
tsk list
```

## add

Create one task, or apply a JSON plan.

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

`--json` on a flag add prints one object: `outcome` (`created` or `existing`),
`id`, `number`, `title`, and `project` (or `null`).

Plan JSON is an array:

```json
[{"title": "...", "notes": "...", "project": "...", "thread": "..."}]
```

`thread` may be `null`. The result is `{ "created": [...], "existing": [...], "failed": [...] }`.
Notes are not echoed.

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

A store-global task number (`T12`, `t12`, or bare `12`) or a task UUID lists
that one task and its steps (`[x]` / `[ ]` plus the step short id). Direct lookup
ignores cwd. A task operand cannot combine with scope, thread, or status
filters.

`--json` is a flat array of `id`, `number`, `title`, `status`, `project`, and
`thread`. Single-task JSON also attaches `steps`.

To recover a typo scope: `tsk list --all --json`.

## steps

```
tsk steps <task> add <text> [--state-dir <dir>]
tsk steps <task> toggle <step-short-id> [--state-dir <dir>]
```

See [steps](/docs/steps/) for the board side. `toggle` is not idempotent. Verify
with `tsk list <task>` before a retry.

## Exit contract

| Exit | Meaning |
| --- | --- |
| 0 | listed, or every add item created/existed, or the step applied |
| 1 | one or more item refusals (add/steps). Retry only the failed subset. For toggle, list first. |
| 2 | usage or parse error. Nothing persisted. |
| 3 | store I/O. Commit is indeterminate. `tsk list` before retrying. |

Add refusal codes include `empty-title`, `invalid-title`, `invalid-item`. Any C0
control in a title or step text is `invalid-title` / `invalid-step-text` before
trimming.

Steps refusals (exit 1): `empty-step-text`, `invalid-step-text`, `unknown-task`,
`soft-deleted-task`, `unknown-step`, `ambiguous-step`.

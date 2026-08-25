---
name: tsk-cli
description: Use when asked to add tasks, a task list, or a plan to the Tasks board, or to inspect Tasks board items. Use `tsk add` and `tsk list`, never the TUI.
---

# tsk CLI

Use `tsk add` to create a task or JSON plan, and `tsk list` to
inspect the shared board store before and after adding work.

## Adding

```sh
tsk add -t "Draft release notes"
tsk add -t "Buy milk" --desk
tsk add -t "Fix widget" --project widget --thread release-2026
tsk add --title="-fix parser" --notes="-5 degrees" --project="-maintenance"
tsk add --file plan.json
cat plan.json | tsk add
```

Use `--title=<value>`, `--notes=<value>`, `--project=<value>`,
`--state-dir=<dir>`, or `--file=<path>` when a value begins with `-`.
`--thread <name>` assigns the normalized thread name to a flag add. Item flags
plus `--file` are usage (exit 2, nothing persists). Piped stdin with item flags
is ignored and not read. Use `--json` with a flag add when another
tool needs one result object. It contains `outcome` (`created` or `existing`),
`id`, trimmed `title`, and resolved `project` (or `null` for the desk).

Plan JSON accepts a per-item `thread` string or `null`, for example
`[{"title":"Release notes","thread":"release-2026"}]`. Invalid plan thread
values are item refusals with code `invalid-thread`; valid sibling items still
persist and the command exits 1.

## Exit and retry contract

- exit 0: every item was created or already existed.
- exit 1: one or more plan items were refused, including `invalid-thread`.
  Valid siblings still persist. Retry only the `failed` subset. `created` and
  `existing` items both succeeded, so you must never whole-plan-retry an exit 1 run.
- exit 2: usage or parse error, nothing persisted. Correct the invocation, then run it.
- exit 3: store I/O, commit indeterminate. Run `tsk list --all --json` to
  check every scope, then retry only missing work. You must never whole-plan-retry an exit 3 run.

Add is idempotent by trimmed title and resolved project scope. A non-soft-deleted
matching task is a success: plain flag add prints `task already exists`, and plan add
reports it in `existing` with its `i`, `id`, and `title`.

## Listing scope and filters

`tsk list` defaults to ready, started, blocked, and review tasks in the
invocation project, or your desk outside a repository. Use `-p`/`--project <scope>`
for the same basename-or-path resolution as add, `--desk` for desk tasks, or `--all`
for every scope. `--thread <name>` normalizes then filters tasks after scope
selection. An invalid thread name is a usage error (exit 2), not an empty result.
JSON rows always include `thread`, with `null` for unthreaded tasks. Use
`--project=<scope>` or `--state-dir=<dir>` when either value begins with `-`,
`-p -maintenance` is usage. `--done` lists done tasks only; `--deleted` lists
soft-deleted tasks only, regardless of stored status. Scope selectors are mutually
exclusive, as are `--done` and `--deleted`.

A typo in a project name silently files the task under a new scope. Use
`tsk list --all --json` to recover the resulting scope.

## Steps

```sh
tsk steps <task-id> add "Draft outline"
tsk steps <task-id> toggle <step-short-id>
tsk list <task-id>
```

`<task-id>` is a task UUID from `tsk list --json`. A step short id is
the shortest unambiguous prefix of the step id. `tsk list <task-id>`
prints one line per step with its `[x]`/`[ ]` state and short id; with `--json`
the row's `steps` array carries each step's `id`, `text`, `done`, and
`short_id`.

`toggle` flips the step state: a retry after an unseen success flips it back.
Never blind-retry a `steps` invocation — run `tsk list <task-id>`
first and retry only a real refusal. `steps` exits 0 when the step was created
or toggled, 1 for a refusal (stable tokens `empty-step-text`,
`invalid-step-text`, `unknown-task`, `soft-deleted-task`, `unknown-step`,
`ambiguous-step`), 2 for a usage error, and 3 for store I/O — verify with
`list` before retrying an exit 3, same as add.

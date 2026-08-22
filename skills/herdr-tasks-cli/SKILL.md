---
name: herdr-tasks-cli
description: Use when asked to add tasks, a task list, or a plan to the Tasks board, or to inspect Tasks board items. Use `herdr-tasks add` and `herdr-tasks list`, never the TUI.
---

# herdr-tasks CLI

Use `herdr-tasks add` to create a task or JSON plan, and `herdr-tasks list` to
inspect the shared board store before and after adding work.

## Adding

```sh
herdr-tasks add -t "Draft release notes"
herdr-tasks add -t "Buy milk" --global
herdr-tasks add -t "Fix widget" --project widget
herdr-tasks add --title="-fix parser" --notes="-5 degrees" --project="-maintenance"
herdr-tasks add --file plan.json
cat plan.json | herdr-tasks add
```

Use `--title=<value>`, `--notes=<value>`, `--project=<value>`,
`--state-dir=<dir>`, or `--file=<path>` when a value begins with `-`. Use
`--json` with a flag add when another tool needs one result object.
It contains `outcome` (`created` or `existing`), `id`, trimmed `title`, and resolved
`project` (or `null` for global).

## Exit and retry contract

- exit 0: every item was created or already existed.
- exit 1: one or more plan items were refused. Retry only the `failed` subset.
  `created` and `existing` items both succeeded, so you must never whole-plan-retry an
  exit 1 run.
- exit 2: usage or parse error, nothing persisted. Correct the invocation, then run it.
- exit 3: store I/O, commit indeterminate. Run `herdr-tasks list --all --json` to
  check every scope, then retry only missing work. You must never whole-plan-retry an exit 3 run.

Add is idempotent by trimmed title and resolved project scope. A non-soft-deleted
matching task is a success: plain flag add prints `task already exists`, and plan add
reports it in `existing` with its `i`, `id`, and `title`.

## Listing scope and filters

`herdr-tasks list` defaults to ready, started, blocked, and review tasks in the
invocation project, or global scope outside a repository. Use `-p`/`--project <scope>`
for the same basename-or-path resolution as add, `--global` for global tasks, or `--all`
for every scope. Use `--project=<scope>` or `--state-dir=<dir>` when either list value
begins with `-`, `-p -maintenance` is usage. `--done` lists done tasks only; `--deleted` lists soft-deleted tasks only,
regardless of stored status. Scope selectors are mutually exclusive, as are `--done` and
`--deleted`.

A typo in a project name silently files the task under a new scope. Use
`herdr-tasks list --all --json` to recover the resulting scope.

## Checklist items

```sh
herdr-tasks check <task-id> add "Draft outline"
herdr-tasks check <task-id> toggle <item-short-id>
herdr-tasks list <task-id>
```

`<task-id>` is a task UUID from `herdr-tasks list --json`. An item short id is
the shortest unambiguous prefix of the item id. `herdr-tasks list <task-id>`
prints one line per item with its `[x]`/`[ ]` state and short id; with `--json`
the row's `checklist` array carries each item's `id`, `text`, `done`, and
`short_id`.

`toggle` flips the item state: a retry after an unseen success flips it back.
Never blind-retry a `check` invocation — run `herdr-tasks list <task-id>`
first and retry only a real refusal. `check` exits 0 when the item was created
or toggled, 1 for a refusal (stable tokens `empty-item-text`,
`invalid-item-text`, `unknown-task`, `soft-deleted-task`, `unknown-item`,
`ambiguous-item`), 2 for a usage error, and 3 for store I/O — verify with
`list` before retrying an exit 3, same as add.

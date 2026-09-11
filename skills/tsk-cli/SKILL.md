---
name: tsk-cli
description: Use when asked to add, list, edit, or change status of Tasks board items. Use `tsk add`, `tsk list`, `tsk status`, `tsk edit`, and `tsk steps`, never the TUI.
---

# tsk CLI

Use `tsk add` to create a task or JSON plan, `tsk list` to inspect the shared
board store, `tsk status` to set human status, `tsk edit` to change title or
notes, and `tsk steps` to add, toggle, rename, or remove steps. Human output
escapes terminal controls in titles, step text, and project names; JSON does not.

## If tsk is not installed

Run `command -v tsk`. If it is missing, give the user
https://gettsk.sh/docs/install.md and stop. Never run `install.sh`, `brew`, or
`cargo build` unless they asked.

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
`id`, numeric `number`, trimmed `title`, and resolved `project` (or `null` for the desk).

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
  check open work across scopes. Also check `--done` and `--archived` if a matching
  task could be hidden there; use direct lookup when its number is known. Retry
  only missing work. Never whole-plan-retry an exit 3 run.

Add is idempotent by trimmed title, resolved project scope, and normalized thread. A non-soft-deleted
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
soft-deleted tasks only, regardless of stored status: live soft-deletes plus
trash entries from `trash.jsonl` (kept 30 days), deduped by task with the live
copy winning, newest deletion first. Scope selectors are mutually
exclusive, as are `--done` and `--deleted`.

A typo in a project name silently files the task under a new scope. Use
`tsk list --all --json` to recover the resulting scope.

Rows the board paints as `N1`… are human-only notices: starter guides and the
`What's new in tsk` release note. `tsk list` never shows them, no command addresses
them, and agents ignore them.

## Direct task lookup and steps

A task number is the human handle: resolve `T12` with `tsk list T12`.
`T12`, `t12`, bare digits, and UUIDs are valid task operands. Direct lookup ignores cwd,
invocation default, and task scope: `tsk list T12` finds its one task even in
another project, including done and live soft-deleted tasks still in `tsk.json`.
A task that has left the live store for `trash.jsonl` is not found that way; use
`tsk list --deleted` and `tsk trash restore T12`. JSON list rows include
numeric `number` beside `id`. Do not combine a direct task operand with scope,
thread, or status filters.

```sh
tsk status T12 start
tsk edit T12 --title "Draft outline" --notes "Scope note"
tsk steps 12 add "Draft outline"
tsk steps 12 toggle <step-short-id>
tsk steps 12 rename <step-short-id> "Write the failing test"
tsk steps 12 remove <step-short-id>
tsk list 12
```

A step short id is the shortest unambiguous prefix of the step id. `tsk list 12`
prints one line per step with its `[x]`/`[ ]` state and short id; with `--json`
the row's `steps` array carries each step's `id`, `text`, `done`, and
`short_id`. UUID remains valid in each command where a task number is shown.

`toggle` flips the step state: a retry after an unseen success flips it back.
`rename` is idempotent on the trimmed text. `remove` is not: a retry after an
unseen success is `unknown-step`. Never blind-retry a `steps` invocation, run
`tsk list 12` first and retry only a real refusal. `steps` exits 0 when the
step was created, toggled, renamed, or removed, 1 for a refusal (stable tokens
`empty-step-text`, `invalid-step-text`, `unknown-task`, `soft-deleted-task`,
`unknown-step`, `ambiguous-step`), 2 for a usage error, and 3 for store I/O —
verify with `list` before retrying an exit 3, same as add.

## Status and edit

Agents set human status and rewrite title or notes by task address. Direct
lookup ignores cwd, same as `tsk list T12`.

```sh
tsk status T12 start
tsk status T12 blocked
tsk status T12 review
tsk status T12 done
tsk edit T12 --title "New title"
tsk edit T12 --notes "Replacement notes"
tsk edit T12 --title="-fix parser" --notes="-5 degrees"
```

`status` accepts `ready`, `started` (or `start`), `blocked`, `review`, or `done`.
Output always uses the stored name (`started`, not `start`). Repeating the same
status is idempotent.

`edit` needs at least one of `--title` or `--notes`. Scope and thread stay as
they are. Notes that trim to nothing are cleared. Newlines and tabs in notes are
kept, same as add. Repeating the stored values
is idempotent. Values that start with `-` need `--title=<value>` or
`--notes=<value>`.

Both exit 0 on success, 1 for a refusal (`unknown-task`, `soft-deleted-task`,
and for edit also `empty-title`, `invalid-title`), 2 for usage,
and 3 for store I/O. Verify with `tsk list T12` before retrying an exit 3.

## Archived tasks and projects

An archived task keeps its human status and leaves every working view. Archive
by task address and bring it back the same way:

```sh
tsk archive T12
tsk unarchive T12
```

Both are idempotent and exit 0 on repeat with the same line
(`archived T12 <title>` / `unarchived T12 <title>`). An unknown task refuses
with `T12 is not on the board` and a soft-deleted task with `T12 is deleted`
(exit 1).

Whole projects archive too, by `!p` name rules (basename case-insensitive or a
`/path` verbatim):

```sh
tsk project archive widget
tsk project unarchive widget
```

Unarchiving returns every task in the status it had; a task's own archived flag
is independent. A name matching no project that has tasks exits 1 with
`no project named <name> has tasks`.

`tsk list --archived` lists archived tasks and tasks of archived projects, one
row per id, marked `archived` or `project archived`; default list views exclude
both. `tsk add` into an archived project exits 1 with code `project-archived`
and the hint names `--desk`, `-p <other project>`, and
`tsk project unarchive <name>` — nothing persists.

## Trash

A soft-deleted task leaves the board store once it is no longer undoable, or
after 7 days, and lives in `trash.jsonl` for 30 days. `tsk list --deleted`
shows trash entries beside live soft-deletes, keeping their `T<n>` number.

```sh
tsk trash restore T12
```

`restore` puts the task back on the board: not soft-deleted, with a `restored`
history event, a new revision, and its old number. A missing line, or a task
that is already live, refuses with `T12 is not in trash` (exit 1). Usage
errors exit 2; store I/O exits 3 — verify with `tsk list --deleted` before
retrying an exit 3, same as add.

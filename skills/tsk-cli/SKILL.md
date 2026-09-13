---
name: tsk-cli
description: Work the user's tsk task board from the command line. Use when asked to add, update, edit, start, block, finish, archive, or restore a task on the board (or "tsk", "the tsk board", "the desk"), to add or tick steps, or to answer "what's on the board", "what's next", "what's on deck", "what needs me". Always `tsk add|list|status|edit|steps|archive|trash`, never the TUI.
version: 1.2.0
---

# tsk: the user's task board

tsk is the user's task board. Tasks have a human status (`open` · `ready` · `started` · `blocked` ·
`review` · `done`), live on the **desk** (no project) or in a **project** (a Git repo root, named
by its basename), and may carry a **thread** label. The user sees the board in a TUI; you never
open it. Every read and write goes through the CLI below, and the user's board updates live.

## If tsk is not installed

Run `command -v tsk`. If it is missing, give the user https://gettsk.sh/docs/install.md and
stop. Never run `install.sh`, `brew`, or `cargo build` unless they asked.

## Quick reference

| Ask | Command |
| --- | --- |
| Add a task here | `tsk add -t "Title"` (project of the cwd repo, else desk) |
| Add to the desk / a project / a thread | `tsk add -t "Title" --desk` · `-p widget` · `--thread rel-2026` |
| Add with notes | `tsk add -t "Title" -n "Notes"` |
| Add many | `tsk add --file plan.json` or pipe JSON to `tsk add` |
| What's on the board | `tsk list --json` · `tsk list --desk --json` · `tsk list --all --json` |
| What's next / on deck | `tsk list --json`, rows whose `status` is `ready` |
| What's in the inbox / untriaged | `tsk list --json`, rows whose `status` is `open` |
| What needs the user | `tsk list --json`, rows whose `status` is `blocked` or `review` |
| What's in motion | `tsk list --json`, rows whose `status` is `started` |
| One complete task | `tsk list T12 --json` |
| Set inbox / on deck / start / block / hand back / finish | `tsk status T12 open` · `ready` · `start` · `blocked` · `review` · `done` |
| Change title or notes | `tsk edit T12 --title "…"` · `--notes "…"` |
| Steps | Read ids with `tsk list T12 --json`; then `add` · `toggle` · `rename` · `remove` |
| Archive / unarchive | `tsk archive T12` · `tsk unarchive T12` · `tsk project archive widget` |
| Done, open, ready, archived, deleted | `tsk list --done --json` · `--open` · `--ready` · `--archived` · `--deleted` |
| Bring back a deleted task | `tsk trash restore T12` |
| User-facing display | omit `--json` only when showing output to the user or troubleshooting |

`T12`, `t12`, `12`, and the UUID all address the same task. Prefer `T12`, it is what the user sees.

## Rules

1. **Human status is the user's.** When your work on a task is finished, set `review`. Set
   `done` only when the user says the task is done or told you to close it. Never mark tasks
   done from your own progress.
2. **Check the scope before `-p name`.** A typo silently files the task under a new project.
   Confirm with `tsk list --all --json` (the `project` field) when unsure; recover a stray task
   the same way, then `tsk edit` cannot move it, so re-add in the right scope and archive the stray.
3. **Thread names**: lowercase ASCII letters, digits, `-` and `.`, starting with a letter or
   digit, at most 32 characters. `--thread` normalizes case; anything else is refused.
4. **Never pass `--state-dir`** unless the user asked. It points at a different board.
5. **Read JSON, not presentation.** Always add `--json` to `tsk list`. Do not parse human list
   formatting; use it only when showing output to the user or troubleshooting interactively.
   Mutation acknowledgements may be human-only: trust their exit and refusal codes, then verify
   state with `tsk list … --json`.
6. **Never blind-retry.** Read the exit code, then act (contract below). `add` is idempotent,
   `steps toggle` and `steps remove` are not.
7. **Ignore notice rows.** Rows the board paints as `N1`… are human-only (starter tasks and
   release notes). `tsk list --json` never shows them and no command addresses them.
8. Values that begin with `-` need the `=` form: `--title="-fix parser"`, `--project="-x"`,
   `--notes="-5 degrees"`, `--state-dir=<dir>`, `--file=<path>`.

## Exit contract (all commands)

| Exit | Meaning | Do |
| --- | --- | --- |
| 0 | done, or already true (idempotent success) | after a mutation, verify the affected task or view with `tsk list … --json`; otherwise nothing |
| 1 | refusal of one or more items; valid siblings persisted | fix and retry only the refused subset |
| 2 | usage or parse error, nothing persisted | correct the invocation, run again |
| 3 | store I/O, commit indeterminate | `tsk list … --json` (also `--done`, `--archived` if hidden), retry only what is missing |

After exit 1 or exit 3, never whole-plan-retry: retry the refused or missing subset only.
Refusals carry a stable code: `empty-title`,
`invalid-title`, `invalid-thread`, `unknown-task`, `soft-deleted-task`, `project-archived`,
`empty-step-text`, `invalid-step-text`, `unknown-step`, `ambiguous-step`. The human message
is not a contract.

## Adding

```sh
tsk add -t "Draft release notes"
tsk add -t "Buy milk" --desk
tsk add -t "Fix widget" --project widget --thread release-2026
tsk add --title="-fix parser" --notes="-5 degrees" --project="-maintenance"
tsk add --file plan.json
cat plan.json | tsk add
```

- Default scope is the Git repo root of the cwd; outside Git, the desk. `-p` takes a project
  basename (case-insensitive) or a `/full/path`. `--desk` forces the desk. New tasks start `open`
  in the inbox; set `ready` when the user has picked them for the on-deck queue.
- Idempotent on trimmed title + resolved scope + normalized thread: a matching live task is a
  success (`task already exists`, exit 0). Safe to retry.
- `--json` (flag add) prints one object: `outcome` (`created` | `existing`), `id`, `number`,
  `title`, `project` (`null` for desk).
- Plan JSON: `[{"title": "…", "notes": "…", "project": "…", "thread": "…"}]`, `notes`,
  `project`, `thread` optional (`thread` may be `null`). Result:
  `{"created": [...], "existing": [...], "failed": [...]}`; `created` and `existing` rows
  carry `i` (item index), `id`, `number`, `title`, failed rows carry `i`, `title`, `code`,
  `error`. Scope comes from each item's `project`: item flags (including `--desk`) together
  with `--file` are usage (exit 2). Piped stdin is ignored when item flags are present.
- Adding into an archived project refuses with `project-archived` (exit 1). A flag add persists
  nothing; in a plan add the other items still persist, retry only the refused ones.

## Listing

```sh
tsk list --json                    # open tasks in this repo's project
tsk list --desk --json             # open desk tasks
tsk list --all --json              # open tasks in every scope
tsk list --thread rel-1 --json     # thread within the selected scope
tsk list --done --json
tsk list --open --json
tsk list --ready --json
tsk list --deleted --json
tsk list --archived --json
tsk list T12 --json                # one complete task
```

- JSON is the agent read contract. Human output is presentation for the user or interactive
  troubleshooting; never parse it. Human output groups by status: `STARTED`, `READY`, `OPEN`,
  `BLOCKED`, `REVIEW`; a filtered row is
  ` - <number> <title> #<thread>`. Map board language onto it: *on deck* = READY + OPEN (inbox = OPEN),
  *in motion* = STARTED, *needs you* = BLOCKED + REVIEW. All human list content wraps to the attached terminal
  width with hanging indentation, supported from 50 columns. Direct task output uses separate notes,
  steps, and `#thread` blocks in that order, with a blank line between blocks that exist; steps
  show state and text without ids.
- `--json` is a flat array of `id`, `number`, `title`, `status`, `project` (`null` for desk),
  `thread` (`null` if none), in display order. `tsk list T12 --json` returns the complete task in
  this field order: `id`, `number`, `project`, `status`, `title`, `notes`, `steps`, `thread`.
  Missing notes are `null`, missing steps are `[]`; each step carries `id`, `done`, `short_id`,
  and `text`.
- Scope selectors (`-p`, `--desk`, `--all`) are mutually exclusive. Status selectors
  `--open`, `--ready`, `--done`, `--deleted`, and `--archived` are mutually exclusive. An invalid
  `--thread` is exit 2, not an empty result.
- `tsk list T12 --json` ignores cwd and scope and finds the task anywhere, including done and live
  soft-deleted tasks. A task already moved to trash needs `--deleted`. Do not combine a task
  operand with scope, thread, or status filters.
- `--deleted` shows live soft-deletes plus `trash.jsonl` entries (kept 30 days), newest first.
- Human output escapes terminal controls in titles, notes, steps, and project names; JSON does not.

## Status and edit

```sh
tsk status T12 open       # untriaged inbox
tsk status T12 ready      # picked, on deck
tsk status T12 start      # ready/open → started (start = started)
tsk status T12 blocked
tsk status T12 review     # your work is done, the user decides
tsk status T12 done       # only when the user said so
tsk edit T12 --title "New title"
tsk edit T12 --notes "Replacement notes"
```

- `status` accepts `open`, `ready`, `started` (or `start`), `blocked`, `review`, `done`; output
  uses the stored name. Repeating a status is idempotent.
- `edit` needs `--title` and/or `--notes`; it never changes scope or thread. Notes that trim
  to nothing clear the notes. Newlines and tabs are kept. Repeating stored values is
  idempotent. Notes render a small markdown subset on the board (`**bold**`, `*em*`,
  `` `code` ``, `#` headings, `-` lists, fenced code).
- Refusals: `unknown-task`, `soft-deleted-task`, and for edit `empty-title`, `invalid-title`.

## Steps

```sh
tsk steps T12 add "Write the failing test"
tsk steps T12 toggle a3
tsk steps T12 rename a3 "Write the failing test first"
tsk steps T12 remove a3
tsk list T12 --json       # shows [x]/[ ] state and each step's short_id
```

- A step short id is the shortest unambiguous prefix of the step id, printed by
  `tsk list T12 --json`.
- `toggle` flips: a retry after an unseen success flips it back. `rename` is idempotent on
  trimmed text. `remove` is not: a retry after an unseen success is `unknown-step`. Run
  `tsk list T12 --json` before retrying any `steps` command.

## Archive

```sh
tsk archive T12
tsk unarchive T12
tsk project archive widget
tsk project unarchive widget
```

- An archived task keeps its status and leaves every working view; `tsk list --archived --json` shows
  archived tasks and tasks of archived projects (`archived` / `project archived`).
- Both task verbs are idempotent (exit 0 on repeat). Unknown task: `T12 is not on the board`;
  deleted task: `T12 is deleted` (exit 1).
- Project names follow `-p` rules. A name with no tasks exits 1
  (`no project named <name> has tasks`). Unarchiving a project restores each task's own
  status; a task's own archived flag is independent.

## Trash

```sh
tsk list --deleted --json
tsk trash restore T12
```

Deleted tasks leave the live store once undo can no longer reach them (or after 7 days) and
stay in `trash.jsonl` for 30 days, keeping their number. `restore` puts the task back with
its old number. A task not in trash, or already live, refuses with `T12 is not in trash`
(exit 1).

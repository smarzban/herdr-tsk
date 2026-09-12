---
name: tsk-cli
description: Work the user's tsk task board from the command line. Use when asked to add, update, edit, start, block, finish, archive, or restore a task on the board (or "tsk", "the tsk board", "the desk"), to add or tick steps, or to answer "what's on the board", "what's next", "what's on deck", "what needs me". Always `tsk add|list|status|edit|steps|archive|trash`, never the TUI.
version: 1.1.0
---

# tsk: the user's task board

tsk is the user's task board. Tasks have a human status (`ready` · `started` · `blocked` ·
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
| What's on the board | `tsk list` (this project) · `tsk list --desk` · `tsk list --all` |
| What's next / on deck | `tsk list`, READY group |
| What needs the user | `tsk list`, BLOCKED and REVIEW groups |
| What's in motion | `tsk list`, STARTED group |
| One task and its steps | `tsk list T12` |
| Start / block / hand back / finish | `tsk status T12 start` · `blocked` · `review` · `done` |
| Change title or notes | `tsk edit T12 --title "…"` · `--notes "…"` |
| Steps | `tsk steps T12 add "…"` · `toggle <id>` · `rename <id> "…"` · `remove <id>` |
| Archive / unarchive | `tsk archive T12` · `tsk unarchive T12` · `tsk project archive widget` |
| Done, archived, deleted | `tsk list --done` · `--archived` · `--deleted` |
| Bring back a deleted task | `tsk trash restore T12` |
| Machine output | add `--json` to `add` or `list` |

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
5. **Never blind-retry.** Read the exit code, then act (contract below). `add` is idempotent,
   `steps toggle` and `steps remove` are not.
6. **Ignore notice rows.** Rows the board paints as `N1`… are human-only (starter tasks and
   release notes). `tsk list` never shows them and no command addresses them.
7. Values that begin with `-` need the `=` form: `--title="-fix parser"`, `--project="-x"`,
   `--notes="-5 degrees"`, `--state-dir=<dir>`, `--file=<path>`.

## Exit contract (all commands)

| Exit | Meaning | Do |
| --- | --- | --- |
| 0 | done, or already true (idempotent success) | nothing |
| 1 | refusal of one or more items; valid siblings persisted | fix and retry only the refused subset |
| 2 | usage or parse error, nothing persisted | correct the invocation, run again |
| 3 | store I/O, commit indeterminate | `tsk list …` (also `--done`, `--archived` if it could be hidden), retry only what is missing |

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
  basename (case-insensitive) or a `/full/path`. `--desk` forces the desk.
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
tsk list                 # ready, started, blocked, review in this repo's project
tsk list --desk          # same for the desk
tsk list --all           # every scope, grouped by status then project
tsk list --thread rel-1  # filter within the selected scope
tsk list --done | --deleted | --archived
tsk list T12             # one task, with its steps
tsk list --json
```

- Human output groups by status: `STARTED`, `READY`, `BLOCKED`, `REVIEW`; a row is
  ` - <number> <title> #<thread>`. Map board language onto it: *on deck* = READY, *in motion* =
  STARTED, *needs you* = BLOCKED + REVIEW.
- `--json` is a flat array of `id`, `number`, `title`, `status`, `project` (`null` for desk),
  `thread` (`null` if none), in display order. `tsk list T12 --json` adds `steps`:
  `id`, `text`, `done`, `short_id`.
- Scope selectors (`-p`, `--desk`, `--all`) are mutually exclusive, so are `--done` and
  `--deleted`. An invalid `--thread` is exit 2, not an empty result.
- `tsk list T12` ignores cwd and scope and finds the task anywhere, including done and live
  soft-deleted tasks. A task already moved to trash needs `--deleted`. Do not combine a task
  operand with scope, thread, or status filters.
- `--deleted` shows live soft-deletes plus `trash.jsonl` entries (kept 30 days), newest first.
- Human output escapes terminal controls in titles, steps, and project names; JSON does not.

## Status and edit

```sh
tsk status T12 start      # ready → started (start = started)
tsk status T12 blocked
tsk status T12 review     # your work is done, the user decides
tsk status T12 done       # only when the user said so
tsk edit T12 --title "New title"
tsk edit T12 --notes "Replacement notes"
```

- `status` accepts `ready`, `started` (or `start`), `blocked`, `review`, `done`; output uses the
  stored name. Repeating a status is idempotent.
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
tsk list T12              # shows [x]/[ ] and each step's short id
```

- A step short id is the shortest unambiguous prefix of the step id, printed by `tsk list T12`.
- `toggle` flips: a retry after an unseen success flips it back. `rename` is idempotent on
  trimmed text. `remove` is not: a retry after an unseen success is `unknown-step`. Run
  `tsk list T12` before retrying any `steps` command.

## Archive

```sh
tsk archive T12
tsk unarchive T12
tsk project archive widget
tsk project unarchive widget
```

- An archived task keeps its status and leaves every working view; `tsk list --archived` shows
  archived tasks and tasks of archived projects (`archived` / `project archived`).
- Both task verbs are idempotent (exit 0 on repeat). Unknown task: `T12 is not on the board`;
  deleted task: `T12 is deleted` (exit 1).
- Project names follow `-p` rules. A name with no tasks exits 1
  (`no project named <name> has tasks`). Unarchiving a project restores each task's own
  status; a task's own archived flag is independent.

## Trash

```sh
tsk list --deleted
tsk trash restore T12
```

Deleted tasks leave the live store once undo can no longer reach them (or after 7 days) and
stay in `trash.jsonl` for 30 days, keeping their number. `restore` puts the task back with
its old number. A task not in trash, or already live, refuses with `T12 is not in trash`
(exit 1).

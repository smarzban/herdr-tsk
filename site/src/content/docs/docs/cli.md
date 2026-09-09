---
title: CLI
description: Headless tsk add, list, steps, status, edit, trash, and archive. The agents' door to the board.
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
  toggle or remove, because those are not idempotent.
- Move work with `tsk status <task> start` (or `started`, `blocked`, `review`, `ready`, `done`).
  Repeating the same status is safe. Rewrite title or notes with `tsk edit`.
- Prefer `--json` and read the exit code. Exit 1 means retry only the failed
  items. Exit 3 means list before retrying.

The repo ships the same rules as an agent skill in
[`skills/tsk-cli/SKILL.md`](https://github.com/smarzban/herdr-tsk/blob/main/skills/tsk-cli/SKILL.md).
`tsk guide` prints that skill with the YAML frontmatter removed.

## Commands

| Command | Does |
| --- | --- |
| `tsk` | opens the board |
| `tsk capture` | opens quick capture: the expanded quick-add page (also `TSK_MODE=capture`) |
| `tsk add` · `tsk list` · `tsk steps` · `tsk status` · `tsk edit` · `tsk trash` · `tsk archive` · `tsk unarchive` · `tsk project` | headless; below |
| `tsk guide` | print the agent workflow skill (frontmatter stripped), exit 0 |
| `tsk --help` | usage, exit 0 |
| `tsk --find-board-pane` | herdr helper: reads `pane list` JSON on stdin, prints the id of the pane labelled `tsk`; exit 1 when none |

Every headless command takes `--state-dir <dir>` to work against another store.

## guide

Print the embedded agent skill, YAML frontmatter stripped. Stderr is empty.

```
tsk guide
```

Exit 0. The same body is the source for `/docs/agents/` and `tsk setup <agent>`.

Human-readable stdout and stderr escape C0/C1 controls in stored titles, step
text, and project names as `\u{00xx}`. JSON keeps the underlying values.

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
- `--deleted` soft-deleted only: live soft-deletes plus trash entries from
  `trash.jsonl` (kept 30 days), deduped by task with the live copy winning,
  newest deletion first
- `--archived` archived only: individually archived tasks plus tasks of
  archived projects, one row per id, each marked `archived` or
  `project archived`; JSON rows carry the mark in `archived`. Default views
  never list archived tasks or tasks of archived projects
- `--thread` filters within the selected scope

Human output groups rows under `STARTED`, `READY`, `BLOCKED`, `REVIEW`, then
`DONE` or `DELETED` when asked, as `- <number> <title> #thread`. With `--all`, a
trailing scope label (`desk` or `project: <path>`) tells the groups apart.

A store-global task number (`T12`, `t12`, or bare `12`) or a task UUID lists
that one task and its steps (`[x]` / `[ ]` plus the step short id). Direct lookup
ignores cwd and searches the live store, including done and live soft-deleted
tasks. A task that has moved to `trash.jsonl` is not found by `tsk list T12`;
use `tsk list --deleted`, then `tsk trash restore T12`. A task operand cannot
combine with scope, thread, or status filters. An address that matches nothing
is a usage error (exit 2).

`--json` is a flat array of `id`, `number`, `title`, `status`, `project`, and
`thread`. Single-task JSON also attaches `steps`.

To recover a typo scope: `tsk list --all --json`.

## steps

```
tsk steps <task> add <text> [--state-dir <dir>]
tsk steps <task> toggle <step-short-id> [--state-dir <dir>]
tsk steps <task> rename <step-short-id> <text> [--state-dir <dir>]
tsk steps <task> remove <step-short-id> [--state-dir <dir>]
```

Output is `added <short-id> <text>`, `toggled <short-id> [x] <text>`,
`renamed <short-id> <text>`, or `removed <short-id> <text>`.

See [steps](/docs/steps/) for the board side. `toggle` and `remove` are not
idempotent. `rename` is idempotent on the trimmed text. Verify with
`tsk list <task>` before a retry of toggle or remove.

## status

```
tsk status <task> <status> [--state-dir <dir>]
```

`<status>` is `ready`, `started` (or `start`), `blocked`, `review`, or `done` (`tsk status <task> done`). Output is
`status T<n> <status> <title>` and always uses the stored name (`started`). Repeating the same status is idempotent.

Unknown task: `unknown-task`. Soft-deleted: `soft-deleted-task`.

## edit

```
tsk edit <task> [--title <title>] [--notes <notes>] [--state-dir <dir>]
```

At least one of `--title` or `--notes` is required. Scope and thread are
unchanged. Notes that trim to nothing are cleared. Newlines and tabs in notes are kept, same as add. Output is
`edited T<n> <title>`. Repeating the stored values is idempotent.

Values that start with `-` need `--title=…` or `--notes=…`.

Refusal tokens: `unknown-task`, `soft-deleted-task`, `empty-title`,
`invalid-title`.

## trash

```
tsk trash restore <task> [--state-dir <dir>]
```

A soft-deleted task leaves the board store once it is no longer undoable, or
after 7 days, and lives in `trash.jsonl` for 30 days. `tsk list --deleted`
shows trash entries beside live soft-deletes, with their `T<n>` number.

`tsk trash restore T<n>` puts the task back on the board: not soft-deleted,
with a `restored` history event, a new revision, and its old number. A missing
line, or a task that is already live, refuses with `T<n> is not in trash`
(exit 1). Usage errors exit 2; store I/O exits 3.

## archive and unarchive

```
tsk archive <task> [--state-dir <dir>]
tsk unarchive <task> [--state-dir <dir>]
```

`archive` sets a task's archived flag; `unarchive` clears it. The task keeps its
human status and its `T<n>`. Repeating the verb is idempotent: the same
`archived T7 <title>` / `unarchived T7 <title>` line prints and nothing changes.
An unknown task refuses with `T<n> is not on the board` and a soft-deleted task
with `T<n> is deleted` (both exit 1); usage errors exit 2; store I/O exits 3.

## project archive / unarchive

```
tsk project archive <name> [--state-dir <dir>]
tsk project unarchive <name> [--state-dir <dir>]
```

`<name>` follows the `!p` rules: a project basename (case-insensitive) or a
`/path` verbatim. Archiving writes one lazy project record; unarchiving removes
it, and every task returns in the status it had. A task's own archived flag is
independent. Output is `archived project <short>` / `unarchived project <short>`,
idempotent on repeat. A name matching no project that has tasks exits 1 with
`no project named <name> has tasks`.

## Exit contract

| Exit | Meaning |
| --- | --- |
| 0 | listed, every add item created/existed, the step applied, status or edit written (or already had the value), the task restored, or the archive flag written (or already had the value) |
| 1 | one or more item refusals (add/steps/status/edit), a trash restore with no matching line, an unknown or deleted `archive`/`unarchive` task, or a project action matching nothing. Retry only the failed subset. For toggle and remove, list first. |
| 2 | usage or parse error, including a `list` address that matches nothing. Nothing persisted. |
| 3 | store I/O. Commit is indeterminate. `tsk list` before retrying. |

Add refusal codes: `empty-title`, `invalid-title`, `invalid-thread`, `invalid-item`,
`project-archived`. `project-archived` prints `project <name> is archived. Use
--desk, -p <other project>, or tsk project unarchive <name>` and persists
nothing. This covers the cwd default and an explicit `-p`.
Any C0 control in a title or step text is `invalid-title` / `invalid-step-text`
before trimming.

Steps refusals (exit 1): `empty-step-text`, `invalid-step-text`, `unknown-task`,
`soft-deleted-task`, `unknown-step`, `ambiguous-step`.

Status refusals (exit 1): `unknown-task`, `soft-deleted-task`.

Edit refusals (exit 1): `unknown-task`, `soft-deleted-task`, `empty-title`,
`invalid-title`.

## setup

```
tsk setup
tsk setup herdr
tsk setup claude | pi | cursor | grok | codex
tsk setup --skill-dir <path> [--force] [--json]
```

Bare `tsk setup` lists targets and writes nothing. `tsk setup herdr` registers
the embedded plugin assets and adds prefix+t (board) and prefix+a (quick capture).
It uses the same installed binary, with no source checkout or second build. Herdr
0.9+ must be on PATH. Conflicting shortcuts require interactive confirmation;
declining preserves them. A noninteractive conflict aborts without writes.
`tsk setup herdr --help` is read-only.

Agent targets write the embedded `skills/tsk-cli/SKILL.md` (frontmatter kept) into
that tool's user-level skills directory as `tsk-cli/SKILL.md`:

- `claude` → `~/.claude/skills/`
- `pi` → `~/.pi/agent/skills/`
- `cursor` → `~/.cursor/skills/`
- `grok` → `~/.grok/skills/`
- `codex` → `~/.agents/skills/`
- `--skill-dir <path>` → `<path>/tsk-cli/SKILL.md`

A second run without `--force` exits 1 with `skill-exists` and leaves the file.
`--force` overwrites. `--json` emits `outcome` (`written`, `exists`, or `listed`),
`target`, and `path`. Two targets, or `herdr` plus `--skill-dir`, is usage (exit 2).
An empty `--skill-dir` is usage. A symlink at the skills root, the `tsk-cli`
directory, or `SKILL.md` is refused.

Exit codes: 0 success/help/list, 1 setup or confirmation failure or `skill-exists`,
2 invalid arguments. Herdr setup does not edit task data. See
[installation](/docs/install/#herdr-setup-with-an-installed-binary)
for config paths, backups, reload, upgrades and removing integration.

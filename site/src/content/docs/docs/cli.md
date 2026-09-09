---
title: CLI
description: Commands, arguments, output, and retry rules for the shared task board.
---

The CLI reads and updates the same tasks as the board. A running board picks up saved changes automatically.

```sh
tsk add -t "Fix login timeout"
tsk list
```

## For agents

Install the skill with `tsk setup pi`, or use your [agent's setup target](#setup). `tsk guide` prints the workflow.

1. Read the task with `tsk list T12`.
2. Set its status with `tsk status T12 started`.
3. Update notes or steps as work progresses.
4. Set `review`, `blocked`, or `done` explicitly.

The CLI can mark a task done with `tsk status <task> done`. Agent lifecycle does not change task status automatically.

Use `--json` on `add` or `list` for machine-readable output. Read the [exit contract](#exit-contract) before retrying a write.

## Commands

| Command | Action |
| --- | --- |
| `tsk` | Open the board |
| `tsk capture` | Open capture; save or discard exits |
| `tsk add` | Add a task or JSON plan |
| `tsk list` | Read tasks |
| `tsk status` | Set task status |
| `tsk edit` | Replace title or notes |
| `tsk steps` | Add, toggle, rename, or remove steps |
| `tsk archive` / `tsk unarchive` | Hide or restore a task |
| `tsk project archive` / `tsk project unarchive` | Hide or restore a project |
| `tsk trash restore` | Restore a task from trash |
| `tsk setup` | Configure Herdr or install an agent skill |
| `tsk guide` | Print the agent workflow |
| `tsk --help` | Show help |

Data commands accept `--state-dir <dir>`. Setup and guide do not use that flag.

### Task addresses

`T12`, `t12`, `12`, and a task UUID identify the same task. Direct lookup ignores the current project.

### Scope

Without a scope flag, `add` and filtered `list` use the launch repository, or the current directory outside Git. Commands addressed to a task ignore that default.

| Flag | Scope |
| --- | --- |
| `--desk` | Desk |
| `-p name` / `--project name` | Project matching that basename, ignoring case |
| `-p /path` | Project path |
| `--all` on list | All scopes |

A missing or ambiguous basename stays as typed. A typo can create a separate scope. Check with `tsk list --all --json`.

## add

```sh
tsk add -t "Fix login timeout" -n "Reproduce on a slow connection" --thread auth
tsk add -t "Buy coffee" --desk
tsk add -t "Draft release notes" -p atlas --json
```

| Option | Purpose |
| --- | --- |
| `-t`, `--title` | Required task title |
| `-n`, `--notes` | Optional notes |
| `-p`, `--project` or `--desk` | Destination |
| `--thread` | [Thread name](/docs/capture/#thread-names) |
| `--json` | One result object |
| `--file <path>` or `--file -` | Read a JSON plan |
| `--state-dir <dir>` | Alternate state directory |

Duplicate detection compares the trimmed title, resolved project, and normalized thread. A matching non-deleted task returns successfully without changes, even if done or individually archived. Adding into an archived project refuses.

Plain output: `added <title>` or `task already exists`.

JSON output: `outcome` (`created` or `existing`), `id`, `number`, `title`, and `project` (`null` for desk).

Values starting with `-` use equals syntax: `--title="-fix parser"`, `--notes="-5 degrees"`, `--project="-maintenance"`, `--file=...`, or `--state-dir=...`.

Blank notes are omitted. C0 control characters in titles are rejected before trimming.

### JSON plans

```json
[
  {"title": "Reproduce login timeout", "project": "atlas", "thread": "auth"},
  {"title": "Draft release notes", "notes": "Include migration instructions"}
]
```

```sh
tsk add --file plan.json
cat plan.json | tsk add
```

| Field | Meaning |
| --- | --- |
| `title` | Required string |
| `notes` | Optional notes |
| `project` omitted | Invocation default |
| `project: null` | Desk |
| `project` string | Project name or path |
| `thread` omitted or `null` | No thread |
| `thread` string | Normalized thread |

Output contains `created`, `existing`, and `failed` arrays. Items carry their input index `i`; failures include `code` and `error`. Successful entries include task ID, number, and title. Notes are not echoed.

Valid items persist even if another item fails. Retry only failed or confirmed-missing items.

Do not mix item flags with `--file`. Piped input is ignored when item flags are present.

## list

```sh
tsk list
tsk list T12
tsk list --all --json
tsk list -p atlas --thread auth
tsk list --done --all
tsk list --archived --all
tsk list --deleted --all
```

```text
tsk list [<task>] [-p <project> | --desk | --all] [--thread <name>] [--done | --deleted | --archived] [--json] [--state-dir <dir>]
```

| Filter | Result |
| --- | --- |
| Default | Ready, started, blocked, and review tasks; excludes archived and deleted work |
| `--done` | Completed tasks |
| `--archived` | Individually archived tasks and tasks in archived projects, across statuses |
| `--deleted` | Deleted tasks in the main store and trash, newest first |
| `--thread` | Filter within the selected scope |

Scope flags are mutually exclusive. So are `--done`, `--deleted`, and `--archived`.

A direct task address searches the main store, including done, archived, and recently deleted tasks. It cannot be combined with scope, thread, or status filters. A missing task exits 2. Tasks already moved to trash require `--deleted`.

Human output groups by status and includes the task number and thread. `--all` adds scope labels. Single-task output includes steps and their short IDs.

JSON returns an array with `id`, `number`, `title`, `status`, `project`, and `thread`. Direct lookup also includes `steps`. Archived listings include an `archived` mark: `archived` or `project archived`.

## status

```sh
tsk status T12 started
tsk status T12 review
```

Accepts `ready`, `started` (or `start`), `blocked`, `review`, and `done`.

Unlike keyboard toggles, this command sets the requested status directly. Repeating the same value is safe.

Output: `status T12 <status> <title>`. The output uses `started`, even when the input was `start`.

## edit

```sh
tsk edit T12 --title "Fix timeout on slow connections"
tsk edit T12 --notes "Reproduced with a delayed response"
```

Requires `--title`, `--notes`, or both. Scope and thread stay unchanged.

Blank notes clear the field. Notes preserve newlines and tabs. Use `--title=...` or `--notes=...` for values starting with `-`.

Output: `edited T12 <title>`. Repeating the same values is safe.

## steps

```text
tsk steps <task> add <text> [--state-dir <dir>]
tsk steps <task> toggle <step-short-id> [--state-dir <dir>]
tsk steps <task> rename <step-short-id> <text> [--state-dir <dir>]
tsk steps <task> remove <step-short-id> [--state-dir <dir>]
```

Read short IDs with `tsk list T12`. Full step UUIDs also work.

| Action | Output prefix | Safe to repeat unchanged? |
| --- | --- | --- |
| Add | `added <short-id> <text>` | No; creates another step |
| Toggle | `toggled <short-id> [x] <text>` | No; flips the value again |
| Rename | `renamed <short-id> <text>` | Yes |
| Remove | `removed <short-id> <text>` | No; refuses after removal |

Read back before retrying an uncertain result. [Using steps on the board](/docs/steps/).

## archive and unarchive

```sh
tsk archive T12
tsk unarchive T12
```

Keep the task's status and number. Repeating either command is safe.

Output: `archived T12 <title>` or `unarchived T12 <title>`.

Unknown tasks refuse with `T12 is not on the board`; deleted tasks with `T12 is deleted`.

## project archive / unarchive

```sh
tsk project archive atlas
tsk project unarchive atlas
```

Accepts a project basename or path. Repeating an action is safe. An unknown project refuses with `no project named <name> has tasks`.

Restoring a project preserves task statuses and leaves individually archived tasks archived.

Output: `archived project <name>` or `unarchived project <name>`.

## trash

```sh
tsk list --deleted --all
tsk trash restore T12
```

Restore returns a task from trash with its original number. A task absent from trash, or already live, refuses with `T12 is not in trash`.

Recent deletions may still be in the main store; use board undo until they move to trash. [Retention and storage](/docs/storage/#deleted-tasks).

## setup

```sh
tsk setup
tsk setup herdr
tsk setup pi
tsk setup --skill-dir /path/to/skills
```

Bare `tsk setup` lists targets and writes nothing. Choose one target per call.

### Herdr

Requires Herdr 0.9+ on PATH. Registers the installed binary and adds **prefix+t** and **prefix+a**. Shortcut conflicts require confirmation; noninteractive conflicts stop before writes.

[Reload, upgrades, and removal](/docs/install/#herdr-setup-with-an-installed-binary).

### Agent skills

| Target | Skills directory |
| --- | --- |
| `pi` | `~/.pi/agent/skills/` |
| `claude` | `~/.claude/skills/` |
| `cursor` | `~/.cursor/skills/` |
| `grok` | `~/.grok/skills/` |
| `codex` | `~/.agents/skills/` |
| `--skill-dir <path>` | The supplied directory |

Setup writes `tsk-cli/SKILL.md` under the selected directory.

| Option | Action |
| --- | --- |
| `--force` | Replace an existing skill |
| `--json` | Return `outcome`, `target`, and `path` |

Without `--force`, an existing skill returns `skill-exists` and exits 1. Symlinks at the skills root, skill directory, or file are refused. Herdr setup does not accept these agent options.

Setup exits 0 for success/help/list, 1 for setup failure, or 2 for invalid arguments.

## guide

```sh
tsk guide
```

Prints the embedded [agent skill](/docs/agents/) without YAML frontmatter. Exits 0. It is the same workflow installed by agent setup.

## Exit contract

For data commands:

| Exit | Meaning | Next step |
| --- | --- | --- |
| `0` | Success, including an already-existing task or unchanged value | Continue |
| `1` | Refusal; a plan may have saved other items | Correct refusals; retry only failed items |
| `2` | Invalid arguments or input; nothing saved | Fix the invocation |
| `3` | Storage error; a write may have committed | Read back before retrying |

After an uncertain add, inspect `tsk list --all --json`. Also check `--done` and `--archived` when a duplicate could be hidden there. If you know the task number, use direct lookup.

| Command | Refusal codes |
| --- | --- |
| Add | `empty-title`, `invalid-title`, `invalid-thread`, `invalid-item`, `project-archived` |
| Steps | `empty-step-text`, `invalid-step-text`, `unknown-task`, `soft-deleted-task`, `unknown-step`, `ambiguous-step` |
| Status | `unknown-task`, `soft-deleted-task` |
| Edit | `unknown-task`, `soft-deleted-task`, `empty-title`, `invalid-title` |

Invalid thread flags fail argument parsing with exit 2; an invalid thread in a JSON plan is an item refusal with exit 1. An archived-project refusal saves nothing for that item; other valid plan items can still save.

Human-readable output escapes stored terminal control characters. JSON retains the underlying text.

## Herdr helper

`tsk --find-board-pane` reads Herdr `pane list` JSON from stdin and prints the ID of the pane labelled `tsk`. It exits 1 if none matches.

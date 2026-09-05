# Data model

Persisted and wire shapes verified against `src/domain/`, `src/store.rs`, and the CLI.
`tsk.json` is a strict current schema: unknown fields are rejected, and older formats
load only through the store's migration chain (`MIGRATIONS` in `src/store.rs`).

## Store document: `tsk.json`

`TaskStore` writes one JSON document under `default_state_dir()` (`TSK_STATE_DIR`,
then `$HOME/.tsk`, then `.tsk-state`). Siblings are `tsk.json.lock`, `tsk.json.1`,
a `tsk.json.v<N>` copy of a pre-migration document, `trash.jsonl`, and in-flight
`.tsk.json.tmp.*` files.

A missing file creates an empty `DomainState`. A present document must carry
`"format_version": 2`; a missing, `0`, or newer version is refused and is never
rewritten. A v1 document loads through the migration chain into memory (the live
file is untouched until the next save, which leaves a `tsk.json.v1` backup).
Saves require the current version and clear transient merge bases.

| Field | Type | Semantics |
| --- | --- | --- |
| `format_version` | `u32` | Required and exactly `2` on disk. |
| `next_task_number` | `u64` | Next store-global number, allocated under the store lock. |
| `tasks` | `Vec<Task>` | All tasks, including soft-deleted and archived tasks. |
| `projects` | `Map<path, ProjectRecord>` | Per-project records keyed by scope path. Always serialized (an empty map writes `"projects": {}`); a project appears only while it is archived. |
| `undo_stack` | `Vec<UndoEntry>` | LIFO records for complete and soft-delete. |

Unknown fields on `DomainState` and `Task` are rejected, so an older binary refuses a
newer file instead of dropping fields.

## Task

`DomainState::create(title, notes, scope, provenance, thread)` creates a ready task.
It trims and requires the title, mints `id` and `revision`, and appends `Created`.

| Field | Type | Semantics |
| --- | --- | --- |
| `id` | `Uuid` | Stable v4 identity. |
| `number` | `Option<u64>` | Absent until the locked persistence boundary allocates it. |
| `revision` | `Uuid` | Required opaque semantic revision. |
| `merge_base_revision` | `Option<Uuid>` | Transient save intent, cleared after a successful durable write. |
| `title` | `String` | Trimmed and non-empty. |
| `notes` | `Option<String>` | Free text, stored verbatim. |
| `thread` | `Option<String>` | Normalized name or absent. |
| `status` | `HumanStatus` | Human source of truth. |
| `scope` | `TaskScope` | Desk or project path. |
| `provenance` | `ProvenanceOrigin` | Creation source. |
| `history` | `Vec<TaskEvent>` | Append-only history. |
| `steps` | `Vec<Step>` | Flat ordered steps. Empty is omitted on write. |
| `soft_deleted` | `bool` | Hidden from live views when true. |
| `archived` | `bool` | Kept, off the radar: hidden from every working lens, visible only in the done drawer's archived group. Human status is independent of it. Omitted when false. |
| `created_at` / `updated_at` | `SystemTime` | `[secs, nanos]` since UNIX epoch. |

### Enums and nested records

`HumanStatus` is exactly `ready`, `started`, `blocked`, `review`, or `done`.
`TaskScope::Global` serializes as `"global"` and displays as desk.
`ProvenanceOrigin` is `manual`, `capture`, or `selection`.

`Step` has a stable `id`, trimmed `text`, and `done` flag. `TaskEvent` is
`{ kind, at }`; kinds are `created`, `edited`, `status_set`, `completed`,
`reopened`, `soft_deleted`, `restored`, `archived`, `unarchived`, `step_added`,
`step_checked`, `step_unchecked`, `step_renamed`, and `step_removed`.

`UndoEntry` is either `{ "soft_delete": { "id", "expected_revision" } }` or
`{ "complete": { "id", "expected_revision" } }`. `expected_revision` is required.

## Trash: `trash.jsonl`

A soft-deleted task that is no longer undoable (or has been deleted for 7 days)
leaves `tsk.json` for `trash.jsonl` at the next save. Each line is
`TrashLine { deleted_at, task }`. The file is rewritten atomically (never
appended), readers dedupe by task id with the last line winning and skip torn
lines, lines expire after 30 days, and the trash is durable before the live
document loses a task. Nothing on the board reads trash; `tsk list --deleted`
and `tsk trash restore` do.

## Invocation context

`HERDR_PLUGIN_CONTEXT_JSON` is parsed into `RawHostContext`. Only effective cwd and
selected text affect current behavior: cwd resolves project scope, selected text
prefills the title and selects `ProvenanceOrigin::Selection`. Host metadata is not
persisted on tasks. See [context](context.md).

## Other wire formats

The CLI's add/list/steps formats are described in [cli](cli.md). Queue sections,
collapse state, selection, peek, and forms are session-only `BoardModel` state.

# Data model

Every persisted and wire shape. Ownership, lifecycle, and field semantics. Verified
against `src/domain/task.rs`, `src/domain/events.rs`, `src/domain/undo.rs`,
`src/domain/dispatch_attempt.rs`, `src/domain/time_serde.rs`, `src/store.rs`,
`src/config.rs`, and `src/cli/{add,list,presenter,steps}.rs`.

There is no SQL schema and no migration runner. Compatibility is serde defaults,
aliases, and `format_version`.

## Store document — `tsk.json`

**Ownership.** `TaskStore` under `default_state_dir()` (`TSK_STATE_DIR` else `$HOME/.tsk`
else `.tsk-state`). One JSON document named `tsk.json`. Sibling files: `tsk.json.lock`
(lock, not JSON), `tsk.json.1` (previous successful live document), `.tsk.json.tmp.*`
(in-flight, swept under the lock).

**Lifecycle.** Missing file → empty `DomainState::new()`. Load holds the exclusive lock.
Save stamps `format_version` to `STORE_FORMAT_VERSION` (1) and clears
`merge_base_revision` on every task. A document whose `format_version` is **greater
than** 1 is refused (`StoreError::UnsupportedFormat`); it is not rewritten.

### `DomainState`

| Field | Type | Semantics |
| --- | --- | --- |
| `format_version` | `u32` | Writer's document version. Missing field deserializes as `LEGACY_STORE_FORMAT_VERSION` (**1**, must remain 1 if the current constant is later bumped). |
| `tasks` | `Vec<Task>` | All tasks including soft-deleted. Lookup is linear scan by id. |
| `active_attempts` | `Vec<DispatchAttempt>` | In-flight dispatch journals. Default empty. Removed on completion or full cleanup, not retained as history. v1 board does not create these. |
| `undo_stack` | `Vec<UndoEntry>` | LIFO. Only complete and soft-delete push. |

Verified against: `src/domain/task.rs` `DomainState`.

### `Task`

One unit of intended work. Created by domain `create` / `create_with_thread` (status
`ready`). Soft-deleted rows stay here until a future hard-delete that this tree does
not implement.

| Field | Type | Semantics |
| --- | --- | --- |
| `id` | `Uuid` | Stable identity (v4). CLI addresses the full UUID. |
| `revision` | `Option<Uuid>` | Opaque semantic revision. Missing on legacy tasks; assigned on next mutation. |
| `merge_base_revision` | `Option<Uuid>` | Transient save intent: revision observed before this in-memory mutation. Cleared after a successful durable write. Nil UUID means “legacy disk had no revision”. |
| `title` | `String` | Required, trimmed, non-empty. |
| `notes` | `Option<String>` | Free text. Stored verbatim (including CRLF). Markdown is a paint concern. |
| `thread` | `Option<String>` | Normalized name or absent (unthreaded). Old stores without the field decode unthreaded. |
| `status` | `HumanStatus` | Source of truth. |
| `scope` | `TaskScope` | Desk (`Global`) or a project path. |
| `capsule` | `Option<ContextCapsule>` | Frozen capture context. Missing fields stay `None`; never invented. |
| `agent_meta` | `Option<AgentMeta>` | Passive host identity. Empty meta is stored as `None`. Does not affect status. |
| `last_observed` | `Option<ObservedStatus>` | Last host observation, serde compatibility only. Absent if never observed. |
| `provenance` | `ProvenanceOrigin` | How it was created. |
| `history` | `Vec<TaskEvent>` | Append-only. |
| `steps` | `Vec<Step>` | Flat ordered list. Serde alias `checklist` on read; empty omitted on write. |
| `soft_deleted` | `bool` | Hidden from live views when true. |
| `created_at` / `updated_at` | `SystemTime` | Wire format: `[secs, nanos]` since UNIX_EPOCH (`time_serde`). |

### `HumanStatus`

```
ready | started | blocked | review | done
```

serde `snake_case`. `ready` aliases `todo`; `started` aliases `doing`. Old stores still
load. The board never writes the old names.

### `TaskScope`

```
"global" | { "project": { "path": "<string>" } }
```

Display name for `Global` is **desk**. Internal name stays `TaskScope::Global`. Path is
the resolved project root (string), not a repo id.

### `Step`

| Field | Type | Semantics |
| --- | --- | --- |
| `id` | `Uuid` | Stable. CLI `steps toggle` uses an unambiguous prefix of this id. |
| `text` | `String` | One line, trimmed at the domain boundary. |
| `done` | `bool` | Progress only. Never implies task status. |

### `ContextCapsule`

Optional strings: `repo_path`, `worktree_path`, `branch`, `cwd`, `source_pane_id`,
`selected_text`, `file`, and optional `line: u32` (1-based). All skipped when `None`.
`is_empty` is true when every field is absent. Capsule is a snapshot of the *invocation*,
not of the task's current `scope` — overriding scope at capture still stores the original
capsule.

### `AgentMeta` / `AgentSessionIdentity`

`agent_id`, `pane_id`, `agent_session: { source, value }`. Empty meta is omitted.
v1 board does not link or unlink agents (`TaskEventKind::AgentLinked` exists for old
history).

### `ObservedStatus`

`working | blocked | idle | done | unknown`. Retained. Board code does not apply it.

### `ProvenanceOrigin`

`manual | capture | selection`. Board quick-add and headless add use `capture`.
Selected-text invocation uses `selection`.

### `TaskEvent` / `TaskEventKind`

`{ kind, at }` with the same time tuple. Kinds:

`created`, `edited`, `status_set`, `completed`, `reopened`, `soft_deleted`, `restored`,
`parked`, `agent_linked`, `agent_unlinked`, `dispatched`, `step_added`, `step_checked`,
`step_unchecked`, `step_renamed`, `step_removed`.

Step kinds alias the old `checklist_item_*` names on read. Park / link / dispatch kinds
are historical; this tree does not append them.

### `UndoEntry`

```
{ "soft_delete": { "id", "expected_revision"? } }
{ "complete":    { "id", "expected_revision"? } }
```

Missing `expected_revision` is a legacy entry and fails `undo` as stale without popping.

### Dispatch-attempt journal (retained, unused by v1 board)

Still part of the document so older files load. Fields on `DispatchAttempt`: `id`,
`task_id`, `mode` (`here` \| `new_worktree`), `kind` (string), `steps`
(`create_worktree` / `open_pane` / `start_agent` / `send_prompt` with `completed`),
`owned_resources` (worktree / pane / agent receipts), `phase`
(`dispatching` \| `failed` \| `cleaning`), `last_error`, `revision`, `updated_at`.

Transitions are revision-guarded. Do not invent receipts from ambient paths.

**Relations.** `DispatchAttempt.task_id` → `Task.id`. A task may have at most one active
attempt. Attempts do not have a foreign-key cascade; removing a task is not implemented.

## Config directory: `walkthrough.json`

The config directory defaults to `$HOME/.tsk` and can be overridden with
`TSK_CONFIG_DIR`. It is separate from the task document's schema and revision.

### `walkthrough.json`

```json
{ "dismissed": true }
```

The only durable fact: the walkthrough was completed or skipped. Missing, empty,
malformed, unreadable, or `dismissed: false` → not dismissed. Field is
`#[serde(default)]` so siblings can be added without a migration.

## Headless wire formats

Not stored. Produced/consumed by [`cli`](cli.md).

### Flag add `--json` (one object)

```json
{ "outcome": "created" | "existing", "id": "<uuid>", "title": "<string>", "project": "<path>" | null }
```

`project` is null for desk.

### Plan add (stdin or `--file`)

Input: a JSON **array** of objects. Fields: `title` (required string), optional `notes`,
`project` (string, JSON null = desk, omitted = invocation default), `thread` (string or
null; missing/null = unthreaded).

Output:

```json
{
  "created":  [{ "i": 0, "id": "<uuid>", "title": "..." }],
  "existing": [{ "i": 1, "id": "<uuid>", "title": "..." }],
  "failed":   [{ "i": 2, "title": "..." | null, "code": "empty-title", "error": "..." }]
}
```

`i` is the input index. Notes are not echoed. Error `code` is the machine contract;
`error` is not.

Stable refusal codes: `empty-title`, `invalid-title`, `invalid-item`, plus `store-error`
on I/O.

### `list --json`

A JSON array of `{ id, title, status, project, thread }`. `project` / `thread` are
`null` when absent. Single-task listing (`tsk list <uuid>`) attaches `steps`:
`[{ id, done, short_id, text }, ...]` in stored order on that one row. Other listings
keep the row shape without `steps`.

### herdr pane list (stdin to `--find-board-pane`)

Expected: `{ "result": { "panes": [ { "pane_id", "label", ... } ] } }`. Match
`label == "tsk"`. First flag-safe `pane_id` wins. Invalid JSON → no match (the binary
exits 1 for none, stderr only on I/O / parse errors of stdin read — unparseable JSON
is `Ok(None)` from `find_board_pane_id`).

### `HERDR_PLUGIN_CONTEXT_JSON`

See [context](context.md). Unknown keys ignored. Missing/malformed → empty
`RawHostContext` (no invented fields).

## What is not a data model

- Queue sections, thread blocks, collapse sets, selection, peek, forms: session-only
  (`BoardModel`). Collapse does not persist.
- Search: in-memory filter, no index.
- Site content and the web demo: not the store.

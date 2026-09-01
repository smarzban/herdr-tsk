# Domain

**Responsibility.** Task lifecycle and the in-memory document: create, human-status
verbs, edit, soft-delete/restore, undo, steps, thread normalization, and (retained)
dispatch-attempt journaling. Persistence is [store](store.md). Presentation is
[board UI](board-ui.md) / [CLI](cli.md). This layer does not talk to the filesystem
or the terminal.

**Public surface.** `src/domain/` re-exports task, events, thread, undo, and
dispatch_attempt. `time_serde` is private. Callers typically hold a `DomainState` and
pass `&mut` into `create`, `set_status`, `complete`, `reopen`, `soft_delete`,
`restore`, `edit`, `undo`, `add_step` / `toggle_step` / `rename_step` / `remove_step`.
Signatures: generated [rustdoc](reference.md).

## How it works

`DomainState` is a `Vec<Task>` plus `undo_stack` and `active_attempts`. Lookup is a
scan by id (`SHORTCUT` in source: fine until the store is large). Soft-deleted tasks
remain gettable.

Create trims the title, assigns v4 `id` and `revision`, status `ready`, and a single
`Created` event. `create_with_thread` is the path that can set an already-normalized
thread in that same mutation; `create` is the unthreaded compatibility wrapper.

Mutations other than create go through `record_mutation`: set `merge_base_revision`
to the previous revision (or nil for legacy), mint a new v4 revision, stamp
`updated_at`, append history. That merge base is what [store](store.md) uses to
detect a concurrent writer on the same task.

Status verbs are explicit. `complete` / `soft_delete` also push `UndoEntry`. `undo`
inspects the top entry's `expected_revision`, refuses stale/legacy without popping,
then `restore` or `reopen`. Empty stack is `Ok(())`.

Steps are appended, toggled, renamed, or removed. None of those commands touch
`status`, `scope`, or `notes`. Old event names `checklist_item_*` still deserialize.

`normalize_thread` is a pure function (see [data model](data-model.md)). The domain
does not call it on `edit` — callers (board reducer, CLI parser, capture tokens)
normalize first and pass `Option<String>`.

Dispatch attempts are a durable journal with revision-guarded transitions and owned
resource receipts. v1 board never starts one. Merge rules still matter: disk owns
attempt *existence*, so a local copy must not resurrect an attempt disk already
removed.

Idle merge (`merge_tasks_from_disk`) is for “another process wrote the file”: on
revision mismatch, disk wins. There is no mutation baseline, so wall-clock is not
used. Save merge (`merge_for_save`) is the opposite situation: this process has
uncommitted mutations that must apply only if disk still has the revision they
started from.

## Invariants

Owned subset of [invariants](invariants.md) §§1–12, plus:

- `get` finds soft-deleted tasks; views filter them.
- `active_attempt_for_task` is unique.
- History is append-only; do not rewrite past events.
- Do not auto-complete from `ObservedStatus` or from `steps` all-done.

## Error paths

`DomainError`: `EmptyTitle`, `UnknownId`, `SoftDeleted`, `StaleUndo`,
`UnknownDispatchAttempt`, `EmptyStepText`, `UnknownStep`, `ActiveDispatchAttempt`,
wrapped `DispatchAttempt`. CLI maps a subset to stable codes; the board paints
`Display` on the chrome row (stale-undo is reordered reason-first so a clipped row
keeps the meaning).

## Extension points

New human-status values need serde aliases if old names exist, queue `status_rank`,
glyphs in `render::status_glyph`, CLI list groups, and site/demo updates. New
event kinds need a `TaskEventKind` variant. Bumping `STORE_FORMAT_VERSION` is a
store concern: keep `LEGACY_STORE_FORMAT_VERSION` at 1.

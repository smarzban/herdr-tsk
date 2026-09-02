# Domain

**Responsibility.** Task lifecycle and the in-memory document: create, human-status
verbs, edit, soft-delete/restore, undo, steps, and threads. Persistence is
[store](store.md). This layer does not access the filesystem or terminal.

**Public surface.** `src/domain/` re-exports task, events, thread, and undo.
Callers use `DomainState::create`, `set_status`, `complete`, `reopen`,
`soft_delete`, `restore`, `edit`, `undo`, and the step commands.

## How it works

`DomainState` is a `Vec<Task>` plus an undo stack. Lookup scans by id; soft-deleted
tasks remain findable. `create(title, notes, scope, provenance, thread)` trims and
requires the title, creates a v4 id and revision, marks the task ready, and records
`Created`.

Every later semantic mutation records its previous required revision as
`merge_base_revision`, mints a new revision, updates time, and appends history. The
store uses that base to reject concurrent edits to the same task without wall-clock
arbitration.

`complete` and `soft_delete` push a required revision-guarded `UndoEntry`. `undo`
keeps a stale entry in place and returns `StaleUndo`; an empty stack succeeds.

Steps are appended, toggled, renamed, or removed. They never alter status, scope, or
notes. Thread normalization belongs at the board, CLI, or capture boundary before the
domain receives the optional thread.

Idle merge is disk-wins on revision mismatch. Save merge accepts a local mutation only
when disk still holds the required base revision.

## Invariants

- `get` finds soft-deleted tasks while views filter them.
- History is append-only.
- Human status never follows step completion.
- Revisions and undo expected revisions are required UUIDs.

## Error paths

`DomainError` covers empty titles, unknown or soft-deleted tasks, stale undo, empty
step text, and unknown steps. The board and CLI map these to their surface-specific
messages.

# Capture pipeline

**Responsibility.** Turn an `InvocationSnapshot` plus user fields into one domain
create, optionally persisting. Used by Capture UI and conceptually by board
quick-add (which calls domain create itself after token parse). Not the TUI form —
that is [capture UI](capture-ui.md).

**Public surface.** `capture_save`, `CaptureError`, `TaskId` (alias of `Uuid`).

## How it works

```
capture_save(state, store, snapshot, title, notes, scope_override, thread)
```

- Scope: `scope_override` if `Some`, else `snapshot.default_scope`.
- Provenance always comes from the snapshot.
- `thread` is already normalized by the caller.
- Create goes through `DomainState::create`. Empty title →
  `CaptureError::Domain(EmptyTitle)`, no task.
- If `store` is `Some`, `reload_merge_save` so a concurrent board writer is merged
  rather than clobbered.

Board quick-add parses `!p`/`!t` in the reducer, then calls `capture_save` with
`store: None` so provenance still comes from the snapshot. Persistence is
`IntentOutcome::Persist` through the board save-recovery path — the draft is not
discarded until that boundary confirms. The expanded Tab form uses `capture_save`
the same way. Do not add a third create helper that skips the snapshot.

## Invariants

- Domain is the only creator; this module does not write JSON itself except via
  `TaskStore`.
- Title trim/empty rules live in domain, not here.
- Optional persist uses merge-save, not blind `save`.

## Error paths

`CaptureError::Domain` / `Store`. Capture UI maps domain empty-title to the
on-form `TITLE_REQUIRED_MESSAGE` and store errors into capture save-recovery.

## Extension points

New capture behavior belongs on the snapshot or as explicit domain create arguments.
Do not persist UI-only drafts through this function.

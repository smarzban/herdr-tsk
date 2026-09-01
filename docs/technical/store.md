# Store

**Responsibility.** Durable `DomainState` as pretty-printed JSON in one directory.
Atomic replace, exclusive inter-process lock, format-version gate, last-good backup.
Does not interpret human status.

**Public surface.** `TaskStore`, `TaskStateStore`, `StoreError`, `default_state_dir`.
The trait exists so dispatch-era tests could inject save failures; production is
`TaskStore`.

## How it works

`TaskStore::new` takes the **directory**, not the file. Live document: `tsk.json`.
Lock: `tsk.json.lock` (`OpenOptions` create/read/write, `File::lock()`, unlock on
`Drop`). Backup: `tsk.json.1`.

`default_state_dir`: non-empty `TSK_STATE_DIR`, else non-empty `$HOME/.tsk`, else
`.tsk-state`. Empty strings fall through. `HERDR_PLUGIN_STATE_DIR` is not read.

**Load.** Lock, sweep orphan `.tsk.json.tmp.*`, missing file → `DomainState::new()`,
else peek `format_version` (missing → 1), refuse if `found > STORE_FORMAT_VERSION`,
then serde.

**Save.** Takes the exclusive lock, clones, `clear_merge_bases`,
`stamp_format_version`, then writes while that lock is still held
(`save_unlocked` is the helper that assumes the caller already locked). Prefer
`reload_merge_save` when another process may have written since this snapshot
was loaded (board + capture).

**`reload_merge_save`.** Lock, load disk, `check_format_version`, `merge_for_save`
(local mutations kept only if disk still has their merge base; siblings merged in;
attempts that vanished on disk are dropped), write a clone with bases cleared, then
clear bases on the caller's state. On any write-stage failure the caller's merge
bases stay set so [save recovery](app.md#save-recovery) retries the same intent.

**`locked_transition` / `locked_transition_if_changed`.** Load → closure → optional
save under one lock. CLI add/steps use `if_changed` so an idempotent existing match
does not rewrite the file.

**Atomic write.** Unique temp `.tsk.json.tmp.{pid}.{nanos}` in the same directory
(so rename is atomic on the same filesystem). Write, `sync_file`, rename over
`tsk.json`, `sync_directory` (Linux/macOS; other OS: no-op). Failure unlinks the
temp. Before replacing a *valid* live file, `retain_last_good` hard-links it to
`tsk.json.1` (remove old backup first). A live file that fails JSON parse is
replaced without touching the backup — a corrupt live document must not clobber
last-good.

`AtomicFilesystem` is a private trait so tests can fail create/write/sync/rename
independently. Production is `StdFilesystem`.

## Invariants

[Invariants](invariants.md) §§12–16. Additional:

- Sweep temps only under the exclusive lock (a concurrent writer's in-flight temp
  must not be deleted).
- Never fall back to `std::env::temp_dir()` for the state directory.
- Peek format version before full serde so an unsupported newer document is a typed
  error, not a confusing serde failure.

## Error paths

`StoreError::Io`, `StoreError::Json`, `StoreError::UnsupportedFormat { found, supported }`.
CLI maps store errors to exit 3. Board save maps them into `SaveRecovery`.

## Extension points

A format bump: increment `STORE_FORMAT_VERSION`, keep `LEGACY_STORE_FORMAT_VERSION`
at 1, teach serde defaults/aliases for new fields, never rewrite a newer file. A
second document (index, attachments) is a new filename beside `tsk.json`, not a
silent new meaning for this one.

# Store

**Responsibility.** Durable `DomainState` as pretty-printed JSON in one directory.
It owns atomic replacement, an exclusive inter-process lock, a strict format guard,
and a last-good backup. It does not interpret human status.

**Public surface.** `TaskStore`, `StoreError`, and `default_state_dir`.

## How it works

`TaskStore::new` takes a directory. The live document is `tsk.json`; the lock is
`tsk.json.lock`; the previous successful document is `tsk.json.1`.

`default_state_dir` chooses non-empty `TSK_STATE_DIR`, then non-empty `$HOME/.tsk`,
then `.tsk-state`. It ignores `HERDR_PLUGIN_STATE_DIR`.

**Load.** Under the lock, orphan temp files are swept. A missing document returns
`DomainState::new()`. A present document must contain `format_version: 1`. Missing,
older, and newer versions return `StoreError::UnsupportedFormat` without any write.
Only then does serde deserialize the strict current schema.

**Save.** A state whose format is not 1 is refused. Under the lock, task numbers are
allocated and merge bases are cleared before atomic replacement. `reload_merge_save` reloads
disk under that same lock and accepts a local mutation only when its task revision
base still matches disk. On a write-stage failure, the caller keeps merge bases for
save recovery retry.

`locked_transition` and `locked_transition_if_changed` hold load, transition, and
optional save under one lock. CLI add and steps use the latter for idempotent work.

**Atomic write.** A unique `.tsk.json.tmp.{pid}.{nanos}` file is created in the same
directory, written and synced, renamed over `tsk.json`, then the directory is synced
on Linux and macOS. A valid live document is hard-linked to `tsk.json.1` before
replacement. Corrupt JSON is replaced without changing an existing backup.

**Permissions.** On Unix every file is owner-only: temp files are created `0600`, the
state directory is `0700`, and a load or save tightens any state file already on disk
(`tsk.json`, `tsk.json.1`, `tsk.json.v<N>`, `trash.jsonl`, the lock, leftover temps)
that an older version or looser umask left readable. Tightening only strips bits
(`fsperm`); a stricter existing mode is kept. Other platforms claim no mode.

## Invariants

- Format version is exactly 1, never upgraded automatically.
- A refused document is never rewritten.
- Temp sweeping runs only under the exclusive lock.
- The state directory never falls back to a shared temporary path.
- State files are owner-only on Unix; tightening strips bits and never grants them.

## Error paths

`StoreError` is I/O, JSON, or unsupported format. CLI maps store errors to exit 3;
board save maps them into save recovery.

## Extension points

A future incompatible schema must use a new format version and an explicit migration
command or tool. Do not add automatic upgrades or read aliases to the runtime store.

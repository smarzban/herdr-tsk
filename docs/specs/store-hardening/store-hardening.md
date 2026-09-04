# Store hardening (spec A)

Pre-release hardening of the on-disk store. No UI change. The single-JSON store stays; this
adds a forward path for the schema, a trash file for deleted tasks, bounded undo, a reliable
change signature, and a docs note. A later spec (B, archive) rides the migration hook added here.

Branch: `feat/store-hardening`, this worktree. One commit per task below, conventional prefix
(`feat:` / `fix:` / `test:` / `docs:`). Do not push. `HANDOFF.md` is out of scope; do not create
or edit one. Do not touch `~/.tsk`; every test and smoke uses `TSK_STATE_DIR` under `/tmp`.

Read `AGENTS.md` first. Rules that bite here: a regression test must fail without its fix (revert
the hunk by hand, never `git checkout <file>`); temp state dirs need a per-binary atomic counter;
`clippy::if_same_then_else` is on; green bar is
`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`.

Existing files are **not** converted. The owner is still testing and regenerates their store.
`format_version` 0 (missing) and any version above the supported one are refused as today.

## Files touched (expected)

- `src/store.rs`: migration chain, trash append/purge, signature helper.
- `src/domain/task.rs`: undo cap and pruning, trash eligibility, merge rules for trashed tasks.
- `src/domain/undo.rs`: `UNDO_CAP`.
- `src/app.rs`: `store_state_signature` uses the new helper.
- `src/cli/list.rs`, `src/cli/router.rs`, `src/cli/parser.rs`, new `src/cli/trash.rs`.
- `README.md`, `site/src/content/docs/docs/install.md`, `site/src/content/docs/docs/cli.md`,
  `skills/tsk-cli/SKILL.md`.
- Tests: `src/store.rs` unit tests, `tests/` integration where the CLI is exercised.

## T1 · Migration hook

**Model.** `STORE_FORMAT_VERSION` stays 1. `store.rs` gains
`fn migrate(document: serde_json::Value, from: u32) -> Result<serde_json::Value, StoreError>`
that walks a chain `MIGRATIONS: &[fn(Value) -> Result<Value, StoreError>]` where index `i`
converts version `i + 1` to `i + 2`. The chain is empty at v1. A private
`migrate_with(document, from, steps)` takes the chain as a parameter so tests can inject a fake
step without shipping one.

**Load path** (`load_unlocked`):

- `found == STORE_FORMAT_VERSION`: as today.
- `found > STORE_FORMAT_VERSION`: `UnsupportedFormat`, as today. Old binaries never rewrite newer
  files; `deny_unknown_fields` stays on `Task` and `DomainState`.
- `0 < found < STORE_FORMAT_VERSION`: run the chain, deserialize, return the migrated state
  **in memory**. The live file is not rewritten by load.
- `found == 0`: `UnsupportedFormat`, as today (pre-versioned files are not supported).

**First write after a migrated load.** Before `retain_last_good` in `save_unlocked_with`, if the
live file's peeked version is below `STORE_FORMAT_VERSION`, copy it to `tsk.json.v<found>` in
the state dir. Never overwrite an existing `tsk.json.v<N>`; the first backup of a given version
is the one that matters. Then replace as today. `tsk.json.1` semantics are unchanged.

**AC.**

- AC-1 A v1 document loads and saves exactly as before (existing tests stay green, byte-identical
  output for an unchanged state).
- AC-2 With an injected step `v1 → v2` and `STORE_FORMAT_VERSION` treated as 2 (test-only
  parameter, do not bump the constant), a v1 file loads as the migrated state and the first save
  leaves `tsk.json.v1` byte-identical to the original and `tsk.json` at v2. A second save does
  not touch `tsk.json.v1`.
- AC-3 A file with version above the supported one is refused with `UnsupportedFormat` and no
  file in the state dir changes (compare directory listing and bytes before and after).
- AC-4 A migration step returning `Err` surfaces as a `StoreError` from `load`, and no file
  changes.

## T2 · Store signature

**Model.** `store_state_signature` in `src/app.rs` moves to `store.rs` as
`pub fn state_signature(&self) -> Option<StoreSignature>`. On Unix, `StoreSignature` is
`(dev: u64, ino: u64, modified: SystemTime, len: u64)` via `std::os::unix::fs::MetadataExt`.
Elsewhere it is `(modified, len)`. Every save renames a fresh temp file over the live one, so
the inode changes on every replace; two saves inside one mtime tick with equal length are no
longer indistinguishable. `StoreWatch` stores the new type. No behaviour change otherwise.

**AC.**

- AC-5 Two consecutive saves whose documents have identical byte length, with the second file's
  mtime forced equal to the first via `File::set_modified`, produce different signatures on
  Unix. The test must fail with the old `(mtime, len)` signature.
- AC-6 A missing live file yields `None`; an unchanged file yields an equal signature across two
  reads.

## T3 · Undo cap and pruning

**Model.** `UNDO_CAP = 50` in `undo.rs`. At the locked persistence boundary (the same place
`assign_numbers_for_persistence` runs, so every save path and `locked_transition` gets it):

1. Drop undo entries whose target task is missing, or whose `expected_revision` no longer
   matches the task's current revision (stale, can never succeed).
2. Truncate the oldest entries until `len <= UNDO_CAP`.

Order matters: prune stale first so a cap never evicts a live entry to keep a dead one.
`merge_undo_entries` still unions, the cap runs after it. `undo()` behaviour on a stale top entry
is unchanged in memory (refuse, retain) because pruning happens only at save.

**AC.**

- AC-7 51 undoable actions then a save leaves exactly 50 entries, the oldest gone.
- AC-8 An entry whose task was edited after the undoable action (revision moved on) is absent
  from the saved document; a live entry beneath it is kept.
- AC-9 Existing undo tests stay green.

## T4 · Trash file

**Model.** `trash.jsonl` in the state dir, one JSON object per line, append-only, written only
under the store lock. Line shape:

```json
{"deleted_at": <time_serde value>, "task": <Task as in tsk.json>}
```

`deleted_at` is the `at` of the task's last `soft_deleted` history event.

**Eligibility** (computed at the locked persistence boundary, after undo pruning): a task with
`soft_deleted == true` moves to trash when either

- no undo entry on the stack targets it (undo can no longer reach it; note the LIFO stack means
  any later undoable action already makes it unreachable, and T3's pruning removes it once the
  stack moves), or
- `deleted_at` is more than 7 days before now.

**Move order** under the lock: append every eligible task to `trash.jsonl` and `sync_all` the
trash file, then remove those tasks from the live state and drop any undo entry targeting them,
then replace the live file as today. A crash between the two steps leaves a task in both places;
readers dedupe by task id with the live copy winning. Never the reverse order.

**Purge.** On every trash append, also rewrite `trash.jsonl` without lines whose `deleted_at` is
more than 30 days before now. Rewrite is temp file in the same dir, `sync_all`, rename, dir sync,
the same durability as the live file. Lines that fail to parse (a torn tail after a crash) are
dropped on rewrite and skipped on read. If nothing is eligible to append and nothing is due to
purge, do not touch the file.

**Merge rules** (`src/domain/task.rs`), needed because a second process may have trashed a task
this process still holds:

- `merge_tasks_from_disk`: a local task absent from disk that is `soft_deleted` with
  `merge_base_revision == None` is removed from local state.
- `merge_for_save`: same rule. A local task absent from disk that is not soft-deleted, or that
  carries a `merge_base_revision`, is kept as today (it is a local creation or mutation).

**Readers.**

- `tsk list --deleted` shows live soft-deleted tasks and trash entries, deduped by id, ordered
  by `deleted_at` descending. Trash entries keep their `T<n>` number. Existing output format for
  the live entries is unchanged; trash entries render the same way.
- New `tsk trash restore T<n>`: under `locked_transition`, find the line by number, remove it
  from trash (rewrite), insert the task into live state with `soft_deleted = false`, a
  `restored` history event, new revision, `updated_at = now`. If the id already exists live,
  refuse with "T<n> is not in trash". Unknown number refuses the same way. Exit codes follow the
  existing CLI convention in `src/cli/router.rs`.
- Nothing on the board reads trash. `sync_from_domain` and every lens already filter
  `soft_deleted`, so the only visible change is a task disappearing from `tsk list --deleted`
  after 30 days.

**AC.**

- AC-10 Soft-delete a task, complete another task (so the delete is no longer the top undo
  entry), save: the task is in `trash.jsonl` with the right `deleted_at`, absent from
  `tsk.json`, and no undo entry references it.
- AC-11 Soft-delete a task and save immediately: it stays in `tsk.json` (top undo entry), and
  `undo` still restores it.
- AC-12 A soft-deleted task whose `deleted_at` is 8 days old moves to trash on save even while
  it is the top undo entry.
- AC-13 A trash line with `deleted_at` 31 days old is gone after the next append; a 29-day line
  stays. A malformed line is dropped by the rewrite and skipped by `tsk list --deleted`.
- AC-14 Process A holds a soft-deleted task in memory; process B (a second `TaskStore` on the
  same dir) trashes it; A's next `reload_merge_save` does not resurrect it in `tsk.json` and
  does not append a second trash line.
- AC-15 `tsk list --deleted` lists a trashed task with its number; `tsk trash restore T<n>`
  brings it back as `ready`, not soft-deleted, with a `restored` event, and the line is gone from
  `trash.jsonl`. Restoring an unknown number or a live number is refused with a message and a
  non-zero exit.
- AC-16 Injected failure on the trash `sync_all` (use the `AtomicFilesystem` seam or a sibling
  seam for the trash path) leaves `tsk.json` unchanged and the task still live.

## T5 · Docs

- `README.md` and `site/src/content/docs/docs/install.md`: `~/.tsk` must be on a local disk.
  The lock is `flock`-style and the replace is rename-based; NFS, Dropbox, iCloud Drive and
  similar synced folders can break both. `TSK_STATE_DIR` is the escape hatch. Mention
  `trash.jsonl` beside `tsk.json`, `tsk.json.1` and `tsk.json.v<N>`. Windows is not tested in
  CI.
- `site/src/content/docs/docs/cli.md` and `skills/tsk-cli/SKILL.md`: `tsk trash restore T<n>`,
  and that `tsk list --deleted` includes trash for 30 days.
- `CHANGELOG.md`: one entry per user-visible item (trash, restore, undo cap, synced-folder
  note). Version bump is not part of this spec.

## Done means

- Green bar passes in this worktree.
- `cd site && npm test` passes (docs pages parse).
- Every AC above maps to a named test in a `verification-report.md` beside this file, and every
  regression test was watched failing without its fix (note the commit that proves it).
- `build-report.md` beside this file holds the task ledger: task, commit, ACs, notes.
- Live smoke, `HERDR_ENV=1`: with `TSK_STATE_DIR=/tmp/tsk-store-hardening`, run the built
  `./target/release/tsk` board in a Herdr pane, create three tasks, `ctrl+x` one, `ctrl+d`
  another, quit; confirm `trash.jsonl` has one line and `tsk list --deleted` shows it; restore it
  with the CLI and confirm it paints on the board. Record what was seen in the build report.

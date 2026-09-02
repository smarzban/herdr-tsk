# Config

**Responsibility.** One small walkthrough-dismissal document beside the store. It is
not the task document and deliberately has weaker durability than the
[store](store.md).

**Public surface.** `default_config_dir`, `WalkthroughRecord`.

## How it works

`default_config_dir` mirrors the store's precedence with `TSK_CONFIG_DIR` /
`$HOME/.tsk` / `.tsk-config`, and also ignores `HERDR_PLUGIN_CONFIG_DIR`. Empty env
values fall through: an empty `TSK_CONFIG_DIR` must not resolve to `""`, which would
write `walkthrough.json` in the board's launch directory. Resolution is split into a
pure `resolve_config_dir` so tests do not mutate process-wide environment variables.

### Walkthrough

`walkthrough.json`: `{ "dismissed": true }`. `is_dismissed` is true only when the
file parses and `dismissed` is true. Every other failure mode is not dismissed.

`record_dismissed` writes `dismissed: true` via temp+rename+file sync. It does **not**
lock and does **not** fsync the directory. A crash can lose a just-recorded dismissal,
so the card may reappear once. The payload is a constant, making concurrent writers
harmless.

`run_board` does not call `open_walkthrough_for_launch`. That helper remains for tests:
if the record is not dismissed it dispatches `BoardIntent::OpenWalkthrough`, the same
intent the palette's replay uses. Dismissal write is `record_walkthrough_dismissal`,
presented but never propagated. `Unpresentable` or `Interrupted` write nothing.

## Invariants

[Invariants](invariants.md) §§17, 22, 30.

- Do not reuse this write path for a varying payload, which needs a lock.
- Do not propagate a walkthrough write failure from a board key handler, which would
  tear the board down on a read-only config directory.

## Error paths

`StoreError` is the shared I/O/JSON type. Walkthrough reads have no error channel by
construction.

## Extension points

`WalkthroughDocument` uses `#[serde(default)]` on fields so a sibling key can be added
without a migration. A new setting that must not be lost on crash needs the store's
lock and directory fsync, or its own document with those guarantees.

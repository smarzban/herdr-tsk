# Config

**Responsibility.** Two small documents beside the store: verb-modifier settings and
walkthrough dismissal. Not the task document. Deliberately weaker durability than
[store](store.md).

**Public surface.** `default_config_dir`, `WalkthroughRecord`, `SettingsRecord`,
`VerbModifier`.

## How it works

`default_config_dir` mirrors the store's precedence with `TSK_CONFIG_DIR` /
`$HOME/.tsk` / `.tsk-config`, and also ignores `HERDR_PLUGIN_CONFIG_DIR`. Empty env
values fall through — an empty `TSK_CONFIG_DIR` must not resolve to `""` (that would
write `walkthrough.json` in whatever cwd the board was launched from). Resolution is
split into a pure `resolve_config_dir` so tests do not `setenv` (this crate's tests
run in threads in one process).

### Settings

`settings.json`: `{ "verb_modifier": "ctrl" }`. Ctrl is the fixed Ctrl modifier.
Missing, unreadable, or malformed files default to Ctrl. Legacy
`{ "verb_modifier": "alt" }` settings deserialize as Ctrl. `prefix()` always returns
`"ctrl+"` for help and verb-bar labels.

### Walkthrough

`walkthrough.json`: `{ "dismissed": true }`. `is_dismissed` is true only when the
file parses and `dismissed` is true. Every other failure mode is not dismissed.

`record_dismissed` writes `dismissed: true` via temp+rename+file sync. It does **not**
lock and does **not** fsync the directory. The module comment is the spec: a crash can
lose a just-recorded dismissal (card reappears once); the payload is a constant so two
writers are harmless.

`run_board` does not call `open_walkthrough_for_launch`. That helper remains for tests:
if the record is not dismissed it dispatches `BoardIntent::OpenWalkthrough` — the same
intent the palette's replay uses. Dismissal write is `record_walkthrough_dismissal` —
**presented, never propagated**. One close → one write attempt; `Unpresentable` /
`Interrupted` write nothing.

## Invariants

[Invariants](invariants.md) §§17, 22, 30.

- Do not reuse this write path for a varying payload (you need a lock).
- Do not `?` a walkthrough write out of a board key handler (that tears the board
  down on a read-only config dir).

## Error paths

`StoreError` (shared I/O/JSON type). Settings/walkthrough reads have no error
channel by construction.

## Extension points

`WalkthroughDocument` / `SettingsDocument` use `#[serde(default)]` on fields so a
sibling key can be added without a migration. A new setting that must not be lost
on crash needs the store's lock + directory fsync, or its own document with those
guarantees — not a copy of this module.

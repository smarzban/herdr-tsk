---
name: verify-tsk
description: Drive tsk (terminal task board) as a user would — isolated TUI board plus tsk add/list/steps CLI. Use when proving a board, capture, status-verb, task-page, or CLI change against the real binary, not unit tests.
---

# Verify tsk

tsk is a terminal task board. The user-facing product is the `tsk` binary: an interactive board (default argv) and headless `add` / `list` / `status` / `edit` / `steps`. This skill drives those surfaces against a disposable store. It does not drive `~/.tsk`, a herdr pane, or the marketing site in `site/`.

Helpers live at `.cursor/skills/verify-tsk/scripts/control-tsk`. Every command below is run from the repo root. Feature recipes are in `features/`.

## Launch

Build once per checkout if `target/release/tsk` is missing or stale:

```bash
cargo build --release
```

Once per checkout, create the skill venv so board dumps decode cursor-addressed paint (same `pyte` pin as `scripts/parity-requirements.txt`):

```bash
python3 -m venv .cursor/skills/verify-tsk/.venv
.cursor/skills/verify-tsk/.venv/bin/pip install -r scripts/parity-requirements.txt
```

Allocate an isolated run (state, config, non-git cwd, evidence dir). This is required before doctor, CLI, or the board:

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk init
./.cursor/skills/verify-tsk/scripts/control-tsk doctor
```

`init` writes `.cursor/skills/verify-tsk/.run` and prints JSON with `run_id`, `state_dir`, `config_dir`, `cwd`, `evidence_dir`, and `binary`. The store is `$state_dir/tsk.json`. Config holds a dismissed `walkthrough.json` so the board opens without the first-run card. The board's cwd is an empty non-git directory, so capture defaults to the desk rather than this repo.

Ready for the TUI: home tabs paint `desk` · project · `projects`. The first full board open seeds four notice tasks (`N` ids) plus a possible `What's new` notice. They paint on the desk under NEEDS YOU / IN MOTION / ON DECK · desk, but `tsk list` never shows them. Selection starts on the Welcome notice (review). Empty-desk copy (`no open tasks here`) appears only after those notices are gone, or on a project board with no tasks. Standard size is 80×24. The harness cwd basename lands in slot 2 (not `select project`).

Start or stop the board PTY only when the claim is about the TUI:

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk board start
./.cursor/skills/verify-tsk/scripts/control-tsk board quit
```

Teardown the run (keeps evidence, removes scratch state and the board process):

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk cleanup
```

## Doctor

Run whenever anything looks off:

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk doctor
```

Doctor is read-only. It requires the release binary, real (non-symlink) state/config/cwd dirs, a dismissed walkthrough file, and a state dir that is not `~/.tsk`. Exit 0 means drive. Exit 2 means refuse.

## Drive

Prefer the CLI for store mutations. Prefer the board PTY for paint, keys, and peek/page flows. Never call internal Rust setters or edit `tsk.json` by hand and call that a product proof.

CLI (injects `--state-dir` for mutating/list commands):

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk cli -- add -t "verify title" --desk --json
./.cursor/skills/verify-tsk/scripts/control-tsk cli -- list --desk --json
./.cursor/skills/verify-tsk/scripts/control-tsk cli -- status T1 started
./.cursor/skills/verify-tsk/scripts/control-tsk cli -- steps 1 add "first step"
./.cursor/skills/verify-tsk/scripts/control-tsk store
```

Board keys use named chords from the product keymap (mutating verbs are Ctrl):

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk board keys down
./.cursor/skills/verify-tsk/scripts/control-tsk board keys j
./.cursor/skills/verify-tsk/scripts/control-tsk board keys ctrl-s
./.cursor/skills/verify-tsk/scripts/control-tsk board wait "IN MOTION"
./.cursor/skills/verify-tsk/scripts/control-tsk board dump
./.cursor/skills/verify-tsk/scripts/control-tsk board keys enter
./.cursor/skills/verify-tsk/scripts/control-tsk board keys 'text:Verify quick add'
./.cursor/skills/verify-tsk/scripts/control-tsk board keys ctrl-q
```

Named keys include `enter`, `esc`, `tab`, `up`/`down`/`left`/`right`, `ctrl-s`/`ctrl-d`/`ctrl-o`/`ctrl-b`/`ctrl-r`/`ctrl-e`/`ctrl-n`/`ctrl-x`/`ctrl-u`/`ctrl-f`/`ctrl-q`, single characters, and `text:<string>`. Quote `text:` when the string has spaces (`board keys 'text:Verify quick add'`); otherwise argv splits it and later words become unknown keys. Confirm the `▸` marker sits on the intended row before a mutating chord — first open selects Welcome, and a starter already paints `IN MOTION`, so `wait "IN MOTION"` is not start proof.

Read `features/README.md`, then the matching feature file, before claiming a feature verified. A proof that takes one convenient entry point is incomplete when the map lists others.

## Evidence

Proof artifacts land under `.cursor/skills/verify-tsk/evidence/<run_id>/` and survive `cleanup`.

Standards:

- Exercise the real user path (CLI argv or board keys), not test-only hooks.
- Capture the action and the resulting state. CLI runs write `cli-*.json` (command, stdout, stderr, exit). Board dumps write `board-screen.txt` plus `.ansi`.
- Confirm side effects with `control-tsk store` or a second `list --json` after a mutation.
- Never treat unit-test `TestBackend` goldens alone as product verification for a user-facing change this skill covers.

## Cleanup

```bash
./.cursor/skills/verify-tsk/scripts/control-tsk cleanup
```

Cleanup stops the board and proxy this run started (by recorded pids), deletes the scratch tree under `/tmp`, and removes `.run`. It never deletes `evidence/<run_id>/`. Do not `pkill tsk`.

## Helpers

| Command | Purpose |
| --- | --- |
| `control-tsk init` | Create isolated state/config/cwd + evidence dir; write `.run` |
| `control-tsk doctor` | Read-only health check |
| `control-tsk cli -- …` | Run `target/release/tsk` with the run's env and `--state-dir` |
| `control-tsk board start\|keys\|wait\|dump\|quit` | PTY board lifecycle |
| `control-tsk store` | Print `$state_dir/tsk.json` |
| `control-tsk cleanup` | Tear down instance; keep evidence |

Override the binary with `TSK_BIN=/path/to/tsk` when needed. Live herdr pane smoke is out of scope here; when `HERDR_ENV` is unset, say live herdr smoke was not run.

# Build report: archive

Date started: 2026-09-04
Branch: `feat/archive`
Base: `main` at `ed68cda`
Worktree: `/Users/saeed/.herdr/worktrees/herdr-tasks/feat-archive` (herdr worktree, workspace `archive`, inherited by build)
Tier: full spec, but per owner instruction after T-2 the per-task reviewer was dropped in favour
of a continuous run (T-3..T-13, then live smoke) by one implementer (pi, `zai/glm-5.3-flash`,
Herdr pane `w1F:p1`), with the whole-change review by Review panel at PR time (as spec A).
Conductor: claude; verifies each task's green bar from its own run and records it here.
Gate: `gate-report.md` ready to build at `ea63a4b`; F-1 resolved (no `Enter` on the launch card).

## Baseline

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 764 passed, 0 failed (sum over test binaries)
cargo build --release: finished release profile
```

## Task ledger

| Task | Status | Commit | AC advanced | Review | Notes |
| --- | --- | --- | --- | --- | --- |
| T-1 | done | 100b321 | AC-7, AC-9, AC-34 | conductor spot-check | one review unit with T-2, commits adjacent as the plan requires; revert proofs in `T-1+T-2-implementer-report.md` |
| T-2 | done | d076d93 | AC-33, AC-34 | conductor spot-check | `STORE_FORMAT_VERSION = 2`, chain `[migrate_v1_to_v2]`, `current_store_v2.json` fixture added, `current_store_v1.json` byte-unchanged |
| T-3 | in progress | | AC-20, AC-21 | | |
| T-4 | in progress | | AC-5, AC-6, AC-9, AC-19 | | |
| T-5 | in progress | | AC-10..AC-15 | | |
| T-6 | in progress | | AC-1..AC-4, AC-7, AC-35 | | |
| T-7 | in progress | | AC-8 | | |
| T-8 | in progress | | AC-16..AC-19 | | |
| T-9 | in progress | | AC-22..AC-26 | | |
| T-10 | in progress | | AC-27 | | |
| T-11 | in progress | | AC-29, AC-31, AC-32 | | |
| T-12 | in progress | | AC-28, AC-30 | | |
| T-13 | in progress | | AC-36 | | |

## Evidence

Per-task green-bar captures are appended below by the conductor from its own runs.

### T-1 + T-2 (conductor run, head `d076d93`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
exit 0
cargo test: 768 passed, 0 failed (baseline 764, +4)
```

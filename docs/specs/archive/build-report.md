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
| T-3 | done | e912bb0 | AC-20, AC-21 | conductor spot-check | project verbs, `is_hidden`, transient project intents merge |
| T-4 | done | ffa848c | AC-5, AC-6, AC-9, AC-19 | conductor spot-check | `query_board` with archived project set |
| T-5 | done | 809cf5b | AC-10..AC-15 | conductor spot-check | ARCHIVED drawer group, sentinel header row, `done_drawer_archived` golden; golden-count guard 6→7 |
| T-6 | done | fce4211 + 4fd23f8 + conductor fix | AC-1..AC-4, AC-7, AC-35 | conductor spot-check | remediation: `f` verb moved last on the board bar (implementer) and the task-page bar (conductor) so `+ capture` and `ctrl+a step` stay on the 80-col bar; guards restored to `main` assertions; board goldens differ from main only by the clipped ` · f…` tail |
| T-7 | done | 3792793 | AC-8 | conductor spot-check | header slot `archived` |
| T-8 | done | 2863b96 | AC-16..AC-19 | conductor spot-check | picker tabs, `ProjectPickerSwitchTab` / `SelectPickerTab` |
| T-9 | done | d8d3dbb | AC-22..AC-26 | conductor spot-check | launch card, no `Enter` default (gate F-1) |
| T-10 | done | 1456b86 | AC-27 | conductor spot-check | quick-add `!p` refusal |
| T-11 | done | 263ce1c | AC-29, AC-31, AC-32 | conductor spot-check | `tsk archive|unarchive`, `list --archived` |
| T-12 | done | 521e39e | AC-28, AC-30 | conductor spot-check | `tsk project archive|unarchive`, `project-archived` error code |
| T-13 | done | a3886a3 | AC-36 | conductor spot-check | docs, site, skill, changelog, verification-report |

## Evidence

Per-task green-bar captures are appended below by the conductor from its own runs.

### T-1 + T-2 (conductor run, head `d076d93`)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
exit 0
cargo test: 768 passed, 0 failed (baseline 764, +4)
```

### T-3 .. T-13 plus remediation (conductor run, head after `fix(T-6)` task-page commit)

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
exit 0
cargo test: 803 passed, 0 failed (baseline 764, +39)
cargo build --release: finished release profile
$ cd site && npm test
pass 10, fail 0
```

Live smoke: `.agent-sdlc/briefs/archive/SMOKE-report.md` (implementer, `HERDR_ENV=1`,
`TSK_STATE_DIR=/tmp/tsk-archive-smoke`, 23 steps, nothing failed live). Conductor did not re-drive
the smoke; the review panel runs next on the whole change.

## Review fixes (Review panel on main..HEAD, eight kept findings, all fixed)

| K | Commit | Regression test (watched failing first) |
| --- | --- | --- |
| K1 | `5dd3850` fix(K1): plan-form add refuses archived-project items | `tests/cli_add.rs::plan_items_resolving_to_an_archived_project_refuse_with_project_archived` (red: all three items saved, exit 0) |
| K2 | `137b8d8` fix(K2): list `--archived` conflicts are usage errors | `tests/cli_list.rs::archived_conflicts_are_usage_errors` (red: no conflict validation, exit 0) |
| K3 | `3bf08ef` fix(K3): keep archived is a non-persisting outcome | `tests/archive_launch_card.rs::keep_archived_persists_nothing` (red: outcome was `Persisted`, store rewritten) |
| K4 | `7b5c4f3` fix(K4): archived header pin drives viewport follow | `tests/queue_board_render.rs::archived_header_selection_follows_the_viewport` (red: header stayed below the fold) |
| K5 | `f5dde44` fix(K5): idle merge resets project focus archived by another process | `tests/queue_board_loop.rs::idle_merge_leaves_a_project_focus_archived_by_another_process` (red via hand-revert: `selected_project` stayed `Some("/repos/focus")`) |
| K6 | `1319319` fix(K6): v1 migration strips defensive archived keys | `tests/store_persist.rs::v1_migration_strips_defensive_archived_keys` (red: `archived == true` survived) |
| K7 | `d5df66f` fix(K7): archive refusals name the invoked verb | `tests/cli_archive.rs::unarchive_refusals_name_the_unarchive_verb` (red: `tsk archive: T99 ...` hardcoded) |
| K8 | `dc3d802` fix(K8): picker file refusal for a taskless project paints on the status slot | `tests/queue_board_verbs.rs::file_on_a_taskless_invocation_repo_refuses_on_the_status_line` (red via hand-revert: no message, picker open) |

Advisory (not fixed, per review): the picker archived tab paints live state while acting on
its snapshot after an idle merge.

Final: `cargo test` 811 passed, 0 failed; clippy `-D warnings` clean; release build ok.

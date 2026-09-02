# Coverage ledger

Built from the filesystem and `Cargo.toml` / `herdr-plugin.toml` / `site/package.json`,
not from memory. Status `done` means a hand-written page covers the item. Status
`generated (rustdoc)` means `cargo doc --no-deps --package tsk-tui` includes every
`pub` symbol in that module (see [reference](reference.md)).

Symbol-level public items are inventoried as **723 `pub` lines** under `src/` (including
`pub mod` / `pub use` / inherent methods). They close as a group per module via rustdoc
rather than a row per function.

## Target

This repository (crate `tsk-tui` + `site/` + plugin manifest). Not a
self/special docs repo.

## Subsystems and modules

| Item | Kind | Doc | Status |
| --- | --- | --- | --- |
| System map | subsystem | [architecture.md](architecture.md) | done |
| Cross-cutting rules | subsystem | [invariants.md](invariants.md) | done |
| Trust / display injection | subsystem | [security-model.md](security-model.md) | done |
| Persisted + wire shapes | data model | [data-model.md](data-model.md) | done |
| Process surfaces | entry pts | [entry-points.md](entry-points.md) | done |
| `src/lib.rs` (`run`, re-exports) | module | rustdoc + [architecture.md](architecture.md) | generated (rustdoc) |
| `src/main.rs` | entry pt | [entry-points.md](entry-points.md) | done |
| `src/domain/*` | subsystem | [domain.md](domain.md) | done |
| `src/domain/mod.rs` | module | [domain.md](domain.md) | done |
| `src/domain/task.rs` publics | module | rustdoc | generated (rustdoc) |
| `src/domain/events.rs` publics | module | rustdoc | generated (rustdoc) |
| `src/domain/thread.rs` publics | module | rustdoc | generated (rustdoc) |
| `src/domain/undo.rs` publics | module | rustdoc | generated (rustdoc) |
| `src/domain/dispatch_attempt.rs` publics | module | rustdoc | generated (rustdoc) |
| `src/domain/time_serde.rs` | internal — load-bearing | [data-model.md](data-model.md), [reference.md](reference.md) | done |
| `src/store.rs` | subsystem | [store.md](store.md) | done |
| `src/config.rs` | subsystem | [config.md](config.md) | done |
| `src/context.rs` | subsystem | [context.md](context.md) | done |
| `src/scope.rs` | module | [context.md](context.md) | done |
| `src/capture.rs` | subsystem | [capture.md](capture.md) | done |
| `src/save_recovery.rs` | module | [app.md](app.md) | done |
| `src/app.rs` | subsystem | [app.md](app.md) | done |
| `src/board_pane.rs` | module | [plugin.md](plugin.md) | done |
| `src/text.rs` | internal — load-bearing | [reference.md](reference.md) | done |
| `src/cli/*` | subsystem | [cli.md](cli.md) | done |
| `src/cli/mod.rs` | module | [cli.md](cli.md) | done |
| `src/cli/router.rs` | module | [cli.md](cli.md) | done |
| `src/cli/parser.rs` | module | [cli.md](cli.md) | done |
| `src/cli/add.rs` | module | [cli.md](cli.md) | done |
| `src/cli/list.rs` | module | [cli.md](cli.md) | done |
| `src/cli/steps.rs` | module | [cli.md](cli.md) | done |
| `src/cli/presenter.rs` | module | [cli.md](cli.md) | done |
| `src/ui/board/*` | subsystem | [board-ui.md](board-ui.md) | done |
| `src/ui/board/mod.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/board/model.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/board/apply.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/board/commands.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/board/chrome.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/board/draw.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/queue.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/input.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/mouse.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/render.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/edit.rs` | internal — load-bearing | [board-ui.md](board-ui.md), [reference.md](reference.md) | done |
| `src/ui/markdown.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/selection.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/scheduler.rs` | module | [app.md](app.md) | done |
| `src/ui/scrollbar.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/text_select.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/tier.rs` | module | [board-ui.md](board-ui.md) | done |
| `src/ui/capture.rs` | subsystem | [capture-ui.md](capture-ui.md) | done |
| `src/ui/mod.rs` presenters | internal — load-bearing | [security-model.md](security-model.md) | done |
| `herdr-plugin.toml` | entry pt | [plugin.md](plugin.md) | done |
| `scripts/open-board.sh` | entry pt | [plugin.md](plugin.md) | done |
| `scripts/open-capture.sh` | entry pt | [plugin.md](plugin.md) | done |
| `scripts/tsk-cli.sh` | entry pt | [entry-points.md](entry-points.md) | done |
| `site/` Astro app | subsystem | [site.md](site.md) | done |

## Data models

| Item | Kind | Doc | Status |
| --- | --- | --- | --- |
| `DomainState` / `tsk.json` | data model | [data-model.md](data-model.md) | done |
| `Task`, `Step`, `HumanStatus`, `TaskScope` | data model | [data-model.md](data-model.md) | done |
| `ContextCapsule`, `AgentMeta`, `ObservedStatus` | data model | [data-model.md](data-model.md) | done |
| `TaskEvent`, `ProvenanceOrigin` | data model | [data-model.md](data-model.md) | done |
| `UndoEntry` | data model | [data-model.md](data-model.md) | done |
| `DispatchAttempt` and receipts | data model | [data-model.md](data-model.md) | done |
| `walkthrough.json` | data model | [data-model.md](data-model.md) | done |
| CLI plan/flag/list JSON | data model | [data-model.md](data-model.md) | done |
| `HERDR_PLUGIN_CONTEXT_JSON` | data model | [context.md](context.md) | done |

## Exclusions (declared)

| Item | Kind | Reason |
| --- | --- | --- |
| `target/` | excluded | generated build + rustdoc output |
| `site/node_modules/` | excluded | vendored npm |
| `site/dist/` | excluded | generated Astro build |
| `#[cfg(test)]` modules and `tests/` | excluded | test-only; they *exercise* the surface, they are not product API |
| `skills/` | excluded | operator skill docs, not crate internals |
| `docs/specs/` (except ADRs linked from architecture) | excluded | feature-spec artifacts; ADRs are linked, not duplicated |
| `archive/dark-engine-pre-v1` | excluded | not present in this tree; historical engines |
| `src/host`, `src/dispatch`, `src/attention`, `src/resume` | excluded | not in this tree (stale `target/doc/herdr_tasks/` rustdoc is from an old crate name — ignore it) |
| Adding `cargo doc` to CI / green bar | excluded | owner-approved change, not a docs-task default |

## Stats

- Hand-written pages: 19 under `docs/technical/` (landing, 4 spine, 11 subsystems,
  coverage, entry-points, reference).
- Inventoried `pub` lines under `src/`: 723, closed via rustdoc per module.
- Inventoried crate-private / `pub(crate)` lines: 217; load-bearing subset listed in
  [reference.md](reference.md#load-bearing-internals).
- Exclusions: 8 rows, each with a reason.

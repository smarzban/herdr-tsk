# Technical documentation

Maintainer-grade map of **tsk**: how it works, why it is shaped this way, and a complete
surface inventory. This tree is the internals.

The **user guide** is the Starlight site (`site/src/content/docs/docs/`), published at
https://tsk-gules.vercel.app/docs/. The repo README is the front door and points there.

Crate: `tsk-tui` 0.4.0. Binary: `tsk`. Plugin: `herdr-tsk`.

## Start here by need

| You want | Read |
| --- | --- |
| Where a change belongs, and which constraints shaped the layout | [Architecture](architecture.md) |
| Why a rule exists and what must not break | [Invariants](invariants.md) |
| Every persisted / wire entity | [Data model](data-model.md) |
| Trust boundaries, untrusted input, fail-open vs fail-closed | [Security model](security-model.md) |
| How a subsystem works (mechanism + invariants) | [Subsystems](#subsystems) |
| What a function/type does (signatures, defaults) | [Symbol reference](reference.md) (generated rustdoc) |
| CLI, TUI, plugin, env, and script entry points | [Entry points](entry-points.md) |
| Coverage ledger (documented vs excluded) | [Coverage](coverage.md) |

## Subsystems

| Subsystem | Responsibility |
| --- | --- |
| [Domain](domain.md) | Task lifecycle, human status, steps, threads, and undo |
| [Store](store.md) | One JSON document, exclusive lock, revision merge, atomic replace |
| [Config](config.md) | `walkthrough.json` dismissal record and config-directory resolution |
| [Context and scope](context.md) | Host JSON → invocation snapshot; project path / desk resolution |
| [Capture pipeline](capture.md) | Snapshot + user fields → domain create (+ optional persist) |
| [CLI](cli.md) | `add` / `list` / `steps` routing, parse, persist, present |
| [App loop](app.md) | Mode select, board/capture event loops, save recovery, idle merge |
| [Board UI](board-ui.md) | Queue query, intent reducer, chrome, paint, keys, mouse |
| [Capture UI](capture-ui.md) | Standalone capture form (`AppMode::Capture`) |
| [Plugin / host](plugin.md) | `herdr-plugin.toml`, pane identity, launcher scripts |
| [Site](site.md) | Astro + Starlight marketing/docs site (not in the binary) |

## Reference strategy

**Hybrid.** The crate has hundreds of `pub` items (internal library used by the binary and
integration tests, `publish = false`). Signatures and rustdoc comments are generated with
`cargo doc`. Architecture, invariants, data model, security, and per-subsystem mechanism
are hand-written here. Load-bearing *private* internals that rustdoc would miss are listed
in [reference.md](reference.md#load-bearing-internals).

Do not add rustdoc generation to CI or the green bar without an explicit owner decision.

## File tree

```
docs/technical/
  README.md              landing (this page)
  architecture.md        system map, constraints, where a change belongs
  invariants.md          rules a maintainer must not break
  data-model.md          tsk.json, settings, CLI/host JSON
  security-model.md      trust boundaries, fail-open / fail-closed
  coverage.md            ledger + exclusions
  entry-points.md        argv, env, plugin, scripts
  reference.md           rustdoc how-to + load-bearing privates
  domain.md
  store.md
  config.md
  context.md
  capture.md
  cli.md
  app.md
  board-ui.md
  capture-ui.md
  plugin.md
  site.md
```

## Related trees

- User docs: [`site/src/content/docs/docs/`](../../site/src/content/docs/docs/)
- Glossary: [`CONTEXT.md`](../../CONTEXT.md)
- Standing agent rules: [`AGENTS.md`](../../AGENTS.md)

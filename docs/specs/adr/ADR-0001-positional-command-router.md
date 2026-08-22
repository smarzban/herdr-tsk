# ADR-0001 — Positional command router

**Status:** accepted · 2026-08-21 · feature: `headless-add`

## Context

The binary today selects a surface by scanning the whole argv (`capture` anywhere, `--find-board-pane` via `any()`). A headless `add` that accepts free-text titles would let `add -t capture` or `add -t "--find-board-pane"` hijack the mode.

## Decision

Surfaces are selected only by the first non-flag argument, or by a closed set of global flags recognized only before that argument. Tokens after the subcommand never select a surface. An unknown first positional is a usage error, never the board.

## Consequences

- Titles and plan fields cannot change which surface runs.
- `herdr-tasks --find-board-pane` still works; `add -t "--find-board-pane"` is add.
- Unknown argv no longer opens the board TUI. That is a deliberate break for agent-safety.
- Pre-subcommand tokens cannot carry subcommand flags (`--state-dir` lives after `add` / `list`).

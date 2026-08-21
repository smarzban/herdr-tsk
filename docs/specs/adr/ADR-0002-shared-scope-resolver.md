# ADR-0002 — Shared scope resolver, bit-identical board `!p`

**Status:** accepted · 2026-08-21 · feature: `headless-add`

## Context

Board quick-add resolves `!p` against "projects available to this board" inside board apply. Headless add needs the same path rule. Copying the function would drift. Changing the rule while lifting would silently retarget live board captures.

## Decision

Lift only the path matcher (candidate set, unique ASCII-case-insensitive basename, slash verbatim, otherwise verbatim). The `!p` token split stays in board apply. After the lift, board `!p` semantics are bit-identical. The plan writes the regression that fails on copy-drift first.

## Consequences

- One rule for board and CLI. A typo still files a new scope (verbatim fallback), on both surfaces.
- Candidate set stays the current derivation, including project paths on soft-deleted tasks.
- The lift is a refactor of live board behaviour; it is not done if any existing `!p` case changes.

# Gate report: headless-add

Re-run after the B1–B6 amendment pass. Read-only over `docs/specs/headless-add/headless-add.md`. Full-chain entry. No provenance markers, no `untraced` links, no `constitution.md`. First-pass `sdlc-record` already ran; this re-run does not record again.

Prior F1 (AC-4 untraced on T-4) and F2 (exit-1 wording) are closed. Prior F3 and F4 remain as Lows.

## Chain coverage

In-stack fast-path honoured: **No new products — reuses the declared stack**.

| Criterion | Component | Product | Task(s) | Status |
| --- | --- | --- | --- | --- |
| AC-1 | Headless parser, Bulk add, Headless presenter, Task domain, Document store | declared stack | T-3 | traced |
| AC-2 | Headless parser, Bulk add, Task domain | declared stack | T-3 | traced |
| AC-3 | Headless parser, Bulk add, Task domain | declared stack | T-3 | traced |
| AC-4 | Scope resolver, Context resolver | declared stack | T-1, T-3, T-4 | traced |
| AC-5 | Headless parser, Bulk add, Document store | declared stack | T-4 | traced |
| AC-6 | Command router, Headless parser, Headless presenter | declared stack | T-3, T-4 | traced |
| AC-7 | Headless parser, Headless presenter | declared stack | T-3 | traced |
| AC-8 | Headless parser, Bulk add, Headless presenter, Document store | declared stack | T-4 | traced |
| AC-9 | Headless parser, Headless presenter | declared stack | T-4 | traced |
| AC-10 | Headless parser, Headless presenter | declared stack | T-4 | traced |
| AC-11 | Headless presenter | declared stack | T-4 | traced |
| AC-12 | Headless parser, Document store | declared stack | T-3, T-5 | traced |
| AC-13 | List query | declared stack | T-5 | traced |
| AC-14 | List query, Headless presenter | declared stack | T-5 | traced |
| AC-15 | List query, Headless presenter | declared stack | T-5 | traced |
| AC-16 | Headless presenter | declared stack | T-7 | traced |
| AC-17 | Agent discovery | declared stack | T-8 | traced |
| AC-18 | Agent discovery | declared stack | T-8 | traced |
| AC-19 | Command router, Headless presenter | declared stack | T-2 | traced |
| AC-20 | Bulk add, List query, Headless presenter, Document store | declared stack | T-6 | traced |
| AC-21 | Headless parser, Headless presenter | declared stack | T-3, T-4 | traced |

Reverse walk: every numbered component is justified by at least one AC. Every task cites an existing AC. AC-21 is on both maps. Negative criteria stay untraced by design.

Amendment check: plan-source is argv-only; exit 3 is indeterminate; `invalid-title` is in the closed code set; creates are `Capture` with no capsule/`agent_meta`; T-1 live-smoke note matches `AGENTS.md`; T-4 advances AC-4.

## Findings

### F3 (Low, carried): AC-6 maps to Command router, those cases live on the parser

*Location:* Design criterion-to-component map AC-6; Plan T-3/T-4.
*Owner:* design.
*Next:* optional map cleanup. Router safety is AC-19.

### F4 (Low, carried): AC-19 presenter mapping lands in `main` during T-2

*Location:* Design map AC-19 → Headless presenter; Plan T-2 prints usage from `src/main.rs` and adds a process test.
*Owner:* plan.
*Next:* none required. Exit 2 is asserted by `tests/cli_router_process.rs`.

### F5 (Low): T-2 still offers a `resolve_mode_from` fork

*Location:* Plan T-2 (“change `resolve_mode_from` … or keep it only for TUI”).
*Owner:* plan.
*Next:* pick one in build: router is the process boundary; `resolve_mode_from` may stay TUI-only if `main` never forwards `add`/`foo` into `run`.

## Verification integrity

- All 21 ACs are test-backed with an oracle kind. None are reviewer-checked.
- Both maps are complete (every AC once).
- Green bar is declared: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`.
- Load-bearing claims remain `verified-by-probe` → `docs/specs/headless-add/probes/2026-08-22-serde-json-and-is-terminal.txt` (present, non-empty). Gate did not re-run the probe.

## Hygiene

No TBD or placeholder. T-2’s reserved Add/List exit 2 until T-3/T-5 is sequenced work. The `resolve_mode_from` “or” is F5, not an open product decision.

## Constitution

No `constitution.md`. Nothing to violate.

## Mid-chain entry / coverage note

Not applicable.

## Checker corroboration

`node /Users/saeed/.pi/agent/git/github.com/smarzban/agent-sdlc/checker/sdlc-check.mjs docs/specs/headless-add/headless-add.md` → `sdlc-check 0.19.0: all checks passed — 0 findings, 0 notes.` Exit 0.

## Experiment recording

Skipped (re-run after a fix).

## Verdict

**Ready to build.** No Critical or High findings. Checker passed. Residual Lows only (F3–F5).

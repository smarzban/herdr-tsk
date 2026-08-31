# Gate report — task-numbers

Spec: `docs/specs/task-numbers/task-numbers.md` · gated 2026-08-31 · read-only; nothing
modified but this report.

## Chain coverage (criterion → component → product → task)

`## Tech Stack` declares the in-stack fast-path (**No new products — reuses the
declared stack**), which the walk honours as the component → product link for all
components at once. Product column therefore reads `in-stack` throughout.

| AC | Component(s) | Product | Task(s) | Gap |
| --- | --- | --- | --- | --- |
| AC-1 | Number allocator, Task number field | in-stack | T-1 | — |
| AC-2 | Number allocator, Task number field | in-stack | T-1 | — |
| AC-3 | Number allocator | in-stack | T-1 | — |
| AC-4 | Number allocator | in-stack | T-1 | — |
| AC-5 | CLI number presentation, Number allocator | in-stack | T-2 | note F-3 |
| AC-6 | Task number field | in-stack | T-1 | — |
| AC-7 | Task number field | in-stack | T-1 | — |
| AC-8 | Operand resolver | in-stack | T-3 | — |
| AC-9 | Number allocator | in-stack | T-1 | — |
| AC-10 | Number allocator | in-stack | T-1 | — |
| AC-11 | Number allocator, Task number field | in-stack | T-1 | — |
| AC-12 | Number allocator, Task number field | in-stack | T-1 | — |
| AC-13 | Number allocator | in-stack | T-1 | — |
| AC-14 | Operand resolver | in-stack | T-3 | — |
| AC-15 | Operand resolver | in-stack | T-3 | — |
| AC-16 | Operand resolver | in-stack | T-3 | — |
| AC-17 | Operand resolver | in-stack | T-3 | — |
| AC-18 | Operand resolver | in-stack | T-3 | — |
| AC-19 | Operand resolver | in-stack | T-3 | — |
| AC-20 | CLI number presentation | in-stack | T-2 | — |
| AC-21 | CLI number presentation | in-stack | T-2 | — |
| AC-22 | CLI number presentation | in-stack | T-2 | — |
| AC-23 | Board number chrome | in-stack | T-4 | — |
| AC-24 | Board number chrome | in-stack | T-4 | — |
| AC-25 | Board number chrome | in-stack | T-4 | — |
| AC-26 | Board number chrome | in-stack | T-4 | — |
| AC-27 | Board number chrome | in-stack | T-4 | — |
| AC-28 | CLI skills doc | in-stack | T-3 | note F-2 |
| AC-29 | Site number docs | in-stack | T-5 | — |

Reverse walk: every task (T-1..T-5) advances ≥1 AC (no gold-plating). Every AC
reaches ≥1 task. NC-1..NC-12 are `NC`-prefixed and outside the mechanical coverage
set. Reviewer-checked AC-28 and AC-29 have carrying tasks (T-3 produces
`skills/tsk-cli/SKILL.md`; T-5 produces the site files).

Green bar declared: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release`.

## Mid-chain entry / coverage note

`## Brief` carries provenance `spawn investigation (fable, sol, kimi) settled with
the user on store-global 12, 2026-08-31`. Later sections are hand-authored in this
run. No `untraced` links. Chain entered at idea.

## Findings

**Critical:** none. **High:** none.

**F-1 · Low · owner: plan.** Design component **Task number field** is not named in
any task's `*Component:*` field. T-1 implements it (literals, immutability tests)
under **Number allocator**. Acceptable slice; recorded so build does not treat the
field as unowned.

**F-2 · Low · owner: plan.** Design component **CLI skills doc** is not named in
any task's `*Component:*` field. T-3's `*Advances:*` includes AC-28 and its files
list `skills/tsk-cli/SKILL.md`. Coverage-forward holds; the `*Component:*` line
names Operand resolver.

**F-3 · Low · owner: plan.** AC-5's design owners include Number allocator, but
only T-2 advances it (`existing_add_does_not_advance_the_counter`). The allocator
half lives in that CLI test. No action required.

## Checker corroboration

`sdlc-check 0.20.1: all checks passed — 0 findings, 0 notes.`
Invoked as `node /Users/saeed/.pi/agent/git/github.com/smarzban/agent-sdlc/checker/sdlc-check.mjs docs/specs/task-numbers/task-numbers.md`. Exit 0.

Load-bearing library claims: none (in-stack fast-path; integer fields and format
refusal are in-repo precedent).

Constitution: no `constitution.md`; standing `AGENTS.md` product rules checked,
no conflict.

## Verdict

**Ready to build.** No Critical or High findings. Checker passed.

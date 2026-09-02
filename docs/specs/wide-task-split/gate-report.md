# Gate report: wide task split

Date: 2026-09-02. Spec: `docs/specs/wide-task-split/wide-task-split.md`.

This is the clean rerun after the first gate routed two blocking findings back to their owning stages: the plan/component trace was repaired by separating geometry from composition and reassigning resize verification, forbidden em dashes were removed, and the pre-build exact-file list was amended to carry the approved glossary update in T-1.

## Chain coverage table

| Criterion | Component | Product | Task(s) |
| --- | --- | --- | --- |
| AC-1 | Responsive layout resolver, Split frame compositor | in-stack fast-path | T-2 |
| AC-2 | Responsive layout resolver, Split frame compositor | in-stack fast-path | T-2 |
| AC-3 | Responsive layout resolver | in-stack fast-path | T-1 |
| AC-4 | Responsive layout resolver, Split frame compositor | in-stack fast-path | T-2 |
| AC-5 | Task-page session, Split frame compositor | in-stack fast-path | T-2 |
| AC-6 | Split frame compositor | in-stack fast-path | T-2 |
| AC-7 | Focused input router, Surface focus controller | in-stack fast-path | T-3 |
| AC-8 | Focused input router, Surface focus controller, Task-page session | in-stack fast-path | T-3 |
| AC-9 | Focused input router, Task-page session, Split frame compositor | in-stack fast-path | T-4 |
| AC-10 | Focused input router, Surface focus controller, Task-page session | in-stack fast-path | T-4 |
| AC-11 | Focused input router, Surface focus controller | in-stack fast-path | T-4 |
| AC-12 | Responsive layout resolver, Surface focus controller, Task-page session | in-stack fast-path | T-3 |
| AC-13 | Responsive layout resolver, Surface focus controller, Task-page session | in-stack fast-path | T-3 |
| AC-14 | Responsive layout resolver, Surface focus controller, Task-page session | in-stack fast-path | T-5 |
| AC-15 | Responsive layout resolver, Surface focus controller, Task-page session | in-stack fast-path | T-3 |
| AC-16 | Task-page session, Focused input router | in-stack fast-path | T-5 |
| AC-17 | Task-page session, Split frame compositor | in-stack fast-path | T-5 |
| AC-18 | Task-page session, Split frame compositor | in-stack fast-path | T-2 |
| AC-19 | Responsive layout resolver, Surface focus controller, Split frame compositor | in-stack fast-path | T-3 |
| AC-20 | Responsive layout resolver, Surface focus controller | in-stack fast-path | T-3 |

No gaps: each criterion reaches at least one named design component, the declared existing-stack product choice, and a task whose declared component is in that criterion's design map. No orphans: every component is used by criteria and represented by at least one task; every task advances criteria.

## Checks 1 through 5

1. **Coverage, both directions:** clean, as shown above.
2. **Consistency:** brief, criteria, design, stack, and plan agree on the inclusive 110-column threshold, equal split after one divider column, focused-surface resize behavior, task-page parity at equal task-surface geometry, dirty retarget refusal, and narrow-mode peek retention. Terminology matches `CONTEXT.md`.
3. **Constitution:** no `constitution.md` exists. Following established repository precedent, `AGENTS.md`, `AGENTS.local.md`, and `docs/technical/invariants.md` were walked as standing constraints. The plan preserves human-status authority, modifier-protected mutation, form/save-recovery lifetime, visible-surface selection, bounded untrusted text, mono styling, and settle-paint-wait ordering.
4. **Verification integrity:** all 20 criteria are test-backed with named oracle kinds; both maps are complete; the green bar is concrete; the no-new-products fast path covers every component. The load-bearing terminal-stack claim is tagged `verified-by-probe`, with non-empty output at `docs/specs/wide-task-split/probes/existing-terminal-stack.txt`.
5. **Hygiene:** no unresolved TBD, TODO, conditional implementation placeholder, deferred decision, or forbidden em dash remains in the feature artifacts.

## Findings by severity

### Low

1. Location: repository root. No `constitution.md`; project instruction and invariant documents were used as the stand-in. Owner: repo-setup, optional later. Not blocking by established repository precedent.

No Critical, High, or Medium findings.

## Mid-chain entry / coverage note

The `## Brief` carries a provenance marker for the user-guided interaction design settled in chat on 2026-09-02. Acceptance criteria, design, tech stack, and plan were authored from that materialized brief. No link is marked `untraced`.

## Checker corroboration

`node /Users/saeed/.pi/agent/git/github.com/smarzban/agent-sdlc/checker/sdlc-check.mjs docs/specs/wide-task-split/wide-task-split.md` ran with sdlc-check 0.20.1 and exited 0: 0 findings, 0 notes.

## Operational pre-build note

A clean Herdr worktree now exists at `/Users/saeed/.herdr/worktrees/herdr-tasks/feat-wide-task-split`, branch `feat/wide-task-split`, based on current `main` at `3f6f722`. The ignored local spec and gate artifacts are materialized there. The original working copy remains untouched by build work.

## Verdict

**Ready to build, spec gate passed.** No blocking spec findings; mechanical corroboration passed. Execute T-1 through T-5 test-first with the declared green bars and isolated live Herdr smoke.

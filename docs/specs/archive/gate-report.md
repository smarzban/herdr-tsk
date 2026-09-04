# Gate report: archive

Date: 2026-09-04 · Spec: `docs/specs/archive/archive.md` @ `eb98b0c` · Level: feature, full chain.
Read-only walk over Brief, Acceptance Criteria, Design, Tech Stack, Plan, root `CONTEXT.md`.
No `constitution.md` exists in this repo; `AGENTS.md` product rules were used as the standing
principles (mutating verbs need Ctrl, mono only, human status is truth, keymap changes need a
`site/` update, goldens regenerate by command).

## Chain coverage

Product column: Tech Stack declares the in-stack form (no new products, every component on the
existing crates or std), which the gate honours for all components at once.

| AC | Component(s) | Product | Task(s) | Gap |
| --- | --- | --- | --- | --- |
| AC-1 | Archive domain, Board model and intents, Key and mouse mapping | in-stack | T-6 | none |
| AC-2 | Archive domain, Board model and intents | in-stack | T-6 | none |
| AC-3 | Board model and intents, Archive domain | in-stack | T-6 | none |
| AC-4 | Board model and intents | in-stack | T-6 | none |
| AC-5 | Lens query | in-stack | T-4 | none |
| AC-6 | Lens query, Archive domain | in-stack | T-4 | none |
| AC-7 | Archive domain | in-stack | T-1, T-6 | none |
| AC-8 | Board renderer, Board model and intents | in-stack | T-7 | none |
| AC-9 | Store format v2, Archive domain | in-stack | T-1, T-4 | none |
| AC-10 | Lens query, Board renderer | in-stack | T-5 | none |
| AC-11 | Board model and intents, Lens query | in-stack | T-5 | none |
| AC-12 | Board model and intents, Key and mouse mapping | in-stack | T-5 | none |
| AC-13 | Board renderer, Lens query | in-stack | T-5 | none |
| AC-14 | Lens query | in-stack | T-5 | none |
| AC-15 | Board renderer | in-stack | T-5 | none |
| AC-16 | Project picker, Archive domain | in-stack | T-8 | none |
| AC-17 | Project picker, Board renderer | in-stack | T-8 | none |
| AC-18 | Project picker, Archive domain | in-stack | T-8 | none |
| AC-19 | Lens query, Project picker | in-stack | T-4, T-8 | none |
| AC-20 | Archive domain | in-stack | T-3 | none |
| AC-21 | Archive domain, Store format v2 | in-stack | T-3 | none |
| AC-22 | Launch card, Capture scope resolution | in-stack | T-9 | none |
| AC-23 | Launch card, Archive domain | in-stack | T-9 | none |
| AC-24 | Launch card, Board model and intents | in-stack | T-9 | none |
| AC-25 | Board model and intents | in-stack | T-9 | none |
| AC-26 | Launch card | in-stack | T-9 | none |
| AC-27 | Capture scope resolution, Board model and intents | in-stack | T-10 | none |
| AC-28 | Capture scope resolution, CLI archive surfaces | in-stack | T-12 | none |
| AC-29 | CLI archive surfaces, Archive domain | in-stack | T-11 | none |
| AC-30 | CLI archive surfaces, Capture scope resolution | in-stack | T-12 | none |
| AC-31 | CLI archive surfaces, Archive domain | in-stack | T-11 | none |
| AC-32 | CLI archive surfaces | in-stack | T-11 | none |
| AC-33 | Store format v2 | in-stack | T-2 | none |
| AC-34 | Store format v2 | in-stack | T-1, T-2 | none |
| AC-35 | Board renderer, Key and mouse mapping | in-stack | T-6 | none |
| AC-36 | Docs and site | in-stack | T-13 (reviewer-checked, carried) | none |

Reverse direction: every one of the 11 components is named by at least one AC; every task T-1..T-13
advances at least one AC. No orphans, no gold-plating found.

## Checks

1. **Coverage**: complete both ways (table above).
2. **Consistency**: terminology matches `CONTEXT.md` (`archived`, `archived project`, `project
   record`, `archived group`, `file`, `hidden`, `working lens`, `session`, `open task`). The plan's
   two refinements (sentinel header row id; transient project-intent map for the merge) are
   recorded back into the Design (merge amendment under Archive domain) and the plan Notes; no
   artifact contradicts another. One minor drift, see F-1.
3. **Standing principles**: `ctrl+f` is a Ctrl chord (mutating verbs need Ctrl); no colour
   (AC-15, NC-7); no status auto-change (NC-1, NC-3); site pages updated in T-13; goldens regenerate
   by command (plan Notes). No violation.
4. **Verification integrity**: every test-backed AC names an oracle kind and has a task; AC-36
   names axis Spec Conformance, a pass/fail question, a justification and a carrying task (T-13).
   Design map and plan coverage map are both complete. Green bar declared concretely in Tech
   Stack. All three load-bearing claims are `verified-by-probe` with the kept evidence being
   existing code and tests in the repo (`src/domain/task.rs` serde attributes, `src/ui/input.rs`
   chord tests, `tests/queue_board_render.rs` goldens); presence confirmed, truth not re-verified.
5. **Hygiene**: no TBD / TODO / placeholder markers in the spec or glossary.
6. **Checker corroboration**: `node checker/sdlc-check.mjs docs/specs/archive/archive.md` →
   `sdlc-check 0.20.1: all checks passed — 0 findings, 0 notes`, exit 0. Corroborated.

## Findings

| ID | Severity | Location | Issue | Owner | Next action |
| --- | --- | --- | --- | --- | --- |
| F-1 | Low | Plan T-9, `map_launch_card` | Plan maps `Enter` to `unarchive` on the launch card. AC-23 names only `y` and a click; AC-24 names `n`, `Esc`, click. `Enter` as the affirmative default is a UX choice not settled upstream. | plan (or criteria if the owner wants Enter) | Resolved before build: `Enter` dropped from `map_launch_card` in the plan (T-9). |
| F-2 | Low | Plan T-1 / T-2 Notes | T-1 alone leaves the task shape changed under the v1 version number; the plan already requires T-2 to land immediately after and in the same PR. Recorded so the build report can show the two commits are adjacent. | plan | None beyond the existing note. |

No Critical, no High, no Medium.

## Mid-chain entry / provenance

None. Every section was authored in-chain; the Design carries one dated in-chain amendment from
the plan stage (merge rule), not an external source.

## Verdict

**Ready to build.** Two Low findings, neither blocking. F-1 should be resolved by the build
conductor before T-9 (one line either way).

## Delta gate: owner-smoke amendment (2026-09-04)

Amendment ingested into `## Acceptance Criteria` (AC-13 and AC-22 amended, AC-37..AC-45 added) and
`## Plan` (T-14..T-17, provenance-marked) after the T-13 build and the owner's live smoke on
`/tmp/tsk-b-try`. Walk: every new AC → component (design map rows added) → in-stack product →
task (coverage rows added); T-14..T-17 each advance ≥ 1 AC; no orphan. `sdlc-check` trace and
coverage rules: 0 findings. Its `green-bar-evidence` and `artifact-parse` findings concern the
ledger and verification-report *shape* (`### T-N (@ SHA)` evidence headings, `Criterion | Type |
Proof` table), to be reshaped at close-out; they do not touch the chain. Verdict for the delta:
**ready to build**.

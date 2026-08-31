# ADR-0004: Task numbers are store-global, not per-project

Date: 2026-08-31 · Status: accepted

## Context

tsk needs a human-readable task address so a person can say "implement task 12"
and an agent can resolve it. Identity today is a UUID. Three numbering scopes
were considered: (A) one sequential integer unique across the whole store;
(B) per-project (and per-desk) integers, GitHub-style, with a prefix or cwd
to disambiguate; (C) no new number, expose a UUID short prefix the way steps
already do. Independent spawn reviews (fable, sol, kimi) all recommended (A).
The owner settled on (A) with canonical form `12`.

## Decision

Assign each durably created task one store-global sequential integer. The
number is unique across desk, every project path, and every thread, including
done and soft-deleted tasks. Cwd does not participate in resolution. UUID
remains internal identity.

## Why

- **"Task 12" has to mean one task.** Under (B), desk-12 and widget-12 are both
  "task 12" until the speaker adds a prefix. The spoken form the feature exists
  to support becomes ambiguous the moment two scopes each have a 12.
- **The store is the ambiguity unit.** One JSON document, one person. Numbering
  per project copies a multi-repo forge model onto a single personal store.
- **Project identity here is a path, not a key.** Basenames collide, paths
  move, desk is not a project. A `proj-12` form needs a second identity system
  just to avoid a global counter, and it eats board cells.
- **cwd-relative 12 is the same bug.** The same words would mean different
  tasks in different terminals, including a project cwd hiding a desk task.
- **UUID prefixes fail as speech.** They are not memorable, and a shortest
  unambiguous prefix lengthens as the store grows, so a handle an agent cached
  last week can become ambiguous. Step prefixes work because their namespace
  is one task's small sibling list.

## Consequences

- Numbers grow across the whole store (3 to 4 digits in ordinary use). Still
  speakable, still a few cells on a row.
- A task that changes project keeps its number. That is the point.
- An older writer that dropped the number on save would silently invalidate
  every spoken handle, so the store must refuse that rewrite.

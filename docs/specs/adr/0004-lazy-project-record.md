# ADR-0004: Project records are lazy, and archive is a flag not a place

Date: 2026-09-04 · Status: proposed (spec B, `docs/specs/archive/`)

## Context

Until now a project has no representation of its own: it is the `path` string inside
`TaskScope::Project` on each task, and the projects tab and `P` picker derive the set of
projects from the tasks present. Archiving a project needs somewhere to write `archived: true`
that survives the project having zero live tasks. Three shapes were considered: (A) a lazy
`projects` map on `DomainState`, keyed by scope path, holding a record only for archived
projects; (B) an eager record for every project ever seen, with the derived set replaced by the
map; (C) a separate archive file holding archived tasks and projects.

## Decision

(A). `DomainState` gains `projects: BTreeMap<String, ProjectRecord>` where the record's only
field for now is `archived: bool`. A record exists exactly while its project is archived;
unarchive removes it. The set of projects the board paints is still derived from tasks, filtered
by the map. Archived tasks are an `archived: bool` flag on `Task`. Nothing archived leaves
`tsk.json`. Store format moves from 1 to 2 through the migration chain added by spec A.

## Why

- **The board paints archived rows** (drawer group, picker tab), so a separate file (C) would be
  loaded and merged on every start and idle poll anyway, doubling the lock, merge and fsync
  surface for no size or speed win. Files are for data the board never paints, which is trash.
- **Lazy keeps the derived model.** (B) would make the map the source of truth for "which
  projects exist", forcing every task create, scope change and CLI `add` to maintain it, and
  raising the question of what a project with a record and no tasks means. Under (A) a project
  still exists exactly when a task carries it, plus a record can pin it while archived.
- **A record is the future home for rename/move.** When a project gets a stable id or a path
  rewrite, the map is where it lives. Making it eager then is a migration step, which the
  chain now supports; making it eager now is YAGNI.

## Consequences

- Rename or move of a project directory still orphans tasks (the path is the key). Out of scope
  here; the record gives the later fix a place to land.
- `deny_unknown_fields` stays on `Task` and `DomainState`, so any future field on either is a
  format bump with a chain step. Accepted: old binaries must never silently drop fields.
- Archived tasks and archived projects are independent flags. A project unarchive never flips
  task flags, and an individually archived task stays archived after its project comes back.

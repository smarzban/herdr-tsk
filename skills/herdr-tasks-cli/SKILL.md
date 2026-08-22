---
name: herdr-tasks-cli
description: Use when asked to add tasks, a task list, or a plan to the Tasks board, or to inspect Tasks board items. Use `herdr-tasks add` and `herdr-tasks list`, never the TUI.
---

# herdr-tasks CLI

Use `herdr-tasks add` to create a task or JSON plan, and `herdr-tasks list` to
inspect the shared board store before and after adding work.

## Recovery

- Add is idempotent by trimmed title and resolved project scope. A non-soft-deleted
  matching task is a success: flag add prints `task already exists`, and plan add reports it
  in `existing` with its `i`, `id`, and `title`.
- On exit 1, retry only the `failed` subset. `created` and `existing` items both
  succeeded, so you must never whole-plan-retry an exit 1 run.
- On exit 3, the commit is indeterminate. Run `herdr-tasks list`, then retry
  only what is missing. You must never whole-plan-retry an exit 3 run.

## Scope check

A typo in a project name silently files the task under a new scope. Use
`herdr-tasks list` to check the resulting scope.

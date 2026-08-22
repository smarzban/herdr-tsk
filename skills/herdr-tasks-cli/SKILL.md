# herdr-tasks CLI

Use `herdr-tasks add` to create a task or JSON plan, and `herdr-tasks list` to
inspect the shared board store before and after adding work.

## Recovery

- On exit 1, retry only the `failed` subset. You must never whole-plan-retry
  an exit 1 run, because successful items were already created.
- On exit 3, the commit is indeterminate. Run `herdr-tasks list`, then retry
  only what is missing. You must never whole-plan-retry an exit 3 run.

## Scope check

A typo in a project name silently files the task under a new scope. Use
`herdr-tasks list` to check the resulting scope.

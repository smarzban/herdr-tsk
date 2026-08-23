# Changelog

## 0.1.0

Queue board for capture, organization, and human-status verbs inside herdr.

Standard layout at 78×24, compact below. Human status is `ready`, `started`,
`blocked`, `review`, `done`. Mutating keys use Alt (Ctrl from the palette).
Peek, task page, project scope, done drawer, and undo are on the board.

Park, resume, linking, and dispatch-start are not board actions. Dispatch
recovery still opens if a persisted attempt is already in the store.

Per-task steps: a flat, ordered list on the task page (between notes and
the meta footer) with a step cursor and modifier-protected add, toggle,
rename, and delete verbs. Headless, `herdr-tasks steps <task-id> add <text>`
creates one step and `herdr-tasks steps <task-id> toggle <step-short-id>`
flips one step by unambiguous id prefix; single-task
`herdr-tasks list <task-id>` prints one line per step with its `[x]`/`[ ]`
state and step short id. Step progress never changes task status.

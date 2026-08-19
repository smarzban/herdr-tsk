# Changelog

## 0.1.0

Queue board for capture, organization, and human-status verbs inside herdr.

Standard layout at 78×24, compact below. Human status is `ready`, `started`,
`blocked`, `review`, `done`. Mutating keys use Alt (Ctrl from the palette).
Peek, task page, project scope, done drawer, and undo are on the board.

Park, resume, linking, and dispatch-start are not board actions. Dispatch
recovery still opens if a persisted attempt is already in the store.

# Board status verbs

Board status verbs let a user move the selected task between ready, started, blocked, review, and done with Ctrl chords while watching the board update. This recipe proves start (`ctrl+s`) and done (`ctrl+d`).

## Sub-features

- `board-select` moves selection with arrows or `j`/`k` until `▸` sits on the seeded title.
- `board-start` starts a ready task with `ctrl+s`.
- `board-done` marks the selected task done with `ctrl+d`. Bare `d` opens the done drawer; it does not mark done.

## How to get to it (user POV)

- Open the board with `tsk` or Herdr prefix+t.
- Select a ready task, then press `ctrl+s` to start. The row moves under **IN MOTION**.
- Press `ctrl+d` to mark done. The row leaves the live lanes. Press `d` to open the drawer and see **DONE**.

## Driving it with control-tsk

Preconditions:

- `control-tsk doctor` reports `"ok": true`.
- A desk task titled `Verify board start` exists at status `ready` (seed with the CLI feature if needed).
- Board is running at 80×24: `control-tsk board start`.

- **Seed if empty.** Run `control-tsk cli -- add -t "Verify board start" --desk --json` when the store has no matching task.
- **Confirm paint.** Run `control-tsk board wait "Verify board start"`. The decoded screen contains the title. First open also paints notice rows; Welcome (`N1`, review) is selected and a starter already sits under **IN MOTION**.
- **Select the seeded row.** Run `control-tsk board dump` and `control-tsk board keys j` (or `k`) until `▸` is on `Verify board start`. Do not send `ctrl-s` while Welcome is selected (`ctrl+s` is a no-op on review).
- **Start task.** Run `control-tsk board keys ctrl-s`.
- **Store proof.** Run `control-tsk store` (or `cli -- list <number> --json` using the number from add). The matching task has `"status":"started"`.
- **Screen proof.** Run `control-tsk board dump --path .cursor/skills/verify-tsk/evidence/$RUN_ID/board-start.txt`. The dump shows the title under **IN MOTION**, not merely that the header exists.
- **Mark done.** Run `control-tsk board keys ctrl-d`. Store (or `list --done`) shows `"status":"done"`. The title is gone from NEEDS YOU / IN MOTION / ON DECK.
- **Drawer.** Run `control-tsk board keys d` then `control-tsk board wait "DONE"`. Dump a `board-done.txt`. At 80×24 with notices filling the frame, the done row may sit below the fold; the **DONE** header plus store status is enough. Bare `d` toggles the drawer; `ctrl+d` does not open it.
- **Quit board.** Run `control-tsk board quit`.

## Gotchas

- Mutating verbs need Ctrl. Bare `s` does nothing; bare `d` toggles the drawer.
- A running board keeps the old binary until quit. Rebuild before `board start` when testing code changes.
- `wait "IN MOTION"` is not start proof: a starter notice already occupies that lane.
- Live Herdr pane smoke is a separate gate (`HERDR_ENV=1`) and is not covered here.

# Desk and projects tabs

Desk and projects tabs are the persistent destinations on every normal board surface. `1` opens desk, `3` opens the projects overview.

## Sub-features

- `tab-desk` focuses desk with `1`.
- `tab-projects` focuses the projects index with `3`.
- `tab-project-open` opens the selected index row with Enter (the harness cwd `here` row is present without a CLI-seeded project).

## How to get to it (user POV)

- Press `1` for desk, `3` for projects, or click the tab labels.
- Press Enter (or double-click) on a projects-index row to open that project in slot 2.
- Press `p` for the project picker (adjacent surface; not required for this recipe).

## Driving it with control-tsk

Preconditions:

- `control-tsk doctor` reports `"ok": true`.
- Board is running: `control-tsk board start`.

- **Desk.** Run `control-tsk board keys 1`. Then `control-tsk board wait "ON DECK"` and dump. Expect underlined `desk`, slot 2 = cwd basename + `▾` (not `select project`), no `Overview` / `PROJECT` legend, idle footer ` desk` or notice rows under `ON DECK · desk`. Do not wait for `desk` alone: that word is on the tab row of every surface.
- **Projects.** Run `control-tsk board keys 3`. Then `control-tsk board wait "/ search"` and dump. Expect `Overview ▾`; legend `PROJECT` + `NEEDS YOU` / `IN MOTION` / `READY`; one `cwd · here` row; status line = full cwd path; verbs `enter open · / search · ? help`. No `THREADS` at 80×24.
- **Open project.** Run `control-tsk board keys enter`. Dump: slot 2 still cwd, `all ▾` instead of `Overview`, no `PROJECT` legend, empty project paints `ON DECK` (not `· desk`) and `no open tasks here — p rescope or + add`.
- **Proof.** Dumps succeed without store corruption. `control-tsk store` still reads valid JSON if a store file exists.

## Gotchas

- Digits `1`/`2`/`3` are keyboard-only destinations applied outside the shared normal keymap table; they still work in this harness as literal key bytes.
- Project rows are navigation, never task rows. Do not send status verbs on the projects index.
- Slot 2 is empty (`select project`) only when there is no invocation directory. This harness cwd is a real non-git directory, so slot 2 shows that basename and the index has a `here` row.
- Closed-overview `/ search` is on the verb bar. Do not expect the open-search help line `/ search · type · enter open · esc clear`.

---
title: CLI
description: Headless tsk add and tsk list.
---

For scriptable board work:

```bash
tsk add -t "Draft release notes"
tsk list
```

`add` creates tasks without opening the TUI. `list` prints tasks from the store
without changing them.

Task numbers are store-global, so `tsk list 12` finds task `12` regardless of
your current directory. JSON output exposes that value as `number`; UUID lookup
continues to work too.

State is still `~/.tsk` whether you use the board, herdr, or the CLI.

See the app README and `skills/tsk-cli/SKILL.md` in
[herdr-tsk](https://github.com/smarzban/herdr-tsk) for exit codes, plan-shaped
bulk add, and retry rules.

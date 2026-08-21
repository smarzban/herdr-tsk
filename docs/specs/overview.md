# herdr-tasks specs

Living feature list. Shipped feature specs stay snapshots under `docs/specs/<feature>/`.

## Architecture

One process. Interactive surfaces (board, capture form) and headless surfaces (add, list) share the task domain and the document store. A positional command router selects the surface. Headless input is validated at a parser boundary; durable writes still go through the existing lock/merge store. The board watches that document and reloads; headless add does not talk to the board process.

## Features

- [Headless add](headless-add/headless-add.md): scriptable capture and read-only list on the board store (`herdr-tasks add`, `herdr-tasks list`).

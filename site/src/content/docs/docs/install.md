---
title: Install
description: Build tsk and open the board.
---

## Requirements

- Rust 1.96.0 (pinned in `rust-toolchain.toml`)
- Linux or macOS (Windows is not tested in CI)
- herdr 0.7.5 or newer, for plugin mode only

## Build

```bash
git clone git@github.com:smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
```

The binary is `target/release/tsk`.

## Standalone

```bash
./target/release/tsk
```

State lives in `~/.tsk/tsk.json`, with the previous version in `tsk.json.1`, a
pre-format-version backup in `tsk.json.v<N>` after a format migration, deleted
tasks in `trash.jsonl`, and a `tsk.json.lock` guarding writers. Override the
directory with `TSK_STATE_DIR` (`TSK_CONFIG_DIR` for config). Board, CLI, and
agents share the store safely: saves merge by task and revision, and an idle
board picks up an outside change within a quarter of a second.

Keep `~/.tsk` on a local disk. The writer lock is `flock`-style and every save
is a rename-based atomic replace; NFS, Dropbox, iCloud Drive, and similar
synced folders can break both. `TSK_STATE_DIR` is the escape hatch: point it
at a directory on a local disk.

```bash
tsk add -t "Draft release notes"
tsk list
```

## As a herdr plugin

The pane runs `./target/release/tsk`, so build before you link:

```bash
herdr plugin link "$PWD"
```

`herdr plugin list` should show `herdr-tsk` enabled against that path.

Then **Open tsk board**, or:

```bash
herdr plugin action invoke open-board --plugin herdr-tsk
```

It opens a **tsk** split beside the current pane. Invoke it again to focus the
board you already have.

**Quick capture** opens the capture form without a persistent board. Selected text
in the focused pane becomes the title:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

Rebuild after you pull. A running board keeps the old binary until you quit it.

herdr injects `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR`. tsk ignores
them on purpose: one store (`~/.tsk`) whether you are in herdr, another
multiplexer, or a bare terminal.

## Next

[Board](/docs/board/) · [Keys](/docs/keys/) · [Capture](/docs/capture/)

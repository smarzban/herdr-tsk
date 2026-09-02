---
title: Install
description: Build tsk and open the board.
---

## Requirements

- Rust 1.96.0 (pinned in `rust-toolchain.toml`)
- Linux or macOS
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

State lives in `~/.tsk/tsk.json`; walkthrough dismissal is kept beside it in
`walkthrough.json`. Override with `TSK_STATE_DIR` / `TSK_CONFIG_DIR`.

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

**Quick capture** opens the capture form without a persistent board:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

Rebuild after you pull. A running board keeps the old binary until you quit it.

herdr injects `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR`. tsk ignores
them on purpose: one store (`~/.tsk`) whether you are in herdr, another
multiplexer, or a bare terminal.

## Next

[Board](/docs/board/) · [Keys](/docs/keys/) · [Capture](/docs/capture/)

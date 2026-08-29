---
title: Install
description: Build tsk from source and open the board.
---

## Requirements

- Rust 1.96.0 (pinned in `rust-toolchain.toml`)
- Linux or macOS
- herdr 0.7.5 or newer (plugin mode only)

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

State lives in `~/.tsk` (`tsk.json` and `settings.json`). Override with
`TSK_STATE_DIR` / `TSK_CONFIG_DIR`.

```bash
tsk add -t "Draft release notes"
tsk list
```

## As a herdr plugin

```bash
herdr plugin link "$PWD"
```

Then **Open tsk board**, or:

```bash
herdr plugin action invoke open-board --plugin herdr-tsk
```

Rebuild after you pull. A running board keeps the old binary until you quit it.

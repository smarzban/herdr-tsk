---
title: Install
description: Install tsk, open the board, and connect it to Herdr.
---

## Homebrew

```sh
brew install smarzban/tap/tsk
```

Use `brew update && brew upgrade tsk` to upgrade and `brew uninstall tsk` to remove
it. The formula pins a release's platform URL and checksum, not `main`.

## Install script

Download and inspect the script, then run it:

```sh
curl -fsSL https://gettsk.sh/install.sh -o install-tsk.sh
sh install-tsk.sh
```

It verifies the latest stable release's SHA-256 and installs to `~/.local/bin`.
Add that directory to PATH. Requires `curl`, `tar`, and `sha256sum` or `shasum`.
`TSK_INSTALL_DIR` selects another absolute directory; `TSK_VERSION=vX.Y.Z` pins a
stable release tag. `--help` makes no downloads. Rerun to upgrade; remove the
installed executable to uninstall. No sudo, shell edits, or task-data changes.
Checksum failures and symlink destinations are refused without replacing the binary.

## Manual archives

Download `tsk-vX.Y.Z-<target>.tar.gz` and `SHA256SUMS` from the same
[release](https://github.com/smarzban/herdr-tsk/releases). Choose macOS ARM64/Intel
or Linux ARM64/x86-64 musl. Check the archive with `sha256sum` (Linux) or
`shasum -a 256` (macOS), compare its full digest with `SHA256SUMS`, then extract
`tsk` into a directory on PATH. Never run a binary whose checksum differs.

## Source build

Requires Rust 1.96.0 and Linux or macOS:

```sh
git clone https://github.com/smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

For source-checkout plugin development with Herdr 0.7.5+, run
`herdr plugin link "$PWD"` after building. Rebuild after pulling changes.

## Herdr setup with an installed binary

Requires Herdr 0.9+ on PATH:

```sh
tsk setup herdr
herdr server reload-config
```

Setup registers bundled plugin assets and the same stable installed binary, without
a checkout or second build. It adds **prefix+t** for the board and **prefix+a** for
quick capture, asking before replacing conflicts. Declining preserves the shortcut;
a noninteractive conflict aborts without changes. Accepted builtin conflicts remove
the override, restoring Herdr's default for that action. Config edits get a backup.

Rerun setup after upgrading: it replaces the registration and removes the intact old
managed asset root. Use the normal PATH command, not a versioned Homebrew Cellar path.
Installation alone never relinks a plugin. To remove integration, run
`herdr plugin unlink herdr-tsk`, remove its two command bindings, and reload Herdr.
[Setup details and recovery](https://github.com/smarzban/herdr-tsk/blob/main/packaging/README.md#herdr-setup-and-upgrades).

## Run and store

Run `tsk` to open the board, or `tsk add -t "Draft release notes"` to capture from
the CLI. Use one installation channel and reopen running boards after upgrading.
An older binary may refuse a newer store format.

Board and CLI share `~/.tsk/tsk.json`. Uninstalling the binary keeps task data.
Use `TSK_STATE_DIR` / `TSK_CONFIG_DIR` to override state/config directories. Keep them
on a local disk, not NFS or a synced folder, and use real directories, not symlinks.
Herdr-injected plugin directories do not change where tsk stores tasks.

## Next

[Board](/docs/board/) · [Keys](/docs/keys/) · [Capture](/docs/capture/)

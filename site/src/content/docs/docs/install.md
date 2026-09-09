---
title: Install
description: Install tsk, open the board, and connect it to Herdr.
---

Install tsk with the installer or Homebrew, then open the board from a
repository. Optional Herdr setup registers the plugin and shortcuts.

## Install

```sh
curl -fsSL https://gettsk.sh/install.sh | sh
```

Or install with Homebrew:

```sh
brew install smarzban/tap/tsk
```

The installer sets up PATH for Bash and Zsh. If prompted, reopen your terminal
or run the printed `export` command before continuing.

## Add to Herdr (optional)

With Herdr 0.9+ installed:

```sh
tsk setup herdr
herdr server reload-config
```

To give an agent the CLI skill instead, run `tsk setup claude` (or `pi`,
`cursor`, `codex`) after tsk is on PATH. That writes `tsk-cli/SKILL.md` into
the tool's user-level skills directory. `tsk setup --skill-dir <path>` is the
same write against any directory. See [CLI setup](/docs/cli/#setup).

## Open the board

Run `tsk`, or press **prefix+t** in Herdr.

## Add a task

```sh
tsk add -t "your task title"
```

Or press **prefix+a** in Herdr.

## Installer details

The script verifies the latest stable release's SHA-256 and installs to `~/.local/bin`.
It adds a duplicate-safe PATH entry to Bash or Zsh startup files when the directory
isn't already on PATH, without replacing existing content. Reopen the terminal or
run the printed `export` command to use `tsk` in the current shell.

Zsh uses `$ZDOTDIR/.zshrc` (otherwise `~/.zshrc`). Bash uses `~/.bashrc` and the
first existing login file (`~/.bash_profile`, `~/.bash_login`, or `~/.profile`).
Symlinked, non-regular or unwritable startup files are left alone. Other shells
require manual PATH setup; the installer prints guidance without undoing the install.

Requires `curl`, `tar`, `sed`, and `sha256sum` or `shasum`. `TSK_INSTALL_DIR` selects
another absolute directory (no colons or newlines); `TSK_VERSION=vX.Y.Z` pins a
stable release tag. `--help` makes no downloads. Rerun to upgrade; remove the
installed executable to uninstall. To undo PATH setup too, remove the `# tsk PATH`
comment and its following `case` line from the startup files listed above.
No sudo or task-data changes. Checksum failures
and symlink destinations are refused without replacing the binary or editing PATH.
For Homebrew upgrades, use `brew update && brew upgrade tsk`; remove it with
`brew uninstall tsk`. Reopen running boards after upgrades.

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
the override, restoring Herdr's default for that action. Successful config edits get a backup;
failed linking leaves no backup file.

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

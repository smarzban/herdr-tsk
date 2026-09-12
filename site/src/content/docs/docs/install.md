---
title: Install
description: Install tsk, connect Herdr, and give your agent the skill.
---

Available for macOS and Linux, on ARM64 and x86-64.

## Install

```sh
curl -fsSL https://gettsk.sh/install.sh | sh
```

Or use Homebrew:

```sh
brew install smarzban/tap/tsk
```

Reopen your terminal if prompted, or run the printed `export` command.

If Herdr is already installed, the curl installer asks whether to run `tsk setup herdr` when a terminal is available. When global agent skill roots are detected, it also asks once whether to install or update the tsk skill for those agents. The installer closes with one `Done.` line — `Done. Run tsk in a project directory to open the board`, plus `, or press prefix+t in Herdr` after an accepted Herdr setup — and one row per step that did not run: `Herdr plugin:  tsk setup herdr` when Herdr setup was declined, skipped under CI or no TTY, or failed, and `Agent skills:  tsk setup` when the skill ask was declined or skipped. With `TSK_INSTALL_DIR` set, the installer skips both asks and prints `Custom install directory: setup was not run. When you are ready:` with the full binary path in both rows. Homebrew stays noninteractive and prints the Herdr setup command as a caveat.

## Add to Herdr

[Install Herdr](https://herdr.dev/docs/install/) first. Installed-binary setup requires Herdr 0.9+.

```sh
tsk setup herdr
herdr server reload-config
```

| Shortcut | Action |
| --- | --- |
| **prefix+t** | Open or focus the board in this workspace |
| **prefix+a** | Quick capture |

Use your configured Herdr prefix. An existing board in this workspace is focused across tabs without resetting its view. If none exists here, a new board opens; other workspaces keep their boards. [Herdr keyboard guide](https://herdr.dev/docs/keyboard/).

## Agent skill

```sh
tsk setup
tsk setup pi
```

On a TTY, bare `tsk setup` detects installed agents and asks once to install or update the skill. Use `omp`, `claude`, `cursor`, `grok`, `codex`, or `opencode` instead of `pi` for a single named target. Setup installs the CLI skill in that agent's user-level skills directory; OMP uses its active profile.

[Detection, version updates, and overwrite options](/docs/cli/#setup).

## First task

Open the board and press `+`, type a title, then `Enter`. Or use the CLI:

```sh
tsk add -t "your task title"
```

Give your agent the task number to work on it. [Board guide](/docs/board/).

For standalone use, run `tsk` directly in your terminal.

## Upgrade

Run `tsk update` to upgrade an installer-managed copy. It immediately downloads and runs the same checksum-verifying installer used for the first install. A Homebrew copy stays under Homebrew's control: `tsk update` prints `brew update && brew upgrade tsk` instead.

| Installed with | Upgrade |
| --- | --- |
| Installer | `tsk update` |
| Homebrew | `brew update && brew upgrade tsk` |
| Source | Pull changes and rebuild |

Close and reopen running boards to use the new binary. After upgrading an installed Herdr plugin, rerun `tsk setup herdr`, then reload Herdr's config.

The board shows a notice when a newer release is available. [Update-check settings](/docs/storage/#update-check).

## Uninstall

| Installed with | Remove |
| --- | --- |
| Installer | Delete `tsk` from its installation directory |
| Homebrew | `brew uninstall tsk` |

Task data remains in place.

To remove Herdr integration, run `herdr plugin unlink herdr-tsk`, remove the two shortcut bindings, and reload Herdr's config.

To undo installer PATH changes, remove the `# tsk PATH` comment and its following `case` line from the startup files named during installation.

## Installer details

The installer verifies the release's SHA-256 checksum and installs to `~/.local/bin`. It does not require sudo or change task data.

| Setting | Purpose |
| --- | --- |
| `TSK_INSTALL_DIR` | Choose an absolute installation directory (no colons or newlines); the post-install setup asks are skipped |
| `TSK_VERSION=vX.Y.Z` | Install a specific stable release |
| `--help` | Show help without downloading |

Requires `curl`, `tar`, `sed`, and `sha256sum` or `shasum`.

### PATH

If the install directory is missing from PATH, the installer adds it to:

| Shell | Startup files |
| --- | --- |
| Zsh | `$ZDOTDIR/.zshrc`, or `~/.zshrc` |
| Bash | `~/.bashrc` and the first existing login profile; defaults to `~/.profile` |

The installer does not source these files. Symlinked, non-regular, or unwritable files are skipped with manual instructions. Other shells require manual PATH setup. Successful edits remain if another file cannot be updated.

### Herdr prompt

When `herdr` is on PATH after the binary is installed, the installer may ask to run plugin setup:

- Interactive terminal (stdin TTY, or `/dev/tty` under `curl | sh`) using the default `~/.local/bin` destination: asks `[y/N]`. Yes runs the newly installed `tsk setup herdr`. An overridden `TSK_INSTALL_DIR` never executes the newly published binary; the closing block prints the `tsk setup herdr` and `tsk setup` commands with the full binary path instead.
- `CI` set, or no usable TTY: skips the ask so the install never hangs.
- Herdr absent: no Herdr prompt.

The install always ends with one closing block:

- Setup accepted: `Done. Run tsk in a project directory to open the board, or press prefix+t in Herdr.`
- Declined, CI/no-TTY skip, or setup failure: `Done. Run tsk in a project directory to open the board.` followed by a `    Herdr plugin:  tsk setup herdr` row.
- Agents detected but the skill batch declined or skipped: a `    Agent skills:  tsk setup` row.
- Herdr absent: the `Done.` line only (no Herdr row).

Setup failures do not undo the install. Homebrew does not run this prompt; use the formula caveat or run `tsk setup herdr` yourself.

## Herdr setup with an installed binary

Setup registers bundled plugin files using the installed executable. No source checkout is needed.

- Shortcut conflicts ask for confirmation. Declining keeps the existing binding, and the output says so.
- A noninteractive conflict stops before writing.
- Config changes create a backup beside the config, `config.toml.tsk-backup-<YYYYMMDD-HHMMSS>` in UTC; a second setup within the same second appends `-1`, `-2`, ....
- Rerunning setup updates the registration without duplicating bindings.
- Use the stable command on PATH, not a versioned Homebrew Cellar path.

Installation alone does not update an existing plugin registration unless you answer yes to the curl installer's Herdr prompt. [Setup recovery and configuration](https://github.com/smarzban/herdr-tsk/blob/main/packaging/README.md#herdr-setup-and-upgrades).

## Manual archives

Download the archive for your platform and `SHA256SUMS` from the same [release](https://github.com/smarzban/herdr-tsk/releases).

| Platform | Archive target |
| --- | --- |
| macOS Apple silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Linux ARM64 | `aarch64-unknown-linux-musl` |
| Linux x86-64 | `x86_64-unknown-linux-musl` |

Archives are named `tsk-vX.Y.Z-<target>.tar.gz`. Verify with `shasum -a 256` on macOS or `sha256sum` on Linux. Compare the full digest with `SHA256SUMS`, then extract `tsk` into a directory on PATH.

## Source build

Requires Rust 1.96.0:

```sh
git clone https://github.com/smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

For plugin development from a checkout, Herdr 0.7.5+ supports `herdr plugin link "$PWD"`. Rebuild after pulling changes.

[Contributing](https://github.com/smarzban/herdr-tsk/blob/main/CONTRIBUTING.md) · [Storage and configuration](/docs/storage/)

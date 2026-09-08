---
title: Install
description: Build tsk and open the board.
---

## Native packages (not published yet)

Release packaging is being prepared. The installer and Homebrew commands below are
for use **after the first packaged release and tap are published**; they are not
working public install routes yet. Until then, use the source build below.

The planned archives support Apple Silicon and Intel macOS (deployment target 11+),
and ARM64 and x86-64 Linux using musl. No Rust toolchain is needed to run a prebuilt
binary. Native packaging installs standalone `tsk`; plugin linking still uses the
source checkout described below.

### Install script

After publication, download and inspect the script, then run it:

```sh
curl -fsSL https://gettsk.sh/install.sh -o install-tsk.sh
sh install-tsk.sh
```

It downloads the latest published stable GitHub release, never a build of `main`,
verifies the archive's SHA-256 against that release's `SHA256SUMS`, and installs to
`~/.local/bin`. It does not use sudo or edit shell configuration. Add that directory
to your PATH if it is not there already. Requirements: `curl`, `tar`, and
`sha256sum` (Linux) or `shasum` (macOS).

`sh install-tsk.sh --help` shows usage without downloading anything.
`TSK_INSTALL_DIR` selects another absolute installation directory. `TSK_VERSION`
pins an existing stable tag, for example `TSK_VERSION=v0.5.1 sh install-tsk.sh`
(the example is not a claim that this version is published). Unsupported platforms,
missing assets and checksum failures refuse installation. A checksum failure leaves
an existing binary unchanged. A symlink at the destination is refused; use that
installation's package manager instead.

Run the script again to upgrade. To uninstall, remove only the installed executable,
for example `rm "$HOME/.local/bin/tsk"`. Task data in `~/.tsk` is not removed.

### Homebrew

Once the tap and release assets are public:

```sh
brew install smarzban/tap/tsk
```

Alternatively, `brew tap smarzban/tap` once, then `brew install tsk`. The formula
pins a specific release's platform URL and checksum, so Homebrew follows formula
updates, not `main` or GitHub's latest-release endpoint.

```sh
brew update
brew upgrade tsk
brew uninstall tsk
```

Use one installation channel to avoid multiple `tsk` binaries on PATH. Quit a
running board and reopen it after upgrading. Downgrading the binary does not
migrate task data backwards; an older binary may refuse a newer store format.

### Manual archives and checksums

Each release will provide `tsk-vX.Y.Z-<target>.tar.gz` and `SHA256SUMS`. Download both
from the same release, verify the selected archive, then extract the `tsk` binary.
On Linux use `sha256sum <archive>`; on macOS use `shasum -a 256 <archive>` and compare
the full digest with that archive's entry in `SHA256SUMS`. Do not run a binary whose
checksum differs. These are checksummed downloads over HTTPS, not signed binaries;
the checksums and binaries share GitHub's trust boundary.

crates.io publishing is a separate follow-up and is not available yet.

## Source-build requirements

- Rust 1.96.0 (pinned in `rust-toolchain.toml`)
- Linux or macOS (Windows is not tested in CI)
- herdr 0.7.5 or newer, for plugin mode only

## Build

```bash
git clone https://github.com/smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
```

The binary is `target/release/tsk`. Run it with that path, or put the directory
on `PATH` for this shell:

```bash
export PATH="$PWD/target/release:$PATH"
```

## Standalone

```bash
./target/release/tsk
```

State lives in `~/.tsk/tsk.json`, with the previous version in `tsk.json.1`, a
pre-format-version backup in `tsk.json.v<N>` after a format migration, deleted
tasks in `trash.jsonl`, and a `tsk.json.lock` guarding writers. Everything there
is user-private on Unix: the directory is `0700` and the files `0600`. Looser
modes left by an older version are tightened on the next launch; stricter ones
are kept. Override the directory with `TSK_STATE_DIR` (`TSK_CONFIG_DIR` for
config). Board, CLI, and agents share the store safely: saves merge by task and
revision, and an idle board picks up an outside change within a quarter of a
second.

Keep `~/.tsk` on a local disk. The writer lock is `flock`-style and every save
is a rename-based atomic replace; NFS, Dropbox, iCloud Drive, and similar
synced folders can break both. `TSK_STATE_DIR` is the escape hatch: point it
at a directory on a local disk. State and config directory roots must be real
directories, not symlinks.

```bash
./target/release/tsk add -t "Draft release notes"
./target/release/tsk list
```

After `export PATH="$PWD/target/release:$PATH"`, the same commands work as
`tsk add` and `tsk list`.

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

**Quick capture** opens the expanded quick-add page in a short-lived popup, without
a persistent board. Selected text in the focused pane becomes the title:

```bash
herdr plugin action invoke quick-capture --plugin herdr-tsk
```

Rebuild after you pull. A running board keeps the old binary until you quit it.

herdr injects `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR`. tsk ignores
them on purpose: one store (`~/.tsk`) whether you are in herdr, another
multiplexer, or a bare terminal.

## Next

[Board](/docs/board/) · [Keys](/docs/keys/) · [Capture](/docs/capture/)

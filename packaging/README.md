# Native distribution

The GitHub `Prepare release` workflow is manually dispatched and creates a **draft**, never a published release. Publishing and changing repository visibility require owner approval.

## Artifacts

Each stable `vX.Y.Z` release has four archives:

| Target | Build runner |
| --- | --- |
| `aarch64-apple-darwin` | `macos-15` |
| `x86_64-apple-darwin` | `macos-15-intel` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| `x86_64-unknown-linux-musl` | `ubuntu-24.04` |

Names are `tsk-vX.Y.Z-<target>.tar.gz`, containing `tsk`, `LICENSE`, and `README.md` at the archive root. macOS builds set `MACOSX_DEPLOYMENT_TARGET=11.0`. Linux builds use a native musl compiler rather than depending on the runner's glibc version. Actual minimum-OS installation still needs release smoke testing, not just a successful compile.

`SHA256SUMS` covers the four archives and `install.sh`. `tsk.rb` is generated from those exact archive digests. This is checksum verification over HTTPS, not code signing or independent publisher authentication. The checksum file and binaries share the GitHub trust boundary.

## Local packaging

Python 3.11+ is required for maintainer scripts, not end-user installation. Set `TSK_TEST_BINARY` to a built binary to include the installer binary smoke and setup PTY test; Rust CI runs these after its release build. The installer workflow runs packaging tests and ShellCheck on installer, release-script, or packaging-test changes without rebuilding Rust.

```sh
python3 -m unittest discover -s tests/packaging
shellcheck site/public/install.sh
python3 scripts/release.py check-version v0.5.0
```

For a real release, use its actual stable tag and matching Cargo version, not the example above. Build from that tag with `cargo build --release --locked --target <target>`, then:

```text
python3 scripts/release.py package <tag> <target> <binary-path> <output-directory>
python3 scripts/release.py assemble <tag> <output-directory>
```

Package refuses to overwrite an archive. Assemble requires all four platform archives and validates their contents before writing checksums or a formula. It refuses existing files or symlinks for every output (install.sh, SHA256SUMS, tsk.rb); use a clean output directory. These commands do not build or validate the machine architecture of a supplied binary; the workflow's native build matrix owns that contract. Do not feed one host binary into multiple targets for a real release.

## Owner-gated release sequence

1. Review and merge the packaging, choose the next version, and update the crate, lockfile, plugin and site version together. Obtain approval before pushing the stable tag. The old `v0.5.0` tag does not include this workflow's scripts and cannot be used to package this change.
2. Confirm Actions runner availability and billing, then run `Prepare release` from the reviewed workflow with that existing tag. It resolves the tag once to a commit, uses the same commit for every platform, tests the target build, and checks that the tag has not moved before creating a draft. There is no automatic tag creation, public publishing, or replacement of existing releases. A tiny tag-move race remains between the final check and GitHub creating the draft: inspect the draft tag before publishing and enable GitHub immutable releases when available.
3. Review the `release-bundle` workflow artifact and draft assets. Test installation on every supported architecture, including Homebrew. Replace the draft checklist with actual release notes. If a draft/upload was interrupted, inspect it first; the workflow intentionally refuses an existing release rather than silently replacing artifacts. Delete an incomplete draft only with approval before retrying.
4. Obtain approval to make the source repository publicly accessible if still private, and publish the release. The anonymous installer cannot download private or draft assets. Mark the intended stable release as latest; the installer uses GitHub's latest-release redirect, not a semantic-version sort or `main`.
5. Create the public `smarzban/homebrew-tap` repository with approval, using `packaging/homebrew/` as its scaffold. Put the generated `tsk.rb` in `Formula/tsk.rb`, test it, then commit and push. Repeat the formula update for every release, only after that release's assets exist publicly. No cross-repository write token is configured here.
6. Smoke the public installer and tap on clean machines.

## User contracts

The installer uses `curl`, `tar`, `sed`, and `sha256sum` or `shasum`. It detects the running OS/architecture (an Intel/Rosetta shell selects Intel), defaults to `~/.local/bin`, and accepts `TSK_INSTALL_DIR` and a stable `TSK_VERSION=vX.Y.Z`. It verifies the selected archive before staging and atomically replacing the executable. It refuses a symlink or directory at the destination rather than modifying a package-manager-owned installation. After installation, it appends a duplicate-safe PATH entry for Bash (`~/.bashrc` and the first existing login profile, default `~/.profile`) or Zsh (`${ZDOTDIR-$HOME}/.zshrc`) when needed. It never sources startup files or overwrites their content, and leaves symlinked, non-regular or unwritable files alone with manual guidance. Unsupported shells also get manual guidance. PATH coaching prints reopen/export guidance when needed. When `herdr` is already on PATH, an interactive install (stdin TTY or usable `/dev/tty`) asks whether to run the newly installed `tsk setup herdr`; `CI` or no TTY skips the ask so the install never hangs. The install always ends with a short wrap-up: board-only when Herdr is absent; board plus `prefix+t` after a successful setup; board plus `tsk setup herdr` when declined, CI/no-TTY skipped, or setup failed. Herdr absent prints no setup nudge. Setup failure does not undo the binary install. To undo PATH setup on uninstall, remove the `# tsk PATH` comment and its following `case` line from the startup files listed above. Install paths cannot contain PATH separators (colon/newline). Task data is untouched. The binary embeds the plugin manifest/launchers: explicit `tsk setup herdr` materializes them, registers the stable installed executable path, and installs conflict-confirmed prefix+t / prefix+a bindings. Herdr 0.9+ is required for setup. No second binary or archive assets are needed. Run the same installer to upgrade, or pin an older published release to select that binary; an older binary may refuse newer task-store formats.

Homebrew follows the formula's explicit release URLs and SHA-256 values. A release alone does not update Homebrew. Homebrew installs are noninteractive and do not run the curl installer's Herdr prompt; the formula caveat tells users to run `tsk setup herdr`. Uninstall through the channel used to install: `brew uninstall tsk`, or remove the installer-created executable. Neither should delete `~/.tsk`.

## References

- [Homebrew tap structure](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Formula configuration and tests](https://docs.brew.sh/Formula-Cookbook)
- [GitHub runner images](https://github.com/actions/runner-images)

crates.io publishing is separate task T29. `publish = false` remains in Cargo.toml.

## Herdr setup and upgrades

`tsk setup herdr` requires Herdr 0.9+ on PATH. It uses `HERDR_CONFIG_PATH`, otherwise
`$XDG_CONFIG_HOME/herdr/config.toml`, otherwise `~/.config/herdr/config.toml`.
Empty config-path environment values are treated as unset. Config-file and final-directory symlinks are refused. An open directory descriptor anchors all setup writes,
backups, renames and cleanup; a replaced parent cannot redirect them. A kernel lock
on `.tsk-setup.lock` serializes setup and is released on process death. The lock file
stays on disk and does not imply a running setup.

The embedded manifest and launchers are materialized in `tsk-plugins/<content-hash>`
beside that config. The root name uses fixed FNV-1a-64 over versioned, length-prefixed UTF-8 asset names and contents, not Rust's implementation-dependent DefaultHasher. This is change detection, not cryptographic authentication. The manifest includes the crate version, so upgrading changes
the root. Herdr 0.9.0 replaces registrations by plugin ID: the online handler inserts
into its ID-keyed map; the offline CLI removes entries with that ID then inserts
the replacement. Setup checks the new registration before removing the intact old
managed root. It does not call `plugin unlink herdr-tsk` after linking: that would
unlink the new registration. Source-checkout and other installation roots are never
removed. A missing prior root needs no cleanup. Modified stale assets are retained and reported with their path.

References: Herdr 0.9.0
[`handle_plugin_link`](https://github.com/herdrdev/herdr/blob/v0.9.0/src/app/api/plugins/mod.rs)
and [`persist_plugin_offline`](https://github.com/herdrdev/herdr/blob/v0.9.0/src/cli/plugin.rs).

Setup retains the invoked binary path, including the unversioned Homebrew symlink,
not its canonical Cellar target. Board and capture share it. Reopen boards after a
binary upgrade and rerun setup to update the manifest and launchers. No installer
runs setup automatically. Explicit setup replaces an existing `herdr-tsk` link.

The shortcut planner preserves the configured prefix and unrelated settings.
It asks before replacing each conflicting prefix+t / prefix+a binding. Declining
keeps that shortcut; a noninteractive conflict aborts before filesystem or host
changes. If an accepted builtin override has no remaining bindings, it is removed,
restoring Herdr's default. Candidate config is checked by Herdr before registration;
existing config bytes are staged in a temporary backup before linking. The config is replaced atomically afterward, then the backup gets its final name. A failed link removes the temporary backup; a config replacement or backup-promotion failure names any retained recovery copy.
Registration failure leaves config intact; post-registration failures identify the
partial state. Success prints the plugin root and any backup path. Reload config
with `herdr server reload-config` or restart Herdr to apply shortcuts.

To remove integration, close its board/popups, run `herdr plugin unlink herdr-tsk`,
remove its two command bindings, and reload config. Restore a setup backup only if
it will not discard later edits. Removing the binary alone leaves Herdr config and
`~/.tsk` intact.

For isolated smoke tests, override XDG config/state roots **and** `HERDR_SOCKET_PATH`,
as well as `TSK_STATE_DIR` / `TSK_CONFIG_DIR`. `HERDR_CONFIG_PATH` alone does not
isolate Herdr's running session or plugin registry. Never relink a daily plugin for
a test. A failed asset integrity check names the file to inspect; do not erase an
unrelated checkout or package-manager installation to recover setup.

Review scope decisions (F-1/F-7/F-16, F-6, F-11): ancestor symlinks in the Herdr
config path are trusted; the final directory is fd-pinned and final-component
symlinks are refused. Rejecting a symlinked `~/.config` would break common dotfile
setups, and Herdr follows that ancestor too. `TSK_INSTALL_DIR` is likewise a trusted
installation location: an attacker who can swap it can replace the executable
directly, so protecting that directory is the owner's boundary (F-6). F-11 is an
accepted workflow-wiring coverage limitation: artifact and draft-handoff tests do
not prove the native runner matrix; owner-run release smoke remains that check.

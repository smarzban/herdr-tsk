# Native distribution

Status: preparation only. No tap repository, tag, or release is created by these local scripts.
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

## Local preparation

Python 3.11+ is required for maintainer scripts, not end-user installation.

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

Package refuses to overwrite an archive. Assemble requires all four platform archives and validates their contents before writing checksums or a formula. These commands do not build or validate the machine architecture of a supplied binary; the workflow's native build matrix owns that contract. Do not feed one host binary into multiple targets for a real release.

## Owner-gated release sequence

1. Review and merge the packaging, choose the next version, and update the crate, lockfile, plugin and site version together. Obtain approval before pushing the stable tag. The old `v0.5.0` tag does not include this workflow's scripts and cannot be used to package this change.
2. Confirm Actions runner availability and billing, then run `Prepare release` from the reviewed workflow with that existing tag. It resolves the tag once to a commit, uses the same commit for every platform, tests the target build, and checks that the tag has not moved before creating a draft. There is no automatic tag creation, public publishing, or replacement of existing releases. A tiny tag-move race remains between the final check and GitHub creating the draft: inspect the draft tag before publishing and enable GitHub immutable releases when available.
3. Review the `release-bundle` workflow artifact and draft assets. Test installation on every supported architecture, including Homebrew. Replace the draft checklist with actual release notes. If a draft/upload was interrupted, inspect it first; the workflow intentionally refuses an existing release rather than silently replacing artifacts. Delete an incomplete draft only with approval before retrying.
4. Obtain approval to make the source repository publicly accessible if still private, and publish the release. The anonymous installer cannot download private or draft assets. Mark the intended stable release as latest; the installer uses GitHub's latest-release redirect, not a semantic-version sort or `main`.
5. Create the public `smarzban/homebrew-tap` repository with approval, using `packaging/homebrew/` as its scaffold. Put the generated `tsk.rb` in `Formula/tsk.rb`, test it, then commit and push. Repeat the formula update for every release, only after that release's assets exist publicly. No cross-repository write token is configured here.
6. Smoke the public installer and tap on clean machines. Remove the site's "not published yet" notices only when those install routes work. A source build remains the documented working route until then.

## User contracts

The installer uses `curl`, `tar`, and `sha256sum` or `shasum`. It detects the running OS/architecture (an Intel/Rosetta shell selects Intel), defaults to `~/.local/bin`, and accepts `TSK_INSTALL_DIR` and a stable `TSK_VERSION=vX.Y.Z`. It verifies the selected archive before staging and atomically replacing the executable. It refuses a symlink or directory at the destination rather than modifying a package-manager-owned installation. It does not add PATH entries or touch task data. Run the same installer to upgrade, or pin an older published release to select that binary; an older binary may refuse newer task-store formats.

Homebrew follows the formula's explicit release URLs and SHA-256 values. A release alone does not update Homebrew. Uninstall through the channel used to install: `brew uninstall tsk`, or remove the installer-created executable. Neither should delete `~/.tsk`.

## References

- [Homebrew tap structure](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Formula configuration and tests](https://docs.brew.sh/Formula-Cookbook)
- [GitHub runner images](https://github.com/actions/runner-images)

crates.io publishing is separate task T29. `publish = false` remains in Cargo.toml.

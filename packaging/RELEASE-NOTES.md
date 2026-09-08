## Installation

Download the archive for your platform and verify it against `SHA256SUMS` before extracting `tsk`.
The Linux builds target musl (ARM64 and x86-64); macOS builds target Apple Silicon and Intel with a macOS 11 minimum deployment target.

The attached `install.sh` installs the latest published stable release into `~/.local/bin` by default. Set `TSK_VERSION` to this release's tag to pin it, and `TSK_INSTALL_DIR` to choose another absolute directory. The installer never builds from main, uses sudo, edits your shell configuration, or modifies task data.

The generated `tsk.rb` is for the maintainer's Homebrew tap, not an application file to install by hand. Homebrew availability depends on the corresponding formula being published in `smarzban/homebrew-tap`.

Run `tsk setup herdr` explicitly to register the embedded plugin assets with Herdr 0.9+, sharing the same installed binary for board and capture. Setup adds prefix+t and prefix+a, asks before replacing conflicts, and requires an interactive terminal if any conflict exists. Reload Herdr configuration afterwards. Installing/upgrading alone does not relink a plugin.

## Maintainer checklist before publishing

Replace this checklist with reviewed release notes. Verify all four archives, their checksums, and actual platform installation. Confirm the release tag/version and public repository access, then publish explicitly. Only after the assets are public should the generated formula be tested and committed to the tap. Do not advertise Homebrew availability until that step passes.

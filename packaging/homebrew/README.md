# smarzban Homebrew tap

This is the local scaffold for the proposed `smarzban/homebrew-tap` repository.
The tap is not published yet. Add `Formula/tsk.rb` from the reviewed release bundle,
then test and publish the tap after owner approval. Do not create placeholder
checksums or point a formula at an unpublished release.

Once published, users can install with:

```sh
brew install smarzban/tap/tsk
```

Or add the tap once, then use the short name:

```sh
brew tap smarzban/tap
brew install tsk
```

Updates and removal:

```sh
brew update
brew upgrade tsk
brew uninstall tsk
```

The formula installs versioned, checksummed GitHub release binaries, not builds
from main. It does not install the herdr plugin or remove task data when uninstalled.

## Maintenance

Copy this README into the tap repository, replace the preparation notice above once
published, and add the generated `tsk.rb` under `Formula/`. Update the formula for
every release, after its assets are public. Keep the old formula until then.

On each supported platform, tap the checkout and run:

```sh
brew tap smarzban/tap /absolute/path/to/homebrew-tap
brew install smarzban/tap/tsk
brew test smarzban/tap/tsk
brew audit --strict smarzban/tap/tsk
```

Run these in a clean test environment, not over someone's existing installation.
The formula test captures a task in Homebrew's temporary test directory, not `~/.tsk`.
Include these checks in the tap's own CI before publishing formula updates.

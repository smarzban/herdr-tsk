# Contributing to tsk

Thanks for helping improve tsk. Bug reports and focused pull requests are welcome.

## Development setup

Requirements:

- Rust 1.96.0, pinned by `rust-toolchain.toml`
- Linux or macOS
- Node.js 22 when changing `site/`
- herdr 0.7.5 or newer to link the checkout as a plugin; 0.9.0 or newer to smoke `tsk setup herdr`

```bash
git clone https://github.com/smarzban/herdr-tsk.git
cd herdr-tsk
cargo build
```

Run the binary with isolated state while developing changes that write tasks:

```bash
TSK_STATE_DIR=/tmp/tsk-dev-state TSK_CONFIG_DIR=/tmp/tsk-dev-config cargo run
```

Do not point development builds at a board containing data you care about.

For local website setup, builds, and deployment configuration, see the
[site README](site/README.md).

## Making a change

Keep changes focused and add regression coverage for behavior changes. A regression test must fail without its fix and pass with it. User-visible behavior changes must update the relevant page under `site/src/content/docs/docs/`, plus the README, landing page, demo, or `site/public/llms.txt` when they describe the changed behavior.

The web demo intentionally uses bare verb keys because browsers reserve control chords. The TUI continues to require Ctrl for mutating verbs.

## Verification

Run the Rust green bar:

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
```

For site changes:

```bash
cd site && npm ci && npm test && npm run build
```

Changes affecting the board, host integration, or panes should also be exercised through the real herdr flow. Use isolated `TSK_STATE_DIR` and `TSK_CONFIG_DIR` values when the smoke test mutates tasks.

## Pull requests

Describe the user-visible result and include the commands and manual flows you verified. Keep generated directories such as `target/`, `site/node_modules/`, and `site/dist/` out of commits.

Security vulnerabilities use a private reporting path, not a public issue. See [SECURITY.md](SECURITY.md).

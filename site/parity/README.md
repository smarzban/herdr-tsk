# Demo parity harness

The harness compares the browser demo with Rust-rendered reference frames.
It also exercises the built homepage, project/thread navigation, task steps,
selection styling, and the sample CLI output using a disposable store.

## Run

From `site/`, with Rust 1.96.0 and Node installed:

```sh
npm ci
npx playwright install chromium
npm run test:parity
```

The command builds the release CLI and site, regenerates references, and runs
Playwright. Installed Chrome is used on macOS; `PARITY_CHROME` overrides it.
To regenerate references alone, run `npm run parity:reference`.
The ignored Rust generator writes to `site/parity-reference/`.

Port 4178 serves the fixed fixture in `tests/fixtures/demo-parity/store.json`.
Port 4180 serves the actual production build. Both bind only to localhost.
Browser contexts are fresh; CLI checks use temporary state, never user tasks.

For a real release-binary PTY smoke, from the repository root:

```sh
python3 -m pip install -r scripts/parity-requirements.txt
cargo build --release
python3 scripts/parity-pty-smoke.py
```

The harness does not establish complete visual parity. Full field editing,
project archive, save recovery, and complete Unicode/emoji wrapping are outside
its coverage. Browser status shortcuts intentionally use bare keys.

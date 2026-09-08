# First board parity slice

This harness exercises the shipping `public/board-demo.js` and `src/styles/landing.css`, with the same four tasks as the Rust app. It does not serve a second implementation of the board. The fixture is `../../tests/fixtures/demo-parity/store.json`: fixed IDs, numbers, timestamps, project/thread labels and a title that crosses every tested width.

## Regenerate and check

From `site/`:

```sh
npm ci
# macOS uses installed Google Chrome. Else install Playwright Chromium:
npx playwright install chromium
npm run test:parity
```

`PARITY_CHROME=/absolute/path/to/chrome` overrides the executable. The harness binds only to localhost port 4178. `npm run parity:reference` regenerates app references without starting a browser. Neither command reads the user's task store. Browser runs always use a fresh context.

The nine scenarios cover the initial desk; keyboard selection/start/open/Escape; complete long-title text, continuation alignment and two-cell right clearance; project, thread and unlabeled peeks; and the 109/110-column threshold plus rail-click focus, and the landing column readout excluding padding. Browser shortcuts remain bare intentionally. The title lines are compared directly with the Rust renderer's output, not with independently authored expected strings. Other checks assert DOM outcomes and geometry; they are not a whole-frame pixel equality claim.

Outputs (gitignored):

- `parity-reference/app-*.txt`: Ratatui **TestBackend** frames at 40×24, 78×24, 109×24 and 110×24; these are not terminal screenshots.
- `parity-reference/title-*.json`: app-derived title lines consumed by the browser checks.
- `parity-results/*/*.png`: actual Chrome screenshots cropped to the board root; 14px monospace, 20px line height, explicit usable column width and 24 rows. The website shell is excluded. These are inspection artifacts, not approved screenshot baselines.
- `parity-reference/browser-provenance.json`: source revision, working diff, input hashes and browser version.

Browser fixture time is frozen at Unix 1700000200. App renders and actual PTY runs use the real clock, so task-page age strings are deliberately not compared. The fixture uses ASCII titles for exact line matching. CJK/combining examples have unit coverage; complete Unicode-width and emoji-cluster parity remains unverified.

## Real terminal execution

From the repository root, build the binary, then run:

```sh
cargo build --release
python3 -m venv /tmp/tsk-parity-venv
/tmp/tsk-parity-venv/bin/pip install -r site/parity/requirements.txt
/tmp/tsk-parity-venv/bin/python site/parity/terminal-smoke.py
```

Each size gets a newly created temporary state/config directory and the shared fixture (project paths relocate to a temporary project with the same basename). The script launches the actual release binary in a controlling PTY, drives its real keys, decodes the emitted ANSI with pyte, checks the page/back screen and verifies the persisted start in the scratch store, then quits and removes the store. `parity-reference/pty/` retains ANSI transcripts, decoded screen text and provenance. This is actual process/terminal-stream evidence, **not a GUI terminal screenshot or a Herdr host smoke**. When `HERDR_ENV=1`, the repository still requires a separate real Herdr flow.

## Provenance and limits of this slice

The implementation started from clean `36ed5c4` (PR #45), following the earlier inventory at `3fb5972`. PR #45 already moved attribution out of collapsed rows and into peeks and removed CSS ellipsis. This slice aligns continuation indentation and widths, omits empty IN MOTION, restores ON DECK · desk, keeps navigation tabs on project boards, makes a narrow page replace the board with one footer, and returns rail clicks to board focus. The canvas uses monochrome selection while website chrome keeps its theme.

The regression scenario failed against the landed demo with only fixture injection retained: initial desk incorrectly contained IN MOTION 0. The padding/readout check also failed with its measurement fix removed (82 reported columns instead of 78). Both fixes were restored before final checks. Existing source-presence assertions for replaced rendering expressions were removed in favor of behavioral checks; docs/static anatomy checks remain.

Remaining work from the inventory is not silently excluded: transactional creation/editing and validation, steps, local/cross-project thread filters, project archive/read-only focus, save recovery, deletion/undo, palette/help completeness, project index clicks/search, scroll/copy behavior and complete Unicode/markdown rendering. Full task-page visual equivalence (including steps and metadata geometry), compact height behavior at 40×10, typography/style parity and whole-frame visual baselines are also outstanding. CLI, host launch/reopen, persistence simulation and website-only controls still need explicit scope choices. This first slice does not claim those features match.

Validation for the initial slice: 957 Rust tests passed, 5 ignored; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo build --release` passed. Site unit checks: 16 passed. Browser checks: 9 passed. Real isolated PTY flow: all four sizes passed. Astro production build passed with existing deprecation/404-content notices. No Herdr smoke was run because `HERDR_ENV` was unset.

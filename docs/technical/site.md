# Site

**Responsibility.** Marketing landing page and operator docs. Astro 7 + Starlight
0.41 in `site/`. **Not** packaged into the `tsk` binary. `cargo build` ignores it.

**Public surface (product, not rustdoc).** Production
https://tsk-gules.vercel.app. Vercel root directory `site`. `site/vercel.json`
pins `npm ci`, `npm run build`, output `dist`, framework `astro`, and
`ignoreCommand` so a commit that does not touch `site/**` skips the deploy.

## How it works

| Path | Role |
| --- | --- |
| `src/pages/index.astro` | Custom homepage (not the Starlight index) |
| `src/pages/privacy.astro`, `terms.astro` | Legal pages |
| `src/content/docs/docs/` | User guide (Starlight): install, board, keys, capture, task page, steps, CLI |
| `src/content/docs/404.md` | Docs 404 |
| `public/board-demo.js`, `capture.js`, `landing.js`, `theme.js` | Interactive demo + chrome |
| `src/styles/landing.css`, `starlight.css` | Page styles |
| `src/components/ThemeSelect.astro` | Starlight `ThemeSelect` slot (`astro.config.mjs`) |
| `src/components/Wordmark.astro` | Landing/legal pages only (not a Starlight override) |
| `scripts/generate-apple-touch.mjs` | Apple touch icons; run before `dev`/`build` |
| `test/site.test.mjs` | `node --test` |

Site config (`astro.config.mjs`): `site: 'https://tsk-gules.vercel.app'`, sitemap,
Starlight title `tsk`, GitHub edit links under `.../edit/main/site/`, sidebar
Overview / Install / Board / Keys / Capture / CLI. Default theme script prefers
dark (`tsk-theme` / `starlight-theme` in localStorage).

CI: `.github/workflows/site.yml` runs `npm ci && npm test && npm run build` in
`site/`. The Rust workflow `paths-ignore`s `site/**`.

The web board demo uses **bare** verb keys. Browsers steal Alt-chords; matching
the TUI's modifier requirement would make the demo inoperable. When keymap,
status verbs, or tab/section semantics change, update
`site/src/content/docs/docs/{keys,board,capture,cli}.md` **and**
`public/board-demo.js` together.

## Invariants

- Do not fold this tree into the Rust crate or the green-bar `cargo test`.
- Do not “fix” demo keys to require Alt.
- `node_modules/` and `dist/` are generated (excluded from the coverage ledger).

## Error paths

Site tests fail the site workflow only. A docs typo cannot break `tsk`. A missed
`board-demo.js` update ships a demo that lies about the board.

## Extension points

New operator pages: add a Starlight markdown file and a sidebar entry. New
landing sections: `index.astro` + `landing.css` + `landing.js`. Keep the binary
and the site's claims aligned by the dual update rule above.

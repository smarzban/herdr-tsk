# tsk

**A task board for your terminal.**

tsk captures work and moves it through human status: ready, started, blocked,
review, done. It ships as a [herdr](https://herdr.dev) plugin, and the `tsk`
binary also runs standalone against `~/.tsk`.

- Queue board with desk · projects · threads
- Quick-add on the board, plus a herdr capture overlay
- Task page for notes, thread, scope, and steps
- Headless `tsk add`, `tsk list`, and `tsk steps`

Park, resume, attention, linking, and dispatch are not part of this tree.

## Quickstart

```bash
git clone git@github.com:smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
./target/release/tsk add -t "Draft release notes"
./target/release/tsk
```

As a herdr plugin: `cargo build --release`, then `herdr plugin link "$PWD"`,
then **Open tsk board**. Rebuild after you pull. A running board keeps the old
binary until you quit it.

Full install (standalone vs plugin, env vars, quick capture):
[Install](https://tsk-gules.vercel.app/docs/install/).

## Usage

How to use the board and CLI lives on the site, not in this file:

- [Overview](https://tsk-gules.vercel.app/docs/)
- [Board](https://tsk-gules.vercel.app/docs/board/)
- [Keys](https://tsk-gules.vercel.app/docs/keys/)
- [Capture](https://tsk-gules.vercel.app/docs/capture/)
- [Task page](https://tsk-gules.vercel.app/docs/task-page/)
- [Steps](https://tsk-gules.vercel.app/docs/steps/)
- [CLI](https://tsk-gules.vercel.app/docs/cli/)

The store is `~/.tsk` (`tsk.json` and `settings.json`). Override with
`TSK_STATE_DIR` / `TSK_CONFIG_DIR`. herdr's injected plugin dirs are ignored, so
the pane and the CLI edit the same board.

Mutating keys need Ctrl. Bare letters do nothing. Legacy `alt` modifier settings
are read as Ctrl.

## Docs

| Audience | Where |
| --- | --- |
| Using tsk | https://tsk-gules.vercel.app/docs/ (`site/`) |
| Changing tsk | [`docs/technical/`](docs/technical/) |

```bash
cd site
npm install
npm run dev
```

`npm run build` writes `site/dist/`. Vercel Root Directory is `site`.

## Verify

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
```

Site-only: `cd site && npm ci && npm test && npm run build`.

## License

[MIT](LICENSE)

# tsk

**A task board for you and your agents.**

You and your agents put work on one board and move it through human status:
ready, started, blocked, review, done. Agents reach the board through the CLI.
Only a human marks a task done. It ships as a [herdr](https://herdr.dev) plugin,
and the `tsk` binary also runs standalone against `~/.tsk`.

- Queue board with desk · projects · threads
- Quick-add on the board, plus a herdr capture overlay
- Task page for notes, thread, scope, and steps
- Headless `tsk add`, `tsk list`, and `tsk steps`

Park, resume, attention, linking, and dispatch are not part of this tree.

## Quickstart

```bash
git clone https://github.com/smarzban/herdr-tsk.git
cd herdr-tsk
cargo build --release
./target/release/tsk add -t "Draft release notes"
./target/release/tsk
```

As a herdr plugin: `cargo build --release`, then `herdr plugin link "$PWD"`,
then **Open tsk board**. Rebuild after you pull. A running board keeps the old
binary until you quit it.

Full install (standalone vs plugin, env vars, quick capture):
[Install](https://gettsk.sh/docs/install/).

## Usage

How to use the board and CLI lives on the site, not in this file:

- [Overview](https://gettsk.sh/docs/)
- [Board](https://gettsk.sh/docs/board/)
- [Keys](https://gettsk.sh/docs/keys/)
- [Capture](https://gettsk.sh/docs/capture/)
- [Task page](https://gettsk.sh/docs/task-page/)
- [Steps](https://gettsk.sh/docs/steps/)
- [CLI](https://gettsk.sh/docs/cli/)

The store is `~/.tsk/tsk.json`; walkthrough dismissal is kept beside it in
`walkthrough.json`. Deleted tasks live in `trash.jsonl` beside the store; the
previous document is kept as `tsk.json.1`, and a format migration backs up the
pre-migration document as `tsk.json.v<N>`. Override with `TSK_STATE_DIR` /
`TSK_CONFIG_DIR`. herdr's injected plugin dirs are ignored, so
the pane and the CLI edit the same board. Archived tasks and projects stay on
the board but out of every working view (`ctrl+f`, `tsk archive`,
`tsk project archive`, `tsk list --archived`).

Keep `~/.tsk` on a local disk. The writer lock is `flock`-style and every save
is a rename-based atomic replace; NFS, Dropbox, iCloud Drive, and similar
synced folders can break both. Point `TSK_STATE_DIR` at a local disk instead.
State and config directory roots must be real directories, not symlinks.
Windows is not tested in CI.

Mutating keys need Ctrl. Bare letters do nothing. Legacy modifier settings are ignored.

## Docs

The user guide lives at https://gettsk.sh/docs/ and is built from `site/`.

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

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for development and pull-request guidance.
Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)

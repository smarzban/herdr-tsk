# Entry points

Every way a process starts, plus env vars and herdr hooks. Verified against
`src/main.rs`, `src/cli/router.rs`, `src/app.rs`, `src/store.rs`, `src/config.rs`,
`src/context.rs`, `herdr-plugin.toml`, and `scripts/`.

## Binary: `tsk`

`[[bin]] name = "tsk"` → `src/main.rs`. After argv0, `cli::router::route(args, TSK_MODE)`
selects a `Surface`.

| Surface | How selected | Behavior |
| --- | --- | --- |
| `Board` | default (no subcommand, no global flag, `TSK_MODE` unset) | TUI board (`tsk_tui::run`) |
| `Capture` | positional `capture`, or `TSK_MODE=capture` with no subcommand | TUI capture form; exits after save/cancel |
| `Add` | positional `add` | `cli::run_with` |
| `Steps` | positional `steps` | `cli::run_with` |
| `List` | positional `list` | `cli::run_with` |
| `FindBoardPane` | `--find-board-pane` before any positional | read stdin pane-list JSON, print pane id |
| `GlobalHelp` | `--help` before any positional | usage + command list, exit 0 |
| `Usage` | unknown positional, unknown flag, or global flag combined with anything else | stderr usage, exit 2 |

Global flags are a **closed set**: `--find-board-pane` and `--help`, and only before
the first positional. `tsk add -t capture` stays `Add`. A second global flag is usage.

Library entry used by the TUI surfaces: `tsk_tui::run` → `app::run`
(`src/lib.rs`, `src/app.rs`). `--find-board-pane` is handled in `main` before that.

Headless stdout/stderr/exit are `CliOutput { stdout, stderr, code }` written by `main`.
A TUI error prints `tsk: {err}` and exits 1.

## CLI commands

Full mechanism: [cli](cli.md). Help text in `src/cli/presenter.rs` is the user-facing
contract; this table is the maintainer index.

| Command | Mutations | Notable flags |
| --- | --- | --- |
| `tsk add` | yes, unless idempotent existing | `-t/--title`, `-n/--notes`, `-p/--project`, `--desk`, `--thread`, `--json`, `--file`, `--state-dir`, `--help` |
| `tsk list` | no | optional task UUID, `-p/--project`, `--desk`, `--all`, `--thread`, `--done`, `--deleted`, `--json`, `--state-dir`, `--help` |
| `tsk steps` | yes on success | `<task-id> add <text>` \| `toggle <step-short-id>`, `--state-dir`, `--help` |

`--desk` is the flag (not `--global`). `--project` / `--state-dir` / `--file` values
that start with `-` need `--flag=value`.

Exit codes: [invariants](invariants.md#cli) and `CONTEXT.md` (“exit contract”).

## Environment

| Variable | Read by | Default / fallback |
| --- | --- | --- |
| `TSK_STATE_DIR` | `store::default_state_dir` | `$HOME/.tsk`; empty falls through; last resort `.tsk-state` (relative) |
| `TSK_CONFIG_DIR` | `config::default_config_dir` | `$HOME/.tsk`; empty falls through; last resort `.tsk-config` |
| `TSK_MODE` | `cli::router` (as `capture_env`) and `app::MODE_ENV` | unset → board. `capture` (case-insensitive) → capture TUI when no subcommand |
| `HERDR_PLUGIN_CONTEXT_JSON` | `context::RawHostContext::from_env` | missing/malformed → empty snapshot |
| `HERDR_PLUGIN_STATE_DIR` | **ignored** | — |
| `HERDR_PLUGIN_CONFIG_DIR` | **ignored** | — |
| `HOME` | state/config dir resolution | if empty/missing, relative last-resort dirs |
| `TSK_BIN` | `scripts/open-board.sh` only | `$script_dir/../target/release/tsk`. `open-capture.sh` does not read it. |
| `HERDR_BIN_PATH` | launcher scripts | `herdr` on PATH |

`HERDR_ENV=1` is an operator convention for live herdr smoke (`AGENTS.md`); the binary
does not read it.

## herdr plugin

`herdr-plugin.toml`:

- id / name `herdr-tsk`, version `0.4.0`, `min_herdr_version = "0.7.5"`, platforms
  linux + macos
- `[[build]]` `cargo build --release`
- `[[panes]]` id `board`, title `tsk` (**must** equal `BOARD_PANE_LABEL`), placement
  `split`, command `./target/release/tsk`
- actions: `open-board` → `scripts/open-board.sh`; `quick-capture` →
  `scripts/open-capture.sh`

A running pane keeps the old binary until it is quit.

## Scripts

| Script | Role |
| --- | --- |
| `scripts/open-board.sh` | Idempotent open-or-focus: `pane list` → `tsk --find-board-pane` → `plugin pane focus` or `plugin pane open --entrypoint board --placement split --focus` |
| `scripts/open-capture.sh` | Overlay the board entrypoint with `--env TSK_MODE=capture` |
| `scripts/tsk-cli.sh` | Operator helper around the CLI (not the TUI loop) |

## Site (separate process)

Astro app in `site/`. Production: `https://tsk-gules.vercel.app` (Vercel root
directory `site`). Dev: `npm run dev` in `site/`. Not loaded by `tsk`. See
[site](site.md).

## Tests as drivers (not product entry points)

Integration tests under `tests/` construct `TaskStore` / `BoardModel` / `cli::run_with`
directly. Golden fixtures: `tests/fixtures/queue_board/*.txt`, regenerated with
`cargo test --test queue_board_render regenerate_golden_fixtures -- --ignored`.

# Security model

tsk is a single-user local tool. There is no authentication, no network API, and no
multi-tenant boundary. The trust questions that remain are: what input is untrusted,
what the process will execute, and what fails open vs closed.

## Trust boundaries

| Surface | Trust | Enforcement |
| --- | --- | --- |
| `tsk.json` on disk | Trusted as the user's own file, untrusted as *terminal content* | JSON parse; newer `format_version` refused; titles/notes escaped at paint |
| `settings.json` / `walkthrough.json` | Same | Malformed → safe defaults (Ctrl modifier / not dismissed) |
| `HERDR_PLUGIN_CONTEXT_JSON` | Untrusted structured input from the host | serde; unknown keys ignored; missing/malformed → empty snapshot; no field invented |
| herdr `pane list` JSON on stdin | Untrusted | Must parse; label must equal `tsk`; pane id must be flag-safe before it is passed to another CLI |
| Task title, notes, thread, step text, project path | Untrusted display strings | `ui::terminal_text` (C0/C1 → `\u{00xx}`); wrap rather than clip; markdown is a styled subset with no color and no raw control passthrough |
| CLI argv and plan JSON | Untrusted | Closed flag sets; plan must be an array of objects; C0 in titles/step text refused |
| OSC 52 clipboard (`copy_to_clipboard`) | Local terminal | Payload capped (`COPY_MAX_CHARS` = 100_000); still a terminal feature, not a sandbox |

The host (herdr) is trusted to spawn the binary and inject context, **not** to choose
the state directory. Injected `HERDR_PLUGIN_STATE_DIR` / `HERDR_PLUGIN_CONFIG_DIR` are
ignored so a host cannot silently point the plugin at a different board than the CLI.

## What is not trusted to change status

Agent identity (`AgentMeta`), host observations (`ObservedStatus`), and dispatch-attempt
phase are **not** authorities. Human verbs (and the CLI commands that wrap the same
domain functions) are. Auto-complete from agent status is forbidden.

## Fail closed

- Newer store `format_version` than this binary: refuse load/save
  (`StoreError::UnsupportedFormat`). Do not rewrite the file.
- Concurrent same-task mutation: `merge_for_save` returns a typed failure; disk is not
  overwritten.
- Empty / whitespace title, empty step text, invalid thread shape: no durable write.
- CLI usage/parse errors: exit 2, nothing persisted.
- `--find-board-pane`: pane ids that start with `-`, contain spaces, or fall outside
  `[A-Za-z0-9_.:-]+` are skipped so they cannot be parsed as flags by
  `herdr plugin pane focus`.
- Unknown global flags: usage, not a best-effort board open.

## Fail open (deliberate)

- Missing `tsk.json`: empty board (first run).
- Unreadable / malformed `walkthrough.json`: treat as not dismissed. A broken config
  location must not keep the board shut (and must not pretend the card was skipped).
- Walkthrough / settings write failure: present on the chrome row, keep the board
  running.
- Malformed `HERDR_PLUGIN_CONTEXT_JSON`: empty snapshot, default scope from cwd-walk
  or desk.
- Failed idle `store.load`: do not advance `StoreWatch`; retry next tick. Do not treat
  the failed read as “caught up”.
- `HERDR_PLUGIN_CONTEXT_JSON` selected text used as title prefill: still escaped at
  paint; provenance becomes `selection`.

Config's lack of lock/directory-fsync is a durability trade, not an access-control
trade. Anyone who can write `~/.tsk` can change the board; that is the single-user
model.

## Process and host

- Plugin actions are explicit (`open-board`, `quick-capture`). No background host
  poll in this tree.
- `scripts/open-board.sh` shells out to `$HERDR_BIN_PATH` (else `herdr` on PATH) and
  `./target/release/tsk` (override `TSK_BIN`). It does not eval pane-list JSON in the
  shell; it pipes JSON to `tsk --find-board-pane`.
- Capture overlay sets `TSK_MODE=capture` via herdr `--env`; it does not inherit a
  second state dir.

## Display injection

The terminal is the dangerous output. Two layers:

1. **Escape.** `terminal_text` / `present_line` / `present_lines` so a title containing
   ESC, BEL, or C1 cannot drive the emulator. Line breaks are the only control
   consumed rather than escaped, and only through `split_line_breaks`.
2. **Markdown subset** (view/peek only): `**bold**`, `*em*`/`_em_` (underline),
   `` `code` `` (dim, ticks kept), `#`–`######`, `-`/`*` lists, fenced ` ``` `
   (no inline inside). Unmatched `*` and word-internal `_` stay literal. No HTML, no
   links-as-commands, no color.

Clipboard copy uses OSC 52 (`ui::text_select::osc52_clipboard`). Treat that as “user
copied from their own board”, not as a network exfil boundary.

## Secrets

tsk does not handle API keys or credentials. Do not add a path that logs
`HERDR_PLUGIN_CONTEXT_JSON` (it may contain selected source) or that writes store
contents to a shared temp directory.

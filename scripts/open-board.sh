#!/usr/bin/env bash
# Idempotent open-or-focus for the Tasks board.
#
# - No Tasks pane yet  -> plugin pane open --placement split --focus
# - Tasks pane exists  -> plugin pane focus <pane_id> (no second board)
#
# herdr actions run a command (no declarative "open this pane" field), so this shells
# out via $HERDR_BIN_PATH (herdr injects it; fall back to `herdr` on PATH).
# Existing board is recognized by label/title "Tasks" from `pane list` JSON
# (matches [[panes]] title in herdr-plugin.toml).
#
# Focus selection is done by herdr-tasks --find-board-pane (Rust), not python3,
# so this works on hosts without python3.
#
# Manual check: invoke open-board twice; the second focus keeps a single Tasks board.
set -uo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
plugin_id="herdr-tasks"
entrypoint="board"
board_label="Tasks"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Same relative layout as other herdr plugins: scripts/ next to target/release/.
plugin_bin="${HERDR_TASKS_BIN:-$script_dir/../target/release/tsk}"

open_board() {
  exec "$herdr_bin" plugin pane open \
    --plugin "$plugin_id" \
    --entrypoint "$entrypoint" \
    --placement split \
    --focus
}

# Extract first flag-safe pane_id whose label or stripped title is Tasks.
# Uses herdr-tasks --find-board-pane (reads pane-list JSON on stdin).
find_board_pane_id() {
  local panes
  panes="$("$herdr_bin" pane list 2>/dev/null || true)"
  if [ -z "$panes" ]; then
    return 1
  fi
  if [ ! -x "$plugin_bin" ]; then
    return 1
  fi
  printf '%s' "$panes" | "$plugin_bin" --find-board-pane
}

pane_id="$(find_board_pane_id || true)"
if [ -n "${pane_id:-}" ]; then
  # Prefer the plugin-pane focus API (herdr >= 0.7). Fall back to open if focus fails
  # (stale list race) so the keypress is never a silent no-op.
  if "$herdr_bin" plugin pane focus "$pane_id"; then
    exit 0
  fi
fi

open_board

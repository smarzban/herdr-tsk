#!/usr/bin/env bash
# Open or focus one Tasks board in the invoking workspace, across all its tabs.
# Boards in other workspaces are independent views of the same task store.
set -uo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
workspace_id="${HERDR_WORKSPACE_ID:-}"
target_pane="${HERDR_PANE_ID:-}"
origin_tab="${HERDR_TAB_ID:-}"
if [ -z "$workspace_id" ] || [ -z "$target_pane" ] || [ -z "$origin_tab" ]; then
  printf 'tsk: cannot open board without a Herdr workspace, tab and pane\n' >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
plugin_bin="${TSK_BIN:-$script_dir/../target/release/tsk}"
if [ ! -x "$plugin_bin" ]; then
  printf 'tsk: board launcher binary is unavailable: %s\n' "$plugin_bin" >&2
  exit 1
fi

# Never search globally or treat a failed lookup as an empty workspace.
if ! panes="$("$herdr_bin" pane list --workspace "$workspace_id")"; then
  printf 'tsk: could not list panes in workspace %s\n' "$workspace_id" >&2
  exit 1
fi
pane_id="$(printf '%s' "$panes" | "$plugin_bin" --find-board-pane || true)"
if [ -n "$pane_id" ]; then
  tab_id="$(printf '%s' "$panes" | "$plugin_bin" --find-board-tab || true)"
  if [ -z "$tab_id" ]; then
    printf 'tsk: could not locate the existing board tab\n' >&2
    exit 1
  fi
  # Herdr 0.9.0 plugin pane focus updates server focus but does not navigate
  # attached clients. Activate the tab through the public navigation route first.
  if ! "$herdr_bin" tab focus "$tab_id"; then
    exit 1
  fi
  # Focus preserves the board's view and edits. Do not publish a shared reopen
  # request: another workspace's board could consume it.
  if "$herdr_bin" plugin pane focus "$pane_id"; then
    exit 0
  fi
fi

# Split placement requires a target pane, not a workspace argument. Anchor it to
# the invoking pane so a focus change cannot redirect creation to another workspace.
# Plugin open does not navigate attached clients either. Return to the invoking
# tab before creation, including when a stale board was found in another tab.
if ! "$herdr_bin" tab focus "$origin_tab"; then
  exit 1
fi
# New panes still receive the host's invocation context.
exec "$herdr_bin" plugin pane open --plugin herdr-tsk --entrypoint board --placement split --target-pane "$target_pane" --focus

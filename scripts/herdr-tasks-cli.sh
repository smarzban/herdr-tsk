#!/usr/bin/env bash
set -euo pipefail

herdr_bin=${HERDR_BIN_PATH:-herdr}

# A plugin pane receives this from Herdr. Agents run outside that pane, so use
# Herdr's default plugin-state location rather than the binary's standalone
# XDG-data fallback. An explicit caller override still wins.
if [[ -z ${HERDR_PLUGIN_STATE_DIR:-} ]]; then
  xdg_state_home=${XDG_STATE_HOME:-"$HOME/.local/state"}
  export HERDR_PLUGIN_STATE_DIR="$xdg_state_home/herdr/plugins/herdr-tasks"
fi

plugin_root=$(
  "$herdr_bin" plugin list --json | python3 -c '
import json
import sys

for plugin in json.load(sys.stdin)["result"]["plugins"]:
    if plugin["plugin_id"] == "herdr-tasks" and plugin["enabled"]:
        print(plugin["plugin_root"])
        break
else:
    raise SystemExit("herdr-tasks plugin is not enabled")
'
)
binary="$plugin_root/target/release/herdr-tasks"

if [[ ! -x "$binary" ]]; then
  printf 'herdr-tasks binary is missing: %s\nRebuild the enabled plugin, then retry.\n' "$binary" >&2
  exit 127
fi

exec "$binary" "$@"

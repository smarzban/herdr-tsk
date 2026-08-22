#!/usr/bin/env bash
set -euo pipefail

herdr_bin=${HERDR_BIN_PATH:-herdr}
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

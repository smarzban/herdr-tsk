#!/usr/bin/env bash
set -euo pipefail

herdr_bin=${HERDR_BIN_PATH:-herdr}

# tsk owns one store (~/.tsk by default), so agents need no state plumbing: the same
# files back the herdr pane and a bare terminal run.

plugin_root=$(
  "$herdr_bin" plugin list --json | python3 -c '
import json
import sys

for plugin in json.load(sys.stdin)["result"]["plugins"]:
    if plugin["plugin_id"] == "herdr-tsk" and plugin["enabled"]:
        print(plugin["plugin_root"])
        break
else:
    raise SystemExit("herdr-tsk plugin is not enabled")
'
)
binary="$plugin_root/target/release/tsk"

if [[ ! -x "$binary" ]]; then
  printf 'tsk binary is missing: %s\nRebuild the enabled plugin, then retry.\n' "$binary" >&2
  exit 127
fi

exec "$binary" "$@"

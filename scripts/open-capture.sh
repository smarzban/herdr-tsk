#!/usr/bin/env bash
# Quick-capture launcher: short-lived popup (overlay) in capture mode.
#
# Opens the board entrypoint as an overlay and sets TSK_MODE=capture so
# the binary can enter Capture UI without first opening a persistent split board.
# Capture surfaces are short-lived. Board focus idempotency is in open-board.sh.
#
# herdr injects $HERDR_BIN_PATH; fall back to `herdr` on PATH.
set -uo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
plugin_id="herdr-tsk"
entrypoint="board"

exec "$herdr_bin" plugin pane open \
  --plugin "$plugin_id" \
  --entrypoint "$entrypoint" \
  --placement overlay \
  --focus \
  --env TSK_MODE=capture

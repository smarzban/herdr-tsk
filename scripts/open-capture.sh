#!/usr/bin/env bash
# Quick-capture launcher: short-lived popup holding the expanded quick-add page.
#
# Opens the board entrypoint as a fixed-size popup (width/height in cells) and sets
# TSK_MODE=capture so the binary seeds the capture session: the task page with the
# cursor in Title. Saving or discarding the draft exits the process, which closes the
# popup; a single Esc discards. Board focus idempotency is in open-board.sh.
#
# 80x15 content keeps the popup inside the compact board's operable range (40x10
# floor) and below the 110-column wide threshold, so no stage rail is offered.
#
# herdr injects $HERDR_BIN_PATH; fall back to `herdr` on PATH.
set -uo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
plugin_id="herdr-tsk"
entrypoint="board"

exec "$herdr_bin" plugin pane open \
  --plugin "$plugin_id" \
  --entrypoint "$entrypoint" \
  --placement popup \
  --width 80 \
  --height 15 \
  --focus \
  --env TSK_MODE=capture

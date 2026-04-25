#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_SCRIPT="$REPO_DIR/scripts/run_daily_update.sh"
STATE_DIR="$HOME/.local/state/yt-adguard-rules"
LOG_FILE="$STATE_DIR/daily-update.log"
MARK_BEGIN="# BEGIN yt-adguard-rules autosync"
MARK_END="# END yt-adguard-rules autosync"

mkdir -p "$STATE_DIR"

tmp="$(mktemp)"
trap 'rm -f "$tmp" "$tmp.new"' EXIT

crontab -l >"$tmp" 2>/dev/null || true
awk -v begin="$MARK_BEGIN" -v end="$MARK_END" '
  $0 == begin {skip = 1; next}
  $0 == end {skip = 0; next}
  !skip {print}
' "$tmp" >"$tmp.new"

{
  printf '%s\n' "$MARK_BEGIN"
  printf '0 12 * * * %s >> %s 2>&1\n' "$RUN_SCRIPT" "$LOG_FILE"
  printf '%s\n' "$MARK_END"
} >>"$tmp.new"

crontab "$tmp.new"

echo "Installed daily autosync cron:"
echo "0 12 * * * $RUN_SCRIPT >> $LOG_FILE 2>&1"

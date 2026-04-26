#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_SCRIPT="$REPO_DIR/scripts/run_daily_update.sh"
MARK_BEGIN="# BEGIN yt-adguard-rules autosync"
MARK_END="# END yt-adguard-rules autosync"

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
  printf '0 12 * * * YT_ADGUARD_LOG_TO_FILE=1 "%s"\n' "$RUN_SCRIPT"
  printf '%s\n' "$MARK_END"
} >>"$tmp.new"

crontab "$tmp.new"

echo "Installed daily autosync cron:"
echo "0 12 * * * YT_ADGUARD_LOG_TO_FILE=1 \"$RUN_SCRIPT\""

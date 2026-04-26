#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRANCH="${YT_ADGUARD_BRANCH:-main}"
REMOTE_URL="git@github.com:YTjungle666/yt-adguard-rules.git"
STATE_DIR="${YT_ADGUARD_STATE_DIR:-$HOME/.local/state/yt-adguard-rules}"
LOG_FILE="${YT_ADGUARD_LOG_FILE:-$STATE_DIR/daily-update.log}"

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:${PATH:-}"

export GIT_TERMINAL_PROMPT=0

if [ "${YT_ADGUARD_LOG_TO_FILE:-0}" = "1" ]; then
  mkdir -p "$STATE_DIR"
  exec >>"$LOG_FILE" 2>&1
  printf '\n[%s] Starting daily update\n' "$(date '+%Y-%m-%d %H:%M:%S %z')"
  trap 'status=$?; printf "[%s] Finished daily update (status=%s)\n" "$(date "+%Y-%m-%d %H:%M:%S %z")" "$status"; exit "$status"' EXIT
fi

cd "$REPO_DIR"

git remote set-url origin "$REMOTE_URL"
git fetch origin "$BRANCH"
git pull --ff-only origin "$BRANCH"

cargo build --release --quiet --locked --bin update_rules
./target/release/update_rules

if ! git diff --quiet -- blocklist.txt allowlist.txt; then
  git add blocklist.txt allowlist.txt
  git commit -m "Daily rule merge $(date +%F)"
  git push origin "HEAD:$BRANCH"
fi

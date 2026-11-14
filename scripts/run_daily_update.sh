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
  # Keep the append-only run log bounded: one day of output is appended per run and
  # nothing else ever trims it. Rotation happens before the redirect so the "previous
  # file exceeded N bytes" note lands in the fresh log.
  log_max="${YT_ADGUARD_LOG_MAX_BYTES:-65536}"    # 64 KiB (~100 days at the measured 0.6 KB/day)
  log_keep="${YT_ADGUARD_LOG_KEEP:-2}"             # .1 and .2 kept, older dropped
  log_size=$(stat -c %s "$LOG_FILE" 2>/dev/null || echo 0)
  if [ "$log_size" -ge "$log_max" ]; then
    rm -f "$LOG_FILE.$log_keep"
    for ((i = log_keep - 1; i >= 1; i--)); do
      if [ -f "$LOG_FILE.$i" ]; then mv -f "$LOG_FILE.$i" "$LOG_FILE.$((i + 1))"; fi
    done
    mv -f "$LOG_FILE" "$LOG_FILE.1"
    printf '[%s] log rotated: previous file exceeded %s bytes\n' \
      "$(date '+%Y-%m-%d %H:%M:%S %z')" "$log_max" >>"$LOG_FILE"
  fi
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
  if git log -1 --format=%s | grep -q '^Daily rule merge'; then
    # Replace the previous daily snapshot so the generated lists keep a single
    # history entry instead of accumulating one full copy per day.
    git commit --amend -m "Daily rule merge $(date +%F)"
    git push --force-with-lease origin "HEAD:$BRANCH"
    # The amended-away blob stays alive in the reflog and would add a full copy
    # of the lists to .git every day (~3 MB/day); drop it now that it is pushed.
    git reflog expire --expire=now --all
    git gc --prune=now --quiet
  else
    git commit -m "Daily rule merge $(date +%F)"
    git push origin "HEAD:$BRANCH"
  fi
fi

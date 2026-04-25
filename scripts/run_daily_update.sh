#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRANCH="${YT_ADGUARD_BRANCH:-main}"
REMOTE_URL="git@github.com:YTjungle666/yt-adguard-rules.git"

export GIT_TERMINAL_PROMPT=0

cd "$REPO_DIR"

git remote set-url origin "$REMOTE_URL"
git fetch origin "$BRANCH"
git pull --ff-only origin "$BRANCH"

python3 scripts/update_rules.py

if ! git diff --quiet -- blocklist.txt allowlist.txt; then
  git add blocklist.txt allowlist.txt
  git commit -m "Daily rule merge $(date +%F)"
  git push origin "HEAD:$BRANCH"
fi

#!/usr/bin/env bash
set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TOKEN_FILE="$HOME/.config/yt-adguard-rules/github_token"
cd "$REPO_DIR"
python3 scripts/update_rules.py
if ! git diff --quiet -- blocklist.txt allowlist.txt; then
  git add blocklist.txt allowlist.txt
  git commit -m "Daily rule merge $(date +%F)"

  XTRACE_WAS_ON=0
  case "$-" in
    *x*) XTRACE_WAS_ON=1; set +x ;;
  esac

  TOKEN=$(cat "$TOKEN_FILE")
  REMOTE_URL="https://${TOKEN}@github.com/YTjungle666/yt-adguard-rules.git"
  git push "$REMOTE_URL" main

  if [ "$XTRACE_WAS_ON" -eq 1 ]; then
    set -x
  fi
fi

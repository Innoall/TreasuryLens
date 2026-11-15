#!/usr/bin/env bash
# macOS Finder launcher; a terminal command is also documented in START_HERE.
cd "$(dirname "${BASH_SOURCE[0]}")" || exit 1
bash scripts/start.sh
result=$?
if [[ "$result" -ne 0 ]]; then
  printf '\nStartup stopped (exit %s). See START_HERE.zh-CN.md and the messages above.\n' "$result"
  read -r -p 'Press Enter to close this window. ' _tl_reply
fi
exit "$result"

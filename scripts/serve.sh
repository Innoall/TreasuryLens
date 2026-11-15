#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$TL_ROOT"
if [[ ! -f .runtime/deployment.json ]]; then
  echo 'Deploy the project first: bash scripts/deploy.sh' >&2
  exit 1
fi
TL_APP_ID="$(python3 -c 'import json; print(json.load(open(".runtime/deployment.json"))["lyquid_id"])')"
exec shaker serve "$TL_APP_ID" --endpoint "$TL_PRIMARY_WS" "$@"

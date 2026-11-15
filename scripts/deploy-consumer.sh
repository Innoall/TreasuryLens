#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$TL_ROOT"
TL_SOURCE="$(python3 -c 'import json; print(json.load(open(".runtime/deployment.json"))["target"])')"
TL_INPUT="$(cast abi-encode 'constructor(address)' "$TL_SOURCE")"
shaker deploy --endpoint "$TL_PRIMARY_WS" --reference "$TL_REGISTRY:budget-monitor-local" \
 --input "$TL_INPUT" --output json lyquids/budget-monitor/Cargo.toml > .runtime/budget-monitor.json
cat .runtime/budget-monitor.json

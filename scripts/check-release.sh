#!/usr/bin/env bash
# Verify a running normal deployment without resetting its network or data.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$TL_ROOT"
TL_BASE_URL="${TL_BASE_URL:-http://127.0.0.1:8080}"
bash scripts/test.sh
python3 tests/integration.py --base-url "$TL_BASE_URL"
python3 tests/api_v2.py --base-url "$TL_BASE_URL"
TL_BASE_URL="$TL_BASE_URL" python3 tests/consumer.py
mkdir -p output/results
cargo run --locked -q -p treasurylens-core --bin evaluate \
  < evaluation/observed/certified-record.json > output/results/ablation.json
printf '\nRelease checks passed. See output/results/. Browser interaction and outside-user testing are separate.\n'

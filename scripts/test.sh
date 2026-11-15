#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$TL_ROOT"
cargo test --locked -p treasurylens-core
for source in lyquids/treasurylens/assets/*.js; do node --check "$source"; done
node --test tests/client-logic.test.cjs
python3 -m compileall -q scripts tests
for script in scripts/*.sh; do bash -n "$script"; done

#!/usr/bin/env bash
# Run from any directory. Keep this terminal open while using TreasuryLens.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
exec bash scripts/start.sh

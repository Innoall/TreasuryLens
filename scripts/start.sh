#!/usr/bin/env bash
# Start only project-owned processes; Ctrl-C stops these processes only.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$TL_ROOT"
TL_NETWORK_PID=''
cleanup() {
  if [[ -n "$TL_NETWORK_PID" ]] && kill -0 "$TL_NETWORK_PID" 2>/dev/null; then
    kill -TERM "$TL_NETWORK_PID" 2>/dev/null || true
    wait "$TL_NETWORK_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM
bash scripts/bootstrap.sh
mkdir -p .runtime
bash scripts/localnet.sh > .runtime/start-network.log 2>&1 &
TL_NETWORK_PID=$!
TL_READY=false
for ((i=0;i<90;i++)); do
  if ! kill -0 "$TL_NETWORK_PID" 2>/dev/null; then
    cat .runtime/start-network.log >&2
    exit 1
  fi
  if curl -fsS --max-time 2 -H 'content-type: application/json' -d '{}' \
      http://127.0.0.1:13087/lyquor.node.v1.NodeService/GetNodeInfo >/dev/null 2>&1; then
    # Deployment checks bartender readiness independently after node startup.
    TL_READY=true
    break
  fi
  sleep 1
done
if [[ "$TL_READY" != true ]]; then
  echo 'Network did not become ready. See .runtime/start-network.log' >&2
  exit 1
fi
# Wait for the wrapper to finish bootstrap; it prints this only after bartender readiness.
for ((i=0;i<120;i++)); do
  if ! kill -0 "$TL_NETWORK_PID" 2>/dev/null; then cat .runtime/start-network.log >&2;exit 1;fi
  if grep -q 'TreasuryLens localnet is ready' .runtime/start-network.log; then break;fi
  if ((i==119));then echo 'Network bootstrap timed out.' >&2;exit 1;fi
  sleep 1
done
python3 scripts/seed.py
bash scripts/deploy.sh
bash scripts/deploy-consumer.sh
bash scripts/serve.sh

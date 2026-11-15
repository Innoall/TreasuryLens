#!/usr/bin/env bash
# Uses the upstream Apache-2.0 LDK localnet lifecycle helpers and PID cleanup.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
tl_require python3 lyquor shaker anvil cast
[[ -f "$TL_ROOT/.tools/ldk/scripts/localnet.sh" ]] || { echo 'Run scripts/bootstrap.sh first.' >&2; exit 1; }
# Never stop or replace an unrelated service occupying a required port.
python3 - <<'PY'
import socket
occupied=[]
for port in [8545,10080,10087,11080,11087,12080,12087,13080,13087]:
    with socket.socket() as sock:
        sock.settimeout(.2)
        if sock.connect_ex(('127.0.0.1',port))==0: occupied.append(str(port))
if occupied: raise SystemExit('Required ports are already in use: '+', '.join(occupied)+'. Stop the existing network/service yourself; no processes were killed.')
PY
mkdir -p "$TL_RUNTIME"
export LOCALNET_WORK_DIR="$TL_RUNTIME/network"
# Anvil starts a fresh chain each time. Preserve the old node databases for
# inspection, but do not reuse them against a new chain with the same chain ID.
if [[ -d "$LOCALNET_WORK_DIR" ]]; then
    tl_archive="$(mktemp -d "$TL_RUNTIME/previous-network.XXXXXX")"
    mv "$LOCALNET_WORK_DIR" "$tl_archive/network"
    for tl_state in seed.json deployment.json; do
        if [[ -f "$TL_RUNTIME/$tl_state" ]]; then mv "$TL_RUNTIME/$tl_state" "$tl_archive/$tl_state"; fi
    done
    echo "Previous network data preserved at $tl_archive"
fi
printf 'Starting a fresh four-node network on loopback. Keep this terminal open.\n'
# LDK v0.8.3's bundled node 2-4 submitter keys do not correspond to funded
# default Anvil accounts. Override the helper array without editing upstream
# files, deriving the actual default accounts from the public test mnemonic.
source "$TL_ROOT/.tools/ldk/scripts/lib/localnet.sh"
_LOCALNET_SUBMITTER_KEYS=()
tl_public_mnemonic='test test test test test test test test test test test junk'
for tl_index in 0 1 2 3; do
    _LOCALNET_SUBMITTER_KEYS+=("$(cast wallet private-key "$tl_public_mnemonic" "$tl_index")")
done
unset tl_public_mnemonic tl_index
trap localnet_down EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
localnet_up multi
localnet_print_endpoints
printf 'TreasuryLens localnet is ready. Press Ctrl-C here to stop only its own processes.\n'
while :; do
    for tl_pid in "${_LOCALNET_PIDS[@]}"; do
        if ! kill -0 "$tl_pid" >/dev/null 2>&1; then
            echo "A TreasuryLens localnet process ($tl_pid) exited unexpectedly." >&2
            exit 1
        fi
    done
    sleep 2
done

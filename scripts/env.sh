#!/usr/bin/env bash
# Source in project scripts. No edits to the user's shell profile are needed.
if [[ -n "${BASH_VERSION:-}" ]]; then
    tl_env_source="${BASH_SOURCE[0]}"
elif [[ -n "${ZSH_VERSION:-}" ]]; then
    tl_env_source="${(%):-%x}"
else
    echo 'Source env.sh from Bash or Zsh.' >&2
    return 1
fi
TL_ROOT="$(cd "$(dirname "$tl_env_source")/.." && pwd)"
unset tl_env_source
export TL_ROOT
export TL_RUNTIME="$TL_ROOT/.runtime"
export PATH="$TL_ROOT/.tools/bin:$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN=1.99.0
export LYQUOR_BIN_DIR="$TL_ROOT/.tools/bin"
export LYQUOR_IMAGE_TAG=v0.8.3
export TL_RPC_URL=http://127.0.0.1:8545
export TL_PRIMARY_WS=ws://127.0.0.1:10087/ws
export TL_REGISTRY=http://127.0.0.1:10087/lyquids
# Public Anvil development key. Never use this account on a public network.
export TL_ANVIL_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
TL_NODE_PORTS=(10087 11087 12087 13087)

tl_require() {
    local tool
    for tool in "$@"; do
        command -v "$tool" >/dev/null 2>&1 || {
            echo "Missing '$tool'. Run bash scripts/bootstrap.sh first." >&2
            return 1
        }
    done
}

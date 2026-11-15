#!/usr/bin/env bash
# Project-local, pinned tool downloads; source projects remain under .tools/ldk.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
tl_require curl tar python3
case "$(uname -s)" in Darwin) tl_os=darwin ;; Linux) tl_os=linux ;; *) echo 'Supported platforms: macOS and Linux.' >&2; exit 1 ;; esac
case "$(uname -m)" in arm64|aarch64) tl_arch=arm64 ;; x86_64|amd64) tl_arch=amd64 ;; *) echo 'Supported CPUs: arm64 and amd64.' >&2; exit 1 ;; esac
mkdir -p "$TL_ROOT/.tools/bin" "$TL_ROOT/.tools/downloads"
tl_tmp="$(mktemp -d "$TL_ROOT/.tools/downloads/bootstrap.XXXXXX")"
trap 'rm -rf "$tl_tmp"' EXIT
fetch() { curl --fail --location --retry 3 --connect-timeout 20 --max-time 300 "$1" --output "$2"; }

if [[ ! -x "$TL_ROOT/.tools/bin/lyquor" || ! -x "$TL_ROOT/.tools/bin/ladle" ]] || ! "$TL_ROOT/.tools/bin/shaker" --version 2>/dev/null | grep -q 'shaker 0.8.3'; then
    tl_url="https://github.com/lyquor-labs/ldk/releases/download/v0.8.3/lyquor-2026-11-03-154af76-${tl_os}-${tl_arch}.tar.gz"
    echo 'Installing pinned Lyquor tools v0.8.3.'
    fetch "$tl_url" "$tl_tmp/lyquor.tar.gz"
    mkdir "$tl_tmp/bin"
    tar -xzf "$tl_tmp/lyquor.tar.gz" --strip-components=1 -C "$tl_tmp/bin"
    test -x "$tl_tmp/bin/shaker" || { echo 'Unexpected Lyquor release archive layout.' >&2; exit 1; }
    cp "$tl_tmp/bin/"* "$TL_ROOT/.tools/bin/"
fi
if [[ ! -f "$TL_ROOT/.tools/ldk/scripts/localnet.sh" ]]; then
    echo 'Installing LDK source release v0.8.3.'
    if [[ -f "$TL_ROOT/vendor/ldk-v0.8.3.tar.gz" ]]; then
        python3 - "$TL_ROOT/vendor/ldk-v0.8.3.tar.gz" "$TL_ROOT/vendor/ldk-v0.8.3.tar.gz.sha256" <<'PY_VENDOR'
import hashlib,pathlib,sys
archive,manifest=map(pathlib.Path,sys.argv[1:])
if hashlib.sha256(archive.read_bytes()).hexdigest()!=manifest.read_text().split()[0]:
    raise SystemExit('Bundled LDK source checksum verification failed.')
PY_VENDOR
        cp "$TL_ROOT/vendor/ldk-v0.8.3.tar.gz" "$tl_tmp/ldk.tar.gz"
    else
        fetch 'https://github.com/lyquor-labs/ldk/releases/download/v0.8.3/ldk.tar.gz' "$tl_tmp/ldk.tar.gz"
    fi
    mkdir "$tl_tmp/ldk"
    tar -xzf "$tl_tmp/ldk.tar.gz" --strip-components=1 -C "$tl_tmp/ldk"
    test -f "$tl_tmp/ldk/scripts/localnet.sh" || { echo 'Unexpected LDK archive layout.' >&2; exit 1; }
    mv "$tl_tmp/ldk" "$TL_ROOT/.tools/ldk"
fi

if [[ ! -x "$TL_ROOT/.tools/bin/anvil" || ! -x "$TL_ROOT/.tools/bin/cast" ]] || ! "$TL_ROOT/.tools/bin/forge" --version 2>/dev/null | grep -q '1.7.1'; then
    tl_foundry="foundry_v1.7.1_${tl_os}_${tl_arch}"
    echo 'Installing pinned Foundry v1.7.1.'
    fetch "https://github.com/foundry-rs/foundry/releases/download/v1.7.1/${tl_foundry}.tar.gz" "$tl_tmp/foundry.tar.gz"
    fetch "https://github.com/foundry-rs/foundry/releases/download/v1.7.1/${tl_foundry}.sha256" "$tl_tmp/foundry.sha256"
    python3 - "$tl_tmp/foundry.tar.gz" "$tl_tmp/foundry.sha256" <<'PY'
import hashlib,pathlib,re,sys
archive,manifest=map(pathlib.Path,sys.argv[1:])
actual=hashlib.sha256(archive.read_bytes()).hexdigest()
expected=re.search(r'\b[0-9a-fA-F]{64}\b',manifest.read_text())
if not expected or expected.group().lower()!=actual:
    raise SystemExit('Foundry release checksum verification failed.')
PY
    mkdir "$tl_tmp/foundry"
    tar -xzf "$tl_tmp/foundry.tar.gz" -C "$tl_tmp/foundry"
    for tool in anvil cast forge chisel; do
        if [[ -f "$tl_tmp/foundry/$tool" ]]; then cp "$tl_tmp/foundry/$tool" "$TL_ROOT/.tools/bin/$tool"; fi
    done
fi

if ! command -v rustup >/dev/null 2>&1; then
    echo 'Installing Rust through rustup without modifying shell profiles.'
    fetch 'https://sh.rustup.rs' "$tl_tmp/rustup-init.sh"
    sh "$tl_tmp/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain 1.99.0
fi
rustup toolchain install 1.99.0 --profile minimal
rustup target add --toolchain 1.99.0 wasm32-unknown-unknown
shaker --version
forge --version
rustc --version
printf '\nReady. Run bash scripts/localnet.sh in one terminal, then seed and deploy from another.\n'

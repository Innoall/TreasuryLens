#!/usr/bin/env bash
# Oracle setup shape follows the upstream Apache-2.0 LDK demo helpers:
# https://github.com/lyquor-labs/ldk/blob/v0.8.3/lyquid-examples/scripts/lib/demo.sh
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
tl_require python3 shaker cast
cd "$TL_ROOT"
python3 - <<'PY'
import base64
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = Path(os.environ['TL_ROOT'])
RUNTIME = ROOT / '.runtime'
PRIMARY = os.environ['TL_PRIMARY_WS']
KEY = os.environ['TL_ANVIL_KEY']
PORTS = [10087, 11087, 12087, 13087]
CONTRACT = ''

def run(*args, timeout=90, allow_failure=False):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    if result.returncode and not allow_failure:
        raise RuntimeError(f'{args[0]} failed: {result.stderr.strip() or result.stdout.strip()}')
    return result

def call(endpoint, signature, *args):
    raw_signature = signature.split(' returns ')[0]
    raw = run('cast', 'call', '--no-proxy', '--rpc-timeout', '20', '--rpc-url', endpoint,
              CONTRACT, raw_signature, *args, timeout=35).stdout.strip()
    if raw_signature == signature:
        return raw
    return run('cast', 'abi-decode', signature, raw).stdout.strip()

def wait_until(label, probe, timeout=120):
    deadline = time.monotonic() + timeout
    last_error = ''
    while time.monotonic() < deadline:
        try:
            value = probe()
            if value:
                return value
        except (RuntimeError, ValueError, OSError, subprocess.TimeoutExpired) as exc:
            last_error = str(exc)
        time.sleep(1)
    raise RuntimeError(f'Timed out waiting for {label}. {last_error}')

def node_info(port):
    req = urllib.request.Request(f'http://127.0.0.1:{port}/lyquor.node.v1.NodeService/GetNodeInfo',
                                 b'{}', {'content-type': 'application/json'})
    with urllib.request.urlopen(req, timeout=8) as response:
        data = json.load(response)
    node = data.get('node_id', data.get('nodeId', {}))
    value = node.get('value') if isinstance(node, dict) else node
    if not isinstance(value, str) or not value.startswith('Node-'):
        raise RuntimeError(f'No Node ID from port {port}.')
    encoded = value[5:].upper()
    decoded = base64.b32decode(encoded + '=' * ((8-len(encoded)%8)%8), casefold=True)
    if len(decoded) != 35:
        raise RuntimeError(f'Unexpected Node ID length on port {port}.')
    return value, '0x'+decoded[:32].hex()

def send(signature, *args):
    raw = run('cast', 'send', '--no-proxy', '--json', '--rpc-url', PRIMARY,
              '--private-key', KEY, CONTRACT, signature, *args).stdout
    receipt = json.loads(raw)
    status = receipt.get('status')
    if (int(status, 0) if isinstance(status, str) else status) != 1:
        raise RuntimeError(f'Transaction reverted for {signature}.')
    return receipt

def epoch(endpoint):
    return int(call(endpoint, '__lyquor_oracle_dest_epoch_info(string,bool) returns (uint64,bytes32,uint32,bytes)',
                    'treasury', 'false').splitlines()[0].strip())

def parse_deploy(text):
    decoder = json.JSONDecoder()
    for start, char in enumerate(text):
        if char != '{':
            continue
        try:
            value, _ = decoder.raw_decode(text[start:])
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and 'lyquid_id' in value and 'contract' in value:
            return value
    raise RuntimeError('shaker did not return its JSON deployment result. See .runtime/deploy.stdout.log.')

def main():
    global CONTRACT
    RUNTIME.mkdir(exist_ok=True)
    seed_path = RUNTIME/'seed.json'
    if not seed_path.exists():
        raise RuntimeError('Run python3 scripts/seed.py before deployment.')
    seed = json.loads(seed_path.read_text())
    nodes = [node_info(port) for port in PORTS]
    if len({node[0] for node in nodes}) != 4:
        raise RuntimeError('Expected four distinct node identities.')
    print('Building and deploying TreasuryLens on the local four-node network.', flush=True)
    # json goes to stdout; compiler diagnostics remain available in a log file.
    manifest = Path(os.environ.get('TL_MANIFEST', str(ROOT/'lyquids/treasurylens/Cargo.toml'))).resolve()
    deploy_args = ['shaker', 'deploy', '--output', 'json',
                   '--endpoint', PRIMARY, '--private-key', KEY,
                   '--reference', os.environ['TL_REGISTRY']+':treasurylens-local']
    update_id = os.environ.get('TL_UPDATE_LYQUID')
    if update_id:
        if not re.fullmatch(r'Lyquid-[a-z2-7]+', update_id):
            raise RuntimeError('Invalid TL_UPDATE_LYQUID identity.')
        deploy_args.extend(['--update', update_id])
    deploy_args.append(str(manifest))
    output = run(*deploy_args, timeout=900, allow_failure=True)
    (RUNTIME/'deploy.stdout.log').write_text(output.stdout)
    (RUNTIME/'deploy.stderr.log').write_text(output.stderr)
    if output.returncode:
        raise RuntimeError('shaker deployment failed; inspect .runtime/deploy.stderr.log and deploy.stdout.log. '+output.stderr[-2000:])
    result = parse_deploy(output.stdout)
    lyquid_id = result['lyquid_id']
    CONTRACT = result['contract']
    if not isinstance(lyquid_id, str) or not re.fullmatch(r'Lyquid-[a-z2-7]+', lyquid_id):
        raise RuntimeError('Invalid deployed Lyquid ID.')
    if not isinstance(CONTRACT, str) or not re.fullmatch(r'0x[0-9a-fA-F]{40}', CONTRACT):
        raise RuntimeError('Invalid deployment contract address.')
    target = run('shaker', 'to-hex', lyquid_id).stdout.strip()
    if not re.fullmatch(r'0x[0-9a-fA-F]{40}', target):
        raise RuntimeError('shaker to-hex did not return an address.')
    print(f'Lyquid: {lyquid_id}\nContract: {CONTRACT}', flush=True)
    endpoints = [f'ws://127.0.0.1:{port}/ws' for port in PORTS]
    for endpoint in endpoints:
        wait_until(f'TreasuryLens on {endpoint}', lambda e=endpoint: call(e, 'get_node_ids() returns (string[])'))
    committee = '['+','.join(node[1] for node in nodes)+']'
    existing = set(re.findall(r'Node-[a-z2-7]+', call(PRIMARY, 'get_node_ids() returns (string[])')))
    expected_nodes = {node[0] for node in nodes}
    if existing:
        if existing != expected_nodes:
            raise RuntimeError('Existing committee differs from this local network; refusing to replace it.')
        print('Reusing the existing initializer-authorized committee (assets-only update).', flush=True)
    else:
        send('configure_committee(bytes32[])', committee)
    old_epoch = epoch(PRIMARY)
    if existing and old_epoch > 0:
        new_epoch = old_epoch
        print('Committee is already active; preserving its current epoch.', flush=True)
    else:
        print('Activating the initializer-authorized treasury committee.', flush=True)
        wait_until('oracle activation submission', lambda:
                   call(PRIMARY, '__lyquor_oracle_advance_epoch(string,address,bool) returns (bool)',
                        'treasury', target, 'false').strip() == 'true')
        new_epoch = wait_until('target epoch advancement', lambda: (e if (e:=epoch(PRIMARY)) > old_epoch else 0))
        for endpoint in endpoints:
            wait_until(f'epoch {new_epoch} at {endpoint}', lambda e=endpoint: epoch(e) >= new_epoch)
        wait_until('oracle source configuration synchronization', lambda:
                   call(PRIMARY, '__lyquor_oracle_finalize_epoch(string,address,bool) returns (bool)',
                        'treasury', target, 'false').strip() == 'true')
    expected_nodes = {node[0] for node in nodes}
    def committee_matches(endpoint):
        text = call(endpoint, 'get_node_ids() returns (string[])')
        return set(re.findall(r'Node-[a-z2-7]+', text)) == expected_nodes
    for endpoint in endpoints:
        wait_until(f'active committee at {endpoint}', lambda e=endpoint: committee_matches(e))
    print('Setting the seeded query defaults.', flush=True)
    send('set_defaults(string,string,uint64,uint64,string)', seed['token'], seed['treasury'],
         str(seed['from_block']), str(seed['to_block']), seed['budget'])
    deployment = {
        'schema_version': 1, 'created_at': datetime.now(timezone.utc).isoformat(),
        'lyquid_id': lyquid_id, 'contract': CONTRACT, 'target': target, 'manifest': str(manifest),
        'endpoint': PRIMARY, 'rpc_url': os.environ['TL_RPC_URL'],
        'node_endpoints': endpoints, 'committee': sorted(expected_nodes),
        'threshold': 3, 'epoch': new_epoch, 'defaults': {
            key: seed[key] for key in ['chain_id','token','treasury','from_block','to_block','budget']},
        'trusted_setup': 'Initializer-authorized 3-of-4 committee activated and checked on all four nodes.',
    }
    temporary = RUNTIME/'deployment.json.tmp'
    temporary.write_text(json.dumps(deployment, indent=2)+'\n')
    temporary.replace(RUNTIME/'deployment.json')
    print(f'Deployment saved: {RUNTIME/"deployment.json"}\nServe the app with:\n'
          f'  shaker serve {lyquid_id} --endpoint {PRIMARY}', flush=True)

try:
    main()
except (RuntimeError, ValueError, KeyError, OSError, urllib.error.URLError, subprocess.TimeoutExpired) as exc:
    print(f'Deployment failed: {exc}', file=sys.stderr)
    sys.exit(1)
PY

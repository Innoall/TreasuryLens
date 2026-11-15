#!/usr/bin/env python3
"""Create deterministic real ERC-20 transfers on the project's local Anvil chain.

Only uses Python's standard library and pinned Foundry tools. No public RPC or
wallet configuration is accepted; keys are the publicly known Anvil defaults.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / '.runtime'
RPC_URL = 'http://127.0.0.1:8545'
KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80'
DONOR_KEY = '0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d'
TREASURY = '0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266'
DONOR = '0x70997970c51812dc3a010c7d01b50e0d17dc79c8'
RECIPIENTS = ['0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc',
              '0x90f79bf6eb2c4f870365e785982e1f101e93b906',
              '0x15d34aaf54267db7d7c367839aaf71a00a2c6a65']
ENV = os.environ.copy()
ENV['PATH'] = str(ROOT / '.tools/bin') + os.pathsep + ENV.get('PATH', '')

def run(*args: str, timeout: int = 120) -> str:
    try:
        result = subprocess.run(args, cwd=ROOT, env=ENV, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=timeout, check=False)
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        raise RuntimeError(f'{args[0]} unavailable or timed out; run bootstrap and localnet first.') from exc
    if result.returncode:
        raise RuntimeError(f'{args[0]} failed: {result.stderr.strip() or result.stdout.strip()}')
    return result.stdout.strip()

def rpc(method: str, params: list) -> object:
    data = json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params}).encode()
    req = urllib.request.Request(RPC_URL, data, {'content-type': 'application/json'})
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            result = json.load(response)
    except (urllib.error.URLError, TimeoutError) as exc:
        raise RuntimeError('Local Anvil is unavailable. Keep bash scripts/localnet.sh running.') from exc
    if 'error' in result:
        raise RuntimeError(f'{method}: {result["error"]}')
    return result['result']

def as_int(value: str | int) -> int:
    return int(value, 0) if isinstance(value, str) else value

def send(token: str, destination: str, value: int, key: str = KEY) -> dict:
    receipt = json.loads(run('cast', 'send', '--no-proxy', '--json', '--rpc-url', RPC_URL,
                             '--private-key', key, token, 'transfer(address,uint256)', destination, str(value)))
    if as_int(receipt['status']) != 1:
        raise RuntimeError('Token transfer reverted.')
    return receipt

def seed(force: bool = False) -> dict:
    RUNTIME.mkdir(exist_ok=True)
    chain_id = as_int(rpc('eth_chainId', []))
    if chain_id != 31337:
        raise RuntimeError(f'Refusing chain ID {chain_id}; only the local Anvil chain 31337 is allowed.')
    client = str(rpc('web3_clientVersion', []))
    if 'anvil' not in client.lower():
        raise RuntimeError('Refusing a non-Anvil endpoint.')
    previous = RUNTIME / 'seed.json'
    if previous.exists() and not force:
        data = json.loads(previous.read_text())
        block = rpc('eth_getBlockByNumber', [hex(data['to_block']), False])
        code = rpc('eth_getCode', [data['token'], 'latest'])
        if block and block['hash'].lower() == data['snapshot_hash'].lower() and code != '0x':
            print('Existing seed still matches this chain; reusing it. Use --force to create a new dataset.')
            return data
        raise RuntimeError('Saved seed does not match the current chain. Run seed.py --force or restart localnet.')
    for key, expected in [(KEY, TREASURY), (DONOR_KEY, DONOR)]:
        if run('cast', 'wallet', 'address', '--private-key', key).lower() != expected:
            raise RuntimeError('A public development key does not match the intended Anvil account.')
    print('Compiling the local test ERC-20 (solc 0.8.30, EVM paris).', flush=True)
    bytecode = run('forge', 'inspect', '--root', str(ROOT), '--contracts', str(ROOT/'contracts'),
                   '--out', str(RUNTIME/'solidity'), '--cache-path', str(RUNTIME/'solidity-cache'),
                   '--use', '0.8.30', '--evm-version', 'paris',
                   'contracts/TestToken.sol:TestToken', 'bytecode', timeout=300)
    if not re.fullmatch(r'0x[0-9a-fA-F]+', bytecode):
        raise RuntimeError('forge did not return a single bytecode value.')
    # ABI encoding of the one uint256 constructor argument, without dependencies.
    supply = 1_000_000_000
    creation_data = bytecode + f'{supply:064x}'
    receipt = json.loads(run('cast', 'send', '--no-proxy', '--json', '--rpc-url', RPC_URL,
                             '--private-key', KEY, '--create', creation_data))
    if as_int(receipt['status']) != 1 or not receipt.get('contractAddress'):
        raise RuntimeError('Token deployment failed.')
    token = receipt['contractAddress'].lower()
    # Fund a donor outside the query interval. Mint and this setup transfer must
    # not contribute to the query's expected outbound total.
    fund = send(token, DONOR, 10_000_000)
    from_block = as_int(fund['blockNumber']) + 1
    transfers = []
    def record(label: str, source: str, destination: str, amount: int, key: str = KEY):
        tx = send(token, destination, amount, key)
        transfers.append({'label': label, 'from': source, 'to': destination, 'amount': str(amount),
                          'transaction_hash': tx['transactionHash'], 'block_number': as_int(tx['blockNumber'])})
    record('inbound_excluded', DONOR, TREASURY, 500_000, DONOR_KEY)
    for i, (destination, amount) in enumerate(zip(RECIPIENTS, [1_250_000, 2_500_000, 3_750_000]), 1):
        record(f'outbound_{i}', TREASURY, destination, amount)
    record('unrelated_excluded', DONOR, RECIPIENTS[0], 111_111, DONOR_KEY)
    to_block = transfers[-1]['block_number']
    snapshot = rpc('eth_getBlockByNumber', [hex(to_block), False])
    # Freeze the reference historical interval, then add headroom for depth checks.
    rpc('anvil_mine', [hex(3)])
    start = rpc('eth_getBlockByNumber', [hex(from_block), False])
    data = {
        'schema_version': 1, 'chain_id': chain_id, 'rpc_url': RPC_URL,
        'token': token, 'treasury': TREASURY, 'decimals': 6,
        'from_block': from_block, 'to_block': to_block,
        'snapshot_hash': snapshot['hash'], 'from_block_hash': start['hash'],
        'budget': '6000000', 'expected_total': '7500000', 'expected_count': 3,
        'expected_budget_exceeded': True, 'transactions': transfers,
        'setup': {'deployment_transaction': receipt['transactionHash'],
                  'funding_transaction': fund['transactionHash'], 'supply': str(supply)},
        'assumptions': 'Local standard ERC-20; gross outbound Transfer sum in an inclusive block interval. '
                       'Known expected values are independent of the production indexer implementation.'
    }
    temporary = RUNTIME / 'seed.json.tmp'
    temporary.write_text(json.dumps(data, indent=2) + '\n')
    temporary.replace(previous)
    print(f'Seed saved: {previous}\nToken: {token}\nInterval: {from_block}..{to_block}\nExpected outbound: 7500000 units, 3 transfers')
    return data

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--force', action='store_true', help='Deploy another dataset instead of reusing a matching seed.')
    args = parser.parse_args()
    try:
        seed(args.force)
    except (RuntimeError, ValueError, KeyError, OSError) as exc:
        print(f'Seed failed: {exc}', file=sys.stderr)
        return 1
    return 0

if __name__ == '__main__':
    raise SystemExit(main())

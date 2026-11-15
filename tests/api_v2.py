#!/usr/bin/env python3
"""Live read/replay/evidence checks; never deploys, injects faults or mutates chain.

Requires a running v2 app with the seeded query already confirmed (or confirms
that normal seed query through its existing public query API).
"""
from __future__ import annotations
import argparse
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def request(url, payload=None, raw=None, timeout=35):
    data = raw if raw is not None else (None if payload is None else json.dumps(payload).encode())
    req = urllib.request.Request(url, data, {'content-type': 'application/json'})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url', default='http://127.0.0.1:8080')
    args = parser.parse_args()
    base = args.base_url.rstrip('/')
    seed = json.loads((ROOT / '.runtime/seed.json').read_text())
    status_code, status = request(base + '/api/status')
    assert status_code == 200 and status['active'], status
    query = {key: seed[key] for key in ['token', 'treasury', 'from_block', 'to_block', 'budget']}
    code, reply = request(base + '/api/query', query)
    assert code in [200, 202], (code, reply)
    query_id = reply['query_id']
    record = reply.get('record')
    deadline = time.monotonic() + 30
    while record is None and time.monotonic() < deadline:
        records = request(base + '/api/results')[1]['results']
        record = next((value for value in records if value['summary']['query_id'] == query_id), None)
        if record is None:
            time.sleep(.25)
    assert record is not None, 'Seed query not confirmed.'
    rows = []
    evidence = {}

    def case(name, check):
        start = time.monotonic()
        try:
            result = check()
            rows.append({'case': name, 'passed': True,
                         'elapsed_ms': round((time.monotonic() - start) * 1000, 2), 'evidence': result})
            print('PASS', name, flush=True)
        except Exception as error:
            rows.append({'case': name, 'passed': False, 'error': str(error)})
            print('FAIL', name, str(error), flush=True)

    def audit_match():
        code, body = request(base + '/api/audit', {'query_id': query_id})
        assert code == 200 and body['status'] == 'matched', (code, body)
        assert body['scope'] == 'source-replay-not-new-certificate', body
        assert body['record'] == record, body
        report = body['audit']
        assert report['summary'] == record['summary'], report
        assert report['summary']['total'] == seed['expected_total'], report
        assert report['summary']['transfer_count'] == seed['expected_count'], report
        assert sum(int(row['total']) for row in report['recipients']) == int(seed['expected_total']), report
        assert sum(row['count'] for row in report['recipients']) == seed['expected_count'], report
        assert sum(int(row['total']) for row in report['blocks']) == int(seed['expected_total']), report
        assert len(report['transfers']) == seed['expected_count'], report
        assert report['self_transfer_total'] == '0' and report['self_transfer_count'] == 0, report
        if 'token_metadata' in body:
            assert body['token_metadata']['decimals'] == seed['decimals'], body
            assert body['token_metadata']['scope'] == 'source-rpc-not-certified', body
        assert isinstance(body['checked_at_ms'], int), body
        evidence.update({'schema_version': 1, 'query': record['query'],
                         'summary': record['summary'], 'logs': body['logs']})
        return {'query_id': query_id, 'summary': report['summary'],
                'recipient_count': len(report['recipients']), 'block_count': len(report['blocks']),
                'top_recipient_share_bps': report['top_recipient_share_bps'],
                'token_metadata': body.get('token_metadata')}

    case('replay_matches_certified_exact_summary_and_breakdowns', audit_match)

    def revalidate():
        code, body = request(base + '/api/revalidate', {'query_id': query_id})
        assert code == 200 and body['status'] == 'canonical', (code, body)
        assert body['expected_hash'] == body['current_hash'] == record['query']['snapshot_hash'], body
        assert body['confirmations_now'] >= 2 and body['checked_at_ms'] > 0, body
        assert 'not strong finality' in body['warning'], body
        return body
    case('revalidate_current_endpoint_preserves_record', revalidate)

    def verify(bundle, expected):
        assert evidence, 'Replay prerequisite did not yield exportable evidence.'
        code, body = request(base + '/api/verify-evidence', bundle)
        assert code == 200 and body['status'] == expected, (code, body)
        assert body['certificate_verified'] is False and body['scope'] == 'supplied-contents-only', body
        if expected == 'valid':
            assert body['audit']['summary'] == record['summary'], body
        else:
            assert 'audit' not in body and body.get('error'), body
        return {'status': body['status'], 'certificate_verified': False, 'error': body.get('error')}

    case('exported_contents_verify_without_certificate_claim', lambda: verify(evidence, 'valid'))

    def tamper(field, value):
        bundle = copy.deepcopy(evidence)
        bundle['summary'][field] = value
        return verify(bundle, 'invalid')
    case('tampered_total_rejected', lambda: tamper('total', str(int(seed['expected_total']) + 1)))
    case('tampered_digest_rejected', lambda: tamper('logs_digest', '0x' + '11' * 32))

    def tamper_log():
        bundle = copy.deepcopy(evidence)
        assert bundle['logs'], 'No sample log.'
        bundle['logs'][0]['data'] = '0x' + '00' * 32
        return verify(bundle, 'invalid')
    case('tampered_event_rejected', tamper_log)

    def unsupported_schema():
        bundle = copy.deepcopy(evidence)
        bundle['schema_version'] = 2
        return verify(bundle, 'invalid')
    case('unsupported_evidence_schema_rejected', unsupported_schema)

    def untrusted_extras():
        bundle = copy.deepcopy(evidence)
        bundle['provenance'] = {'supporters': ['fake-node'], 'certificate': 'not-a-real-certificate'}
        code, body = request(base + '/api/verify-evidence', bundle)
        assert code == 400 and body['status'] == 'invalid' and body['certificate_verified'] is False, (code, body)
        assert 'audit' not in body, body
        return {'http_status': code, 'status': body['status'], 'certificate_verified': False}
    case('unknown_provenance_fields_rejected_without_certificate_claim', untrusted_extras)

    def oversized():
        code, body = request(base + '/api/verify-evidence', raw=b' ' * (1024 * 1024 + 1))
        assert code == 413 and body['status'] == 'invalid' and body['certificate_verified'] is False, (code, body)
        return {'http_status': code, 'status': body['status']}
    case('oversized_evidence_rejected', oversized)

    def unknown(path):
        code, body = request(base + path, {'query_id': '0x' + '00' * 32})
        assert code == 404 and body['status'] == 'unavailable', (code, body)
        assert 'audit' not in body and 'logs' not in body, body
        return {'http_status': code, 'error': body.get('error')}
    case('unknown_record_audit_unavailable', lambda: unknown('/api/audit'))
    case('unknown_record_revalidation_unavailable', lambda: unknown('/api/revalidate'))

    def malformed_evidence():
        code, body = request(base + '/api/verify-evidence', raw=b'{bad-json')
        assert code == 400 and body['status'] == 'invalid' and body['certificate_verified'] is False, (code, body)
        assert 'audit' not in body, body
        return {'http_status': code, 'status': body['status']}
    case('malformed_evidence_json_rejected', malformed_evidence)

    def invalid_id():
        code, body = request(base + '/api/audit', {'query_id': 'not-a-query-id'})
        assert code == 400 and body['status'] == 'unavailable', (code, body)
        assert 'logs' not in body and 'audit' not in body, body
        return {'http_status': code, 'error': body['error']}
    case('invalid_audit_query_id_rejected', invalid_id)

    def unchanged():
        records = request(base + '/api/results')[1]['results']
        latest = next(value for value in records if value['summary']['query_id'] == query_id)
        assert latest == record, latest
        return {'record_unchanged': True, 'query_id': query_id}
    case('audit_and_revalidation_do_not_mutate_certified_record', unchanged)

    output = ROOT / 'output/results/api-v2.json'
    output.parent.mkdir(parents=True, exist_ok=True)
    report = {'kind': 'real-lyquid-source-replay-and-supplied-content-checks',
              'created_at': datetime.now(timezone.utc).isoformat(), 'app': status['lyquid_id'],
              'source_query_id': query_id, 'tests': rows,
              'passed': sum(row['passed'] for row in rows), 'total': len(rows),
              'scope': 'Source replay/current-endpoint/content reconstruction; no new committee certificate or signature verification.'}
    output.write_text(json.dumps(report, indent=2) + '\n')
    print(f"{report['passed']}/{report['total']} passed; {output}")
    return 0 if all(row['passed'] for row in rows) else 1


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as error:
        print('API v2 setup failed:', error, file=sys.stderr)
        sys.exit(1)

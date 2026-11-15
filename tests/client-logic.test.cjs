'use strict';

// Tests the reusable policy module used by the actual UI; no DOM mocks.
const test = require('node:test');
const assert = require('node:assert/strict');
const math = require('../lyquids/treasurylens/assets/client-math.js');
const MAX = (1n << 256n) - 1n;

function record(overrides = {}) {
  const query = {
    chain_id: 31337,
    token: `0x${'ab'.repeat(20)}`,
    treasury: `0x${'cd'.repeat(20)}`,
    from_block: 10,
    to_block: 20,
    snapshot_hash: `0x${'11'.repeat(32)}`,
    rule_version: 1,
    ...(overrides.query || {}),
  };
  return {
    query,
    summary: {
      total: '9007199254740993123456789',
      transfer_count: 3,
      query_id: `0x${'22'.repeat(32)}`,
      snapshot_hash: query.snapshot_hash,
      logs_digest: `0x${'33'.repeat(32)}`,
      ...(overrides.summary || {}),
    },
  };
}

test('money parsing preserves uint256 max and values beyond Number precision', () => {
  assert.equal(math.parseUint256('9007199254740993123456789'), 9007199254740993123456789n);
  assert.equal(math.parseUint256(MAX.toString()), MAX);
  assert.equal(math.parseUint256('0'), 0n);
});

test('money parser rejects overflow, signs, fractions and exponent/hex notation', () => {
  for (const value of [(MAX + 1n).toString(), '-1', '+1', '1.0', '1e6', '0x10', '', 'NaN', 'Infinity']) {
    assert.throws(() => math.parseUint256(value), undefined, String(value));
  }
});

test('money parser never silently accepts an already rounded unsafe Number', () => {
  assert.throws(() => math.parseUint256(Number.MAX_SAFE_INTEGER + 1));
  assert.throws(() => math.parseUint256(Number.POSITIVE_INFINITY));
  assert.throws(() => math.parseUint256(null));
});

test('display units retain every significant fractional digit without Number conversion', () => {
  assert.equal(math.formatUnits('9007199254740993123456789', 6), '9,007,199,254,740,993,123.456789');
  assert.equal(math.formatUnits('1', 18), '0.000000000000000001');
  assert.equal(math.formatUnits('123456789', 0), '123,456,789');
  assert.equal(math.formatUnits('0', 6), '0');
});

test('formatting does not round uint256 maximum or accept invalid decimal counts', () => {
  const groupedMax = MAX.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  assert.equal(math.formatUnits(MAX.toString(), 0), groupedMax);
  for (const decimals of [-1, 1.5, NaN, Infinity]) {
    assert.throws(() => math.formatUnits('1', decimals));
  }
});

test('ratio uses exact integer floor with widened BigInt intermediates', () => {
  assert.equal(math.ratio(MAX.toString(), MAX.toString()), 10000);
  assert.equal(math.ratio('1', '3'), 3333);
  assert.equal(math.ratio('2', '3'), 6666);
  assert.equal(math.ratio('0', '0'), 0);
  assert.equal(math.ratio('1', '0'), 0);
  assert.equal(math.ratio('1', '3', 100), 33);
});

test('record delta is exact and directional B minus A', () => {
  const a = record();
  const b = record({ summary: { total: '9007199254740993123456790' }, query: { from_block: 21, to_block: 30 } });
  const outcome = math.compareRecords(a, b);
  assert.equal(outcome.comparable, true);
  assert.equal(outcome.overlap, false);
  assert.equal(outcome.delta, '1');
  assert.equal(math.compareRecords(b, a).delta, '-1');
});

test('comparison identity excludes snapshot but binds chain/token/treasury/rule', () => {
  const a = record();
  const nextSnapshot = record({ query: { snapshot_hash: `0x${'44'.repeat(32)}` } });
  assert.equal(math.compareRecords(a, nextSnapshot).comparable, true);
  const casing = record({ query: { token: a.query.token.toUpperCase(), treasury: a.query.treasury.toUpperCase() } });
  assert.equal(math.compareRecords(a, casing).comparable, true);
  for (const query of [
    { chain_id: 1 },
    { token: `0x${'aa'.repeat(20)}` },
    { treasury: `0x${'dd'.repeat(20)}` },
    { rule_version: 2 },
  ]) {
    const outcome = math.compareRecords(a, record({ query }));
    assert.equal(outcome.comparable, false, JSON.stringify(query));
    assert.equal(outcome.delta, null);
    assert.equal(typeof outcome.reason, 'string');
    assert.ok(outcome.reason.length > 0);
  }
});

test('inclusive intervals touching at one endpoint overlap', () => {
  const a = record();
  assert.equal(math.compareRecords(a, record({ query: { from_block: 20, to_block: 30 } })).overlap, true);
  assert.equal(math.compareRecords(a, record({ query: { from_block: 21, to_block: 30 } })).overlap, false);
  assert.equal(math.compareRecords(a, record({ query: { from_block: 0, to_block: 10 } })).overlap, true);
  assert.equal(math.compareRecords(a, record({ query: { from_block: 11, to_block: 19 } })).overlap, true);
});

test('all healthy laboratory inputs permit all three policies', () => {
  const result = math.evaluateLab(['good', 'good', 'good', 'good']);
  for (const policy of [result.single, result.value, result.bound]) {
    assert.equal(policy.accepted, true);
    assert.equal(policy.correct, true);
    assert.equal(policy.total, '100');
  }
});

test('one wrong node exposes single-node acceptance but a three-node quorum remains useful', () => {
  const result = math.evaluateLab(['wrong', 'good', 'good', 'good']);
  assert.equal(result.single.accepted, true);
  assert.equal(result.single.correct, false);
  assert.equal(result.single.total, '150');
  assert.equal(result.value.accepted, true);
  assert.equal(result.value.correct, true);
  assert.equal(result.bound.accepted, true);
  assert.equal(result.bound.correct, true);
});

test('equal totals from split snapshots cannot form a bound three-node quorum', () => {
  const result = math.evaluateLab(['stale', 'stale', 'good', 'good']);
  assert.equal(result.value.accepted, true);
  assert.equal(result.value.total, '100');
  assert.equal(result.bound.accepted, false);
});

test('missing responses never count as committee support', () => {
  const absentFirst = math.evaluateLab(['missing', 'good', 'good', 'good']);
  assert.equal(absentFirst.single.accepted, false);
  assert.equal(absentFirst.bound.accepted, true);
  const insufficient = math.evaluateLab(['good', 'good', 'missing', 'missing']);
  assert.equal(insufficient.value.accepted, false);
  assert.equal(insufficient.bound.accepted, false);
});

test('a dishonest majority remains an explicit limitation of agreement-based policies', () => {
  const result = math.evaluateLab(['wrong', 'wrong', 'wrong', 'good']);
  assert.equal(result.value.accepted, true);
  assert.equal(result.value.correct, false);
  assert.equal(result.bound.accepted, true);
  assert.equal(result.bound.correct, false);
});


test('comparison rejects malformed intervals and amounts without rounding', () => {
  const a = record();
  for (const query of [
    { from_block: 21, to_block: 20 },
    { from_block: -1 },
    { to_block: Number.MAX_SAFE_INTEGER + 1 },
    { from_block: 10.5 },
  ]) {
    const result = math.compareRecords(a, record({ query }));
    assert.equal(result.comparable, false);
    assert.equal(result.delta, null);
  }
  assert.equal(math.compareRecords(a, record({ summary: { total: '1.5' } })).comparable, false);
  assert.equal(math.compareRecords(a, null).comparable, false);
});

test('ratio refuses unsafe scale and clamps only ratios beyond safe Number display range', () => {
  for (const scale of [-1, 1.5, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => math.ratio('1', '2', scale));
  }
  assert.equal(math.ratio(MAX.toString(), '1'), Number.MAX_SAFE_INTEGER);
});

test('one stale observation still leaves three current supporters for value-only majority', () => {
  const result = math.evaluateLab(['stale', 'good', 'good', 'good']);
  assert.equal(result.single.amount_correct, true);
  assert.equal(result.single.snapshot_safe, false);
  assert.equal(result.value.accepted, true);
  assert.equal(result.value.amount_correct, true);
  assert.equal(result.value.snapshot_safe, true);
  assert.equal(result.bound.snapshot_safe, true);
});

test('two stale observations expose binding failure even when numerical total is correct', () => {
  const result = math.evaluateLab(['stale', 'stale', 'good', 'good']);
  assert.equal(result.value.accepted, true);
  assert.equal(result.value.amount_correct, true);
  assert.equal(result.value.snapshot_safe, false);
  assert.equal(result.bound.accepted, false);
});

test('lab rejects unknown or incomplete node configurations instead of treating them as healthy', () => {
  for (const states of [[], ['good'], ['good', 'good', 'good', 'unknown'], null]) {
    assert.throws(() => math.evaluateLab(states));
  }
});

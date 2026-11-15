/* Exact, deterministic browser calculations. No network or certification claims. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.TLMath = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';
  const MAX = (1n << 256n) - 1n;
  function parseUint256(value) {
    if (typeof value !== 'string' && typeof value !== 'bigint') throw new Error('Use an integer string in token base units.');
    const text = String(value).trim();
    if (!/^\d+$/.test(text) || text.length > 78) throw new Error('Enter a non-negative whole number, in token base units.');
    const number = BigInt(text);
    if (number > MAX) throw new Error('This amount exceeds the uint256 range.');
    return number;
  }
  function formatUnits(value, decimals = 0) {
    const number = parseUint256(value);
    if (!Number.isInteger(decimals) || decimals < 0 || decimals > 36) throw new Error('Decimal places must be an integer between 0 and 36.');
    const text = number.toString().padStart(decimals + 1, '0');
    const whole = decimals ? text.slice(0, -decimals) : text;
    const fraction = decimals ? text.slice(-decimals).replace(/0+$/, '') : '';
    return whole.replace(/\B(?=(\d{3})+(?!\d))/g, ',') + (fraction ? `.${fraction}` : '');
  }
  function ratio(numerator, denominator, scale = 10000) {
    const a = parseUint256(numerator), b = parseUint256(denominator);
    if (!Number.isSafeInteger(scale) || scale < 0 || scale > 1000000) throw new Error('Invalid ratio scale.');
    if (!b) return 0;
    const result = a * BigInt(scale) / b;
    if (result > BigInt(Number.MAX_SAFE_INTEGER)) return Number.MAX_SAFE_INTEGER;
    return Number(result);
  }
  function compareRecords(a, b) {
    const no = reason => ({ comparable: false, overlap: false, delta: null, reason });
    if (!a?.query || !b?.query || !a.summary || !b.summary) return no('Choose two confirmed queries.');
    const aa = a.query, bb = b.query;
    if (aa.chain_id !== bb.chain_id) return no('The chain IDs differ.');
    if (typeof aa.token !== 'string' || typeof bb.token !== 'string' || aa.token.toLowerCase() !== bb.token.toLowerCase()) return no('The token contracts differ.');
    if (typeof aa.treasury !== 'string' || typeof bb.treasury !== 'string' || aa.treasury.toLowerCase() !== bb.treasury.toLowerCase()) return no('The treasury addresses differ.');
    if (aa.rule_version !== bb.rule_version) return no('The statistics rule versions differ.');
    for (const q of [aa, bb]) if (![q.from_block, q.to_block].every(n => Number.isSafeInteger(n) && n >= 0) || q.from_block > q.to_block) return no('A query has an invalid block interval.');
    try {
      const delta = (parseUint256(b.summary.total) - parseUint256(a.summary.total)).toString();
      const overlap = Math.max(aa.from_block, bb.from_block) <= Math.min(aa.to_block, bb.to_block);
      return { comparable: true, overlap, delta, reason: overlap ? 'The intervals overlap, including their endpoints. Do not add these totals.' : 'Compatible, non-overlapping query intervals.' };
    } catch { return no('A query contains an invalid integer total.'); }
  }
  function evaluateLab(states) {
    if (!Array.isArray(states) || states.length !== 4 || states.some(s => !['good', 'wrong', 'stale', 'missing'].includes(s))) throw new Error('Choose one supported state for each of four nodes.');
    const observations = states.map(state => state === 'missing' ? null : ({ total: state === 'wrong' ? '150' : '100', snapshot: state === 'stale' ? 'B' : 'A' }));
    const decision = (observation, support) => observation ? ({ accepted: true, total: observation.total, snapshot: observation.snapshot, amount_correct: observation.total === '100', snapshot_safe: observation.snapshot === 'A', correct: observation.total === '100' && observation.snapshot === 'A', support }) : ({ accepted: false, total: null, snapshot: null, amount_correct: false, snapshot_safe: false, correct: false, support: 0 });
    const group = bound => {
      const groups = new Map();
      for (const observation of observations) {
        if (!observation || (bound && observation.snapshot !== 'A')) continue;
        const key = bound ? `${observation.total}:${observation.snapshot}` : observation.total;
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(observation);
      }
      for (const list of groups.values()) if (list.length >= 3) {
        const result = decision(list[0], list.length);
        if (!bound) { result.snapshot = list.every(o => o.snapshot === list[0].snapshot) ? list[0].snapshot : 'mixed'; result.snapshot_safe = list.filter(o => o.snapshot === 'A' && o.total === '100').length >= 3; result.correct = result.amount_correct && result.snapshot_safe; }
        return result;
      }
      return decision(null, 0);
    };
    return { single: decision(observations[0], observations[0] ? 1 : 0), value: group(false), bound: group(true) };
  }
  return { parseUint256, formatUnits, ratio, compareRecords, evaluateLab };
});

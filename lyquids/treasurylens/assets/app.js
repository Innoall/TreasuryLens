'use strict';

(() => {
  const $ = id => document.getElementById(id);
  const U256_MAX = (1n << 256n) - 1n;
  const EVIDENCE_LIMIT = 1048576;
  const STORE = 'treasurylens.workspace.v2';
  const viewNames = { overview: 'Overview', evidence: 'Evidence explorer', compare: 'Compare & budget', lab: 'Integrity lab', workspace: 'Saved workspace' };
  const state = {
    status: null, connected: false, records: [], selectedId: '', audits: new Map(), checks: new Map(),
    busy: false, presetBusy: false, presetRun: 0, auditBusy: false, queryRun: 0, recipientFilter: '', ledgerPage: 0,
    metadata: new Map(), decimals: null, presets: [], lab: ['good', 'good', 'good', 'good'],
    view: 'overview', toastTimer: 0, focusedBeforeDialog: null, lastBudgetRecord: '', queryBudget: null
  };
  const inputs = { token: $('token'), treasury: $('treasury'), from_block: $('from-block'), to_block: $('to-block'), budget: $('query-budget') };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const str = value => value == null ? '' : String(value);
  const idOf = record => str(record?.summary?.query_id);
  const selected = () => state.records.find(record => idOf(record) === state.selectedId) || null;
  const short = (value, left = 10, right = 6) => { const s = str(value); return s.length > left + right + 2 ? `${s.slice(0, left)}…${s.slice(-right)}` : s; };
  const message = error => typeof error === 'string' ? error : error?.message || 'The service returned an unspecified error.';
  const groupInteger = value => str(value).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const amount = (value, decimals = 0) => { try { return TLMath.formatUnits(String(value), decimals); } catch { return '—'; } };
  const uint = value => TLMath.parseUint256(value);
  const ratio = (numerator, denominator, scale = 10000) => TLMath.ratio(String(numerator), String(denominator), scale);
  function decimalsFor(record) {
    if (state.decimals != null) return state.decimals;
    const metadata = state.metadata.get(`${record?.query?.chain_id}:${str(record?.query?.token).toLowerCase()}`);
    return Number.isInteger(metadata?.decimals) && metadata.decimals >= 0 && metadata.decimals <= 36 ? metadata.decimals : 0;
  }
  function el(tag, className = '', text = null) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = str(text);
    return node;
  }
  function setText(id, value) { $(id).textContent = str(value); }
  function setBadge(id, value, kind = 'neutral') { $(id).className = `badge ${kind}`; setText(id, value); }
  function setError(id, error) { $(id).hidden = !error; $(id).textContent = error ? message(error) : ''; }
  function button(text, action, className = 'text-button', title = '') {
    const node = el('button', className, text); node.type = 'button'; if (title) node.title = title;
    node.addEventListener('click', action); return node;
  }
  function toast(text, error = false) {
    clearTimeout(state.toastTimer);
    const inline = $('event-dialog').open ? $('event-notice') : $('query-dialog').open ? $('query-notice') : null;
    if (inline) {
      inline.className = `dialog-notice${error ? ' error' : ''}`;
      inline.textContent = text; inline.hidden = false; $('toast').hidden = true;
      return;
    }
    $('toast').className = `toast${error ? ' error' : ''}`;
    $('toast').textContent = text; $('toast').hidden = false;
    state.toastTimer = setTimeout(() => { $('toast').hidden = true; }, error ? 6500 : 3200);
  }
  async function copy(value, label = 'Copied') {
    try { await navigator.clipboard.writeText(str(value)); toast(label); }
    catch { toast('Clipboard access is unavailable in this browser. Select and copy the visible value.', true); }
  }
  async function request(path, options = {}, timeout = 15000) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeout);
    try {
      const response = await fetch(path, { ...options, signal: controller.signal, headers: { Accept: 'application/json', ...options.headers } });
      let data;
      try { data = await response.json(); } catch { throw new Error(`Unreadable service response (HTTP ${response.status}).`); }
      if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error('The service returned an unexpected response.');
      if (!response.ok) { const error = new Error(message(data.error || `Request failed (HTTP ${response.status}).`)); error.data = data; throw error; }
      return data;
    } catch (error) {
      if (error.name === 'AbortError') {
        const failure = new Error('The connection timed out. A submitted query may still be processing; refresh confirmed records before retrying.');
        markDisconnected(failure); throw failure;
      }
      if (error instanceof TypeError) {
        const failure = new Error('The local service is unreachable. Check that your Lyquor network and hosted application are running.');
        markDisconnected(failure); throw failure;
      }
      throw error;
    } finally { clearTimeout(timer); }
  }
  const post = (path, body, timeout) => request(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }, timeout);
  function saveLocal() {
    try { localStorage.setItem(STORE, JSON.stringify({ decimals: state.decimals, presets: state.presets })); return true; }
    catch { return false; }
  }
  function loadLocal() {
    try {
      const data = JSON.parse(localStorage.getItem(STORE) || '{}');
      if (Number.isInteger(data.decimals) && data.decimals >= 0 && data.decimals <= 36) state.decimals = data.decimals;
      if (Array.isArray(data.presets)) state.presets = data.presets.filter(p => p && typeof p.name === 'string' && typeof p.token === 'string' && typeof p.treasury === 'string' && Number.isSafeInteger(p.from_block) && Number.isSafeInteger(p.to_block)).slice(0, 20);
    } catch { /* A malformed or unavailable local cache never affects network data. */ }
  }
  function navigate(view) {
    if (!viewNames[view]) view = 'overview';
    state.view = view;
    document.querySelectorAll('[data-view-panel]').forEach(panel => { panel.hidden = panel.dataset.viewPanel !== view; });
    document.querySelectorAll('[data-view]').forEach(link => { const active = link.dataset.view === view; link.classList.toggle('active', active); if (active) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current'); });
    setText('breadcrumb-current', viewNames[view]);
    closeMenu();
    if (view === 'compare') { renderComparison(); renderBudget(); }
  }
  function closeMenu() { $('sidebar').classList.remove('open'); $('mobile-shade').hidden = true; $('mobile-menu').setAttribute('aria-expanded', 'false'); }
  function openQuery(preset) {
    if (preset) applyPreset(preset, true);
    else if (!inputs.token.value && state.status?.defaults) applyPreset(state.status.defaults, false);
    state.focusedBeforeDialog = document.activeElement;
    setError('query-error', null);
    if (!state.presetBusy) setPresetFeedback('');
    $('query-notice').hidden = true;
    $('query-dialog').showModal();
  }
  function closeQuery() { $('query-dialog').close(); state.focusedBeforeDialog?.focus?.(); }
  function applyPreset(preset, overwrite = true) {
    for (const [name, input] of Object.entries(inputs)) {
      const value = preset[name];
      if ((overwrite || !input.value) && (typeof value === 'string' || typeof value === 'number')) {
        input.value = str(value); input.removeAttribute('aria-invalid');
      }
    }
  }
  function setPresetFeedback(text, kind = '') {
    $('query-feedback').className = `query-feedback${kind ? ` ${kind}` : ''}`;
    $('query-feedback').textContent = text;
    $('query-feedback').hidden = !text;
  }
  function markDisconnected(error) {
    state.connected = false; state.status = null;
    $('network-pill').className = 'network-pill failed'; $('sidebar-dot').className = 'status-dot failed';
    setText('network-label', 'Unavailable'); setText('sidebar-state', 'Connection unavailable'); setText('sidebar-chain', 'Check the local Lyquid service');
    setText('heading-network', 'Connection unavailable'); setText('heading-block', 'Refresh to read the source head'); setText('stat-head', '—');
    $('connection-alert').hidden = false; $('connection-alert').className = 'connection-alert error';
    setText('connection-alert-title', 'Unable to connect to the local Lyquid.'); setText('connection-alert-copy', message(error));
    renderCommittee(); updateQueryButton();
  }
  function updateQueryButton() {
    $('submit-query').disabled = state.busy || state.presetBusy || !state.connected || !state.status?.active;
    $('submit-query').replaceChildren(el('span', '', state.busy ? 'Verification in progress…' : state.presetBusy ? 'Reading latest block…' : 'Verify outflows'), el('span', '', '↗'));
    $('use-latest').disabled = state.busy || state.presetBusy;
    $('use-defaults').disabled = state.busy || state.presetBusy;
    document.querySelector('.query-presets').setAttribute('aria-busy', String(state.presetBusy));
  }
  async function refreshStatus({ fillDefaults = true } = {}) {
    try {
      const data = await request('/api/status');
      state.status = data; state.connected = true;
      const active = data.active === true;
      const statusClass = active ? 'active' : 'inactive';
      $('network-pill').className = `network-pill ${statusClass}`;
      $('sidebar-dot').className = `status-dot ${statusClass}`;
      setText('network-label', active ? 'Committee active' : 'Setup required');
      setText('sidebar-state', active ? 'Local network connected' : 'Committee not active');
      setText('sidebar-chain', `Chain ${data.chain_id ?? '—'} · ${Array.isArray(data.committee) ? data.committee.length : 0} configured nodes`);
      setText('heading-network', `Chain ${data.chain_id ?? '—'} / Local devnet`);
      setText('heading-block', data.latest_block != null ? `Source head #${groupInteger(data.latest_block)}` : 'Source head unavailable');
      setText('stat-head', data.latest_block != null ? groupInteger(data.latest_block) : '—');
      setText('stat-policy', data.threshold != null && Array.isArray(data.committee) ? `${data.threshold} of ${data.committee.length}` : '—');
      setText('stat-retention', data.result_retention != null ? groupInteger(data.result_retention) : '—');
      setText('stat-confirmations', data.confirmations != null ? `${data.confirmations} successor blocks required` : 'Current source observation');
      $('connection-alert').hidden = active;
      if (!active) { $('connection-alert').className = 'connection-alert'; setText('connection-alert-title', 'The service is reachable, but committee activation is incomplete.'); setText('connection-alert-copy', 'Complete the local setup before starting a new query. Existing shared records remain inspectable.'); }
      if (fillDefaults && !$('query-dialog').open) applyPreset(data.defaults || {}, false);
      renderCommittee(); updateQueryButton();
      return data;
    } catch (error) {
      markDisconnected(error);
      return null;
    }
  }
  async function fillLatestMatureBlock() {
    if (state.presetBusy || state.busy) return;
    const run = ++state.presetRun;
    const previous = { from: inputs.from_block.value, to: inputs.to_block.value };
    state.presetBusy = true; updateQueryButton();
    setError('query-error', null);
    setText('use-latest', 'Reading source head…');
    setPresetFeedback('Reading the latest source head. Your current interval will be kept if the request fails.', 'pending');
    try {
      const data = await refreshStatus({ fillDefaults: false });
      if (run !== state.presetRun) return;
      if (!data) {
        setPresetFeedback('Could not refresh the source head. The block interval was not changed. Check the local connection and try again.', 'error');
        return;
      }
      const head = data.latest_block, successors = data.confirmations ?? 2;
      if (!Number.isSafeInteger(head) || !Number.isSafeInteger(successors) || successors < 0 || head < successors) {
        setPresetFeedback('The refreshed service did not provide a usable mature block. The block interval was not changed.', 'error');
        return;
      }
      if (inputs.from_block.value !== previous.from || inputs.to_block.value !== previous.to) {
        setPresetFeedback(`Source head refreshed to #${head}. Your interval was kept because you edited it while the request was running. Click Latest mature block again to use a single-block interval.`, 'notice');
        return;
      }
      const block = head - successors;
      inputs.from_block.value = String(block); inputs.to_block.value = String(block);
      inputs.from_block.removeAttribute('aria-invalid'); inputs.to_block.removeAttribute('aria-invalid');
      setPresetFeedback(`Filled #${block}–#${block}: one mature source block, with ${successors} successor block${successors === 1 ? '' : 's'} at the refreshed head #${head}.`);
    } finally {
      if (run === state.presetRun) {
        state.presetBusy = false; setText('use-latest', 'Latest mature block'); updateQueryButton();
      }
    }
  }
  async function refreshRecords({ quiet = false } = {}) {
    const control = $('refresh-results'); control.disabled = true;
    try {
      const data = await request('/api/results');
      if (!Array.isArray(data.results)) throw new Error('The records endpoint did not return a results array.');
      state.records = data.results.filter(r => r?.query && r?.summary && /^\d+$/.test(str(r.summary.total)) && idOf(r))
        .sort((a, b) => b.query.to_block - a.query.to_block || b.query.from_block - a.query.from_block || idOf(a).localeCompare(idOf(b)));
      if (!state.records.some(r => idOf(r) === state.selectedId)) state.selectedId = idOf(state.records[0]);
      setError('history-error', null); setText('stat-records', state.records.length);
      fillRecordSelects(); renderSelected(); renderHistory(); renderEvidence(); renderComparison(); renderBudget();
      return state.records;
    } catch (error) { if (!quiet) setError('history-error', `Could not refresh confirmed records. ${message(error)}`); throw error; }
    finally { control.disabled = false; }
  }
  function recordLabel(record) { return `${record.query.from_block}–${record.query.to_block} · ${short(record.query.treasury, 8, 4)} · ${short(idOf(record), 8, 4)}`; }
  function fillSelect(id, wanted, emptyText) {
    const target = $(id); target.replaceChildren();
    if (!state.records.length) { const option = el('option', '', emptyText); option.value = ''; target.append(option); return; }
    state.records.forEach(record => { const option = el('option', '', recordLabel(record)); option.value = idOf(record); target.append(option); });
    if (state.records.some(r => idOf(r) === wanted)) target.value = wanted;
  }
  function fillRecordSelects() {
    const compareA = $('compare-a').value, compareB = $('compare-b').value, budget = $('budget-record').value;
    fillSelect('selected-record', state.selectedId, 'No confirmed queries');
    fillSelect('evidence-record', state.selectedId, 'No confirmed queries');
    fillSelect('compare-a', compareA || idOf(state.records[1]) || state.selectedId, 'No confirmed queries');
    fillSelect('compare-b', compareB || state.selectedId, 'No confirmed queries');
    fillSelect('budget-record', budget || state.selectedId, 'No confirmed queries');
  }
  function selectRecord(id, view) {
    if (!state.records.some(r => idOf(r) === id)) return;
    state.selectedId = id; $('selected-record').value = id; $('evidence-record').value = id;
    state.recipientFilter = ''; state.ledgerPage = 0;
    renderSelected(); renderCommittee(); renderEvidence();
    if (view) { location.hash = view; navigate(view); window.scrollTo({ top: 0, behavior: 'smooth' }); }
  }
  function fact(container, label, value, copyable = false) {
    const row = el('div'); const dt = el('dt', '', label); const dd = el('dd', '', value);
    if (copyable) dt.append(button('Copy', () => copy(value), 'copy-tiny', `Copy ${label}`));
    row.append(dt, dd); container.append(row);
  }
  function renderSelected() {
    const record = selected();
    $('selected-empty').hidden = !!record; $('selected-content').hidden = !record; $('select-record-wrap').hidden = state.records.length < 2;
    setBadge('selected-badge', record ? 'Certified' : 'No selection', record ? 'green' : 'neutral');
    if (!record) { renderCommittee(); return; }
    const d = decimalsFor(record);
    setText('selected-amount', amount(record.summary.total, d));
    setText('selected-unit', d ? `tokens · ${d} decimal places (${state.decimals != null ? 'browser preference' : 'source metadata'})` : 'token base units');
    $('selected-raw').hidden = d === 0; setText('selected-raw', `${groupInteger(record.summary.total)} raw base units`);
    const facts = $('selected-facts'); facts.replaceChildren();
    [['Transfer events', groupInteger(record.summary.transfer_count)], ['Inclusive interval', `#${record.query.from_block} – #${record.query.to_block}`], ['Treasury', short(record.query.treasury, 12, 8)], ['Token contract', short(record.query.token, 12, 8)]].forEach(([label, value]) => { const row = el('div'); row.append(el('label', '', label), el('strong', '', value)); facts.append(row); });
    renderCommittee();
  }
  function renderCommittee() {
    const nodes = Array.isArray(state.status?.committee) ? state.status.committee.filter(n => typeof n === 'string') : [];
    const record = selected(); const target = $('committee-visual'); target.replaceChildren();
    setText('committee-live', nodes.length ? `${state.status.active ? 'Active' : 'Staged'} · ${state.status.threshold} / ${nodes.length} threshold` : 'Awaiting configuration');
    if (!nodes.length) { target.append(el('p', 'muted', 'Committee details appear when the Lyquid is reachable.')); return; }
    nodes.forEach((id, i) => {
      const supporting = record?.supporters?.includes(id); const signing = record?.certifiers?.includes(id);
      const card = el('div', `committee-node${signing ? ' signer' : ''}`); card.title = id;
      const top = el('div', 'committee-node-top'); top.append(el('span', 'node-number', String(i + 1).padStart(2, '0')), el('span', '', `Node ${i + 1}`));
      const roles = el('span', 'node-roles');
      const support = el('i', `role-indicator${supporting ? ' supporting' : ''}`); support.title = supporting ? 'Matching observation' : 'Not listed as a matching observation';
      const sign = el('i', `role-indicator${signing ? ' signing' : ''}`); sign.title = signing ? 'Certificate signer' : 'Not listed as a signer';
      roles.append(support, sign); top.append(roles); card.append(top, el('p', 'node-id', id)); target.append(card);
    });
    setText('committee-explanation', record ? `${record.supporters?.length || 0} matching observations · ${record.certifiers?.length || 0} certificate signers. A signer authorizes the exact shared-state update; it need not have contributed the winning observation.` : 'An observation supports a result. A certificate signer authorizes the exact shared-state update. Select a query to inspect both roles.');
  }
  function renderHistory() {
    const term = $('history-search').value.trim().toLowerCase(); const body = $('history-body'); body.replaceChildren();
    const filtered = state.records.filter(r => `${idOf(r)} ${r.query.treasury} ${r.query.token} ${r.query.from_block} ${r.query.to_block}`.toLowerCase().includes(term));
    $('history-empty').hidden = filtered.length > 0;
    setText('history-empty', state.records.length ? 'No confirmed queries match this search.' : 'No confirmed queries loaded. Run a query to create a shared-state record.');
    filtered.forEach(record => {
      const row = el('tr');
      const q = el('td'); q.append(el('span', '', short(idOf(record), 11, 6)), el('span', 'table-sub', short(record.query.treasury, 10, 6))); q.title = idOf(record);
      const interval = el('td', '', `#${record.query.from_block} – #${record.query.to_block}`);
      const events = el('td', 'numeric', groupInteger(record.summary.transfer_count));
      const total = el('td', 'numeric', groupInteger(record.summary.total));
      const status = el('td'); status.append(el('span', 'badge green', 'Certified'));
      const action = el('td'); action.append(button('Inspect ↗', () => selectRecord(idOf(record), 'evidence'), 'row-open'));
      row.append(q, interval, events, total, status, action); body.append(row);
    });
  }
  function renderEvidence() {
    const record = selected(); const id = idOf(record); const response = state.audits.get(id); const matched = response?.status === 'matched';
    $('audit-record').disabled = !record || state.auditBusy; $('revalidate-record').disabled = !record;
    $('export-evidence').disabled = !matched; $('copy-record').disabled = !record;
    $('evidence-identity').hidden = !record; $('audit-placeholder').hidden = matched; $('audit-content').hidden = !matched;
    setBadge('cert-state-badge', record ? 'Certified' : 'Not selected', record ? 'green' : 'neutral');
    setText('cert-state-copy', record ? `${record.certifiers?.length || 0} signers authorized this shared record. ${record.supporters?.length || 0} matching observations are retained.` : 'Select a confirmed record.');
    if (!record) {
      setBadge('replay-state-badge', 'Not checked'); setText('replay-state-copy', 'A fresh source read must match the full certified summary.');
      setBadge('snapshot-state-badge', 'Not checked'); return;
    }
    const grid = $('identity-grid'); grid.replaceChildren();
    fact(grid, 'Query identifier', id, true); fact(grid, 'Snapshot endpoint hash', record.query.snapshot_hash, true);
    fact(grid, 'Canonical transfer-log digest', record.summary.logs_digest, true); fact(grid, 'Query interval', `${record.query.from_block} – ${record.query.to_block} (inclusive)`);
    fact(grid, 'Treasury address', record.query.treasury, true); fact(grid, 'Token contract', record.query.token, true);
    fact(grid, 'Chain / statistics rule', `${record.query.chain_id} / v${record.query.rule_version}`); fact(grid, 'Gross total / event count', `${record.summary.total} base units / ${record.summary.transfer_count} events`);
    const signers = $('identity-signers'); signers.replaceChildren();
    (record.certifiers || []).forEach((node, i) => { const item = el('span'); item.append(el('strong', '', `Signer ${i + 1} `), button(short(node, 12, 7), () => copy(node, 'Node ID copied'), 'copy-tiny', node)); signers.append(item); });
    if (response) {
      setBadge('replay-state-badge', matched ? 'Matched' : response.status === 'mismatch' ? 'Mismatch' : 'Unavailable', matched ? 'green' : response.status === 'mismatch' ? 'red' : 'amber');
      setText('replay-state-copy', matched ? `All summary fields matched on source replay${response.checked_at_ms ? ` · ${timeLabel(response.checked_at_ms)}` : ''}. This is not a new certificate.` : response.error || 'The source replay did not reproduce the full certified summary. No event breakdown is displayed.');
    } else { setBadge('replay-state-badge', 'Not checked'); setText('replay-state-copy', 'A fresh source read must match the full certified summary.'); }
    renderSnapshotCheck();
    if (matched) renderAudit(response);
  }
  function timeLabel(ms) {
    const date = new Date(Number(ms));
    return Number.isNaN(date.getTime()) ? 'time unavailable' : date.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }
  function renderSnapshotCheck() {
    const record = selected(); const check = state.checks.get(idOf(record));
    if (!check) { setBadge('snapshot-state-badge', 'Not checked'); setText('snapshot-state-copy', 'Check whether the source still reports the expected endpoint hash.'); return; }
    const kind = check.status === 'canonical' ? 'green' : check.status === 'changed' ? 'red' : 'amber';
    setBadge('snapshot-state-badge', check.status === 'canonical' ? 'Same hash' : check.status === 'changed' ? 'Changed' : 'Unavailable', kind);
    const successor = check.confirmations_now != null ? ` ${check.confirmations_now} successor blocks now.` : '';
    const detail = check.status === 'canonical' ? `Current RPC reports the expected endpoint hash.${successor}` : check.status === 'changed' ? `Current RPC endpoint hash differs: ${check.current_hash || 'unavailable'}.` : check.error || check.warning || 'The current source could not be checked.';
    setText('snapshot-state-copy', `${detail} Source observation only${check.checked_at_ms ? ` · ${timeLabel(check.checked_at_ms)}` : ''}; not a new certificate.`);
  }
  async function auditSelected() {
    const record = selected(); if (!record || state.auditBusy) return;
    const id = idOf(record); state.auditBusy = true; state.audits.delete(id); renderEvidence(); $('audit-record').disabled = true; setText('audit-record', 'Replaying…'); setError('audit-error', null);
    if (id === state.selectedId) { setBadge('replay-state-badge', 'Reading source', 'amber'); setText('replay-state-copy', 'Fetching and recomputing the requested source logs.'); }
    try {
      const data = await post('/api/audit', { query_id: id }, 60000);
      if (!['matched', 'mismatch', 'unavailable'].includes(data.status)) throw new Error('The source audit returned an unrecognized status.');
      if (data.status === 'matched' && (!data.audit || !Array.isArray(data.audit.transfers) || !Array.isArray(data.logs) || JSON.stringify(data.audit.summary) !== JSON.stringify(record.summary))) {
        // Compare fields explicitly below; JSON key order is not part of summary equality.
        if (!data.audit?.summary || !sameSummary(data.audit.summary, record.summary) || !Array.isArray(data.audit?.transfers) || !Array.isArray(data.logs)) throw new Error('The matched replay is incomplete or refers to a different summary.');
      }
      state.audits.set(id, data);
      if (data.status === 'matched' && data.token_metadata && Number.isInteger(data.token_metadata.decimals)) {
        state.metadata.set(`${record.query.chain_id}:${record.query.token.toLowerCase()}`, data.token_metadata);
        setText('metadata-note', `Source replay reports ${data.token_metadata.decimals} decimals for ${short(record.query.token)}. Metadata is an RPC observation, not certified. Display preference overrides automatic metadata.`);
      }
      state.recipientFilter = ''; state.ledgerPage = 0;
      if (state.selectedId === id) { renderEvidence(); renderSelected(); }
      renderComparison(); renderBudget();
      toast(data.status === 'matched' ? 'Source replay matched the full certified summary.' : 'Replay did not match. Event details remain hidden.', data.status !== 'matched');
    } catch (error) { state.audits.set(id, {status: 'unavailable', error: message(error)}); if (state.selectedId === id) { renderEvidence(); setError('audit-error', error); } }
    finally { state.auditBusy = false; $('audit-record').disabled = !selected(); setText('audit-record', 'Replay & inspect'); }
  }
  function sameSummary(a, b) { return ['query_id', 'snapshot_hash', 'total', 'transfer_count', 'logs_digest'].every(key => a?.[key] === b?.[key]); }
  async function revalidate() {
    const record = selected(); if (!record) return;
    const id = idOf(record); $('revalidate-record').disabled = true;
    try { const data = await post('/api/revalidate', { query_id: id }, 20000); if (!['canonical', 'changed', 'unavailable'].includes(data.status)) throw new Error('The source check returned an unrecognized status.'); state.checks.set(id, data); if (state.selectedId === id) renderSnapshotCheck(); }
    catch (error) { state.checks.set(id, { status: 'unavailable', error: message(error) }); if (state.selectedId === id) renderSnapshotCheck(); }
    finally { $('revalidate-record').disabled = !selected(); }
  }
  function renderAudit(response) {
    const report = response.audit; const metrics = $('audit-metrics'); metrics.replaceChildren();
    const items = [
      ['MATCHING TRANSFER EVENTS', groupInteger(report.summary.transfer_count), 'Unique, validated logs'],
      ['RECIPIENT ADDRESSES', groupInteger(report.recipients?.length || 0), 'Includes treasury on self-transfers'],
      ['TOP RECIPIENT SHARE', `${(Number(report.top_recipient_share_bps || 0) / 100).toFixed(2)}%`, 'Share of gross outgoing amount'],
      ['SELF-TRANSFER EVENTS', groupInteger(report.self_transfer_count || 0), `${groupInteger(report.self_transfer_total || '0')} base units`]
    ];
    items.forEach(([label, value, hint]) => { const card = el('div', 'audit-metric'); card.append(el('span', '', label), el('strong', '', value), el('small', '', hint)); metrics.append(card); });
    renderRecipients(report); renderBlocks(report); renderLedger();
    setText('audit-caption', `Source: fresh RPC replay of blocks ${selected().query.from_block}–${selected().query.to_block}${response.checked_at_ms ? ` · checked ${timeLabel(response.checked_at_ms)}` : ''}. Shown only because query ID, snapshot, gross total, event count and log digest all matched. This replay is not a new committee certificate.`);
  }
  function renderRecipients(report) {
    const target = $('recipient-chart'); target.replaceChildren();
    const recipients = Array.isArray(report.recipients) ? report.recipients.slice().sort((a, b) => BigInt(a.total) > BigInt(b.total) ? -1 : BigInt(a.total) < BigInt(b.total) ? 1 : a.address.localeCompare(b.address)) : [];
    if (!recipients.length) { target.append(el('p', 'chart-empty', 'No matching Transfer events in this confirmed interval.')); return; }
    recipients.slice(0, 8).forEach(recipient => {
      const row = button('', () => { state.recipientFilter = recipient.address; state.ledgerPage = 0; renderLedger(); $('ledger-search').focus(); }, 'recipient-row', `Filter ledger for ${recipient.address}`);
      const share = BigInt(report.summary.total) === 0n ? '0 base units' : BigInt(recipient.total) > 0n && !recipient.share_bps ? '<0.01%' : `${(Number(recipient.share_bps || 0) / 100).toFixed(2)}%`;
      const head = el('span', 'recipient-row-top'); head.append(el('span', '', `${short(recipient.address, 9, 6)}${recipient.address.toLowerCase() === selected().query.treasury.toLowerCase() ? ' · self' : ''}`), el('span', '', `${share} · ${recipient.count} event${recipient.count === 1 ? '' : 's'}`));
      const track = el('span', 'recipient-bar-track'); const bar = el('span', 'recipient-bar'); bar.style.width = `${Math.min(100, Math.max(0, Number(recipient.share_bps || 0) / 100))}%`; track.append(bar);
      row.append(head, track); target.append(row);
    });
    if (BigInt(report.summary.total) === 0n) target.append(el('p', 'recipient-extra', 'These recipients have zero-value events. There is no positive gross amount to distribute.'));
    if (recipients.length > 8) target.append(el('p', 'recipient-extra', `Showing the 8 largest of ${recipients.length} recipients. All events remain available in the ledger.`));
  }
  function svgNode(tag, attrs = {}, text) { const node = document.createElementNS('http://www.w3.org/2000/svg', tag); Object.entries(attrs).forEach(([key, val]) => node.setAttribute(key, str(val))); if (text != null) node.textContent = str(text); return node; }
  function renderBlocks(report) {
    const target = $('block-chart'); target.replaceChildren();
    const blocks = report.blocks || []; const record = selected();
    if (!blocks.length) { target.append(el('p', 'chart-empty', 'No matching event activity to plot.')); setText('block-caption', 'Source: matched RPC replay. X: source block; Y: gross amount in token base units.'); return; }
    const start = record.query.from_block, end = record.query.to_block; const bucketSize = Math.ceil((end - start + 1) / 60); const slots = Math.ceil((end - start + 1) / bucketSize);
    const buckets = Array.from({ length: slots }, (_, i) => ({ start: start + i * bucketSize, total: 0n, count: 0 }));
    blocks.forEach(block => { const index = Math.min(slots - 1, Math.floor((block.block_number - start) / bucketSize)); if (index >= 0) { buckets[index].total += BigInt(block.total); buckets[index].count += Number(block.count); } });
    const max = buckets.reduce((n, bucket) => bucket.total > n ? bucket.total : n, 0n);
    // Keep labels in normal document flow so they retain readable type at every
    // card width. The SVG contains only the plot; no text can collide with bars.
    const meta = el('div', 'block-chart-meta');
    const scale = el('span', 'block-chart-range', `Scale: 0–${groupInteger(max)}`);
    scale.title = `Maximum bucket: ${max} token base units`;
    meta.append(el('span', '', 'Gross amount · base units'), scale);
    const left = 8, plotWidth = 504, top = 10, baseline = 170, plotHeight = baseline - top;
    const svg = svgNode('svg', { viewBox: '0 0 520 180', role: 'img', 'aria-label': `Gross transferred amount by source block; base units. Scale zero to ${max}.` });
    for (const y of [top, top + plotHeight / 2, baseline]) {
      svg.append(svgNode('line', { x1: left, y1: y, x2: left + plotWidth, y2: y, stroke: y === baseline ? '#cbd7cf' : '#e4eae0', 'stroke-width': 1, 'stroke-dasharray': y === baseline ? 'none' : '4 5', 'vector-effect': 'non-scaling-stroke' }));
    }
    buckets.forEach(bucket => {
      if (bucket.start > end) return;
      const height = Math.max(bucket.count ? 2 : 0, max ? ratio(bucket.total, max, 10000) * plotHeight / 10000 : 0);
      const blockSpan = Math.min(bucketSize, end - bucket.start + 1), slotWidth = plotWidth * blockSpan / (end - start + 1);
      const width = Math.min(84, Math.max(1, slotWidth - Math.min(8, slotWidth / 4)));
      const x = left + plotWidth * (bucket.start - start) / (end - start + 1) + (slotWidth - width) / 2;
      const rect = svgNode('rect', { x, y: baseline - height, width, height, rx: 3, fill: bucket.total === max && max > 0n ? '#247b70' : '#a5bf9f' });
      rect.append(svgNode('title', {}, `Blocks ${bucket.start}–${Math.min(end, bucket.start + bucketSize - 1)}: ${bucket.total} base units, ${bucket.count} events`)); svg.append(rect);
    });
    const axis = el('div', `block-chart-axis${start === end ? ' single' : ''}`);
    axis.append(el('span', '', `#${start}`));
    if (start !== end) axis.append(el('span', '', `#${end}`));
    target.append(meta, svg, axis, el('p', 'block-chart-axis-title', 'Source block'));
    setText('block-caption', `Source: matched RPC replay · ${blocks.length} active block${blocks.length === 1 ? '' : 's'}${bucketSize > 1 ? ` · bars aggregate up to ${bucketSize} consecutive blocks` : ''}. Grid lines mark zero, half-scale and maximum. Zero or very small amounts use a minimal marker above the baseline.`);
  }
  function currentTransfers() { const response = state.audits.get(state.selectedId); return response?.status === 'matched' && Array.isArray(response.audit?.transfers) ? response.audit.transfers : []; }
  function filteredTransfers() {
    const term = $('ledger-search').value.trim().toLowerCase(); const selfOnly = $('self-only').checked; const treasury = selected()?.query?.treasury?.toLowerCase();
    const list = currentTransfers().filter(event => (!state.recipientFilter || event.to === state.recipientFilter) && (!selfOnly || event.to.toLowerCase() === treasury) && `${event.to} ${event.from} ${event.transaction_hash} ${event.block_number} ${event.log_index} ${event.amount}`.toLowerCase().includes(term));
    const sort = $('ledger-sort').value;
    list.sort((a, b) => {
      if (sort.startsWith('amount')) { const aa = BigInt(a.amount), bb = BigInt(b.amount); const diff = aa > bb ? 1 : aa < bb ? -1 : 0; return (sort === 'amount-desc' ? -diff : diff) || a.block_number - b.block_number || a.log_index - b.log_index; }
      return (sort === 'block-desc' ? -1 : 1) * (a.block_number - b.block_number || a.log_index - b.log_index);
    });
    return list;
  }
  function renderLedger() {
    const events = filteredTransfers(); const body = $('ledger-body'); body.replaceChildren(); const pageSize = 20; const pages = Math.max(1, Math.ceil(events.length / pageSize));
    state.ledgerPage = Math.min(state.ledgerPage, pages - 1); const first = state.ledgerPage * pageSize;
    $('clear-ledger-filter').hidden = !state.recipientFilter;
    if (state.recipientFilter) setText('clear-ledger-filter', `Recipient ${short(state.recipientFilter, 6, 4)} ×`);
    events.slice(first, first + pageSize).forEach(event => {
      const row = el('tr');
      const block = el('td', '', `#${event.block_number}`); block.append(el('span', 'table-sub', `log ${event.log_index}`));
      const tx = el('td'); tx.append(button(short(event.transaction_hash, 10, 5), () => inspectEvent(event), 'copy-tiny', event.transaction_hash));
      const recipient = el('td', '', short(event.to, 10, 6)); recipient.title = event.to;
      const total = el('td', 'numeric', groupInteger(event.amount));
      const type = el('td'); type.append(el('span', 'badge neutral', event.to.toLowerCase() === event.from.toLowerCase() ? 'Self' : BigInt(event.amount) === 0n ? 'Zero value' : 'Transfer'));
      const action = el('td'); action.append(button('Inspect ↗', () => inspectEvent(event), 'row-open'));
      row.append(block, tx, recipient, total, type, action); body.append(row);
    });
    if (!events.length) { const row = el('tr'); const cell = el('td', 'table-empty', 'No events match these filters.'); cell.colSpan = 6; row.append(cell); body.append(row); }
    setText('ledger-count', events.length ? `${first + 1}–${Math.min(first + pageSize, events.length)} of ${events.length} matching events` : '0 matching events');
    $('ledger-prev').disabled = state.ledgerPage === 0; $('ledger-next').disabled = state.ledgerPage >= pages - 1; $('export-csv').disabled = !events.length;
  }
  function inspectEvent(event) {
    $('event-notice').hidden = true;
    const target = $('event-detail'); target.replaceChildren();
    [['Amount · base units', event.amount], ['From', event.from], ['To', event.to], ['Token contract', event.token], ['Source block', event.block_number], ['Block hash', event.block_hash], ['Transaction hash', event.transaction_hash], ['Log index', event.log_index]].forEach(([label, value]) => fact(target, label, str(value), true));
    $('event-dialog').showModal();
  }
  function download(name, blob) {
    const url = URL.createObjectURL(blob); const a = el('a'); a.href = url; a.download = name; document.body.append(a); a.click(); a.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  function exportEvidence() {
    const record = selected(); const audit = state.audits.get(idOf(record));
    if (!record || audit?.status !== 'matched' || !Array.isArray(audit.logs)) return toast('Replay a confirmed query successfully before exporting evidence.', true);
    const bundle = { schema_version: 1, query: record.query, summary: record.summary, logs: audit.logs };
    const blob = new Blob([JSON.stringify(bundle)], { type: 'application/json' });
    if (blob.size > EVIDENCE_LIMIT) return toast('This evidence bundle exceeds the 1 MiB import limit. Choose a smaller interval.', true);
    download(`treasurylens-evidence-${idOf(record).slice(2, 14)}.json`, blob); toast('Evidence exported. The bundle supports content verification, not certificate verification.');
  }
  async function importEvidence(file) {
    if (!file) return;
    const report = $('import-report'); report.hidden = false; report.className = 'import-report'; report.replaceChildren(el('h3', '', 'Checking supplied evidence…'), el('p', '', 'Recomputing the uploaded logs against their supplied query and summary.'));
    try {
      if (file.size > EVIDENCE_LIMIT) throw new Error('Evidence files must be at most 1 MiB. Choose a smaller query interval.');
      let bundle;
      const originalText = await file.text();
      try { bundle = JSON.parse(originalText); } catch { throw new Error('This file is not valid JSON.'); }
      if (!bundle || typeof bundle !== 'object' || Array.isArray(bundle) || Object.keys(bundle).sort().join(',') !== 'logs,query,schema_version,summary' || bundle.schema_version !== 1 || !Array.isArray(bundle.logs)) throw new Error('Expected exactly schema_version: 1, query, summary and logs. Upload a TreasuryLens evidence bundle, not an audit response.');
      if (new Blob([originalText]).size > EVIDENCE_LIMIT) throw new Error('This evidence bundle exceeds 1 MiB.');
      let result;
      // Preserve original JSON numeric lexemes and duplicate keys for Rust's strict
      // verifier. Parsing and reserializing would silently round u64 values >2^53.
      try { result = await request('/api/verify-evidence', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: originalText }, 60000); }
      catch (error) { if (error.data?.status === 'invalid') result = error.data; else throw error; }
      if (!['valid', 'invalid'].includes(result.status)) throw new Error('The verifier returned an unrecognized status.');
      const valid = result.status === 'valid'; report.className = `import-report${valid ? '' : ' invalid'}`; report.replaceChildren();
      report.append(el('span', `badge ${valid ? 'green' : 'red'}`, valid ? 'Contents valid' : 'Contents invalid'), el('h3', '', valid ? 'The supplied contents reproduce the summary.' : 'The supplied contents did not verify.'));
      if (valid) { const count = result.audit?.summary?.transfer_count; report.append(el('p', '', `${Number.isSafeInteger(count) ? groupInteger(count) + ' matching events' : 'Contents recomputed (event count outside browser exact range)'} · ${groupInteger(result.audit?.summary?.total ?? '—')} base units.`)); }
      else report.append(el('p', '', result.error || 'The query, summary and logs were not consistent.'));
      const boundary = el('p'); boundary.append(el('strong', '', 'Certificate not verified. '), document.createTextNode('This checks supplied contents only. It does not establish canonical chain membership or committee approval, and it does not add a confirmed record to this workspace.')); report.append(boundary);
      report.append(el('p', '', `File: ${file.name} · ${groupInteger(file.size)} bytes`));
    } catch (error) { report.className = 'import-report invalid'; report.replaceChildren(el('span', 'badge red', 'Import unavailable'), el('h3', '', 'The evidence could not be checked.'), el('p', '', message(error)), el('p', '', 'No network record was created or modified.')); }
    finally { $('evidence-file').value = ''; }
  }
  function csvCell(value) {
    let s = str(value);
    if (/^[\s]*[=+\-@]/.test(s) || /^[\t\r\n]/.test(s)) s = `'${s}`;
    return `"${s.replace(/"/g, '""')}"`;
  }
  function exportCsv() {
    const events = filteredTransfers(); if (!events.length) return;
    const keys = ['block_number', 'block_hash', 'transaction_hash', 'log_index', 'token', 'from', 'to', 'amount'];
    const lines = [keys.map(csvCell).join(','), ...events.map(event => keys.map(key => csvCell(event[key])).join(','))];
    download(`treasurylens-events-${state.selectedId.slice(2, 14)}.csv`, new Blob(['\uFEFF' + lines.join('\r\n')], { type: 'text/csv;charset=utf-8' })); toast(`Exported ${events.length} filtered events. Amounts are raw integer base units.`);
  }
  function renderComparison() {
    const a = state.records.find(r => idOf(r) === $('compare-a').value), b = state.records.find(r => idOf(r) === $('compare-b').value);
    const target = $('compare-content'); target.replaceChildren();
    const comparison = TLMath.compareRecords(a, b);
    if (!comparison.comparable) { target.append(el('p', 'compare-empty', comparison.reason)); return; }
    if (comparison.overlap) target.append(el('div', 'compare-warning', `Overlapping intervals. ${comparison.reason} The delta below compares gross totals; it is not a sum or a measure of independent additional spending.`));
    if (idOf(a) === idOf(b)) target.append(el('p', 'compare-warning', 'Both selectors refer to the same certified query. The difference is zero.'));
    const aa = BigInt(a.summary.total), bb = BigInt(b.summary.total), delta = BigInt(comparison.delta); const d = decimalsFor(a);
    const grid = el('div', 'compare-result-grid');
    [['Interval A', amount(a.summary.total, d), `${a.query.from_block}–${a.query.to_block}`], ['Interval B', amount(b.summary.total, d), `${b.query.from_block}–${b.query.to_block}`], ['Difference · B − A', `${delta > 0n ? '+' : delta < 0n ? '−' : ''}${amount((delta < 0n ? -delta : delta).toString(), d)}`, d ? `tokens · ${d} decimal places` : 'token base units']].forEach(([label, value, note], i) => { const item = el('div'); item.append(el('label', '', label), el('strong', i === 2 ? delta > 0n ? 'delta-positive' : 'delta-negative' : '', value), el('small', '', note)); grid.append(item); });
    target.append(grid);
    const max = aa > bb ? aa : bb; const bars = el('div', 'comparison-bars');
    [['A', aa], ['B', bb]].forEach(([label, total]) => { const row = el('div', 'comparison-bar-row'); const track = el('div', 'comparison-bar-track'); const fill = el('div', 'comparison-bar'); fill.style.width = `${ratio(total.toString(), max.toString()) / 100}%`; track.append(fill); const value = el('span', 'comparison-bar-value', amount(total.toString(), d)); value.title = `${total} base units`; row.append(el('span', '', label), track, value); bars.append(row); });
    target.append(bars);
    const note = el('p', 'field-help', `Same chain ${a.query.chain_id}, token ${short(a.query.token)}, treasury ${short(a.query.treasury)} and rule v${a.query.rule_version}. Each query retains its own snapshot. Bars are normalized to the larger gross total; values use ${d ? `a ${d}-decimal display` : 'base units'}.`); target.append(note);
  }
  function renderBudget() {
    const record = state.records.find(r => idOf(r) === $('budget-record').value); const target = $('budget-visual'); target.replaceChildren();
    if (!record) { target.append(el('p', '', 'Select a confirmed query to explore its gross total against a threshold.')); $('budget-slider').disabled = true; return; }
    const total = uint(record.summary.total);
    $('budget-slider').disabled = total === 0n;
    if (state.lastBudgetRecord !== idOf(record)) {
      state.lastBudgetRecord = idOf(record);
      if (state.queryBudget?.queryId === idOf(record)) $('budget-exact').value = state.queryBudget.threshold;
      else $('budget-exact').value = record.summary.total;
    }
    let threshold;
    try { threshold = uint($('budget-exact').value); setError('budget-error', null); }
    catch (error) { setError('budget-error', error); target.append(el('p', '', 'Enter a valid integer threshold to see the comparison.')); return; }
    const exceeded = total > threshold; const difference = exceeded ? total - threshold : threshold - total;
    target.className = `budget-visual${exceeded ? ' exceeded' : ''}`;
    target.append(el('span', 'tiny-label', 'BROWSER WHAT-IF RESULT'), el('h3', '', exceeded ? 'Above the threshold.' : total === threshold ? 'Exactly at the threshold.' : 'Within the threshold.'), el('p', '', `${amount(difference.toString(), decimalsFor(record))} ${decimalsFor(record) ? 'tokens' : 'base units'} ${exceeded ? 'over' : 'remaining to'} the specified threshold.`));
    const meter = el('div', 'budget-meter'); const fill = el('span'); fill.style.width = `${threshold === 0n ? total > 0n ? 100 : 0 : Math.min(100, ratio(total.toString(), threshold.toString()) / 100)}%`; meter.append(fill); target.append(meter);
    const labels = el('div', 'budget-meter-labels'); labels.append(el('span', '', 'Gross outflow / threshold'), el('span', '', threshold === 0n ? total === 0n ? 'Both zero' : 'Threshold zero' : `${Math.min(99999, ratio(total.toString(), threshold.toString()) / 100).toFixed(1)}%${ratio(total.toString(), threshold.toString()) / 100 > 99999 ? '+' : ''}`)); target.append(labels);
    target.append(el('p', 'budget-detail', `Gross total: ${groupInteger(total)} base units. Threshold: ${groupInteger(threshold)} base units.`), el('p', 'field-help', 'Exact integer comparison in this browser. This is not a committee-certified budget decision, and gross transfers do not equal net spending.'));
    const percent = total === 0n ? 0 : Math.min(200, ratio(threshold.toString(), total.toString(), 100));
    $('budget-slider').value = String(percent); setText('budget-slider-output', total === 0n ? 'Zero gross total · enter an exact threshold above' : `${percent}%${total > 0n && threshold > total * 2n ? '+' : ''}`);
  }
  function renderLab(focusNode = null) {
    const target = $('lab-nodes'); target.replaceChildren();
    const labels = { good: 'Healthy observation', wrong: 'Wrong total', stale: 'Different snapshot', missing: 'No response' };
    state.lab.forEach((mode, i) => {
      const node = el('div', 'lab-node'); node.append(el('span', '', `SYNTHETIC NODE ${i + 1}`), el('h3', '', `Observer ${String(i + 1).padStart(2, '0')}`));
      const select = el('select'); select.setAttribute('aria-label', `Synthetic state for node ${i + 1}`);
      Object.entries(labels).forEach(([value, label]) => { const option = el('option', '', label); option.value = value; select.append(option); }); select.value = mode;
      select.addEventListener('change', () => { state.lab[i] = select.value; renderLab(i); });
      const observation = el('div', 'lab-observation');
      if (mode === 'missing') observation.textContent = 'No input returned.';
      else { observation.append(document.createTextNode('Total '), el('strong', '', mode === 'wrong' ? '150' : '100'), document.createTextNode(' · Snapshot '), el('strong', '', mode === 'stale' ? 'B' : 'A')); }
      node.append(select, observation); target.append(node);
    });
    if (Number.isInteger(focusNode)) target.querySelectorAll('select')[focusNode]?.focus();
    const result = TLMath.evaluateLab(state.lab); const outcomes = $('lab-outcomes'); outcomes.replaceChildren();
    [['single', '01 / SINGLE SOURCE', 'Trust the first node', 'Only synthetic node 1 determines the result.'], ['value', '02 / VALUE-ONLY POLICY', 'Agree on the number', 'Three observations with the same total, even if their snapshots differ.'], ['bound', '03 / SNAPSHOT-BOUND POLICY', 'Agree on the history', 'Three observations must agree on the requested snapshot A and the result.']].forEach(([key, label, title, copy]) => {
      const decision = result[key]; const card = el('section', 'card lab-outcome'); const badge = !decision.accepted ? 'No quorum' : decision.correct ? 'Correct fixture' : 'Unsafe acceptance';
      card.append(el('p', 'eyebrow', label), el('h3', '', title), el('span', `badge ${!decision.accepted ? 'neutral' : decision.correct ? 'green' : 'red'}`, badge), el('strong', '', decision.accepted ? decision.total : '—'), el('p', '', `${copy} ${decision.accepted ? `Accepts total ${decision.total}, snapshot ${decision.snapshot}; ${decision.support} supporting input${decision.support === 1 ? '' : 's'}.` : 'The model withholds an accepted result.'}`)); outcomes.append(card);
    });
  }
  function renderPresets() {
    const target = $('saved-presets'); target.replaceChildren();
    if (!state.presets.length) { target.append(el('p', 'field-help', 'No browser-local presets saved yet. Select a confirmed query, then save its inputs.')); return; }
    state.presets.forEach((preset, i) => { const row = el('div', 'saved-preset'); const content = el('div'); content.append(el('strong', '', preset.name), el('p', '', `${short(preset.treasury)} · ${preset.from_block}–${preset.to_block}`)); const remove = button('Remove', () => { state.presets.splice(i, 1); const saved = saveLocal(); renderPresets(); toast(saved ? 'Browser-local preset removed.' : 'Preset removed for this session, but browser storage is unavailable. The stored copy could not be updated.', !saved); }, 'text-button danger-text'); row.append(content, button('Use', () => openQuery(preset), 'button secondary small'), remove); target.append(row); });
  }
  function validateQuery() {
    setError('query-error', null); Object.values(inputs).forEach(input => input.removeAttribute('aria-invalid'));
    function invalid(text, input) { input.setAttribute('aria-invalid', 'true'); input.focus(); throw new Error(text); }
    const token = inputs.token.value.trim(), treasury = inputs.treasury.value.trim();
    if (!/^0x[\da-fA-F]{40}$/.test(token)) invalid('Enter a token address: 0x followed by 40 hexadecimal characters.', inputs.token);
    if (!/^0x[\da-fA-F]{40}$/.test(treasury)) invalid('Enter a treasury address: 0x followed by 40 hexadecimal characters.', inputs.treasury);
    for (const name of ['from_block', 'to_block']) if (!/^\d+$/.test(inputs[name].value.trim())) invalid('Block numbers must be non-negative whole numbers.', inputs[name]);
    const from = Number(inputs.from_block.value), to = Number(inputs.to_block.value);
    if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to)) invalid('These block numbers exceed the exact range supported by this browser.', inputs.to_block);
    if (from > to) invalid('The end block must be greater than or equal to the start block.', inputs.to_block);
    if (to - from + 1 > 10000) invalid('Choose no more than 10,000 blocks, including both endpoints.', inputs.to_block);
    const result = { token: token.toLowerCase(), treasury: treasury.toLowerCase(), from_block: from, to_block: to };
    if (inputs.budget.value.trim()) { try { result.budget = uint(inputs.budget.value.trim()).toString(); } catch (error) { invalid(message(error), inputs.budget); } }
    return result;
  }
  function progress(title, copy) { $('query-progress').hidden = false; setText('query-progress-title', title); setText('query-progress-copy', copy); }
  async function submitQuery(event) {
    event.preventDefault(); if (state.busy || state.presetBusy) return;
    let input;
    try { input = validateQuery(); } catch (error) { setError('query-error', error); return; }
    if (!state.connected || !state.status?.active) { setError('query-error', 'Connect to an active committee before submitting a query.'); return; }
    const run = ++state.queryRun; state.busy = true; updateQueryButton(); closeQuery();
    progress('Collecting node observations', 'The Lyquid is binding the snapshot and requesting committee agreement.');
    let queryId = '';
    try {
      const data = await post('/api/query', input, 65000);
      queryId = str(data.query_id || idOf(data.record));
      if (queryId && input.budget !== undefined) state.queryBudget = { queryId, threshold: input.budget };
      if (data.status === 'failed') throw new Error(data.error || 'The required committee agreement was not reached. No confirmed record was produced.');
      if (!['confirmed', 'submitted'].includes(data.status) || !queryId) throw new Error('The service did not return a recognized query status and identifier.');
      let record = data.status === 'confirmed' && data.record ? data.record : null;
      if (!record) {
        progress('Waiting for shared-state confirmation', `Submitted ${short(queryId)}. A submission alone is not a confirmed result.`);
        for (let i = 0; i < 12 && run === state.queryRun; i++) { await sleep(1800); const records = await refreshRecords({ quiet: true }); record = records.find(r => idOf(r) === queryId); if (record) break; }
      }
      if (record) {
        await refreshRecords({ quiet: true });
        // Only display certified records returned from shared state, even if POST supplied a record.
        if (!state.records.some(r => idOf(r) === queryId)) { toast('The call was submitted but is not yet visible in shared records. Refresh records before retrying.', true); return; }
        selectRecord(queryId, 'overview'); $('budget-record').value = queryId; state.lastBudgetRecord = ''; renderBudget();
        toast('Query confirmed in shared state. Explore its evidence for a source replay.');
      } else toast('Confirmation is still pending. Refresh shared records before submitting again.', true);
    } catch (error) {
      toast(`${message(error)}${queryId ? ` Query: ${short(queryId)}.` : ''}`, true);
      setError('query-error', error);
      // Keep the input form available after errors; never label a network timeout as a proved certification failure.
      if (!queryId) openQuery();
      setError('query-error', error);
    } finally { state.busy = false; $('query-progress').hidden = true; updateQueryButton(); }
  }
  async function refreshAll() {
    $('refresh-all').disabled = true; $('retry-connect').disabled = true;
    await Promise.allSettled([refreshStatus(), refreshRecords()]);
    // Prefer the seeded meaningful interval on initial load; do not select by hash or an empty test interval.
    if (!state.initialSelectionApplied && state.records.length && state.status?.defaults) {
      const defaults = state.status.defaults;
      const seeded = state.records.find(r => r.query.token.toLowerCase() === str(defaults.token).toLowerCase() && r.query.treasury.toLowerCase() === str(defaults.treasury).toLowerCase() && r.query.from_block === defaults.from_block && r.query.to_block === defaults.to_block);
      if (seeded) { selectRecord(idOf(seeded)); $('compare-b').value = idOf(seeded); $('budget-record').value = idOf(seeded); state.lastBudgetRecord = ''; renderComparison(); renderBudget(); }
      state.initialSelectionApplied = true;
    }
    $('refresh-all').disabled = false; $('retry-connect').disabled = false;
  }

  // Navigation and overlays.
  window.addEventListener('hashchange', () => navigate(location.hash.slice(1)));
  document.querySelectorAll('.sidebar a[href^="#"]').forEach(link => link.addEventListener('click', () => closeMenu()));
  $('mobile-menu').addEventListener('click', () => { const open = !$('sidebar').classList.contains('open'); $('sidebar').classList.toggle('open', open); $('mobile-shade').hidden = !open; $('mobile-menu').setAttribute('aria-expanded', String(open)); });
  $('mobile-shade').addEventListener('click', closeMenu);
  document.querySelectorAll('[data-open-query]').forEach(control => control.addEventListener('click', () => openQuery()));
  document.querySelectorAll('[data-close-dialog]').forEach(control => control.addEventListener('click', closeQuery));
  document.querySelectorAll('[data-close-event]').forEach(control => control.addEventListener('click', () => $('event-dialog').close()));
  for (const id of ['query-dialog', 'event-dialog']) $(id).addEventListener('click', event => { if (event.target === $(id)) { const rect = $(id).getBoundingClientRect(); if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) $(id).close(); } });
  document.addEventListener('keydown', event => {
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName) || event.target.isContentEditable;
    if (event.key === 'Escape') closeMenu();
    if (typing || event.ctrlKey || event.metaKey || event.altKey || $('query-dialog').open || $('event-dialog').open) return;
    if (['1', '2', '3'].includes(event.key)) location.hash = ['overview', 'evidence', 'compare'][Number(event.key) - 1];
    if (event.key.toLowerCase() === 'n') { event.preventDefault(); openQuery(); }
  });
  $('refresh-all').addEventListener('click', refreshAll); $('retry-connect').addEventListener('click', refreshAll);
  $('refresh-results').addEventListener('click', () => refreshRecords().catch(error => toast(message(error), true)));
  $('history-search').addEventListener('input', renderHistory);
  $('selected-record').addEventListener('change', event => selectRecord(event.target.value));
  $('evidence-record').addEventListener('change', event => { setError('audit-error', null); selectRecord(event.target.value); });
  $('explore-selected').addEventListener('click', () => { location.hash = 'evidence'; navigate('evidence'); });
  $('copy-query').addEventListener('click', () => { if (selected()) copy(state.selectedId, 'Query ID copied'); });
  $('copy-record').addEventListener('click', () => { if (selected()) copy(JSON.stringify(selected(), null, 2), 'Certified record JSON copied'); });
  $('query-form').addEventListener('submit', submitQuery);
  $('use-defaults').addEventListener('click', () => {
    if (state.presetBusy || state.busy) return;
    const defaults = state.status?.defaults;
    if (!defaults?.token) { setPresetFeedback('No seeded query defaults are available. Refresh the connection, then try again. Your inputs were not changed.', 'error'); return; }
    applyPreset(defaults); setError('query-error', null);
    setPresetFeedback(`Filled the seeded treasury and token, with interval #${defaults.from_block}–#${defaults.to_block} (both endpoints included).`);
  });
  $('use-latest').addEventListener('click', fillLatestMatureBlock);
  Object.values(inputs).forEach(input => input.addEventListener('input', () => input.removeAttribute('aria-invalid')));
  $('audit-record').addEventListener('click', auditSelected); $('revalidate-record').addEventListener('click', revalidate);
  $('export-evidence').addEventListener('click', exportEvidence);
  $('import-evidence').addEventListener('click', () => $('evidence-file').click());
  $('evidence-file').addEventListener('change', event => importEvidence(event.target.files[0]));
  $('ledger-search').addEventListener('input', () => { state.ledgerPage = 0; renderLedger(); });
  $('ledger-sort').addEventListener('change', () => { state.ledgerPage = 0; renderLedger(); });
  $('self-only').addEventListener('change', () => { state.ledgerPage = 0; renderLedger(); });
  $('clear-ledger-filter').addEventListener('click', () => { state.recipientFilter = ''; state.ledgerPage = 0; renderLedger(); });
  $('ledger-prev').addEventListener('click', () => { state.ledgerPage = Math.max(0, state.ledgerPage - 1); renderLedger(); });
  $('ledger-next').addEventListener('click', () => { state.ledgerPage++; renderLedger(); });
  $('export-csv').addEventListener('click', exportCsv);
  $('compare-a').addEventListener('change', renderComparison); $('compare-b').addEventListener('change', renderComparison);
  $('budget-record').addEventListener('change', renderBudget); $('budget-exact').addEventListener('input', renderBudget);
  $('budget-slider').addEventListener('input', () => { const record = state.records.find(r => idOf(r) === $('budget-record').value); if (!record) return; const threshold = uint(record.summary.total) * BigInt($('budget-slider').value) / 100n; $('budget-exact').value = (threshold > U256_MAX ? U256_MAX : threshold).toString(); renderBudget(); });
  document.querySelectorAll('[data-lab-preset]').forEach(control => control.addEventListener('click', () => { state.lab = control.dataset.labPreset === 'wrong' ? ['wrong', 'good', 'good', 'good'] : control.dataset.labPreset === 'stale' ? ['good', 'good', 'stale', 'stale'] : ['good', 'good', 'good', 'good']; renderLab(); }));
  $('save-current-preset').addEventListener('click', () => { const record = selected(); if (!record) return toast('Select a confirmed query before saving its inputs.', true); const name = $('preset-name').value.trim(); if (!name) { $('preset-name').focus(); return toast('Give this preset a name first.', true); } if (state.presets.length >= 20) return toast('Keep up to 20 browser-local presets. Remove one before saving another.', true); const q = record.query; state.presets.push({ name: name.slice(0, 60), token: q.token, treasury: q.treasury, from_block: q.from_block, to_block: q.to_block }); const saved = saveLocal(); renderPresets(); $('preset-name').value = ''; toast(saved ? 'Query inputs saved in this browser.' : 'Preset is available for this session only. Browser storage is unavailable, so it will not persist.', !saved); });
  $('save-decimals').addEventListener('click', () => { const raw = $('display-decimals').value.trim(), n = Number(raw); if (!/^\d+$/.test(raw) || !Number.isInteger(n) || n < 0 || n > 36) return toast('Choose a whole number of decimal places from 0 to 36.', true); state.decimals = n; const saved = saveLocal(); renderSelected(); renderComparison(); renderBudget(); toast(saved ? 'Display preference applied. Raw amounts are unchanged.' : 'Display preference applied for this session only. Browser storage is unavailable, so it will not persist.', !saved); });
  $('clear-local').addEventListener('click', () => { state.presets = []; state.decimals = null; $('display-decimals').value = '0'; const saved = saveLocal(); renderPresets(); renderSelected(); renderComparison(); renderBudget(); toast(saved ? 'TreasuryLens local preferences cleared. Network records are unchanged.' : 'Preferences cleared for this session, but browser storage is unavailable. Stored preferences could not be updated.', !saved); });

  loadLocal(); $('display-decimals').value = String(state.decimals ?? 0); renderPresets(); renderLab(); renderComparison(); renderBudget();
  navigate(location.hash.slice(1)); refreshAll();
})();

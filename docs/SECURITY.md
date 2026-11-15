# Security review and limitations

TreasuryLens is a local course prototype. This document records its intended assumptions, implemented controls and remaining risks; it is not a third-party audit or a claim of production security.

## Protected statement

A confirmed record states that the configured committee certified the same normalized historical query and summary under rule version 1. It records the endpoint block hash, checked uint256 gross total, event count, canonical event digest, observation supporters and actual certificate signers.

It does **not** establish that the RPC returned every event, that all intermediate blocks are canonical, that the token follows all ERC-20 economic semantics, or that nodes/data sources are independent. A digest commits to supplied event content; it is not a proof against a source-chain receipts root.

## Trust assumptions

- **Initializer and setup:** the intended deployment administrator configures the four correct node identities and completes the first target activation before an unintended configuration is accepted. Initial oracle activation is an operational trust boundary described by the LDK.
- **Committee and runtime:** enough authorized nodes execute the intended observation and aggregation logic, and the LDK's keys, signatures, oracle certificates, target binding and execution checks work correctly. A 3-of-4 matching-observation rule provides useful behavior against one faulty observation under this model; it is not an unrestricted Byzantine fault-tolerance theorem.
- **Source provider:** the configured Anvil/RPC reflects the intended history. All local instances currently read the same underlying chain service, so a common provider error can produce a unanimous wrong result.
- **Chain stability:** the endpoint hash and its history remain meaningful. Two successor blocks are only a depth heuristic. No finalized checkpoint, light client, receipt inclusion proof or reorg-retraction mechanism is implemented.
- **Host:** local processes, downloaded tools, operating-system state and test keys are under the developer's control. Four processes on one laptop are not four independently operated machines.

## Implemented controls and review points

### Untrusted public query input

The adapter validates addresses, range and rule before source reads; caps request bodies at 8 KiB, block ranges at 10,000 inclusive blocks, and returned log arrays at 10,000 entries. Amounts and optional budgets are unsigned uint256 values. The RPC destination is fixed through the runtime's internal sequencer route, rather than accepting arbitrary user URLs.

**Remaining risk:** HTTP input and log limits are application checks, not a complete resource policy. An oversized upstream response may already consume memory before its parsed array length is checked. Public queries incur RPC, oracle and sequencing costs and can be submitted concurrently; there is no user authentication, quota, payment, rate limiter or global concurrency control.

### Source-chain confusion and unstable snapshots

The adapter checks chain ID 31337, requires endpoint contract code, verifies the endpoint number/hash before and after reading, and requires two successor blocks. The core binds the query ID and every summary to the endpoint snapshot.

**Remaining risk:** chain ID is not globally unique and a provider can lie consistently. Code at an address does not guarantee ERC-20 compliance. The implementation does not verify each returned log against a receipts root, fetch every intermediate header, or detect omitted logs. A dishonest provider can forge mutually consistent metadata.

### Parsing, precision and log identity

Strict Transfer-topic/address/sender checks, required mined metadata, range checks, rejection of removed/malformed logs, duplicate consistency, canonical ordering and checked uint256 sums prevent selected parsing and accounting errors. Exact duplicate events count once. The canonical event digest distinguishes histories with equal totals/counts but different validated content.

**Remaining risk:** a standard-looking event can be emitted by a nonstandard or deliberately misleading contract. The statistic includes zero-value and treasury-to-itself events; it is gross outgoing event volume, not money spent, net balance change, token solvency or accounting classification. Rebasing, fee attribution and nonstandard token behavior are outside scope.

### Source replay, drilldown and current-hash checks

The v2 audit endpoint only replays a query obtained from retained network state. It repeats source checks and validates the reconstructed full summary against that stored anchor before releasing transactions, raw logs and grouped totals. Invalid/mismatching evidence does not get a drilldown. Revalidation compares the stored endpoint hash with the current source response and never changes the certificate or deletes the record.

**Remaining risk:** replay uses one instance and its current source RPC; it is not a new committee certificate. Source omissions or consistently forged data can remain undetected, and endpoint stability does not prove every intermediate header or receipt. A canonical-hash response is an observation at a check time, not perpetual or strong finality. Revalidation does not implement revocation; users and consumers must interpret a changed endpoint separately from the original stored attestation.

Recipient and block breakdowns are derived from the same validated canonical event list. Exact duplicates do not inflate aggregates. Shares are floor basis points using U512 intermediates, not floating-point percentages. Optional historical `decimals()` metadata comes from a bounded ABI uint8 decode and is explicitly marked source-RPC metadata; it is not certified and cannot change the base-unit total.

### Uploaded evidence authenticity and resource bounds

The exact schema `{schema_version, query, summary, logs}` rejects unknown fields and unsupported versions. Both HTTP and offline CLI reconstruct strict logs and compare every supplied summary field; input is limited to 1 MiB and 10,000 logs. No uploaded supporter list or apparent certificate metadata is treated as authorization. HTTP returns `certificate_verified: false` for content checks.

**Remaining risk:** internal consistency is not authenticity. An attacker can modify the entire query/log/summary bundle and recompute a self-consistent replacement. The verifier does not authenticate the uploader, fetch a source header, validate a receipts proof or verify committee signatures. Consumers must compare against a separately trusted retained record. The audit endpoint performs this anchor comparison; stateless imports and the offline CLI alone do not. Valid imported data must not become certified history merely by passing reconstruction.

Exported raw-log bounds leave space for a compact evidence bundle; actual serialized byte size still matters, particularly if pretty printed. Application size limits do not prevent host buffering or all concurrent resource pressure. Audit replay adds public RPC/CPU work without quotas or authentication. Retained data can disappear through the existing 200-record policy while an exported file remains only a copy of claims.

### Browser tools and local storage

Comparison, threshold trials, Integrity Lab fault models and browser-saved query presets have no independent consensus authority. Compatible comparisons must match chain, token, treasury and rule; overlapping intervals cannot be summed as nonoverlapping history. Saved presets are browser-local input, not confirmed results. Integrity Lab scenarios are deterministic simulations rather than real node manipulation.

**Remaining risk:** UI arithmetic/validation, local-storage corruption and interaction behavior require their own tests and native-browser review. No security guarantee follows merely from a tab or an explanatory label existing. Browser/UI and outside-user checks remain explicitly separate from API test outcomes.

### Identity, threshold and certified commit

The committee is exactly four distinct IDs, fixed threshold three. LDK verifies signed proposal inputs against the active committee, binds them to the round and reruns aggregation. The application requires matching full summaries from distinct identities and checks supporting membership when committing. The result function exists only under the certified oracle route, not as an ordinary public ETH result setter.

**Remaining risk:** key compromise, authorized-node collusion or a faulty runtime/certificate implementation is outside these checks. The initial four IDs and first activation remain trusted setup. There is no implemented operator key rotation or independent governance protocol. `supporters` and `certifiers` have different meanings and must not be merged into a single claim of source correctness.

### Configuration authority

Committee and UI-default configuration require the initializer at runtime and the creator guard on the Ethereum transaction wrapper. Reconfiguring an already populated committee is rejected. Normal users cannot lower the threshold through a query.

**Remaining risk:** the deployment administrator is trusted. The Ethereum wrapper guard alone would not secure all possible internal paths, which is why runtime authority checks also exist. A successful transaction receipt only means sequencing admission; application effects must be observed afterwards.

### Idempotency and result retention

Code updates through the explicit `TL_UPDATE_LYQUID` development option preserve identity but reset application state in the current LDK. Committee activation/default setup is repeated, prior retained results must be regenerated, and readiness is not guaranteed during the update. Do not treat a stable Lyquid ID or a still-running proxy as proof that old query state survived.

A retained ID with the same summary is idempotent; a conflicting replacement is rejected. New IDs enter a bounded deterministic insertion-order list. At 200 records, a new insertion evicts the oldest record. Existing-ID retries do not refresh insertion order.

**Remaining risk:** anybody able to submit enough valid new queries can evict older results. There is no persistent archival index or protection for frequently consumed records. A record's original insertion supporters/certifiers may differ from a later valid certificate for the same summary. An uncached Budget Monitor check records `status: rejected` when the requested source record is absent. Cached checks, including rejections, are returned without rereading the source. Its 200-entry check cache clears entirely on the next new insertion, so these decisions are disposable observations rather than permanent or continuously refreshed source attestations. Later source-chain reorgs do not automatically retract retained records; consumers must not equate persistence with perpetual canonicality.

### Failure signaling and availability

Invalid source queries fail preflight. When all four signed replies arrive but no three valid summaries match, the adapter certifies a rejection and returns HTTP 422 without submitting a shared-state success update. A signed negative source-read response is counted as a reply, never as a successful summary. A certificate submitted to the sequence backend is separately marked submitted. The client polls shared results before labeling success. Source HTTP calls and oracle collection request bounded timeouts.

**Remaining risk:** the initial 14-case run passed 11 and exposed calls waiting beyond 35 seconds on source-failure/no-quorum paths, despite requesting a 10-second LDK timeout. The revised negative-response/rejection path subsequently passed the full 14/14 selected fault/integration suite. Actual missing responses, transport/runtime scheduling or a powered-off node can still exceed the desired timeout. The browser query timeout is 60 seconds; polling may stop while a submitted call later succeeds, and public service availability is not guaranteed. The explicit `unavailable` test returns a signed source-read error and does not simulate a real process crash, arbitrary network partition or delayed packet schedule.

### Local fault-control isolation

Production `lyquids/treasurylens` has no default features. The separately named test manifest explicitly enables `test-controls`, exposing a public per-instance `set_fault` method. Status identifies whether the current build contains these controls.

**Remaining risk:** the test setter is intentionally unauthenticated and is suitable only for the controlled local experiment. A test deployment must never be presented as the normal protected deployment. Check the manifest in `.runtime/deployment.json` and `test_controls` in `/api/status` before interpreting evidence.

## Local infrastructure review finding

The upstream **v0.8.3 native localnet helper** contains submitter keys for nodes 2–4 that resolve to addresses without the expected default Anvil funding. This was reproduced: token/treasury transactions could succeed while a transaction using a mismatched donor key failed gas estimation because the actual sender had zero ETH.

Our wrapper sources the upstream lifecycle library and overrides only the submitter key array, deriving indices 0–3 from the public default Anvil mnemonic. It leaves node identities, loopback binding and upstream tracked-PID cleanup intact. The seed script also checks that its public test keys resolve to the expected addresses before generating transactions.

During the first already-running development session, the old submitter addresses were funded through **local `anvil_setBalance` only**, without a network restart. The generated session audit is `.runtime/manual-submitters-funding.json`. Fresh starts through our wrapper use the correct derived keys and do not require this manual repair. This finding concerns local development funding, not an exploit of the certificate mechanism.

## Verification evidence and remaining validation

The 2026-11-11 Rust suite had 33 passing tests (28 deterministic-core and 5 synthetic-ablation utility tests), and the initial normal deployment produced a genuine four-node, threshold-certified result matching the seeded total/count. The first broad devnet attempt passed 11 of 14 cases and led to the explicit negative-response/rejection fix described above. The revised test manifest then passed 14/14 checks, and a clean startup/production deployment passed 9/9 normal integration checks. Four additional actual cross-Lyquid consumer checks passed for above/equal/below-threshold behavior and unknown-query rejection. These observed results are saved under `evaluation/observed/`, alongside the initial failing run. Synthetic policy ablations are separately labeled and do not measure real-node performance. Source syntax checks and real-chain seed validation also passed. Those observations cover the listed cases on the recorded revision, not untested future revisions, physical network-failure liveness or independent data sources.

On 2026-11-12, the expanded Rust suite passed **48 tests** (43 library plus 5 synthetic-ablation tests), and the v2 Lyquid built. A separate live test-manifest deployment then passed **14/14 fault/integration checks and 14/14 v2 API checks**. Actual source replay reconciled the 7,500,000 total with recipient amounts 3,750,000 / 2,500,000 / 1,250,000 and shares 5,000 / 3,333 / 1,666 basis points. A compact exported bundle also passed the independent offline CLI content reconstruction. Current production/frontend deployment checks remain separate and pending until recorded. These results do not cover real provider disagreement, physical reorg mutation, powered-off nodes or browser interaction.

Use `bash scripts/test.sh`, then the actual normal/fault integration commands from the README. Preserve current configuration, deployment identity, raw outcomes and command exit status. The generated JSON result files distinguish passed and failed cases; do not replace observed failures with expected outcomes.

Still requires explicit evidence where not yet run:

- native-browser layout, interaction, keyboard and failure-state inspection;
- physical process/network failure experiments if claimed;
- outside-team user operation and feedback;
- independent human review of code, dependency versions, claims and limitations.

## Next improvements

The highest-value follow-ups are independent RPC providers or stronger source-chain proofs; a real finality and reorg policy; admission/rate limits; archival or pinned result retention; explicit deployment/governance lifecycle; and tests for source omissions, provider disagreement and consumer behavior after eviction. Each would need new implementation and measured validation before being advertised as supported.

## References

- [Lyquor certified calls, committee configuration and bootstrap trust](https://docs.lyquor.dev/docs/ldk/oracle/)
- [Lyquor host API category restrictions](https://docs.lyquor.dev/docs/ldk/host-apis/)
- [Ethereum exports and creator-guard limitations](https://docs.lyquor.dev/docs/ldk/external/)
- [Source of the upstream v0.8.3 localnet helper](https://github.com/lyquor-labs/ldk/blob/v0.8.3/scripts/lib/localnet.sh)

## 2026-11-12 release follow-up

The final normal deployment passed 48 Rust unit/evaluator tests, 19 actual UI-helper tests, 9 normal integration cases, 14 source-replay/evidence API cases, and 4 actual cross-Lyquid consumer cases. The separate explicit-test build also passed the 14 fault/integration cases. Saved records use `v2-` names under `evaluation/observed/`. UI source review fixed large-JSON-number import rounding by sending the original file text unchanged, removed stale matched evidence on failed replays, corrected chart interval placement, and clarified synthetic fixture agreement. This is source/automated validation, not an independent human/browser audit.

Assets-only updates were verified to preserve the active committee and retained results; the script refuses an unexpected existing committee and avoids advancing an already-active epoch. Code-digest changes remain state-resetting upgrades with explicit reinitialization.

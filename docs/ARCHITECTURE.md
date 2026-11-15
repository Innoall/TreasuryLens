# Architecture

TreasuryLens answers one narrow question: for a specified standard ERC-20 token and treasury, what is the gross amount represented by outgoing `Transfer` logs in an inclusive block interval, and did the configured Lyquor committee approve the same snapshot-bound result?

## Components

1. **Local source chain:** Anvil, chain ID 31337. The same chain orders Lyquor network calls and holds the seeded ERC-20 contract. This simplifies repeatability; it does not demonstrate independent source operators.
2. **Deterministic core:** `crates/treasurylens-core`, independent of Lyquor host calls, clocks and RPC. It validates queries/logs, constructs identities and digests, performs checked integer summation and selects matching distinct-node observations.
3. **TreasuryLens Lyquid:** four hosted instances. Each reads source RPC and computes a local observation. The built-in LDK oracle gathers signed observations and certifies a network call.
4. **Hosted frontend:** static assets served by a hosting node through `shaker serve`; it displays actual HTTP responses and polls persisted records. Query and Evidence use live endpoints. Comparison, budget calculations, saved browser presets and the Integrity Lab are browser-side tools, with scopes separate from certified state. Native-browser operation still requires explicit validation.
5. **Budget Monitor Lyquid:** an independent consumer whose mutable network transaction invokes the source's read-only network method through `call!`, compares the retained certified total with a supplied uint256 threshold and persists the decision. A separate read-only view retrieves it.

The browser-side budget comparison is a convenience calculation using a confirmed record. It is distinct from invoking the Budget Monitor and is labeled as such.

## State boundaries

**TreasuryLens network state** contains the initializer address, oracle topic `treasury`, authorized committee, default query values, confirmed result map and insertion-order retention list. **Budget Monitor network state** contains its bound source Lyquid ID and a cache of serialized checks indexed by `(query_id, budget)`. A network function cannot fetch HTTP or read a nondeterministic local clock.

**Instance state** contains the latest local observation and a fault-mode value used only when the test-controls feature is compiled. HTTP reads and source-chain observations run as instance functions. Instance state is not inherently consensus state.

The LDK manages oracle source/target configuration, signed-input collection, certificate construction and verification. A normal HTTP client is never allowed to manufacture a list of supporters and directly write a confirmed result.

## Setup and authority

The constructor records its caller as `initializer`. `configure_committee(bytes32[])` requires that initializer inside the function and also uses the creator guard on its generated Ethereum wrapper. It requires exactly four distinct IDs, prevents a second configuration, and stages a threshold of three.

The deployment script obtains node identities from their live local APIs, configures the committee, advances the oracle epoch, waits for every node's target-side epoch, finalizes source synchronization and verifies membership. Initial target activation is a **trusted operational setup step**. The certificate bootstrap cannot by itself infer which first source/committee was intended.

`set_defaults` is initializer-only. Defaults affect the UI's initial inputs, not the committee's fixed threshold. The production build has no exported fault setter.

## Query identity and exact statistic

`QuerySpec` contains:

- `chain_id`: fixed to 31337 in the adapter;
- `token` and `treasury`: normalized 20-byte addresses;
- `from_block`, `to_block`: inclusive u64 interval, at most 10,000 blocks;
- `snapshot_hash`: normalized 32-byte hash of `to_block`;
- `rule_version`: currently 1.

The core computes a Keccak-256 query ID using a domain separator and fixed-width canonical field encoding. A different snapshot or statistics rule produces a different ID. See the [core encoding specification](../crates/treasurylens-core/README.md) for exact bytes.

The statistic is the sum of standard ERC-20 event amounts emitted by the specified contract whose indexed `from` address equals the treasury. Zero-value and treasury-to-itself transfers count. This is not net balance change or economic expenditure. Event-derived amounts use checked uint256 arithmetic and decimal strings at the JSON boundary; JavaScript must not coerce them to floating-point numbers.

## Observation path

For each proposal, the adapter:

1. validates the target sequence backend and target Lyquid;
2. validates query syntax, range and chain ID;
3. checks there is contract code at the token address at the historical endpoint;
4. requires at least two successor blocks after the endpoint;
5. reads and compares the endpoint block hash;
6. issues an `eth_getLogs` request with exact token/topic/from filters;
7. enforces a maximum of 10,000 returned logs and rereads the endpoint hash;
8. invokes the deterministic parser/statistics core.

Requests use `lyquid::well_known::SEQUENCER_RPC_API`, routed to the hosting node's configured sequence backend. HTTP requests have a 3-second host timeout. Two successor blocks are a local depth heuristic, not a finalized-checkpoint protocol. Contract-code existence is not proof of standards compliance.

The parser checks mined metadata, addresses, topics, range, removed status, duplicate consistency and snapshot identities. Exact duplicates count once; conflicting duplicates fail. It sorts validated records into canonical chain order and hashes the full event content, so equal sum/count alone does not make different histories equivalent.

## Signed aggregation and shared commit

`propose_and_certify` drives the LDK two-phase oracle:

1. `propose` returns a `NodeProposal` envelope over the same query: either `summary: Some(Summary)` or `error: Some(...)` for a source-read failure. Both are signed responses; a negative observation is distinct from no response.
2. LDK authenticates and signs node inputs. `aggregate` receives signed inputs and is rerun during validation.
3. `select_quorum` requires three different authenticated node identities agreeing on the **complete summary**: query ID, snapshot hash, gross total, transfer count and canonical logs digest. The application fixes the threshold; the HTTP request cannot lower it.
4. `aggregate` returns `CertifiedCallParams` targeting `commit_result` under `oracle::certified::treasury::two_phase`.
5. `submit_certified_call!` sequences the certified call. The LDK target wrapper verifies the certificate before executing the network function.
6. `commit_result` rechecks identity/supporting membership, records actual certificate signers and stores the result.

If all four signed responses arrive without a matching three summaries, aggregation instead certifies `reject_query`. The HTTP path reports HTTP 422 and deliberately does not submit that rejection as a shared-state update; no confirmed record is created. This closes the selected source-read-failure/no-quorum case, not arbitrary missing-network-response liveness.

`supporters` are identities whose observations supported the chosen summary. `certifiers` are identities that signed authorization of the final call. These are distinct roles and need not be identical lists. Neither is an independent proof of log completeness or source-chain canonicality.

## HTTP lifecycle

- `GET /api/status` returns committee/threshold, oracle activity, chain/head, default inputs, retention size and `test_controls` indicator.
- `POST /api/query` accepts token, treasury, inclusive range and optional decimal-string budget. The adapter resolves the endpoint snapshot itself.
- An already retained matching ID returns HTTP 200 and its confirmed record.
- A successfully submitted certificate returns HTTP 202 with `status: submitted`. The browser must poll `GET /api/results`; no shared result is implied until it appears.
- Bad syntax, oversize input, missing source data and certification/submission failures return failed responses. The request body is capped at 8 KiB. Oracle collection has a 10-second requested timeout, but the initial devnet attempt showed some error/no-quorum calls waiting longer than 35 seconds. Preflight checks and signed negative responses address the selected responding-node failure cases. Actual absent packets or powered-off nodes remain a separate runtime/liveness limitation. The UI waits at most 60 seconds for the query HTTP call and distinguishes a timeout from confirmation.

The input `budget` is validated and used as request-local UI context. It is not part of the historic query identity, and changing it does not create a new source statistic.

## Source replay and drilldown

`POST /api/audit` accepts `{query_id}` for a retained record. It validates the current source chain, checks the endpoint hash, and invokes the same source-log retrieval routine as certification. That routine checks contract code, two successor blocks, bounded logs and snapshot stability across the read. The handler then calls `verify_logs(stored_query, stored_summary, logs)`; no uploaded summary can substitute for this retained anchor.

Only exact equality of all reconstructed `Summary` fields releases `{status: matched, record, audit, logs, checked_at_ms, scope: source-replay-not-new-certificate}`. Optional `token_metadata` contains token, decimals, snapshot block number and `scope: source-rpc-not-certified`. It decodes only a valid 32-byte ABI decimals value fitting u8; it does not round totals or become part of the certificate.

An observed different endpoint produces `mismatch`. Missing source data/RPC failures or evidence too large for export produce `unavailable`. Invalid replay logs or a different reconstructed summary produce `mismatch`. Such responses do not include drilldown/raw evidence and do not mutate or recertify the retained record. Missing retained IDs return HTTP 404.

The core's `AuditReport` derives all views from the same validated, deduplicated, canonically sorted event list:

- `summary`: complete reconstructed summary;
- `transfers`: canonical `TransferRecord` entries with block/hash, transaction hash, log index, token/from/to and decimal amount;
- `recipients`: `{address, total, count, share_bps}`, sorted by total descending and address for ties;
- `blocks`: `{block_number, total, count}`, ascending, containing only blocks with matching events;
- `self_transfer_total`, `self_transfer_count`: included in gross summary, separately explained;
- `top_recipient_share_bps`: highest recipient share.

Amounts remain uint256 base units. Basis points use floor division with U512 intermediate arithmetic, avoiding `uint256_max * 10000` overflow; shares sum to at most 10,000 and can be lower due to flooring. Zero-total histories have zero shares without losing event counts.

This detail is a **current one-instance source replay anchored to a stored summary**. A matched response is not an independently verified receipts-root proof, new committee vote, or proof that a shared provider returned every event.

## Portable evidence and independent content verification

`EvidenceBundle` has exactly `{schema_version: 1, query, summary, logs}`. The Rust core rejects unknown fields and validates rule/query/summary bindings, event parsing and exact reconstruction. `POST /api/verify-evidence` is stateless: it makes no RPC calls, requires no retained result and always sets `certificate_verified: false`, `scope: supplied-contents-only`. Valid content returns its recomputed audit; invalid content returns an error without an audit.

HTTP and the independent `verify-evidence` CLI cap input at 1 MiB and core processing at 10,000 logs. Audit export permits compact raw-log bytes up to 1 MiB minus 8 KiB for bundle overhead; the frontend must still check the exact serialized bundle size and avoid adding provenance fields or excess whitespace. Host-side buffering/resource limits remain distinct from these application caps.

The CLI runs natively from the deterministic core without a Lyquor service. It checks supplied content, not certificate cryptography. A self-consistent rewritten bundle can pass; only comparing with a separately trusted query/summary provides the intended content anchor. Imported analytics must not be merged into the retained certified-record list.

## Current endpoint revalidation

`POST /api/revalidate` accepts `{query_id}` for a retained record, validates source chain ID and fetches the current source hash at `to_block`, plus current head depth. It returns `canonical`, `changed` or `unavailable`, `expected_hash`, optional `current_hash`, `checked_at_ms`, optional `confirmations_now` and a warning. `confirmations_now` counts successor blocks (`head - to_block`), not an irreversible-finality guarantee.

`canonical` means the configured source currently reports the same endpoint identity. It does not verify all intermediate headers or independent providers. The check never deletes, updates or recertifies network records, even after a changed endpoint. The evidence import path cannot claim this online source check.

## Persistence, idempotency and retention

A repeated retained query ID with the same summary returns success without replacing the record. A conflicting summary for an existing ID is rejected. The first inserted record can retain a different valid supporter/certifier subset from a later successful certification of the same statistic.

The network keeps at most **200 records**. A fresh insertion at capacity removes the oldest inserted ID; reads and repeated existing requests do not refresh its age. This is deterministic bounded storage, not an archive. Public queries can evict prior results. A new uncached Budget Monitor check cannot consume a source record after it has been evicted; an already cached decision is a prior observation and can remain present without a fresh source read. There is no cryptographic deletion guarantee, availability SLA or durable external history index.

Development deployment supports an explicit `TL_UPDATE_LYQUID` option mapped to `shaker deploy --update`. It preserves the Lyquid identity used by the serving proxy. An assets-only update preserves application state; the deployment script reuses the matching active committee/epoch instead of initializing or advancing it again. Code updates in the current LDK reset application state: the script repeats committee activation/default setup and confirmed queries must be regenerated. Readiness may be interrupted during update, even when the proxy process stays running. Fresh launcher runs remain separate new-chain sessions. Runtime manifests, seed files and evidence must identify the exact deployment used.

## Budget Monitor transaction and read path

Call `check_budget(query_id, budget)` as a **sequenced network transaction**. The original attempted read-only alias could not perform the atomic inter-Lyquid call in the actual runtime; mutable network execution is required. The method calls the source, compares the certified total with `amount > budget` and persists either `status: confirmed` with its outcome or `status: rejected` when the source has no retained record.

Native `call!` decoding uses the source method's complete `LyquidResult<String>` return type and unwraps that result before parsing JSON; it is not the Ethereum ABI's bare string output. A type mismatch traps the atomic call.

A successful transaction receipt means admission into the sequence. Poll `get_check_json(query_id, budget)` until the serialized decision appears; `"null"` means no cached decision. The latter method is read-only and does not call the source again.

Checks are idempotent by `(query_id, budget)`, including cached rejections. At 200 entries, the next **new** key clears the whole check cache before inserting its decision. This is a simple bounded prototype cache, not source retention or an archive. Repeated cached decisions do not revalidate source retention/canonicality, and a prior rejection does not automatically become confirmed when source data later appears. A cleared decision can be recomputed from a still-retained source record.

## Test separation

`lyquids/treasurylens` has no default features. `lyquids/treasurylens-test` shares the adapter source but explicitly enables `test-controls`. The latter exports `set_fault` for per-instance wrong totals, mismatching snapshots and signed source-read failure responses. These simulate selected application faults on actual nodes; they do not establish that network partitions, physical crashes, Byzantine runtime compromise or independent infrastructure deployment have been tested.

## Official references

- [LDK state and programming model](https://docs.lyquor.dev/docs/ldk/)
- [Certified oracle calls and trusted setup](https://docs.lyquor.dev/docs/ldk/oracle/)
- [HTTP and Ethereum exports](https://docs.lyquor.dev/docs/ldk/external/)
- [Host APIs](https://docs.lyquor.dev/docs/ldk/host-apis/)
- [Local build/deploy/test workflow](https://docs.lyquor.dev/docs/ldk/build-deploy-test/)

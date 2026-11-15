# TreasuryLens deterministic core

This crate is independent of Lyquor, RPC, clocks and node configuration. It can
run in native unit tests and in the WASM Lyquid. The adapter remains responsible
for authenticating committee identities and reading the intended chain.

## Public interface

- `QuerySpec::normalized()` validates rule version 1, an inclusive range of at
  most 10,000 blocks, 20-byte addresses and a 32-byte endpoint block hash, then
  returns lowercase canonical hex strings.
- `QuerySpec::query_id()` hashes all normalized query fields under an explicit
  domain separator. `snapshot_hash` identifies `to_block`.
- `parse_logs(query, logs)` validates a prefiltered `eth_getLogs` response and
  returns deduplicated transfer records in canonical chain order.
- `summarize_logs(query, logs)` returns `Summary`, with decimal uint256 `total`,
  `transfer_count`, a canonical event digest, and query/snapshot identities.
- `Summary::validate_for(query)` checks basic serialized summary consistency.
- `select_quorum(query, observations, threshold)` returns `None` until one
  complete matching summary has enough distinct supporters; contradictory
  observations from one identity or multiple winning groups return an error.

## Exact statistic

The statistic is the **gross sum of standard ERC-20 `Transfer` event amounts
whose indexed sender equals the requested treasury**, within the inclusive block
range and emitted by the requested token contract. Zero-value events count.
Treasury-to-itself events count as sender-matching transfers; this statistic is
not net balance change, economic expenditure, fee attribution or fraud detection.
The implementation does not infer behavior of rebasing or nonstandard tokens.

## Validation boundaries

Every returned log must match the requested token, Transfer topic and sender.
Unexpected logs cause an error rather than silent omission. Mined metadata is
required; removed logs, malformed indexed addresses, out-of-range logs,
conflicting block hashes and conflicting duplicates are rejected. Exact
repetitions are counted once. All sums use checked uint256 arithmetic.

Before calling this crate, the adapter must check chain ID, canonical endpoint
header, sufficient confirmations, and header stability across fetching. The
endpoint comparison inside this crate only checks a log in `to_block` when such
a log exists; an empty response cannot establish chain identity by itself.

A syntactically valid 32-byte zero snapshot is accepted to support query
construction, but it must be replaced or checked against the real block header
before certification. A quorum does not prove a provider returned every log, nor
does it prove independent data sources or prevent an authorized-node collusion.
The Lyquid must fix the quorum threshold and bind IDs to its authorized committee.

## Hash encoding

Query domain bytes: `TreasuryLens/query/v1` followed by a null byte. Fields then
use fixed-width concatenation: chain ID u64 big endian, token 20 bytes, treasury
20 bytes, start/end block u64 big endian, endpoint hash 32 bytes, rule version u16
big endian. Hash function: Keccak-256.

Log domain bytes: `TreasuryLens/logs/v1` followed by a null byte. Deduplicated
records are sorted by block number, log index and transaction hash. Each record
contains block number u64, block hash 32 bytes, transaction hash 32 bytes, log
index u64, token/from/to addresses 20 bytes each, amount uint256, all integers big
endian. Empty logs hash the domain separator alone. A log identity is
(block hash, transaction hash, log index), each block/log-index position must
have one identity, and a transaction hash must belong to exactly one block in
the response (multiple logs within that block remain valid). The digest binds amounts and destinations even when two
histories have the same count and gross sum.

## Test

```sh
cargo test -p treasurylens-core
```

The test suite covers precision, overflow, empty/zero transfer behavior, scope,
metadata, duplicates, conflicting snapshots, canonical identities, invalid
summary representations and unique-node quorum policy.


## Deterministic policy ablation (synthetic fault model)

After fetching an actual persisted record from the deployed Lyquid, pass the
record object (with `query` and `summary`, rather than an enclosing HTTP response)
to the evaluation binary:

```sh
cargo run -p treasurylens-core --bin evaluate < /path/to/certified-record.json > /path/to/ablation.json
```

The source record is real only if the caller obtained it from the real devnet.
This binary validates query/summary consistency, but does not verify a committee
certificate. It derives six explicitly synthetic observation scenarios and
compares a single-node policy, total/count-only majority, and the existing
snapshot-bound quorum policy. Every synthetic observation is included in the
output. No observations are sent to live nodes, and the output is not a network
performance benchmark or measured Byzantine-node failure rate.

A rejection has `accepted=false`, `amount_correct=false` and
`snapshot_safe=false` because it returns no amount or snapshot claim; it is not
an incorrect or unsafe acceptance. The separate `incorrect_accept` and
`unsafe_accept` fields distinguish that. A totals-only majority is classified as
snapshot-safe when at least three of its supporting observations match every
source summary field. Consequently one stale observation with three current
supporters is safe in this particular scenario, while two stale observations
with only two current supporters expose its missing binding check.

## Audit drilldown and replayable evidence

`audit_logs(query, logs)` derives a single `AuditReport` from the same validated,
deduplicated event list as its summary. It includes canonical transfers,
recipient totals/counts, per-block totals/counts, the self-transfer subtotal and
the top-recipient amount share. Recipients sort by numeric total descending and
address ascending for ties; blocks sort by height ascending. Blocks with no
matching events are omitted. Self-transfers remain included in gross transfer
totals and are explained separately rather than silently subtracted.

Shares are integer basis points, `floor(recipient_total * 10000 / gross_total)`.
The intermediate uses uint512, so even uint256 maximum amounts remain safe.
Floored shares need not sum to exactly 10000; an all-zero total produces zero
shares without losing zero-value event counts. Budgets are separate browser or
consumer parameters, and do not change the verified evidence.

`verify_logs(query, expected_summary, logs)` replays raw logs and compares **every
summary field** to a known summary. The caller should obtain that summary through
its trusted persisted-record path. `verify_evidence(bundle)` performs the same
replay for an `EvidenceBundle` containing `schema_version: 1`, `query`, `summary`
and raw JSON-RPC `logs`. These functions reject unknown bundle/query/summary
fields, unsupported schemas and more than 10000 raw logs. They validate content
consistency, not a committee certificate, log completeness or chain membership.
A malicious uploader can rewrite both logs and the supplied summary consistently;
a trusted external anchor is required to detect such a rewrite.

An exported bundle can be checked **offline**, without starting Lyquor:

```sh
cargo run -p treasurylens-core --bin verify-evidence < /path/to/evidence.json
```

The CLI accepts at most 1 MiB of UTF-8 JSON from stdin and prints
`{"status":"valid","certificate_verified":false,"audit":...}` plus a scope
explanation on success. Invalid content prints a structured error on stderr and
exits nonzero. The byte-size cap can reject a large export even when its log
count is within the core limit; split the original query range before exporting
instead of assuming every 10000-log query fits in a 1 MiB artifact.

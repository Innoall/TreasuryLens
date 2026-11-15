# Evaluation evidence

`observed/` contains actual run records, not predicted or fabricated measurements.
The initial fault run exposed requests that exceeded the test client's 35-second deadline.
Later records distinguish the corrected signed-negative-source-response path from genuine
packet loss or powered-off nodes. The latter has no established server-side liveness bound.

Regenerate live checks with `python3 tests/integration.py` and, only on the separate test
manifest, `python3 tests/integration.py --faults`. Output is written under `output/results/`.
Each test records one elapsed time; these are checks, not statistically meaningful latency
benchmarks. Do not report them as p95 performance measurements or as a comparison against
commercial systems. No outside-team user study is included.

## Deterministic policy ablation

The following command seeds six **synthetic fault observations** from an actual retained
certified source record, then compares single-node acceptance, a totals-only 3-node
majority and the snapshot-bound 3-node rule:

```sh
source scripts/env.sh
cargo run --locked -p treasurylens-core --bin evaluate \
  < evaluation/observed/certified-record.json \
  > output/results/ablation.json
```

The committed `observed/ablation.json` is this evaluator's actual output. Every scenario
includes all observations. This is an ablation of application policy, not an evaluation
of Lyquor certificates, independent data sources, real network failures, or deployed
commercial indexers. `false` safety/correctness flags on a rejected output mean no claim
was accepted; inspect `unsafe_accept` / `incorrect_accept` to count accepted bad claims.

## Latest checks (2026-11-11)

- Core library: 28 passing tests; ablation evaluator: 5 passing tests.
- Normal production manifest, fresh four-node network: 9/9 passing integration checks.
- Explicit test manifest with application-level fault injection: 14/14 passing checks.
- Independent Budget Monitor, real sequenced cross-Lyquid reads: 4/4 passing checks.
- Hosted HTML, CSS and JavaScript assets returned HTTP 200.
- Browser visual/interactivity review and real outside-team users remain pending.

The initial failure run is retained to show the discovered no-quorum wait issue. The
fault run was measured before the later clean production restart; the different token
addresses, hashes and deployment IDs refer to those recorded local sessions. They are
not simultaneous claims about a single live chain. No passing result is manufactured
by editing the application data or replacing these logs with predicted outcomes.

## V2 upgrade (2026-11-12)

New v2 records cover anchored source replay, recipient/block drilldowns, current endpoint checks, malformed/oversized evidence and supplied-content tampering. The final normal build passed9 integration+14API+4consumer cases; the separate test build passed14fault/integration cases. Rust tests now number48 and reusable browser-logic tests19. `v2-production-evidence.json` is a compact real exported bundle and `v2-certified-record.json` anchors the local replay. These session records are not a cryptographic certificate export or a public-chain proof. Source/backend/UI-helper checks do not substitute for the blocked browser inspection or real outside-team users.

For public distribution, the original local absolute `deployment.manifest` path in `observed/button-check-2026-11-13-integration.json` was replaced with its project-relative path. No measured outcome, transaction or query data was changed.

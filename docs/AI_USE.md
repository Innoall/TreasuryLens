# AI use statement

## Tools used

**OpenAI Codex**, including delegated coding/research agents within the same project session, was used to assist with planning, implementation, testing and documentation. This statement describes the actual assisted work so far; it does not assert that students have already reviewed or independently validated every output.

## Tasks supported

- Reading the user-provided course specification and translating it into deliverables, implementation scope, a project recommendation and a three-person execution plan.
- Researching official Lyquor documentation and release source for state categories, oracle setup, signed-input aggregation, certified calls, HTTP exports, local networks and tooling.
- Implementing the deterministic Rust query/log/statistics/quorum core and its unit tests; subsequently adding exact-summary source replay, transaction/recipient/block analytics, U512 share calculations and strict portable-evidence reconstruction.
- Implementing the TreasuryLens Lyquid, independent Budget Monitor, hosted HTML/CSS/JavaScript interface and explicit test-only fault controls. Actual runtime testing exposed that the consumer's read-only alias could not perform its atomic inter-Lyquid call; it was changed to a network transaction plus a persisted read-only decision view.
- Creating local setup, network, seed, deploy, serving and test scripts, plus a local Solidity test token.
- Running available build, syntax, unit, real-chain seed and selected live network checks; investigating and correcting failures such as incorrect Anvil donor/submitter account mappings.
- Adding source audit/revalidation/content-verification HTTP endpoints, an independent offline content-verifier CLI, a multi-workspace console, release-check commands and a source-only handoff archive tool.
- Writing implementation, architecture, security, source-attribution and reproducibility documentation.

The project code, reports of tool outcomes and document prose are therefore AI-assisted. Reused protocol patterns and dependencies are identified in [ATTRIBUTION.md](ATTRIBUTION.md).

## Validation actually performed so far

Observed during implementation on 2026-11-11:

- The Rust suite passed 33 tests: 28 deterministic-core cases and 5 synthetic-ablation utility cases. The ablation's generated node observations are synthetic and are not network-performance evidence.
- Project-owned shell syntax and Python compilation checks passed; the frontend JavaScript syntax was checked where recorded by the project test command.
- The local test token was compiled/deployed, actual transfers were generated, and a separate raw-log check found three outgoing transfers totaling 7,500,000 base units.
- A production-manifest deployment activated the four-node oracle and produced a real certificate-backed shared result with that same total/count and signer information.
- Four independent consumer cases passed using actual sequenced cross-Lyquid calls: above/equal/below threshold and unknown-query rejection. The consumer uses a full native `LyquidResult<String>` return when decoding `call!`, and a separate read-only method exposes the persisted outcome.
- Bootstrap repeatability, seed reuse and refusal to restart over occupied ports were checked.
- The first broad devnet attempt passed 11 of 14 cases and exposed requests hanging past 35 seconds despite the requested oracle timeout. The implementation was changed to preflight invalid source queries and return signed negative source-read responses, enabling an explicit all-replies/no-quorum rejection. The original failure record is preserved. After that fix, the revised test deployment passed 14/14 selected integration/fault checks; a clean startup and production deployment then passed 9/9 normal checks. These are saved under `evaluation/observed/`.

Additional observed work on **2026-11-12**:

- The expanded Rust suite passed **48 tests**: 43 deterministic library tests and 5 synthetic policy-ablation tests. The new source-replay/evidence adapter built successfully.
- The explicit v2 test deployment passed **14/14 live fault/integration cases and 14/14 v2 API cases**. These cover the stated replay/content/error cases, not physical node shutdown, real source-chain reorgs or independent data providers.
- Actual replay reconstructed the seeded 7,500,000 total and recipient amounts 3,750,000 / 2,500,000 / 1,250,000, with floor shares 5,000 / 3,333 / 1,666 basis points.
- A compact exported evidence bundle passed the independent native CLI reconstruction. This validates content consistency and does not mean committee signatures were independently verified.
- The updated production/frontend deployment, release-check command and source package have separate execution status; their success is not inferred from the test deployment.

These are specific observed checks, not a statement that unlisted consumer cases, later revisions, physical node failures or browser checks passed. Current-version evidence must come from current command outputs and saved integration result files. Test failures and incomplete checks must remain visible.

AI agents performed implementation and automated checks. **No completed independent student code review, native-browser visual/interactivity review, or outside-team user testing is claimed by this statement.** No fake user comments, measurements or experimental results should be added to the report.

## Student review and responsibility still required

Before course submission, the team should:

1. Read the exact statistic, state model, oracle authority path, snapshot assumptions and known limitations, then explain the full workflow without relying on generated prose.
2. Inspect and understand the core algorithms, setup gates, certified commit and downstream call; check actual dependency versions and source attribution.
3. Reproduce the normal and fault workflows using the submitted revision, compare raw outputs with expected values and retain any failing or unsupported cases honestly.
4. Operate the UI in a real browser and arrange outside-team users to run the README, record actual feedback and document changes or limitations.
5. Verify every result, figure and novelty claim used in the final report, and add the students' real contributions and review actions.

The course team remains responsible for the correctness of code, claims and submitted materials. All members must be able to explain the core workflow and justify design choices. This repository documentation is a project-development record, not the final course report, presentation or demo recording.

## Keeping the statement accurate

When a student or reviewer completes one of the pending actions, record the actual date, revision, scope and outcome. Update this statement to reflect real activity rather than turning suggestions into assertions. Do not mark tests, human review or user feedback complete merely because a script or checklist exists.

## 2026-11-12 implemented follow-up

Codex added exact recipient/block/self-transfer analysis, anchored source replay, endpoint revalidation, strict evidence bundles and an offline verifier. The frontend was rewritten as a functional dashboard with ledger interaction, comparison, budget trials, browser-local inputs and an explicitly synthetic fault lab. Source review identified and corrected raw JSON integer rounding in imports, stale evidence display after failed replay, block-bin positioning and synthetic-policy labeling. These checks were AI-assisted and are not represented as independent student review.

Observed final release checks: 48 Rust tests, 19 client-helper tests, 9 production integration checks, 14 new API checks, 4 consumer checks, plus 14 checks on the separate fault build. Actual results are preserved under evaluation/observed/v2-*. The served assets match the local source. Browser control remains disabled in Work settings, so no visual/browser validation or fabricated user feedback is claimed.

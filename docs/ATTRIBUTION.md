# Attribution and contribution boundaries

This file distinguishes project-specific work from reused infrastructure and examples. It is not a claim of first-ever invention. Academic novelty must be assessed against relevant existing systems and literature before the final course report is written.

## Course specification

The project direction and deliverable requirements came from the user-provided **CS5298 Final Project: Building Web3 Applications on Lyquor Network**. TreasuryLens narrows its suggested Verifiable Indexer direction to a local DAO treasury outgoing-transfer scenario. The course's proposal of that broad direction is not a contribution of this repository.

## Lyquor LDK and examples

The project uses **Lyquor LDK v0.8.3** from [lyquor-labs/ldk](https://github.com/lyquor-labs/ldk/releases/tag/v0.8.3), distributed under the **Apache License 2.0**. The complete unmodified source release is redistributed in `vendor/ldk-v0.8.3.tar.gz`, with LICENSE / NOTICE and a SHA-256 file. Bootstrap verifies and extracts it into the ignored `.tools/ldk/` directory (or downloads the release if no bundled source is present); it downloads platform tools separately. The repository links the `lyquid` crate from that release and pins its local tooling versions.

The following upstream material informed or was adapted for the project:

- [Price-feed Lyquid](https://github.com/lyquor-labs/ldk/blob/v0.8.3/lyquid-examples/price-feed/src/lib.rs): oracle topic declaration, initializer-controlled configuration, two-phase propose/aggregate hooks, certified network result callback, signer lookup and hosted HTTP JSON patterns.
- [Example deployment helpers](https://github.com/lyquor-labs/ldk/blob/v0.8.3/lyquid-examples/scripts/lib/demo.sh): live Node ID discovery/encoding, JSON deployment parsing and the advance-epoch / target-wait / finalize-epoch activation shape. `scripts/deploy.sh` is a project-specific reimplementation of that workflow using Python's standard library and structured subprocess arguments.
- [BasicSwap native inter-Lyquid calls](https://github.com/lyquor-labs/ldk/blob/v0.8.3/lyquid-examples/basicswap/src/lib.rs): the `call!` return declaration includes the callee's `LyquidResult<T>` wrapper, unlike its Ethereum view ABI. The consumer follows this decoding convention and unwraps it before parsing JSON.
- [Native network lifecycle helper](https://github.com/lyquor-labs/ldk/blob/v0.8.3/scripts/lib/localnet.sh): used directly from the fetched release. `scripts/localnet.sh` adds occupied-port refusal, old-session archiving and an override for the incorrect local submitter account mapping; it does not edit the upstream source.
- [LDK documentation](https://docs.lyquor.dev/docs/ldk/), [certified-call documentation](https://docs.lyquor.dev/docs/ldk/oracle/), [HTTP/ETH export documentation](https://docs.lyquor.dev/docs/ldk/external/) and [build/deploy/test documentation](https://docs.lyquor.dev/docs/ldk/build-deploy-test/): programming model and operational API references.

LDK supplies the signature collection, certificate encoding/verification, runtime, native network and protocol mechanics. These must not be described as cryptographic primitives invented by the project team. Preserve applicable upstream license/copyright notices when redistributing upstream source or derived material; the release license is available [here](https://github.com/lyquor-labs/ldk/blob/v0.8.3/LICENSE).

## Project-specific implementation

The current implementation adds the narrow treasury query and exact statistic; canonical snapshot-bound query identity; strict mined-event parsing and checked uint256 accumulation; full-event digest; matching-summary policy; idempotent, bounded shared result storage; independent budget consumer; user interface; real token/transaction seed; controlled fault scenarios; exact-summary-anchored source replay; canonical transaction, recipient and block analytics; current endpoint revalidation; strict-schema portable evidence with an independent content-verifier CLI; browser-side comparison/saved queries/fault models; release/source-packaging tools; and reproducibility documentation.

The replay/evidence code uses the project's deterministic parser and canonical event digest. Recipient shares use Alloy U512 intermediates for exact floor basis-point arithmetic; this is integer arithmetic built from existing primitives, not a new cryptographic method. Source replay, content reconstruction and current-hash checks are explicitly distinguished from a new committee certificate or independent signature verification.

These are the project's implementation contributions relative to the reused example skeleton. They are not assertions that snapshot binding, canonical hashing, threshold voting, ERC-20 indexing or budget checks are novel in the wider literature. The final report should compare relevant approaches and state any actual technical distinction with evidence.

## Other dependencies and tools

- **Rust / Cargo / rustup:** language, package/build management and wasm target installation. [Rust project](https://www.rust-lang.org/), [rustup](https://rust-lang.github.io/rustup/). Rust 1.99.0 is selected in `rust-toolchain.toml`.
- **Foundry v1.7.1:** `anvil` local EVM chain; `forge` Solidity compilation; `cast` encoding, account derivation, signing and RPC operations. [Foundry release](https://github.com/foundry-rs/foundry/releases/tag/v1.7.1). Bootstrap verifies the release archive against its published SHA-256 file.
- **Solidity 0.8.30:** compiles the local `TestToken.sol`; EVM target is `paris`. [Solidity documentation](https://docs.soliditylang.org/). The token implementation is intentionally minimal, has no external Solidity libraries and is not OpenZeppelin code or an audited ERC-20 product.
- **serde / serde_json:** Rust serialization and JSON parsing. [serde](https://serde.rs/), [serde_json](https://github.com/serde-rs/json).
- **alloy-primitives:** checked uint256, widened U512 arithmetic and Keccak-256 primitives. [Alloy repository](https://github.com/alloy-rs/core).
- **Python standard library:** deployment/seed orchestration and integration checks. No external Python packages are required by the project runtime scripts.
- **Browser JavaScript / HTML / CSS:** the interface has no frontend package dependencies, icon library, image service, font download or charting framework. Node.js is used for syntax validation only.

`Cargo.lock` records the resolved Rust dependency graph, including transitive packages. Each dependency retains its own license and attribution requirements. This file is a guide to sources and contribution boundaries, not a substitute for dependency license metadata.

## Data and identities

All demonstrated source data is generated by deploying a local test token and sending actual transactions on the local Anvil chain. Expected values are recorded separately from the production parser and checked using transaction/log evidence. No real DAO financial data, public-chain assets or real customer identities are represented.

The account keys and mnemonic are public Anvil defaults for reproducible local tests. Four node identities come from the local devnet topology. The prototype does not establish independent operators, real distributed machine deployment or independent data sources. Portable evidence copies those locally generated logs and claims; verification of that file alone is not proof of source authenticity. Browser Lab inputs are synthetic model states and do not represent new measured attacks or user data.

## Human and AI attribution

AI-assisted implementation and checks are recorded in [AI_USE.md](AI_USE.md). The repository does not invent personal names, individual student contribution claims, completed human reviews or outside-user feedback. The team must fill actual student contribution information from its real review, development and testing work.

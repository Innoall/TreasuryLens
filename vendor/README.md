# Lyquor LDK v0.8.3 source

`ldk-v0.8.3.tar.gz` contains the complete unmodified LDK v0.8.3 source release used by TreasuryLens, with its upstream LICENSE and NOTICE. This is source, not a platform executable.

Upstream: https://github.com/lyquor-labs/ldk/releases/tag/v0.8.3

`bash scripts/bootstrap.sh` verifies the adjacent SHA-256 file and extracts this archive to `.tools/ldk/` when needed. Its Rust path dependencies and workspace files are preserved. Platform tools and the remaining build/runtime dependencies are downloaded separately.

License: Apache-2.0; see `LDK-LICENSE` and `LDK-NOTICE` and the notices within the archive. No third-party source changes were made.

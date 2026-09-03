# Release record

Release status: implementation in progress until the evidence below is
recorded for the exact commit and tag.

Before publishing a release, run from the repository root:

```bash
cargo +stable fmt --all -- --check
cargo +stable check --all-targets --locked
cargo +stable clippy --all-targets --all-features --locked -- -D warnings
cargo +stable test --all-targets --locked
RUSTDOCFLAGS=-Dwarnings cargo +stable doc --no-deps --locked
cargo +stable package --locked
cargo +stable publish --dry-run --locked
cargo +stable install cargo-audit --version 0.22.2 --locked
cargo audit
cargo audit --file fuzz/Cargo.lock
```

The bounded fuzz gate uses the nightly toolchain and a finite run:

```bash
timeout --foreground 300s env RUSTUP_TOOLCHAIN=nightly cargo fuzz build event-json
timeout --foreground 60s env RUSTUP_TOOLCHAIN=nightly cargo fuzz run event-json -- -max_total_time=10 -verbosity=0 -print_final_stats=1
```

Publish only after CI, Security, CodeQL, and the tag package workflow pass on
the same commit. Verify the crates.io checksum, docs.rs page, GitHub release,
and a fresh install in an isolated Cargo home. Record exact output in
`qa/evidence/` and update the portfolio index after reading the files back.

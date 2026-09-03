# Local operating contract

This repository is a focused Rust project. The product brief and CI workflow
are authoritative.

Read `docs/design.md` for the data model, `docs/limits.md` for supported
boundaries, and `docs/release.md` for release evidence.

## Commands

- Build: `cargo build --locked`
- Test: `cargo test --all-targets --locked`
- Format check: `cargo fmt --all -- --check`
- Lint: `cargo clippy --all-targets --all-features --locked -- -D warnings`
- Documentation: `RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked`
- Package check: `cargo package --locked`
- CLI smoke test: use the commands in `docs/release.md`

## Scope

Keep changes limited to the versioned CLI event protocol and small reference
runner. Do not add frontend code, hosted services, cloud backends, telemetry,
or unrelated compatibility promises.

## Operating loop

1. Plan the change and define a measurable success condition.
2. Make only scoped edits.
3. Read back every changed file.
4. Run the relevant validation commands and record exact results.
5. Review the diff before committing or pushing.

## Safety

The runner does not capture environment variables, but it records argv as
provided. Never put credentials or secret values in command arguments,
fixtures, logs, or tracked files.

## Release

Use `docs/release.md`. A release requires a clean working tree, passing local
and hosted gates, independently verified artifacts, and a truthful record of
the Linux-first and output-capture limits.

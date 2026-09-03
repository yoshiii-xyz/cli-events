# cli-events

cli-events defines a bounded, versioned JSON Lines protocol for one local CLI
execution and provides a small reference runner, validator, and deterministic
summarizer.

Status: 0.1.0 implementation pending release evidence.

CI: https://github.com/joshiii-xyz/cli-events/actions

## Install

```text
cargo install cli-events
```

## Quick start

```text
cli-events run -- cargo test --format jsonl > event-stream.jsonl
cli-events validate event-stream.jsonl
cli-events summarize event-stream.jsonl
```

`run` emits `command.started`, interleaved `command.output` records with
separate stdout and stderr labels, and a final `command.finished` record.
`sequence` is the stable ordering field. Timestamps are wall-clock hints and
can jump when the system clock changes.

## What it solves

Task runners, build tools, test tools, and coding agents can consume one
small, documented event shape without parsing human-oriented terminal output.
The MVP only covers one local command and a JSON Lines file or stdout stream.

## How it works

The runner starts one child process with stdout and stderr pipes. Reader
threads send bounded chunks to one queue, and the main process assigns
sequence numbers in receive order. Validation checks the schema, event order,
required fields, and resource limits. Summary output excludes volatile process
fields so the same stream produces the same result.

## Commands and library API

The CLI provides `run`, `validate`, and `summarize`. The library exposes
`run_command`, `parse_event_line`, `validate_stream`, and `summarize_stream`.
Use `cli-events --help` for the complete option list.

## Output and exit codes

- Exit code 0 means `validate` or `summarize` accepted the stream.
- Exit code 1 means a stream was read but failed protocol validation.
- Exit code 2 means the CLI could not run the requested operation.

Each event is limited to 64 KiB, each captured output stream to 8 MiB, and an
execution to 4096 events. The runner applies a 60 second default timeout and
supports cancellation with `--cancel-after-ms`.

## Safety and data handling

The runner reads only the requested child process output and the current
working directory. It does not capture environment variables, upload data, or
redact argv. Do not put credentials or other secrets in command arguments.
Invalid UTF-8 output is represented with the replacement character.

## Limits and non-goals

See [`docs/limits.md`](docs/limits.md). This is not a general task runner,
scheduler, shell, hosted service, or binary-output recorder. Linux is the
primary tested platform.

## Testing and development

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`docs/release.md`](docs/release.md)
for the verified command set.

## Research

The design references Rust process APIs and [RFC
8259](https://www.rfc-editor.org/rfc/rfc8259). See [`docs/research.md`](docs/research.md)
for the source trail.

## Release and support status

The 0.1.0 release is pending local and hosted evidence. The release record
will be updated only after the exact package, checksum, docs.rs, CI, security,
CodeQL, tag package, and fresh-install checks pass.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT. See [`LICENSE`](LICENSE).

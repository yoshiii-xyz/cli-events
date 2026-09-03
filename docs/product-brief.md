# Product brief

cli-events defines a small, versioned JSON Lines protocol for one local CLI
execution. It includes a reference runner and tools for validating and
summarizing the resulting stream.

The Linux-first MVP supports:

```text
cli-events run -- cargo test --format jsonl
cli-events validate event-stream.jsonl
cli-events summarize event-stream.jsonl
```

Target users are maintainers of local build tools, test tools, task runners,
and coding-agent integrations that need machine-readable process evidence.

Current alternatives include parsing terminal text, consuming tool-specific
JSON formats, or embedding a custom process wrapper in every integration.
Those alternatives vary in field names, often mix stdout and stderr, and do
not consistently state cancellation or size behavior.

The switching wedge is a small stable event shape plus a reference runner that
can be adopted without adopting a task scheduler, hosted service, or shell
implementation.

The protocol has three event types:

- `command.started` records argv, the working directory, and process ID.
- `command.output` records bounded text from stdout or stderr separately.
- `command.finished` records success, failure, signal, or cancellation state.

`schema_version` identifies the wire format. `sequence` is a monotonically
increasing per-stream ordering field. `timestamp_unix_ms` is wall-clock data
for correlation only. It can jump or move backwards when the system clock is
adjusted, so consumers must use `sequence` for ordering.

The reference runner captures no environment variables and has no secret
capture mode. Command arguments are emitted as supplied, so callers must not
place secrets in argv. Each event is limited to 64 KiB, each captured stream
to 8 MiB, and each execution to 4096 events. The runner applies a 60 second
default timeout and accepts a bounded timeout override.

This is a protocol and reference runner, not a general task runner. It does
not schedule multiple commands, provide a daemon, upload data, or define a
hosted event service.

Evidence and inference are separate. The Rust process and JSON documentation
linked in [`docs/research.md`](research.md) support the implementation
constraints. The statements about integration weaknesses and the switching
wedge are product inferences from the narrow protocol brief, not measured
market claims.

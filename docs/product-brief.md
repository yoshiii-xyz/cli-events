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

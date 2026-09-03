# Design

## Data model

Every event has `schema_version`, `sequence`, `timestamp_unix_ms`, and
`event`. `command.started` adds argv, cwd, and pid. `command.output` adds a
stdout or stderr label and text. `command.finished` adds status, exit code or
signal, and cancellation state.

## Interfaces and boundaries

The library exposes `run_command`, `parse_event_line`, `validate_stream`, and
`summarize_stream`. The CLI is a thin adapter around those interfaces. A run
starts one local child process with stdout and stderr pipes. Two bounded reader
threads feed one receive queue so the output event sequence preserves the
observed interleaving while retaining stream identity.

## Failure behavior

Malformed records make validation fail with bounded line-oriented errors. A
timeout, requested cancellation, output limit, or event limit terminates the
child and produces a final `command.finished` event with a cancellation reason.
Signal termination is classified separately when the host exposes a Unix
signal number.

## Resource limits

Events are at most 64 KiB, each captured stream is at most 8 MiB, and a run is
at most 4096 events. Validation rejects streams above the same stream limit.
The runner uses a 60 second default timeout and accepts at most 10 minutes.

## Portability boundary

Portable Rust process APIs are used for spawning, polling, and exit codes.
Unix signal inspection is enabled where available. Linux is the only platform
covered by the release evidence. Windows job objects, PTYs, and process-group
control are not implemented claims.

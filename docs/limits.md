# Limits and non-goals

The 0.1.0 MVP has these explicit limits:

- Linux is the primary tested platform. The library uses portable process APIs
  plus Unix signal inspection when compiled on Unix.
- `run` executes one local process. It is not a task graph, shell, scheduler,
  retry engine, or build system.
- Output is text. Invalid UTF-8 is represented with the Unicode replacement
  character, and raw binary output is not preserved byte-for-byte.
- The default timeout is 60 seconds. A caller can select up to 10 minutes.
- A stream is limited to 8 MiB, an event to 64 KiB, and one execution to 4096
  events.
- The protocol has no network transport, persistence service, authentication,
  environment capture, secret scanner, or redaction policy.
- `timestamp_unix_ms` is not a monotonic clock and must not be used for event
  ordering.

Human usability, physical-device testing, hosted services, and broad
cross-platform compatibility are outside this repository's release gate.

# Operations

## Local installation

Install from crates.io with `cargo install cli-events`, or build from a clean
checkout with `cargo build --locked`. The executable reads no configuration
file and makes no network request while running a command.

## Safe execution

Use `--timeout-ms` and `--cancel-after-ms` for bounded runs. The runner does
not capture environment variables, but it emits argv because argv is part of
the start event. Keep credentials out of command arguments and output.

## Logging and retention

`run` writes JSON Lines to stdout. Redirect it to a location with appropriate
permissions and delete it according to the local retention policy. The tool
does not retain or upload streams after the process exits.

## Troubleshooting

- A nonzero `validate` or `summarize` exit code means the input was read but
  failed protocol validation. Inspect the JSON `errors` array.
- A `cancelled` finish status includes `cancellation_reason` such as
  `requested`, `timeout`, or `output_limit`.
- A `signal` finish status is available only when the host reports a signal.
- Use `command.started.argv` and `command.output.stream` to distinguish the
  child invocation from its two output channels.

## Recovery

The tool does not modify the child working tree. If a child command changes
files, recovery belongs to that command and the operator's normal backups.

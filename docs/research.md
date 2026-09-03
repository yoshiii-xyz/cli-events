# Research notes

Research date: 2026-09-03.

The implementation follows the standard library process model and the JSON
interchange rules rather than inventing a second process abstraction.

- Rust's [`Command`](https://doc.rust-lang.org/std/process/struct.Command.html)
  documents process construction and argument handling.
- Rust's [`Child`](https://doc.rust-lang.org/std/process/struct.Child.html)
  documents piped standard streams, polling, waiting, and termination.
- Rust's [`ExitStatus`](https://doc.rust-lang.org/std/process/struct.ExitStatus.html)
  documents portable success and exit-code inspection.
- Unix's [`ExitStatusExt`](https://doc.rust-lang.org/std/os/unix/process/trait.ExitStatusExt.html)
  exposes signal information for the Linux-first signal event case.
- [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) defines JSON syntax and
  interoperability expectations.

The relevant issue and discussion review was limited to the public Rust
process documentation and its linked issue tracker context:
[Rust process issues](https://github.com/rust-lang/rust/issues?q=is%3Aissue+process%3A+Command)
and [Rust users discussions](https://users.rust-lang.org/search?q=std%3A%3Aprocess%3A%3ACommand).
Neither is treated as a normative source for the wire format.

Distribution signal: the package metadata is prepared for crates.io and the
repository is structured as a standalone Cargo binary. Package availability
or download counts are distribution signals, not evidence of willingness to
pay or broad adoption.

Evidence grade: primary documentation supports the process and JSON behavior.
The ordering, limit, and summary choices below are design inferences for this
focused MVP, not claims that the cited sources prescribe this exact protocol.

Design decisions:

1. `sequence` is the ordering field because wall-clock time is not a reliable
   ordering source.
2. stdout and stderr use separate reader threads and one receive queue, so
   consumers see the observed interleaving without losing stream identity.
3. Size limits are checked before events are emitted and during validation.
   The runner terminates a child when output or event limits are reached.
4. Invalid producers produce bounded validation errors and a nonzero CLI exit
   status. The validator does not attempt to repair or reorder input.
5. Summaries omit timestamps, PIDs, and working directories so identical
   streams produce identical summary JSON.

Rejected alternatives: a general task runner would add scheduling and retry
scope that the brief excludes; a shell parser would add platform-specific
semantics; and a hosted transport would add storage, authentication, and
network failure modes without improving the local protocol boundary.

Decision: keep the first release to one local process, three event types, a
bounded JSON Lines stream, and a small reference runner.

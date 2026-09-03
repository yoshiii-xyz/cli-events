# Research notes

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

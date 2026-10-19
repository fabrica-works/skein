# skein

*One thread, one loop, no async.*

A Rust kit for network services written as step machines over an I/O
loop: counted, bounded, and testable whole in a simulated world.

## The idea

- **Step machines, not async.** A service is plain state machines: a
  domain that decides, a protocol layer that speaks bytes, and a loop of
  about ten lines that feeds them what the kernel completed. One thread
  per process; a service that needs more cores runs more processes.
- **A kit, not a framework.** skein has no service trait, no scheduler
  and no callbacks. A service calls its parts by name, and nothing in
  skein calls into a service.
- **Counted and bounded.** Every part exports its limits and its worst
  case, and a service adds them up: what it can hold is known before it
  runs.
- **Tested whole, and reproducibly.** io speaks to the kernel in owned
  records, over io_uring or over a simulator that plays the kernel and
  its faults. A world runs every process of a system in one seeded loop,
  over either.
- **Built when pulled.** A part is built when its first user needs it,
  and that user is its first test.

## What it holds

- **The base:** bounded containers, cursors and time; io, from sockets
  to processes; the shell that runs the loop; a durable key-value store.
- **Protocols:** HTTP/1.1, JSON, TLS, framed channels, and codecs
  generated from schemas.
- **What services share:** LLM calls and their connections, OAuth, and a
  code forge's calls, client and setup.
- **Interfaces:** an application's state as a semantic tree, shown in a
  terminal or a browser.
- **Testing:** the simulator, the world harness, a counting allocator, a
  browser driver, and fakes of an LLM, an issuer, a forge, a checkout and
  a person.

skein also holds the foundation that every repository of the family
follows, and the tools of its development process.

## Start here

- [`docs/foundation/programming-model.md`](docs/foundation/programming-model.md):
  how code is written, in skein and in everything built on it;
  [`testing-strategy.md`](docs/foundation/testing-strategy.md): how it
  is tested; [`notes.md`](docs/foundation/notes.md): what is still open.
- [`docs/design/README.md`](docs/design/README.md): the parts, each with
  its design, and their crates.
- [`docs/design/examples.md`](docs/design/examples.md): the example
  services, and how a service is built on skein.
- [`docs/design/STATUS.md`](docs/design/STATUS.md): how far the code is
  from the design.
- [`docs/foundation/development.md`](docs/foundation/development.md):
  how work goes from an idea to `main`, here and downstream.

Before changing code, read [`AGENTS.md`](AGENTS.md). `main` moves only
through `land`, which runs the gate, `scripts/check.sh`.

## The family

| Repository | | Builds on |
|---|---|---|
| [skein](https://github.com/forgetic/skein) | One thread, one loop, no async | |
| [smith](https://github.com/forgetic/smith) | Protocol-driven agent loop for AI applications | skein |
| [jig](https://github.com/forgetic/jig) | Engine for agentic applications | skein, smith |
| [temper](https://github.com/forgetic/temper) | Agent-driven software development on a forge | skein, smith, jig |
| [hearth](https://github.com/forgetic/hearth) | Immutable machines, built from a revision | skein |

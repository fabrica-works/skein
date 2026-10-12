# Code organization

The crates and modules this plan leaves, who owns what, the plan's "Don't"
list and its size limits.

## Crates

- **`skein-tls`** (`crates/skein-tls`), a step crate: no `std`, the
  workspace's step lints, `clippy.toml`'s bans (no `include_bytes!`, no
  `copy_from_slice`). Session 01 owns it.
- **`skein-tls-world`** (`tests/tls`), ordinary Rust (programming-model.md,
  10.2) with `tests/clippy.toml`'s bans. Session 02 owns it. Other test
  crates use its `pki`, `server` and `drive::Wire`: those keep their
  paths and names.
- **`scripts/`**: session 03 owns it.

## Modules of `skein-tls` at the end

```
src/lib.rs                  the crate's doc, Config, Name, MaxOut (unchanged but for its doc)
src/config.rs               Config, Name, Refusal, Parsed, ALPN, roots_worst_case; no test code (1.5)
src/held.rs, src/session.rs unchanged
src/client.rs               the client's doc; Request, Event, Agreed, Version, Waiting; Client, up, down;
                            State, Fresh, Line, Open, Handshake, Peer, Writing, Buffers; start, processed, settle;
                            the engine both faces share (1.3)
src/client/limits.rs        tls.md, 3.5: the constants, Limits, room_for, largest_room, worst_case, the MAX_OUTs (1.1)
src/client/error.rs         tls.md, 3.3: Error, Certificate (1.1)
src/client/below.rs         tls.md, 3.2: Below, Demand, Room, Record; open_up, wanted, ended, failed_up, length, fill (1.1)
src/client/above.rs         tls.md, 3.1: Above, Wanted, Reading; stream, sent, answer (1.1)
src/client/close.rs         tls.md, 3.3: Closing; fail, close, close_open, closed, closing_up, closing_settle (1.1)
src/client/native.rs        tls.md, 3.6: the native face (1.2 to 1.4)
src/client/native/output.rs the four output cells' handling, if native.rs is over 800 lines after 1.4
src/client/native/tests.rs  the native face's step tests (1.5)
src/tests.rs                the step tests' fixtures module (1.5)
src/tests/{config,held,machine}.rs
```

Every path that is `pub` today stays reachable at its path: `client.rs`
re-exports what moves (`pub use limits::{…}`, `pub use error::{…}`).
Items that were private to `client.rs` become `pub(super)` in their new
module, or `pub(in crate::client)` where the native face uses them.

## Modules of `skein-tls-world` at the end

```
src/lib.rs          the crate's doc and modules
src/pki.rs          fixtures: certificates, keys, wall times, configurations (adds the named record sizes, 2.4)
src/server.rs       unchanged
src/drive.rs        unchanged but for its users
src/records.rs      a record's length from its header, splitting ciphertext into records, corrupting the first (2.1)
src/meter.rs        SMALL, LARGE, and the metered steps both memory tests share (2.1)
src/native_pair.rs  Pair, its helpers, LIMITS, the lower side's output proxy, the two peers as one enum (2.1)
src/native_peer.rs  the TLS 1.2 peer whose keys are taken out
src/native.rs       the native face over io's sockets on the simulator
src/world.rs        the world's loop (2.3)
src/world/scenario.rs, src/world/check.rs   (2.3)
tests/*.rs          as today; handshake.rs loses two tests (2.4)
```

## Owners of shared things

- **The crate's step-test fixtures** (`crates/skein-tls/src/tests.rs`):
  1.5 makes them; nothing else in the plan adds to them.
- **The world crate's shared harness** (`records.rs`, `meter.rs`,
  `native_pair.rs`): 2.1 makes them; 2.2 adds `Pair`'s helpers and the
  split tests' builders to `native_pair.rs`; 2.3 and 2.4 use them by
  name and add nothing beside them.
- **The engine both faces share** (in `client.rs`): 1.3 makes it; 1.4
  uses it.
- **The words** (README, "Rules"): every increment.

## The plan's "Don't" list

No increment:

- adds a type beside an existing one for the same thing: it changes the
  type in place, and its users in the same increment;
- adds a field or predicate that its counterpart across a boundary lacks;
- adds a test that checks several behaviours under one name;
- adds a lint exception in tests;
- computes an expected value with the code under test;
- writes a word of the README's Pulse `words` list, in code, comments,
  test names or assertion messages;
- changes what a test checks while it changes the production code that
  test covers (README, "Rules");
- edits a file another session owns (README, "Rules"), except 2.2's
  mutant, a temporary edit never committed;
- edits `docs/design/`, this plan or the run's files other than its own
  checkpoint and logs.

## Size limits

- A file of at most 800 lines.
- A test function of at most 40 lines.
- A production function of at most 60 lines, as a guide: aim for it in
  every function an increment writes or moves, and name in the commit
  body each one left over it, with its length.

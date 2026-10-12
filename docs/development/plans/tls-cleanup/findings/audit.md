# Audit

2026-10-12, skein at `73c900b`. Three read-only sub-agents read
`crates/skein-tls` and `tests/tls` in full, one theme each: names and
taste (`N-`), bloat and debt (`B-`), tests (`T-`). Each entry: where,
what, the fix the plan takes, and the increment that takes it. Line
numbers are at `73c900b`; check them with `rg` before relying on them.

The native face, `client::native`, came in one commit (`8c5455f`,
2026-10-06), whose body says "TEMPORARY DRAFT ONLY … not accepted into
any original main". It is on main, with no user yet: its design is
tls.md, 3.6, for a framed connection over TLS (io.md, 3.3). Most of
what follows is in it.

## Production code

- **B-1 / N-3. The native face copies the classic client's engine.**
  `native.rs` imports 11 private items of `client` (`Buffers`, `Fresh`,
  `Line`, `Peer`, `Reading`, `Record`, `Writing`, `Handshake`,
  `intake_cap`, `fill`, `length`) and restates the functions that use
  them, about 85 lines: connecting from `Line` (client.rs:619-623,
  native.rs:361-365); processing then `Ready` (client.rs:829-840,
  native.rs:376-403); reading a header or a body (client.rs:664-686,
  native.rs:440-473); answering from the intake (client.rs:886-915,
  native.rs:784-816); the next piece and `TooLong` (client.rs:922-947,
  native.rs:932-966); a read's largest fill (client.rs:750-754,
  native.rs:996-1001); making `Buffers` (client.rs:375-384,
  native.rs:261-283). Classic `fresh_up` keeps `Line::Failed` when an
  `End` follows it (client.rs:650); native's `stream_up` overwrites it
  with `Line::Ended` (native.rs:413). Keep that difference. Fix: 1.3.
- **B-2. `client.rs` is 1,114 lines:** limits, errors, both sides and
  the close in one file; `stream` is 74 lines, `open_up` 62. Fix: split
  along tls.md, 3, 1.1.
- **B-3. `native.rs` is 1,004 lines,** and about 11 matches list every
  `State` only to reach `State::Open` (native.rs:352-360, 377-385,
  486-494, 500-509, 539-547, 590-598, 759-767, 785-794, 859-875,
  880-888, 936-951). Fix: 1.4.
- **N-4 / B-4. The native `Client` keeps state-specific data in flat
  fields** (native.rs:239-252): `record`, `reading`, `upper_read`,
  `lower_read`, `withdrawn_read` and `lower_ended` mean something only
  while `Open`, against programming-model.md, 5.4's "no field is
  meaningful in some states only". `Writing::Finishing` is tested at
  native.rs:740 but never set on this face; the `State::Fresh(_)` arms of
  `output_up` (native.rs:669, 680) cannot run. Fix: 1.4.
- **N-1 / B-12. The native face's docs.** 47 filler words in production
  code (native.rs 42, lib.rs 3, client.rs 1, config.rs 1); each of the
  8 tuple variants in native.rs:44-136 is documented twice, on the
  variant and on its field; 38 citations of "tls.md, 3.6" in native.rs.
  Fix: 1.2.
- **N-5. Words for the sides.** native.rs says upper and lower, transport,
  physical, resource, owner and bridge; client.rs and tls.md say the side
  above and the side below. lib.rs:8-9 uses "face" for a side. Fix: 1.2.
- **N-6. Comments that tell history or are wrong.** native.rs:147 "the
  original … allowance", :154 "Classic `largest_room` is unchanged",
  :331-333 "inert before classic dispatch" (native never dispatches to
  `client::down`); tests.rs:24, :39 "Original …". Fix: 1.2 and 1.5.
- **N-7. `Waiting::Close` and `Error::Other` mean more on the native
  face** than their docs say: `Close` also while a close waits for the
  owner's `Closed`, and after `TransportClosing`; `Other` also when the
  lower tokens run out. tls.md, 3.6 now says both. Fix: their docs, 1.2.
- **B-7 / N-8. Test code in a production file.** config.rs:203-234,
  `native_test_peer`, `#[cfg(test)]`, with its one caller in
  client/native/tests.rs:506; it reads fixtures through `std::fs` and
  rebuilds what `pki::Server::config` builds. Fix: 1.5.
- **B-13. The public API.** Used outside the crate: `Config`, `Name`,
  `RootCertStore`, `CertificateDer`, `client::{Client, up, down, Request,
  Event, Limits, LARGEST_READ, largest_room, worst_case, MAX_RECORD,
  MAX_PLAINTEXT}`, by skein-llm-connection, skein-oauth-accounts and
  smith. `client::MAX_HANDSHAKE` is used only inside the crate. Nothing
  outside the crate and its world crate uses `client::native`. Fix:
  `MAX_HANDSHAKE` becomes `pub(crate)`, 1.1; the rest stays.

## The crate's step tests

- **T-5. No fixtures module:** the fixture paths are spelled 6 times;
  `failed()` is defined in machine.rs and handshake.rs; `above.is_empty()
  && below.is_empty()` is written 20 times, the withdrawal
  `Demand { read: Read::Nothing, room: 0 }` 7 times; native/tests.rs:508
  writes out the 2030 wall time. Fix: 1.5.
- **T-6. Expected values computed by the code under test:**
  native/tests.rs:94-119 computes native `room_for` as `client::room_for(p)
  + FLIGHT`; tests/config.rs:71-81 rebuilds the worst case from
  `FLIGHT` and `MAX_HANDSHAKE`. Fix: literals, 1.5 (crate) and 2.4
  (world crate).
- **T-11. A real handshake inside the native step tests:** 180 lines of
  support (native/tests.rs:398-577) for two tests; `actual_ready(exhaustion:
  bool)` is 73 lines and holds one test's assertions behind its flag.
  Fix: 1.5.
- **T-3 (crate). Tests that branch on a flag:** native/tests.rs:93 (six
  checks), :180 (`close_first`); machine.rs:145 (`cookie`). Fix: 1.5.

## The world crate

- **B-8 / B-9 / B-10 / T-9. Harness pieces defined more than once:**
  `SMALL`, `LARGE`, the meter's `check` and `finish` (memory.rs:246-252,
  396-411; native_memory.rs:21-22, 119-124, 339-347); the first record's
  corruption (memory.rs:316-319, native_memory.rs:253-255); the lower
  `Output` proxy (native_pair.rs:13-18, native_memory.rs:26-32); the
  peer as two `Option`s (native_pair.rs:29-33, native_memory.rs:76-77);
  the ready native pair (native_output.rs:81-96, 259-276, 362-377,
  native_memory.rs:457-463); a header's record length (9 sites). Fix:
  2.1.
- **T-2. `native_output.rs` writes each request out whole:**
  `pair.down(native::Request::Client(client::Request::Stream(Down::Demand
  { .. })))`, 5 to 7 lines, about 30 times; 17 `use` blocks inside
  functions. Fix: `Pair`'s helpers, 2.1; used in 2.2.
- **T-1 / N-2. The native tests' words and names:** 24 of the 39 native
  test names carry actual, real, genuine or authentic; about 369 such
  words in the native test files; a local variable named `actual`, 57
  times. Fix: 2.2 (world crate), 1.5 (crate).
- **T-3 (world). Tests that branch on loop variables:**
  native_memory.rs:452 (63 lines, a 2×2×2 grid); native_output.rs:409
  (53 lines), :472 (47), :555 (52). Fix: 2.2.
- **B-11 / T-10. `world.rs` is 1,043 lines:** `check` 92 lines, `run` 86,
  `above_acts` 81. Fix: 2.3.
- **T-4. The same behaviour checked twice:** handshake.rs:175 and :188
  have the names of machine.rs:55 and :166, and check the same; their
  only addition is `waiting()` after the event (handshake.rs:180, :198).
  Fix: 1.5 adds those two assertions to machine.rs's tests; 2.4 then
  removes the two world tests.
- **T-7. Formatting on the passing path:** memory.rs:248 formats the
  whole `Case` on every metered step (`Meter::check` takes `&dyn Debug`
  and formats only on failure); exchange.rs:155, 168-170 key results by
  `format!("{:?}")`. Fix: 2.4.

## Left for later

In `later.md`: B-5 (native `waiting()` has no test, and a design gap),
B-6 (session.rs nesting), T-8 (more runs and bytes than the assertions
need), T-12 (fake_peer.rs tests another crate's peer), T-13
(fuzzy_native asserts nothing its focused twin does not), N-9 (requests
after a failure on the native face, now tls.md, 7).

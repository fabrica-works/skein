# Session 01: the crate

skein-tls's production code and its step tests, `crates/skein-tls/**`.
`client.rs` is split along tls.md, 3. The native face's docs are
rewritten in tls.md's words. The face then uses the engine the classic
client already has, instead of a copy, and keeps its state where its
states are. Last, the step tests get one fixtures module and one
behaviour per test. No behaviour a user of the crate can observe
changes.

## Read first

- tls.md, sections 3.1 to 3.6, and 3.6 above all: it was rewritten for
  this plan, and its words are the ones the docs take.
- programming-model.md, sections 4 and 5.4: a state holds what it needs,
  and every match lists every variant (no `_`, no `matches!`).
- `findings/audit.md`, "Production code" and "The crate's step tests".

## What the design asks

- One engine, two faces: "It shares the rustls connection, the session
  and the buffers with the classic client; the two faces differ in how
  room is asked for and granted" (tls.md, 3.6).
- The sides are the side above and the side below (tls.md, 3.1, 3.2).

## Choices this plan makes

- The split follows tls.md, 3's sections: limits (3.5), errors and closes
  (3.3), the side below (3.2), the side above (3.1).
- The engine shared is a set of functions over the classic client's own
  types, which both faces call. No trait, no generic face.
- The native client's cells that only `Open` needs move into its `Open`.

## Rules that bind it

- **No observable behaviour changes:** an increment that changes
  production code changes no test's expected value, and changes only the
  test files its Test shape names (README, "Rules").
- **Only `crates/skein-tls/**`** (README, "Rules").
- **None of the Pulse words** in code, comments, names or messages
  (README, "Pulse"; `code-organization.md`, "Don't").
- **Sizes, once before each commit:** `findings/baseline.md`, section 3's
  two commands over the files the increment changed, with 40 for tests
  and 60 for functions; name each function left over 60 in the commit
  body, with its length.

## Increments

### 1.1 client.rs split along tls.md, 3
Scope: tls.md, 3.1, 3.2, 3.3 and 3.5; findings B-2 and B-13.
Needs: nothing.
Files: `crates/skein-tls/src/client.rs`; new `client/limits.rs`,
  `client/error.rs`, `client/below.rs`, `client/above.rs`,
  `client/close.rs`; `client/native.rs` (its `use super::{…}` only).
Shape:
  - Each item moves whole, with its doc, as `code-organization.md`
    lists: `limits.rs` takes lines 62-211 (the constants to the
    `MAX_OUT`s, `intake_cap` included); `error.rs` takes `Error`, its
    `impl` and `Certificate`; `below.rs` takes `Below`, `Demand`, `Room`,
    `Record`, `open_up`, `wanted`, `ended`, `failed_up`, `length`,
    `fill`; `above.rs` takes `Above`, `Wanted`, `Reading`, `stream`,
    `sent`, `answer`; `close.rs` takes `Closing`, `fail`, `close`,
    `close_open`, `closed`, `closing_up`, `closing_settle`.
  - `client.rs` keeps the rest, at most 500 lines, and re-exports every
    `pub` item that moved: `pub use limits::{…}; pub use error::{…};`.
  - A private item becomes `pub(super)` in its new module, or
    `pub(in crate::client)` where `native.rs` uses it.
  - `client::MAX_HANDSHAKE` becomes `pub(crate)`, re-exported with
    `pub(crate) use limits::MAX_HANDSHAKE;`.
  - Each new module opens with a doc of one to three lines naming its
    tls.md section.
Test shape: no test file changes.
Proof: joint, the suites `land`'s gate runs: `tests/llm-connection` and
  `tests/oauth-accounts` (the classic client under its users) and
  `tests/tls`. Run nothing for it before `land`. Before committing, run
  `git diff --stat origin/main...HEAD -- crates/skein-tls/src/tests
  crates/skein-tls/src/tests.rs crates/skein-tls/src/client/native/tests.rs
  tests` once: it prints nothing. The commit says `Proof: joint, land's
  gate (llm-connection, oauth-accounts, tls worlds), test files
  unchanged`; its `Gate-Result` trailer is the result.
Don't: change a function's body; rename anything; reword a doc.
Size: about 700 lines moved; at most 60 lines added.
Removes: nothing.

### 1.2 The crate's docs in tls.md's words
Scope: tls.md, 3.6 as rewritten; findings N-1, N-5, N-6, N-7, B-12.
Needs: `client.rs` split into its modules, `client/error.rs` among them
  (1.1).
Files: `client/native.rs` (docs and messages only), `lib.rs`,
  `client.rs` (its `pub mod native` doc), `client/error.rs`
  (`Error::Other`), the `Waiting::Close` doc, `config.rs:152` and the
  `expect` messages of `config.rs:205-232`.
Shape:
  - `native.rs`'s module doc: at most 25 lines, in tls.md, 3.6's words:
    what the face is for, its four cells, who closes the stream below,
    and one citation, "(tls.md, 3.6; lib.md, 7.1)".
  - No item or variant is documented twice. A tuple variant's field
    gets no doc of its own: delete the 8 field docs of native.rs:44-136.
  - Cite a tls.md section only where an item implements a rule of its
    own; otherwise the module doc's citation covers it.
  - "the side above" and "the side below", never upper, lower,
    physical, resource or bridge, in prose. Type names stay.
  - Present tense only: no "original", "unchanged", "remain intact".
  - `Waiting::Close`'s doc adds "on the native face, also while a close
    waits for the owner's `Closed`, and after `TransportClosing`";
    `Error::Other`'s adds "on the native face, also when its tokens below
    run out".
  - `lib.rs`'s bullet: "[`client::native`]: the client with independent
    output on both sides (tls.md, 3.6; lib.md, 7.1)."
  - Assertion and `expect` messages change only to drop a Pulse word;
    the rest of a message stays as it is (another session's tests quote
    some of them).
Test shape: one test changes: the `#[should_panic(expected = …)]` of
  `client/native/tests.rs:373`, which quotes a message with "actual".
Proof: unit, `heavy cargo doc -p skein-tls --no-deps` with no warning,
  and README "Done when" item 2's `rg` over `crates/skein-tls/src`
  printing only `client/native/tests.rs` and `src/tests` files; once
  each, at most 3 minutes.
Don't: change code other than strings in assertions and panics.
Size: about 200 lines changed, net about −60.
Removes: the 8 field docs; the history comments.

### 1.3 One engine for both faces
Scope: tls.md, 3.6, "It shares the rustls connection, the session and
  the buffers with the classic client"; findings B-1 and N-3.
Needs: `native.rs`'s docs in tls.md's words (1.2).
Files: `client.rs`, `client/below.rs`, `client/above.rs`,
  `client/native.rs`.
Shape: the classic client's code becomes functions both faces call; the
  native face's copies go. A sketch; departing from it is an
  implementation choice (development.md, 8.2):

  ```rust
  // crates/skein-tls/src/client.rs
  impl Buffers {
      /// The intake, the records held and what TLS owes, for `limits`.
      pub(in crate::client) fn new(limits: &Limits) -> Option<Buffers>;
  }
  /// rustls's connection, or why the stream below already failed it.
  pub(in crate::client) fn connect(fresh: Fresh, wall: Wall) -> Result<Connection, Error>;
  /// `Some` once: when the step just finished the handshake.
  pub(in crate::client) fn ready(tls: &Connection, handshake: &mut Handshake) -> Option<Agreed>;

  // crates/skein-tls/src/client/below.rs
  /// A piece of a record read: kept for rustls; the part to read next.
  pub(in crate::client) fn received(record: Record, bytes: &[u8], buffers: &mut Buffers) -> Record;
  /// The fill of the next piece, or `TooLong`.
  pub(in crate::client) fn next_fill(record: Record, records: &Held) -> Result<u32, Error>;
  /// Whether `bytes` answer `read`, a fill of their length: both faces assert it.
  pub(in crate::client) fn answers(read: Read, bytes: &[u8]) -> bool;

  // crates/skein-tls/src/client/above.rs
  pub(in crate::client) enum Answer { Wait, Bytes(Box<[u8]>), End, Truncated }
  /// What the intake answers a read with, as lib.md, 7 says.
  pub(in crate::client) fn answer(read: Read, reading: &mut Reading, peer: Peer, intake: &mut Intake) -> Answer;
  /// The most a read takes.
  pub(in crate::client) fn largest(read: Read) -> u32;
  ```

  Each face wraps an `Answer` in its own `Event`. The native face's two
  checks that bytes answer a fill (native.rs:445, :456) use `answers`.
  The classic
  `fresh_up` keeps `Line::Failed` when an `End` follows it; the native
  `stream_up` sets `Line::Ended`: keep each as it is.
Test shape: no test file changes.
Proof: joint, the suites `land`'s gate runs: `tests/llm-connection`,
  `tests/oauth-accounts` and `tests/tls` (its `native_*` and
  `fuzzy_native` for the native face, whose highest layer is the world:
  it has no user). Run nothing for it before `land`. Before committing,
  run 1.1's `git diff --stat` and README "Done when" item 4's two `rg`
  once each: all print nothing. The commit says `Proof: joint, land's
  gate (llm-connection, oauth-accounts, tls worlds), test files
  unchanged`.
Don't: add a parameter to a `pub` function; change the order of what a
  step emits.
Size: at most 200 lines of production code changed, net about −40.
Removes: native.rs's `received` copy, `next_read`'s fill, `read_maximum`,
  its `Buffers` literal, its `start`'s `Line` match, its `process`'s
  `Ready`.

### 1.4 The native state where its states are
Scope: programming-model.md, 5.4; findings N-4, B-3, B-4.
Needs: the engine both faces share (1.3).
Files: `client/native.rs`; `client/native/output.rs` if `native.rs` is
  over 800 lines after it; `client/native/tests.rs` (named below).
Shape:

  ```rust
  // crates/skein-tls/src/client/native.rs
  pub struct Client {
      state: State,
      buffers: Buffers,
      upper_output: UpperOutput,
      lower_output: LowerOutput,
      next_lower: Option<u64>,
  }
  enum State { Fresh(Fresh), Open(Open), Failed, Closing, TransportClosing, Closed, TransportClosed }
  struct Open {
      tls: Connection,
      handshake: Handshake,
      peer: Peer,
      writing: Writing,
      record: Record,
      reading: Reading,
      upper_read: Read,
      lower_read: Option<Read>,
      withdrawn_read: Option<Read>,
      lower_ended: bool,
  }
  /// The one match that reaches `Open`; every other state is `None`.
  fn open(state: &mut State) -> Option<&mut Open>;
  ```

  - A field that a state other than `Open` reads stays on `Client`; the
    commit body names it and the state. `lower_read` and
    `withdrawn_read` likely stay: `fail` (native.rs:700-703) and
    `output_up` (native.rs:670-676) withdraw the read after leaving
    `Open`.
  - `size_of::<Client>()` may change, and `native::worst_case` with it,
    which counts it; no test pins that value. The commit body gives both
    values, before and after.
  - Where every arm but `Open` is `unreachable!` (native.rs:352, 377,
    500, 539, 590, 759 at `73c900b`): `let Some(open) = open(&mut
    client.state) else { unreachable!("…") };`. Where other states have
    an answer (native.rs:486 returns, :785 returns `true`, :859 does
    nothing, :880 and :936 return `false`), keep the answer: `else {
    return <its value> };`.
  - Delete `|| open.writing == Writing::Finishing` (native.rs:740: never
    set on this face) and the `State::Fresh(_)` arms of `output_up`
    that cannot run (native.rs:669, 680).
Test shape: in `client/native/tests.rs`:
  - the white-box lines (native/tests.rs:288, 332-334, 352, 389-394 at
    `73c900b`) set fields that stay on `Client`, or match `State`, whose
    variants stay: none should change. If one must, it follows its
    field, and no expected value changes;
  - written first, before the change, two tests of `waiting()`, which has
    none: `a_fresh_native_client_waits_for_its_handshake` and
    `a_transport_closing_client_waits_for_its_close`. Reference:

    ```rust
    #[test]
    fn a_fresh_native_client_waits_for_its_handshake() {
        let machine = Machine::new();
        assert_eq!(machine.client.waiting(), Waiting::Handshake);
    }
    ```
Proof: world, the suites `land`'s gate runs, `tests/tls`'s
  `native_output`, `native_memory` and `fuzzy_native` among them. Run
  nothing for it before `land`. Before committing, run `git diff --stat
  origin/main...HEAD -- tests` once: it prints nothing. The commit says
  `Proof: world, land's gate (tls native worlds), world tests
  unchanged`.
Don't: change a `pub` item; change what a step emits, or its order.
Size: at most 400 lines of production code changed.
Removes: the six flat fields, `Writing::Finishing`'s test, the dead
  `Fresh` arms.

### 1.5 The step tests: one fixtures module, one behaviour each
Scope: testing-strategy.md, 2.1; findings T-5, T-6, T-11, T-3 (crate),
  N-2, N-6, B-7, and T-4's two `waiting()` checks.
Needs: the native state in its `Open` (1.4), whose fields the native
  step tests reach.
Files: `src/tests.rs`, `src/tests/config.rs`, `src/tests/machine.rs`,
  `src/client/native/tests.rs`, `src/config.rs` (its `#[cfg(test)]`
  peer goes).
Shape:

  ```rust
  // crates/skein-tls/src/tests.rs, the step tests' fixtures
  pub(crate) const LIMITS: Limits;                 // as today
  /// 2030-01-01, within every fixture certificate's validity.
  pub(crate) const VALID: Wall = Wall::from_nanos(1_893_456_000 * 1_000_000_000);
  pub(crate) const WITHDRAW: Down = Down::Demand { read: Read::Nothing, room: 0 };
  /// A certificate or key of tests/tls/fixtures, read from disk
  /// (`include_bytes!` is banned in step crates).
  pub(crate) fn fixture(name: &str) -> Vec<u8>;
  /// What a failure tells the side above.
  pub(crate) fn failed(error: Error) -> Vec<Event>;
  /// A TLS 1.2 rustls server for the native step tests (from config.rs).
  pub(crate) fn native_peer() -> (Config, rustls::server::UnbufferedServerConnection);
  ```

  - In `client/native/tests.rs`: `Machine::ready()` and a `struct
    Handshake { … }` in place of `actual_ready` and `take_handshake`
    (T-11). The two tests behind `actual_ready`'s flag become
    `ready_survives_token_exhaustion_in_the_same_step` and
    `token_exhaustion_on_room_fails_three_events_down`, each asserting
    its own events in its own body.
  - Expected values are literals (T-6), in native/tests.rs:94-119 and
    tests/config.rs:71-81 at `73c900b` only; constants used as bounds
    elsewhere stay. Derive each literal by hand from tls.md, 3.5 and 3.6,
    write it, and run `heavy cargo nextest run -p skein-tls <test>` once.
    If it fails and the arithmetic holds, that is a question
    (development.md, 8.7).
  - native/tests.rs:93 checks six things under one name, with a computed
    expected value: split it by behaviour, with literals.
  - A test that branches on its loop variable becomes one test per branch
    (native/tests.rs:180; machine.rs:145;
    `the_stream_ending_or_failing_before_the_handshake_fails_it_when_asked`'s
    `if error == Error::Truncated`). A loop over versions or over inputs
    with the same expected outcome stays a loop.
  - The step tests of the stream ending or failing (`machine.rs`'s
    `the_stream_ending_or_failing_before_the_handshake_fails_it_when_asked`
    and `the_stream_ending_or_failing_during_the_handshake_fails_it`, or
    what they became) assert `waiting()` as the world tests of the same
    names do (tests/tls/tests/handshake.rs:180, :195, :198 at
    `73c900b`): before the handshake, `Waiting::Handshake` after the
    `End` and after the `Failed`; during it, `Waiting::Handshaking`
    before the event, and `Waiting::Close` after the `End` and after the
    `Failed`. Session 02's 2.4 then removes those world tests.
  - None of the Pulse words in `src/tests/config.rs` either.
  - Names as the README's "Rules" say: at most 9 words for a test this
    increment writes or renames; a name over 12 words in these files is
    renamed. None of the Pulse words. A local variable named `actual`
    becomes `outcome`.
Test shape: the files above. Reference, the retry test split:

  ```rust
  #[test]
  fn a_retry_with_a_short_cookie_owes_a_second_hello() {
      let (mut machine, hello) = started(LIMITS);
      let record = retry(&hello, 100);
      let length = u32::try_from(record.len() - 5).unwrap();
      let (_, requests) = machine.bytes(&record[..5]);
      assert_eq!(requests, [Down::Demand { read: Read::Fill(length), room: 0 }]);
      let (events, requests) = machine.bytes(&record[5..]);
      assert_eq!(events, []);
      let [Down::Demand { read: Read::Fill(5), room }] = requests[..] else { panic!("{requests:?}") };
      assert!(room > 100 && room <= client::FLIGHT, "{room}");
  }
  ```

  and `a_retry_whose_cookie_no_flight_holds_fails_the_handshake`. Each
  test at most 40 lines and 20 ms.
Proof: unit, one mutant: make running out of tokens leave the
  connection open (the branch that fails it `Other` when `next_lower` is
  `None`, native.rs:825 at `73c900b`), run `heavy cargo nextest run -p
  skein-tls`, see `ready_survives_token_exhaustion_in_the_same_step`
  fail, and restore the code before landing; once, at most 5 minutes.
  The commit's `Proof:` line names the test that failed.
Don't: change production code other than deleting `config.rs`'s test
  peer; remove, rename or split a test without its line in the commit
  body (README, "Rules").
Size: about 500 test lines touched, net about −80; production −32.
Removes: `config.rs`'s `native_test_peer`; `actual_ready`;
  `take_handshake`; machine.rs's own `failed`.

## Done when

- `client.rs`, `native.rs` and every other file of the crate are within
  800 lines.
- README "Done when" item 2's `rg` over `crates/skein-tls` prints
  nothing.
- README's "Done when", items 4 and 5, hold.
- The step tests pass with 1.4's two new `waiting()` tests and 1.5's
  splits, and 1.5's commit body has a line for each test it removed,
  renamed or split.

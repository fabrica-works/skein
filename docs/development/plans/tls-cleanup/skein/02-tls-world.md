# Session 02: the world crate

`skein-tls-world`, `tests/tls/**`: the worlds and focused tests of the
TLS client. Its harness pieces, now defined two or three times, are
defined once. The native tests check one behaviour each, under plain
names, through `Pair`'s helpers. `world.rs` is split within 800 lines.
The classic tests lose two duplicates of the step tests, and compute no
expected value with the code under test. No production code changes.

## Read first

- tls.md, section 5, and 3.6 for the native face.
- testing-strategy.md, sections 2.4 and 8.
- `findings/audit.md`, "The world crate".
- `code-organization.md`, "Modules of `skein-tls-world` at the end".

## What the design asks

- "Focused tests aim at one outcome each" (tls.md, 5).
- The worlds "assert only what does not depend on" rustls's randoms
  (tls.md, 5): no test pins a ciphertext length that rustls chooses.

## Choices this plan makes

- One module per shared harness piece: `records.rs`, `meter.rs`, and
  `native_pair.rs` grown with helpers.
- A test whose loop variable picks its expected outcome becomes one test
  per outcome; a loop over the TLS versions stays a loop.

## Rules that bind it

- **No production code changes,** and no file outside `tests/tls/**`
  (README, "Rules"). A mutant (2.2) is a temporary local edit, restored
  before landing: it never lands.
- **What a test checks stays checked:** a removed or merged assertion is
  named in the commit body with the test that now holds it.
- **None of the Pulse words** in code, comments, names or messages
  (README, "Pulse").
- **Sizes, once before each commit:** `findings/baseline.md`, section 3's
  first command over the test files the increment changed (40 lines a
  test), and its second over the `src/` files it changed (60 lines a
  function, a guide: name each one left over it in the commit body).

## Increments

### 2.1 The shared harness, defined once
Scope: findings B-8, B-9, B-10, T-9, T-2 (helpers), T-7 (memory.rs).
Needs: nothing.
Files: new `tests/tls/src/records.rs`, `tests/tls/src/meter.rs`;
  `tests/tls/src/{lib.rs, native_pair.rs, native_peer.rs, world.rs,
  native.rs}`; `tests/tls/tests/{memory.rs, native_memory.rs,
  handshake.rs}`; `tests/tls/Cargo.toml` (`skein-heap` from
  `[dev-dependencies]` to `[dependencies]`; line 12's comment loses
  "actual"); `tests/tls/tests/native_output.rs` (its three ready-pair
  builders, and the 18 lines that read `pair.server` or
  `pair.extracted`, which read `Pair`'s `Peer` instead: ruling 13).
Shape:

  ```rust
  // tests/tls/src/records.rs: TLS records as tests read and spoil them
  /// The length of a record's body, from its 5-byte header.
  pub fn length(header: &[u8]) -> usize;
  /// Ciphertext cut into whole records (handshake.rs's `records`).
  pub fn split(ciphertext: &[u8]) -> Vec<&[u8]>;
  /// Changes a byte of the first record's body, so that it fails to decrypt.
  pub fn corrupt_first(ciphertext: &mut [u8]);

  // tests/tls/src/meter.rs: what memory.rs and native_memory.rs share
  pub const SMALL: Limits;  // memory.rs:410, as today
  pub const LARGE: Limits;  // memory.rs:411, as today
  /// The metered steps of one case: memory.rs's `check` and `finish`
  /// (memory.rs:246-252, 396-407), as methods.
  pub struct Steps { /* meter, outside, bound, most */ }

  // tests/tls/src/native_pair.rs
  pub const LIMITS: Limits = Limits { read: 16, send: 16, records: client::MAX_RECORD };
  /// The server: rustls's own, or one whose keys were taken out.
  pub enum Peer { Rustls(Box<Server>), Extracted(Tls12) }  // Box: clippy's large_enum_variant (ruling 16)
  /// The lower side's output right, as the pair answers it.
  pub enum Output { Idle, Wanted { .. }, Granted { .. }, Cancelled { .. } }
  impl Pair {
      /// A pair past `Ready`, with LIMITS.
      pub fn ready(server: &pki::Server) -> Pair;
  }
  impl Peer {
      /// rustls's server; panics on `Extracted` (ruling 13's default).
      pub fn rustls(&self) -> &Server;
      pub fn rustls_mut(&mut self) -> &mut Server;
      /// The TLS 1.2 peer; panics on `Rustls`.
      pub fn extracted(&self) -> &Tls12;
      pub fn extracted_mut(&mut self) -> &mut Tls12;
  }
  ```

  - `native_output.rs`'s three ready-pair builders (native_output.rs:81-96,
    259-276, 362-377 at `73c900b`) become `Pair::ready`, with no
    assertion changed.
  - `memory.rs:46` loses its Pulse word.

  - `native_memory.rs` holds a `Pair`'s `Peer` and `Output` instead of
    its own copies; `memory.rs` and `native_memory.rs` use `meter.rs`;
    every site that reads a header's length uses `records::length`.
  - `memory.rs` takes the root from `pki::ROOT`, not from disk
    (memory.rs:37).
  - `memory.rs`'s metered step passes `&(&self.case, self.steps)` to
    `Meter::check`, which formats only on failure (memory.rs:248).
Test shape: no assertion changes; tests call the shared pieces. The
  test count of `cargo nextest list -p skein-tls-world` is the same
  before and after.
Proof: world, the suites `land`'s gate runs, `tests/tls` among them.
  Run nothing for it before `land`. Before committing, run `heavy cargo
  nextest list -p skein-tls-world | grep -c '^skein-tls-world::'` once
  (the test lines only: heavy's output also holds cargo's build lines),
  and compare with 62, the count at `73c900b` (ruling 15): equal. The
  commit says `Proof: world, land's gate (tls worlds), test count
  unchanged`.
Don't: change a test's name or what it asserts; change `drive::Wire`'s
  or `pki`'s public items, which other test crates use.
Size: about 400 test lines touched, net about −150.
Removes: native_memory.rs's `SMALL`, `LARGE`, `Output` and meter copies;
  memory.rs's root read from disk; the header-length sites' arithmetic.

### 2.2 The native tests: one behaviour each, plain names
Scope: findings T-1, T-2, T-3 (world), N-2, N-6 (world crate).
Needs: `Pair::ready` and `LIMITS` in `native_pair.rs` (2.1).
Files: `tests/tls/tests/{native_output.rs, native_memory.rs,
  fuzzy_native.rs}`; `tests/tls/src/{native.rs, native_peer.rs, lib.rs}`
  (docs, names and messages); `tests/tls/src/native_pair.rs` (docs, names,
  messages, and the helpers below).
Shape:
  - `native_pair.rs` gains the helpers the tests need, and the builders
    the split tests share:

    ```rust
    /// The side above's output right in most tests.
    pub const UPPER: Token = Token::new(1);
    /// The event that grants `right`.
    pub fn granted(right: Token) -> native::Event;
    impl Pair {
        pub fn read(&mut self, bytes: u32) -> Vec<native::Event>;
        pub fn withdraw(&mut self) -> Vec<native::Event>;
        pub fn finish(&mut self) -> Vec<native::Event>;
        pub fn close(&mut self) -> Vec<native::Event>;
        /// Asks for an output right, then settles the pair.
        pub fn room(&mut self, right: Token, bytes: u32) -> Vec<native::Event>;
        pub fn send(&mut self, right: Token, bytes: &[u8]) -> Vec<native::Event>;
        /// The server read the client's `close_notify`.
        pub fn peer_closed(&self) -> bool;
    }
    ```
  - None of the Pulse words in these files, in names, docs or messages.
  - Tests at most 40 lines, through `Pair`'s helpers; the `use` blocks
    inside functions go to the module's top.
  - Split only these, whose loop variable picks the expected outcome:
    native_memory.rs:452; native_output.rs:23, :63, :409, :472, :555 (at
    `73c900b`). Each becomes one test per outcome, sharing a builder. A
    loop over the TLS versions stays a loop.
  - Names as the README's "Rules" say: at most 9 words for a test this
    increment writes or renames, none of the Pulse words. For example:
    `actual_owed_growth_across_an_older_queued_grant_preserves_fifo_prefix_and_suffix`
    → `owed_output_drains_in_order_across_an_older_grant`;
    `native_finish_spends_a_real_held_grant_and_emits_actual_close_notify_and_one_finish`
    → `finish_spends_a_held_grant_and_sends_close_notify`.
  - Docs: one citation per module doc; no history ("original 400 runs …
    without edits"); "the side above" and "the side below" in prose.
Test shape: reference, `native_output.rs`:

  ```rust
  #[test]
  fn finish_spends_a_held_grant_and_sends_close_notify() {
      let mut pair = Pair::ready(&pki::Server::plain());
      assert_eq!(pair.room(UPPER, 16), [granted(UPPER)]);
      assert!(pair.finish().is_empty());
      assert!(pair.settle().is_empty());
      assert!(pair.peer_closed());
      assert_eq!(pair.finished, 1);
  }
  ```

  Each test at most 40 lines; a focused world test under 0.5 s.
Proof: world, one mutant: make the native face's `Finish` send no
  `close_notify` (a temporary edit of `crates/skein-tls/src/client/native.rs`
  in this worktree, never committed), run `heavy cargo nextest run -p
  skein-tls-world --test native_output`, see
  `finish_spends_a_held_grant_and_sends_close_notify` fail, restore;
  once, at most 5 minutes. The commit's `Proof:` line names the test that
  failed.
Don't: change what a test asserts other than splitting it; add a helper
  outside `native_pair.rs`; change `native_pair.rs`'s other items.
Size: about 700 test lines touched, net about −200.
Removes: the umbrella tests; the local variable `actual`.

### 2.3 world.rs within 800 lines
Scope: findings B-11, T-10.
Needs: `records.rs`, which `world.rs` uses since 2.1.
Files: `tests/tls/src/world.rs`; new `tests/tls/src/world/scenario.rs`,
  `tests/tls/src/world/check.rs`.
Shape:
  - `world/scenario.rs`: the scenario and its settings, as drawn from a
    seed (about 260 lines).
  - `world/check.rs`: what a run is checked against; `check` (92 lines)
    becomes `check_handshake`, `check_ending` and `check_left_alone`.
  - `world.rs` keeps the loop; `run` (86 lines) becomes `World::new` and
    the loop; `above_acts` (81 lines) is split by what the side above
    does.
  - Every `world::` path other crates or tests use stays (`pub use`).
Test shape: no test file changes.
Proof: world, the suites `land`'s gate runs:
  `fuzzy_world::exchanges_swept` (400 runs, which asserts that what it
  draws fell) and `exchange.rs` among them. Run nothing for it before
  `land`. Before committing, run `git diff --stat origin/main...HEAD --
  tests/tls/tests` once: it prints nothing. The commit says `Proof:
  world, land's gate (fuzzy_world, exchange), tests unchanged`.
Don't: change what a world draws, or what it checks.
Size: about 450 lines moved, net at most +40.
Removes: nothing.

### 2.4 The classic tests: no duplicates, literal expectations
Scope: findings T-4, T-6 (world crate), T-7 (exchange.rs).
Needs: skein 01's step tests asserting `waiting()` after the stream ends
  or fails, before and during the handshake (1.5).
  These are the step tests of the stream ending or failing, before and
  during the handshake, whatever 1.5 named them.
Files: `tests/tls/tests/{handshake.rs, exchange.rs, native_output.rs,
  native_memory.rs, stack.rs, startup.rs, memory.rs}`,
  `tests/tls/src/pki.rs`.
Shape:
  - Remove handshake.rs's
    `the_stream_ending_or_failing_before_the_handshake_fails_it_when_asked`
    and `the_stream_ending_or_failing_during_the_handshake_fails_it`: the
    step tests of the stream ending or failing check the same, with
    `waiting()` since 1.5. The commit body gives, for each, the step test that holds its
    assertions. If an assertion has no step test to hold it, keep that
    world test unchanged, and say so.
  - Expected values computed with the crate's functions become literals:
    handshake.rs:388 (`client::room_for`), native_memory.rs:423 and :445
    (built from the crate's constants), at `73c900b`, only; derived by
    hand and run once as 1.5 says.
  - The bare record sizes of handshake.rs:344, native_output.rs:205, 298,
    354 and native_memory.rs:217, 429 get names in `pki.rs`: the TLS 1.3
    and TLS 1.2 overheads, a key update's record, a TLS 1.2 alert's
    record.
  - `exchange.rs` collects its results in a `Vec<Waiting>`, checked with
    `contains`, not a set of `format!("{:?}")` strings (exchange.rs:155,
    168-170): `HashSet` is banned in tests (`tests/clippy.toml`), and
    `Waiting` has no `Ord`.
  - `startup.rs` loses its two Pulse words (startup.rs:42, :44).
  - Names as the README's "Rules" say: a name over 12 words in these
    files is renamed to at most 9.
Test shape: the files above; no new test.
Proof: world, the suites `land`'s gate runs, `tests/tls` among them.
  Run nothing for it before `land`. Before committing, run `heavy cargo
  nextest list -p skein-tls-world | grep -c '^skein-tls-world::'` once:
  62 (ruling 15) less the count of world tests the commit body says it
  removed, at most two. The
  commit says `Proof: world, land's gate (tls worlds), N tests fewer`.
Don't: pin a length that rustls's randoms change.
Size: about 150 test lines touched, net about −60.
Removes: two world tests; the `format!` keys.

## Done when

- `world.rs` and every other file of the crate are within 800 lines.
- README "Done when" item 2's `rg` over `tests/tls` prints nothing.
- README's "Done when", items 6 and 7, hold for `tests/tls`.

# Session 02, step s1: the world crate's native files in plain words

A step not in the plan, added by the supervisor after the check of
2026-10-12T01:17Z (`decisions.md`, ruling 21). Session 02 owns it, as it
owns `tests/tls/**`.

2.2 removed "actual" from the native files by writing "observed" in its
place: `rg -i -w -c observed crates/skein-tls tests/tls` sums to 9 at
`73c900b` and to 106 at `3925a5d`, 102 of them in `tests/tls`, for
example `expect("observed record length")`, "observed exact upper right",
"observed physical lifecycle". The plan asks for the same things "in
fewer, plainer words" (README), not for another word in the same place.

### s1 The native files in plain words
Needs: 2.4 landed (it changes `native_output.rs` and `native_memory.rs`).
Files: `tests/tls/src/{native.rs, native_pair.rs, native_peer.rs}`,
  `tests/tls/tests/{native_output.rs, native_memory.rs, fuzzy_native.rs}`:
  messages, docs, comments and local names only.
Shape:
  - Delete "observed" where the message says the same without it; rewrite
    the message plainly where deleting it leaves no sense. Example:
    in `native.rs`'s `TransportClosed` arm,
    `assert!(self.resources.client, "observed physical lifecycle")` becomes
    `assert!(self.resources.client, "TransportClosed after io closed the client's socket")`,
    saying what the assertion checks (`resources.client` is set by io's
    `Closed` for the client).
  - "physical" in prose and messages becomes tls.md's words: "below", "the
    side below", "the Closed from below". The test
    `physical_close_answers_once_after_io_cancellation` and the method
    `queue_physical_close` are renamed (at most 9 words), with their lines
    in the commit body (README, "Rules").
  - A message changes in wording only: no assertion, expected value or
    other test name changes.
Test shape: no test added or removed.
Proof: process, once, seconds: `rg -i -w -c observed tests/tls` sums to at
  most 4, and `rg -i -w physical tests/tls` prints nothing; then land's
  gate. The commit says `Proof: process, observed N in tests/tls (was
  102), physical 0 (was 15), land's gate`.
Don't: change `crates/skein-tls/**`; add a word of the Pulse list, or
  another word in the place of "observed".
Size: about 120 lines changed, net about 0.

# lib

Provisional, 2026-10-03, revised 2026-10-18. The design of `skein-lib`:
the building blocks every step crate shares, in skein and in every
service built on it. The mechanics are those of programming-model.md;
this document says what lib holds and what each part promises.

## 1. In one page

- **Everything step code needs and the subset does not give it.** Typed
  handles and the slabs that issue them, bounded containers, cursors over
  owned bytes, secrets' bytes, base64 and SHA-256, the stream vocabulary
  and its intake, deadlines, time, randomness, the environment a step
  reads, and the digest of a state that replay compares.
- **Bounded, and priced.** Every container has a capacity fixed when it
  is made, refuses past it, and reports its own `worst_case(capacity)`,
  bookkeeping included, so a layer adds up what its containers report.
- **Written once, tested hardest.** Every layer of every service stands on
  lib. Application code does not hand-roll a data structure: what is
  missing goes into lib.
- **The subset, with a few allowances.** lib is step code, `no_std` with
  `alloc`, with the allowances of programming-model.md, 10.2: generic
  types, lifetime parameters on its cursors, hand-written impls of std
  traits for its own types, `Vec`, `VecDeque` and B-trees inside its
  containers, and `while` loops bounded by a container's capacity. Where
  a module builds on a type that is out, it says so with a scoped
  `#[expect(clippy::disallowed_types, reason = "…")]`.

## 2. In skein

lib depends on nothing. Every other crate, in skein and in a service,
depends on it. It is the only part of skein a service's domain uses.

**Built for WebAssembly too.** lib, `skein-codec`, `skein-http` and
`skein-ui` build for `wasm32-unknown-unknown` as well as for the host:
none of them depends on io, the shell or a platform's API. A client
compiled to WebAssembly, an application's in a browser over skein's
browser parts (ui-web.md), links lib's containers and base64, the
generated codecs, HTTP's client and the event-stream reader (http.md,
section 2), and skein-ui's tree, view and words (ui.md), and its shell
hands them what its platform streams. Nothing in them assumes a 64-bit
`usize`: a conversion that could fail on a 32-bit target is checked.
skein's gate builds the four for that target, and `skein-ui-web` beside
them (testing.md, section 6).

## 3. Handles and slabs

- **`Id<T>`** is a slot index plus a generation, typed by the entity it
  names, and `Copy`. It is never reused for another entity: a slot whose
  generation would wrap is never used again. `id.token()` and
  `Id::<T>::from_token(t)` convert to and from the name that crosses a
  boundary.
- **`Slab<T>`** holds one entity kind, at a capacity fixed at startup that
  is the admission limit for that kind.
  - `insert` returns the new `Id`, or hands the value back when the slab
    is full: the refusal.
  - `get` and `get_mut` check the generation and return `None` for a
    handle whose slot holds another entity now.
  - `get2_mut` looks up two entities at once, for the rare step that
    needs both, and fails on equal handles (programming-model.md, 5.1).
  - `retire` marks a slot; `reclaim`, at the iteration's reclaim point,
    frees every retired one, so an entity can be looked up until the
    iteration ends.

## 4. Names across boundaries

- **`Token`** is an opaque `u64`, the only name that crosses a boundary.
  A layer makes one from its own handle and turns it back only for tokens
  it issued, in the record variant it issued them for.
- **`ReplyTo`** wraps a token, and is neither `Copy` nor `Clone`, and
  `#[must_use]`: replying consumes it, so a second reply does not
  compile. A missing one is the simulator's to catch.

## 5. Bounded containers

| Container | What | Over |
|---|---|---|
| `Queue<T>` | the records between stages, first in first out | a ring buffer allocated at capacity |
| `List<T>` | a sequence a step appends to | a buffer allocated at capacity |
| `Map<K, V>` | a table keyed by value, ordered by key | a B-tree, priced in tree nodes |
| `Set<K>` | a `Map` without values | a B-tree, priced in tree nodes |
| `Stack<T>` | what a parser keeps instead of recursing | a buffer allocated at capacity |

Each refuses past its capacity, handing the value back rather than
panicking, with one exception: `Queue::push` is how a step emits into
the room the loop reserved for it (`MAX_OUT`), so a full queue there is a
bug, asserted; `Queue::try_push` is the refusing form. A B-tree allocates
as it fills, so its worst case counts its nodes at capacity, not its
entries.

### 5.1 State digests

A seed replays to the same trace, and to the same digest of each
process's state (testing-strategy.md, section 6). lib gives the hasher,
and every type of its own that a state holds hashes what it holds, never
where it holds it.

- **`StateHasher`** is a `core::hash::Hasher` over SHA-256 (6.2), with
  no key: a digest is a function of the values hashed alone, the same in
  every run, on every machine and every target.
  - `write` hashes the bytes given; `write_u8` to `write_u128` and
    `write_i8` to `write_i128` hash the integer's little-endian bytes at
    its own width; `write_usize` and `write_isize` hash 8 bytes, so a
    32-bit build (section 2) and a 64-bit one digest one state alike.
    std hashes a slice of `usize` or `isize` whole, through `hash_slice`,
    at the target's width, which no `Hasher` reaches: a state holds them
    as fields or in lib's containers, which hash item by item, and a type
    that keeps a slice of them hashes it item by item, by hand.
  - `digest(self) -> [u8; 32]` ends it. `finish()`, which `Hasher`
    requires, gives the digest's first 8 bytes as a big-endian `u64`,
    without ending it.
  - `state_digest(&value) -> [u8; 32]` hashes one value whole.
- **A state is digested through its types' derived `Hash`.** A service's
  state types derive it, as they derive `Debug`. A field that no later
  step reads, such as a measurement kept for display, may be left out by
  a hand-written impl whose comment says why.
- **lib's types hash what their later answers depend on:**

  | Type | Its `Hash` covers |
  |---|---|
  | `Slab<T>` | each slot in index order: free, with its generation; occupied, with its generation, whether it is retired, and its value; or spent. Then the order in which its free and retired slots will be handed out again |
  | `Map<K, V>`, `Set<K>` | the count, then each entry in key order |
  | `Queue<T>`, `List<T>`, `Stack<T>` | the count, then each item: a queue front to back, a list in order, a stack bottom to top |
  | `Deadlines<K>` | the count, then each armed deadline, its time and its key, in the order `expire` takes them |
  | `Journal<W, O>`, `RequestTable<O, R>` | their entries in their own order, and the counters, keys and attempts that decide what they emit next |
  | `Intake` | the bytes it holds, in order |
  | `Id<T>`, `Token`, `Time`, `Duration`, `Wall`, `Secret`, `Rng` | their value |
  | `CryptoRng` | its stream number and how many bytes it has drawn, never its key |

- **Never hashed:** a container's capacity, which its owner fixed, and
  its layout in memory: a B-tree's nodes, a ring's head, a buffer's spare
  room, an address. Two states that would answer every later step alike
  digest alike, however each was reached. A `Slab` holds a slot for each
  of its capacity, so its slots in index order, free ones included, are
  all of it that is hashed.
- **Not state, so no `Hash`:** `Env<L>`, which a step is handed and does
  not keep (programming-model.md, section 3), and `stream::Held`, a buffer
  served as a stream's side below, which the step that makes it reads. A
  service that keeps its layers' envs beside them, as the echo's does,
  digests its domains' state without them: the times are the trace's. A
  state that keeps a `Held` past its step hashes, by hand, what of it is
  left to read.
- **A digest reads the whole state,** so a world asks for one at the end
  of a run, and after each iteration only when it chooses to (testing.md,
  section 5). No step code calls it.

## 6. Bytes

- **Owned bytes** are a `Box<[u8]>` allocated at its final length.
  `bytes::copy_of` copies one, for data a domain keeps and also sends;
  `bytes::zeroed` makes one for the side below to fill, a receive buffer.
  The byte search (`bytes::find`, `find_from`, `count`) runs in linear
  time and allocates nothing.
- **`Reader`** reads a delivery without trusting any length in it: every
  read (`u8`, `u16`, `u32`, `u64`, `bytes`, `skip`) returns an `Option`,
  and a short one is a framing error for the caller, never a panic.
- **`Writer`** builds a message into a box whose length was computed
  first: `Writer::new(len)`, `put` the fields, `finish()`. A write past
  the end is refused whole (`Overflow`), writing nothing; finishing short
  is an assertion.
- **`Decimal`** holds the decimal digits of a count, for a step that
  writes a number as text without formatting: measure it, then put it.
- **`Secret`** holds a secret's owned bytes: a credential, a token, a
  key. Its `Debug` shows its length only, never its bytes, so a record
  that holds one prints, logs and traces without it (codec.md, section 3).
  Its owner reads the bytes with `expose`, which makes each use visible in
  review. Clone, equality and `Hash` are over its bytes, as a record's
  derives need; it neither zeroes nor locks memory (io.md, 5.3).

### 6.1 Base64

`lib::base64` encodes bytes as base64 text and decodes it back (RFC
4648), for every crate that carries bytes in text: PKCE's challenge and a
token's claims (oauth.md), PEM's certificates (shell.md, 6.2), a
browser's screenshots (browser.md), a record in an event's data. skein's
crates use it and keep no copy of their own.

- **Two alphabets, each with its padding:** `Alphabet::Standard` (`+`,
  `/`), padded with `=` to whole groups of four; and `Alphabet::Url` (`-`,
  `_`), unpadded, as JWTs and PKCE use it (RFC 7515, RFC 7636).
- **Measured, then written.** `encoded_len` gives the text's length
  before anything is set aside, and `encode` writes into a `Writer` with
  that much room left (section 6): nothing allocates but the box its
  caller sized.
- **Decoding is bounded and canonical.** `decode` takes a maximum of
  decoded bytes and refuses past it before setting anything aside. It
  refuses, with `base64::Error`: a symbol outside the alphabet; padding
  where the alphabet takes none, or anywhere but the end; a length no
  encoding has; trailing bits that are not zero. So one value has one
  encoding.
- **In pieces.** A `Decoder` takes text in pieces, carrying what a piece
  leaves of a group, within the same maximum and the same refusals, so a
  reader that skips what surrounds the text (PEM's line breaks) or
  receives it in chunks (an event's data) decodes it without copying it
  whole first.

### 6.2 SHA-256

`lib::sha256` is SHA-256 (FIPS 180-4), for every crate that names bytes
by their content: io's content versions (io.md, section 5), PKCE's
challenge (oauth.md), a browser bundle's digest directory (ui-web.md),
replay's state digests (5.1), and a service's own, such as hearth's,
whose domains name every input and output by it. skein's crates use it
and keep no copy of their own.

- **Streaming.** `Sha256::new()`, then `update(&[u8])` with each piece as
  it arrives, cut anywhere, then `finish(self) -> [u8; 32]`.
  `sha256::digest(&[u8]) -> [u8; 32]` is the one-shot form. The digest is
  its 32 bytes in the standard's order, as `sha256sum` prints them in
  hex.
- **No heap, no copy.** The state is eight words, one 64-byte block and
  the count of bytes hashed, so a step keeps one in its state and its
  worst case counts nothing more. `update` compresses each whole block
  straight from the caller's slice, and copies only the part of a block a
  piece leaves: hashing costs the compression, not a copy.
- **`Clone`,** so a prefix common to several digests is hashed once and
  each goes on from its own copy. Its `Debug` shows the count of bytes
  hashed, not the state.
- **The standard's bound,** 2^64 − 1 bits, is asserted: no bounded step
  hashes that much.
- **The users move onto it.** skein-io's `digest` module keeps its public
  API, `Digest([u64; 4])` and `digest(&[u8])`, as a thin layer over lib's
  (io.md, section 5): its words are the digest's 32 bytes read as four
  big-endian `u64`s, so no user of skein-io changes. `skein-oauth`'s PKCE
  and `skein-ui-bundle`'s digests use lib's and keep no copy;
  `skein-ui-bundle`, a build tool in ordinary Rust (ui.md, section 2),
  depends on lib for it.
- **HMAC.** `sha256::Hmac::new(key: &[u8])`, then `update(&[u8])` with
  each piece as it arrives, then `finish(self) -> [u8; 32]`, is
  HMAC-SHA256 (RFC 2104), for a service that checks a signed message as
  it streams. A key longer than a block is hashed first, as the standard
  says. Its state is two SHA-256 states, the inner seeded with the key's
  inner pad and the outer with its outer pad, with no heap. `Clone` as
  `Sha256`'s; its `Debug` shows the count of the message's bytes hashed
  alone (`Hmac { hashed: N }`), the pads not counted, never the key or
  its pads.
- **Comparing in constant time.** `sha256::equal(a: &[u8], b: &[u8]) ->
  bool` takes a time that depends on the lengths alone, never on where
  the bytes differ, for comparing a signature, a state or a token with
  the one expected. `skein-oauth`'s comparison of a returned state uses
  it and keeps no copy.

## 7. Streams

`lib::stream` is the classic shape of a byte boundary
(programming-model.md, 4.3):

```rust
pub enum Read {
    Nothing,
    Fill(u32),
    Scan { until: Delimiter, max: u32 },
    Line { max: u32 },
    Arrived { max: u32 },               // what has arrived: one byte to max
}

pub enum Down {                         // to the side below
    Demand { read: Read, room: u32 },   // what this state needs, and the output room it wants
    Send(Box<[u8]>),                    // moved down, within the room granted
    Finish,                             // nothing more to send: flush, then end the stream
}

pub enum Up {                           // from the side below
    Bytes(Box<[u8]>),                   // the demand: exactly, or what has arrived of it
    Room,                               // the room asked for is free
    End,                                // the other side will send nothing more
    Failed(Fault),                      // the stream is broken: no more Bytes or Room
}
```

A `Fault` says why a stream broke as far as the side above can act on it:
the peer reset it, the side below could not make sense of the peer's data
(a TLS record that fails to decrypt), or anything else.

**The contract of a stream.** Every side below meets it, io's and every
machine's:

- **A demand is answered at most once:** by `Bytes`, what its read asks
  for (exactly, but for a read of what has arrived, below), or by `Room`,
  whichever the side below can give first;
  either answer ends it, and nothing is outstanding until the side above
  states its next. The side above states its next demand only after an
  answer, never in place of one outstanding. A state that wants nothing
  more after an answer states nothing (programming-model.md, 5.4).
- **`Read::Nothing` with no room withdraws** the outstanding demand, and
  only when the side above will read no more: it is closing, or its read
  crossed `End`. An answer
  already on its way may still arrive, and the side above drops it:
  dropping `Bytes` loses data, so only a reader giving up the stream may.
  No other `Bytes` or `Room` come without a demand.
- **`End` comes once nothing the side below holds can meet a demand:**
  with one outstanding, when it can never be met; with none, only when
  nothing is held. It comes once, and ends reading only. A read that
  crosses it is never met, and `End` does not end the demand: it stays
  outstanding until `Room` answers it, if it asked for room, or until the
  side above withdraws it, and the side above states no other demand
  meanwhile. Room may still be granted after `End`, as the stream can
  still send to a peer that only half-closed. A read larger than what is
  left before the end is never met; a side above that must see every byte
  reads by its framing (a scan, or fills no larger than its framing says
  remain), or reads what has arrived, which is met while a byte is left.
- **`Room` grants one `Send`** of at most the room asked for; the side
  above sends within it (an empty `Send` gives it up), or finishes, before
  it demands room again. **`Finish` comes with no read outstanding,**
  other than one that crossed `End`: a side below that reads another
  stream to meet a read, as TLS does, could not send the end behind it.
  io asserts neither; a side above keeps both, so that it works over any
  side below.
- **`Failed` may come at any time,** a demand outstanding or not, and
  nothing follows it. After `End` it says only that the stream can no
  longer send: what was read stands.
- **A scan that meets no delimiter within its maximum delivers exactly
  the maximum.** What that means is the side above's: a framing error for
  a line or a head, one piece of a longer text for a string scanned to its
  quote (json.md, 3.2).
- **A read of what has arrived (`Arrived`) is met as soon as the side
  below holds a byte,** by all it holds then, up to the read's maximum: at
  least one byte, at most the maximum. It is the read of bytes nothing
  frames: a child's output, a body read to its end. It waits for no more
  than one byte, so a reader of such a stream pays one delivery for what
  arrived together, not one per byte.
  - **What a delivery holds is how the bytes arrived:** the receives,
    records or chunks they came in. A side above that reads what has
    arrived makes the same of the bytes whatever the cuts; only their
    order is the stream's. In a simulated world, how bytes arrive is the
    writers' writes and the cuts the seed draws (simulator.md, 1 and 4),
    so a world stays one function of its seed.
  - **Its maximum is at least one byte** (a read of none is `Nothing`). A
    maximum past what the side below can hold is met with what it holds:
    the side below never waits to fill it. The side above's memory counts
    a delivery of its maximum.
  - **A side below that reads another stream to meet it** reads below no
    more than it needs: a read of what has arrived, where its framing
    allows one (an HTTP body within what is left of its chunk or its
    length, http.md, 3.3), or by its framing (TLS, a record at a time,
    tls.md, 3.2).
  - **Exact reads stay where the bytes are framed,** or where the side
    above must not take more than it can act on in one step: a header, a
    length-prefixed frame, a document read up to its end and never past
    (json.md, 3.2), a terminal's keys when each may be an event of its
    own.
- **A line scan (`Line`) ends at the first line end, a CR or an LF,
  whichever comes first,** for text whose lines end with LF, CRLF or CR
  alone, as an event stream's do (http.md, 4.1): a CRLF is two ends, the
  CR ending one delivery and the LF the next, and the side above pairs
  them. Otherwise it is a scan: within its maximum, or exactly the
  maximum. A `Delimiter` stays one exact sequence of bytes; "either of
  two bytes" is a read of its own.

**`Intake`** is the carry-over of a stream: the bytes the side below has
received and the side above has not yet demanded, up to a cap fixed when
it is made. It is allocated once, at the cap, and never grows.

- It appends what arrives while it has room, and reports the room left,
  so receiving stops at the cap.
- It meets a fill, a scan or a line scan as soon as it can, each delivery
  a box of exactly the demanded length, and a read of what has arrived as
  soon as it holds a byte, with all it holds up to the read's maximum. A
  scan that reaches its maximum with no delimiter delivers exactly the
  maximum: the side above sees a scan that does not end with the
  delimiter, and decides what it means.
- A fill larger than the cap, or a scan whose maximum is shorter than the
  delimiter (one byte, for a line scan) or longer than the cap, could
  never be met: the caller's bug, asserted. So is a read of what has
  arrived with a maximum of zero; any other maximum can be met.
- A scan remembers how far it has searched, so a scan for the same
  delimiter, or a line scan after a line scan, does not search the same
  bytes again.
- It says whether what it holds ends partway through a delimiter
  (`ends_partway`): a side below that fills its own intake from another
  stream, as the HTTP client does a body's, then reads a byte at a time,
  so that it never reads past a delimiter its next bytes complete.

### 7.1 Independent output reservations

A framed connection can need to send while its next read waits for a peer
that is itself waiting for that output. Its lower face supports an explicit
independent output vocabulary alongside the classic read vocabulary:

```rust
pub enum OutputDown {
    Room { right: Token, bytes: u32 },
    Cancel { right: Token },
    Send { right: Token, bytes: Box<[u8]> },
    Release { right: Token },
}

pub enum OutputUp {
    Settled { right: Token, outcome: OutputOutcome },
}

pub enum OutputOutcome { Granted, Cancelled, Failed(Fault) }
```

One admitted Room request owns one obligation. Its token is generated by
the caller, never reused for another request on that stream, and never a
wire identity. Token exhaustion prevents a new request before effects.
Its positive byte count fits the lower output cap. The lower grants only
when both those bytes and one Send record slot are reserved, including
output held in flight or stalled. Exactly one Settled terminal consumes
the demand. Granted creates an affine reservation with the same token;
one matching Send, Release, Finish or actual failure/close consumes it.
A Send moves at most the granted bytes and spends the whole reservation,
including unused capacity. An empty Send also consumes it.

There is one independent output demand or grant at a time. It can coexist
with a classic read-only Demand, which it neither answers nor withdraws.
Bytes, End and classic Room cannot settle it. Classic room demands and
grants cannot coexist with independent output ownership, and a classic
Send cannot spend an independent grant. Classic consumers keep the joined
Demand.

Cancel names a pending output demand. Cancellation wins only before the
lower has emitted its terminal. A Granted terminal already queued above
remains the winner; the caller consumes it and releases its grant while
closing. Old Cancel, Send and Release tokens cannot consume a newer right.
They are inert, including a Send with no live matching grant. A matching
Send larger than its reservation is the caller's bug, asserted before
retaining or submitting its bytes. There is no
second terminal when an already granted reservation is retired.

Actual failure settles a pending output demand with Failed before telling
classic stream Failed. Close or Abort settles it with Cancelled before the
entity's real Closed. The caller stops admitting output when closing;
requests naming an already closing or absent entity are not admitted.
Only a positive, within-cap Room on an open stream with idle output and
no competing classic room ownership creates an obligation; the lower
retains only its active cell, without a history of the caller's tokens.
Neither a grant nor Closed proves peer receipt or durable commitment.

Finish keeps its classic restriction: withdraw a read only when closing or
after End, settle any pending output demand, and finish once. There is no
general live-read cancellation and no Finish terminal. End concerns
reading alone; independent output can progress after it. Native lower
support is required: an adapter over an arbitrary classic combined Demand
cannot manufacture independent room.

## 8. Time, deadlines and randomness

- **`Time`** is nanoseconds on a monotonic clock, in a `u64`, read by the
  shell or the simulator. **`Duration`** is a span, with checked and
  saturating arithmetic. **`Wall`** is wall-clock time, nanoseconds since
  the Unix epoch, a type of its own with no arithmetic on spans, so it is
  never mixed with `Time` and never arms a deadline.
- **`Deadlines`** is a layer's own timer table, bounded: `arm` a deadline
  for a key, `cancel` it, ask for the `next` one, and `expire` those that
  have passed, each removed before its handler runs. It is a pair of
  B-trees, priced at capacity.
- **`Rng`** is a seeded generator held in a layer's state: `next_u64`,
  `below`, `between`, `chance`. It is never used for secrets.
- **`CryptoRng`** is a cryptographic generator held in a layer's state,
  ChaCha20 keyed by a 32-byte seed the shell draws from the kernel at
  start, or a world from its own seed: `fill` writes fresh bytes into a
  bounded buffer. Whatever must not be guessed is drawn from it: secrets,
  OAuth states and PKCE verifiers, sign-ins' secrets. Each generator is
  made from the seed and a stream number its owner gives (ChaCha20's
  nonce), distinct for every generator of a process, so several drawn from
  one seed never yield the same bytes.

## 9. The environment

**`Env<L>`** is what a step reads besides its own state: `now`, `wall`,
and its layer's `limits: L`. A step gets it behind a shared borrow.

## 10. Testing

Step tests drive each container through its operations, at and past its
capacity: a full queue, a stale handle, a slot whose generation would
wrap, a refusal at the entrance, a deadline that fires as it is
cancelled, a scan cut at every byte. The step tests are `src/tests.rs`, a
module for each area of this document under `src/tests/`: handles,
containers, bytes, streams, and time.

The byte search is also compared with a naive one, and the intake with a
plain reference, over random cases drawn from a seed: a few hundred in
the step tests, and 20,000 from the same seeds in the fuzzy suite, in
`tests/lib` (testing-strategy.md, 8).

SHA-256 is held, in the step tests, to FIPS 180-4's short examples (the
empty message, `abc`, the 448-bit and 896-bit messages) and to messages
at a block's edges (55, 56, 63, 64 and 65 bytes) against the one-shot
form, each fed whole and at a few splits. The fuzzy suite adds the
million `a`s, sweeps every length from 0 to 200 bytes, each split at
every point, and adds 20,000 random messages, each split at a pair of
points drawn at random. HMAC is held to RFC 4231's test cases, each fed
whole and at a few splits in the step tests, and split at every point in
the fuzzy suite; `equal` to equal and unequal inputs of the same and of
different lengths. State digests are held to their contents: two
containers that reach the same contents by different histories (a map
filled in two orders, a queue that wrapped, a slab whose slots were
reused in the same order) digest alike, a change to one entry changes
the digest, and an integer written as `usize` digests as the same `u64`.
Their step tests are `src/tests/sha256.rs` and `src/tests/digest.rs`.

The request table's step tests are `src/tests/requests.rs`; its independent
obligation model, replay and seeded sweep are in `tests/lib/src/request_table.rs`
and `tests/lib/tests/{request_table,fuzzy_request_table}.rs`.

Each `worst_case` is checked against the counting allocator (testing.md,
5), in `tests/lib/tests/memory.rs`, in the focused suite. Every container
is built at capacities from 0 to 300, filled to them, emptied and filled
again, every operation a step of the meter, and what it held of its own is
never more than `worst_case(capacity)`. What an item owns is its owner's
to count, so the items own no heap; they come in several sizes and
alignments, up to 64 bytes, as a container's price depends on both. An
input moved into a step was counted by whoever made it; the step's bound
covers what it keeps and what it allocates (testing.md, 5). What an
operation hands out (an item taken, a delivery, a list moved into a box)
is no longer the container's, and the meter takes it off.

- **A slab, a queue, a list, a stack and an intake** are allocated once,
  at their capacity, and hold exactly their worst case from the start.
- **A map, a set and a deadline table** are filled in order, each leaf
  left behind holding six entries, then thinned to five a leaf, the fewest
  a leaf holds: a tree within a few nodes of the most its capacity allows,
  and driven at random. Their
  worst case prices every node as an internal one, the larger kind, so at
  a capacity of 12 or more they hold from about 30% of it, for small
  entries, whose nodes are mostly edges, to 96%, for 64-byte keys and
  values. Below 12, a tree is at most one leaf, and the worst case counts
  up to three nodes.

## 11. The journal

`Journal<W, O>` is a bounded commit barrier for writes and outputs. It is
generic over both and inspects neither. It belongs in lib because it is a
data structure, like a queue; a service does not hand-roll one
(programming-model.md, 10.2).

- **Admission by counts:** the owner states a decision's worst-case room
  for writes and held outputs before changing its state. `takes(room)`
  leaves the journal unchanged, and refuses unless the room fits the
  commits in flight, writes per commit, and outputs held. A decision
  reserves that room; accepting it returns unused room.
- **One decision:** a `#[must_use]` value, not `Clone`, collects its
  writes and held outputs. Each addition past its reserved room is
  refused and returns ownership to the caller. Such an overrun is the
  owner's bug: accepting the decision stops the journal.
- **Commit and tags:** accepting a decision makes at most one numbered
  commit, holding all its writes. Numbers begin at one and increase in
  order. A decision without a write makes no commit. Each held output
  follows that decision's commit, or the last commit made if there was
  no write; it leaves only once that commit is durable. A commit answer
  makes that commit and every earlier one durable. Answers outside the
  order the store sends commits are the owner's bug and stop the
  journal.
- **The door and release:** `now(output)` admits an untagged output that
  decides nothing and is ready to leave at once. These outputs leave in
  their own order, within the door's capacity. Held outputs leave in
  the order they were collected, after their tags are durable. Each
  release call moves at most its configured count. The journal does not
  arm a deadline or advance commits by itself.
- **Failure:** when a commit fails, no output tagged with it or a later
  commit leaves, and the journal reports that it has stopped. It admits
  no more decisions or outputs.
- **Memory:** capacities are fixed at construction. `worst_case` prices
  the journal's containers and bookkeeping from its limits with checked
  arithmetic, as other lib containers do. Values it holds own their
  own payload bytes; their owner prices those separately.

The journal uses generic types and lib's containers. It defines no
traits, takes no closures, and uses no `dyn` (programming-model.md,
10.2).

## 12. The request table

`RequestTable<O, R>` keeps bounded keyed requests until their answers are
final. It is generic over the asker's owner value and the requests it
holds, and inspects neither owners, requests nor answers. It belongs in
lib because it is a data structure with a lifecycle, like a queue or the journal; a service does not hand-roll one
(programming-model.md, 10.2).

- **Owner:** each request keeps the asker's own value, of a `Copy` type,
  held and handed back with progress and a final answer, never read. An
  asker composing several parts names each owner with a variant of its
  own enum, carrying the part's token, or naming a restored request whose
  asker is gone. It can then route by the variant to the part that asked
  without keeping a routing table of its own. The owner is not required
  to be a token: a token's holder never interprets it (section 2).
- **Keys:** a key is drawn from the seed given at construction, or given
  by the asker (a digest of a name, say). A drawn key is never reused for
  another request. A given key is the asker's: the table admits it while
  it holds no entry under it. An entry holds its key from its admission
  to its reclaim, retired or not. A restored request keeps its key.
  Exhausting the key space refuses admission (`Full`) rather than reusing
  a key.
- **Scope:** each request keeps an opaque number its asker gives. Only
  requests whose scope is confirmed may be sent.
- **Kept before sent:** each admitted request emits its record for the
  asker's store: its key, scope, request and first sending's wall time,
  if it has been sent. Its first sending updates that record. Records
  leave before the sends they cover, in order; each take emits at most
  its configured count. Retiring a request emits its erase.
- **Not kept:** a request whose record the asker's store did not write
  is retired by its key and the number of its admission (`not_kept`),
  whatever its state: parked, ready, in flight or retrying. The asker hands a send over only once the records taken
  before it are stored, so such a request was never sent. It is never
  sent; an answer for its attempt changes nothing. Its erase is emitted,
  as any retired request's, since an earlier write may hold its record.
  Its owner value is handed back, as a final answer's, with no progress,
  and the asker answers it itself. A key that names no request, or a
  retired one, hands back nothing. So does an admission number earlier
  than the one the key holds: a stale "not kept" never retires a later
  holder of the same given key.
- **One attempt:** a request has at most one attempt in flight. Each
  attempt has its own fresh token, independent of the owner value.
  An answer for a superseded or retired attempt changes nothing.
- **Envelopes:** the protocol layer says which kind of answer came.
  *Final* retires the request and hands back its owner value, so the
  asker routes the answer unopened. *Again* schedules a retry under the
  same key. *Lost* does likewise while the link is up, and parks while
  it is down. *Signed out* parks until the request's scope is confirmed
  again. Retry spans start at a configured first span, double to a
  configured most, and are jittered from the seed.
- **Parking and resuming:** a down link parks requests and sends
  nothing. An up link resumes the confirmed scope's parked requests;
  confirming a scope permits only that scope's requests. Changing the
  scope parks every other scope's requests.
- **Retention:** the asker tells the table how long answers are kept.
  A request first sent longer ago than that span less the configured
  margin is never sent again: it is retired, erased, and its owner told
  that its outcome is unknown. The saved first sending is a wall time,
  so the cutoff survives a restart; retries and the cutoff use monotonic
  deadlines computed from the supplied clocks (programming-model.md,
  section 9). A restored request is not sent until retention is known.
- **Restoration:** records are admitted at startup within the same
  limits, keeping their keys and first sending times. They begin parked
  until the link is up and their scope is confirmed. The asker supplies
  their owner values and declared sizes again.
- **Progress:** changes to in flight, retrying, parked or unknown leave
  with the owner value, for display. The table never reads the answer
  or decides how progress is shown.
- **Admission and memory:** capacities are fixed at construction. `ask`
  and `restore` refuse with a reason, handing back the request and
  changing nothing: `Full` (past the held-request count or the sum of
  owners' declared bytes, or no key left to draw) or `Held` (the key is
  held). A restore of a held key is refused as `Held`. Each admission, by
  `ask` or by `restore`, has a number, which they return with the key.
  Numbers increase with each admission. `not_kept` names the key and the
  number; every other call stays by key: an asker holds one admission per
  key at a time. The table does not measure a request. `worst_case`
  prices its containers, owner values and bookkeeping from the limits
  with checked arithmetic; the owner prices the payload bytes it
  declares separately.

The asker decides each request's scope and contents, supplies its owner
value and declared size, and tells the table the link, confirmed scope,
retention and clocks. The protocol layer classifies envelopes; final
answers go directly to their owner. The table decides keys, attempts,
backoff, parking, retention cutoffs, store records and progress.

The table uses generic types and lib's own containers. It defines no
traits, takes no closures, and uses no `dyn` (programming-model.md,
10.2).

## 13. Not built yet

What of this document is not built is `STATUS.md`'s (its row in section 2,
and section 3).

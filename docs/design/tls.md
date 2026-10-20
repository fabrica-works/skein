# TLS

Provisional, 2026-10-04, revised 2026-10-20. The design of
`skein-tls`: a TLS stream machine over rustls, ciphertext below and
plaintext above. It is the one exception of programming-model.md,
section 3: the only step code that depends on a crate from outside skein
and the service, and the only step code that is not deterministic.

## 1. In one page

- **A stream machine.** Ciphertext comes from the stream below; plaintext
  goes up as a stream to the machine above, which cannot tell it from a
  socket. TLS meets lib.md, 7's contract twice: as the side above of the
  ciphertext, and as the side below of the plaintext.
- **It wraps rustls's unbuffered connection,** which does no I/O, keeps no
  clock, and builds without std, with ring for its cryptography.
- **Records by fills.** It reads a record at a time, its header then its
  body, and hands each to rustls, which deciphers it in place.
- **The client comes first;** a server side comes when a service
  terminates TLS itself.
- **Not deterministic,** so the replaying tiers run in plaintext, and TLS
  is tested on its own, against rustls's own server in memory.

## 2. In skein

`skein-tls` depends on lib and rustls, pinned, without its default
features, with ring (programming-model.md, 2.1). A service's protocol
layer stacks it between io's socket and the machines above it (http.md,
2). It has its own `Limits` and `worst_case`, each entry point declares
its `MAX_OUT`, and it says what it waits for:

```rust
// skein_tls::client, the machine
pub fn up(client: &mut Client, env: &Env<Limits>, ev: stream::Up,
          above: &mut Queue<Event>, below: &mut Queue<stream::Down>)
pub fn down(client: &mut Client, env: &Env<Limits>, rq: Request,
            above: &mut Queue<Event>, below: &mut Queue<stream::Down>)

// made at startup, from configuration (shell.md, 6)
Config::from_der(roots: &[Box<[u8]>], alpn: &[&[u8]]) -> Result<(Config, Parsed), Refusal>
Config::new(roots: RootCertStore, alpn: &[&[u8]]) -> Result<Config, Refusal>
Name::new(text: &str) -> Option<Name>
Name::server(&self) -> Server<'_>   // Dns(&str) or Ip(core::net::IpAddr)
Client::new(&config, name, &limits) -> Client
```

Whoever stacks the client checks at startup that its largest demands fit
the stream below, `client::LARGEST_READ` and `client::largest_room`
against the socket's intake and output caps, and that the machine above
fits it: `Limits::read` and `Limits::send` hold the HTTP client's
`largest_read` and `largest_room`.

## 3. The machine

### 3.1 The side above

```rust
pub enum Request {
    Handshake,                  // first and once: Ready or Failed answers it
    Stream(stream::Down),       // the plaintext stream
    Close,                      // Closed answers it
}

pub enum Event {
    Ready(Agreed),              // for Handshake: the version, and the protocol ALPN agreed
    Stream(stream::Up),         // the plaintext stream: Bytes, Room, End, Failed
    Failed(Error),              // the connection failed, after the stream heard it
    Closed,                     // for Close: terminal
}
```

- **`Handshake` starts the connection,** in its step: rustls's connection
  is made then, its `ClientHello` written, with the step's `env.wall` as
  the time it checks certificates against. The plaintext stream may be
  demanded before `Ready`: its demands wait for the handshake. A
  connection that holds a call while it handshakes (llm-connection.md,
  section 5) makes the call on `Ready`.
- **The plaintext stream** keeps lib.md, 7, and holds the side above to it
  as io does: each demand answered at most once, from the client's intake,
  as lib's intake meets it (exactly, or for a read of what has arrived,
  what the intake holds up to its maximum); `End` once the server's
  `close_notify` was read and nothing held meets a demand; `Room` once
  room for the records of what the side above demanded came below. A
  `Room` grants one `Send`, as io's does (io.md, 3.3), and a demand of
  room comes only once the last grant was sent within, by a `Send`, empty
  or not, or given up by `Finish`. A read that crosses `End` is never met,
  and its demand stays outstanding, as io's does: until `Room` answers it,
  if it asked room, or until the side above withdraws it. A side above
  that writes on after the end waits for that `Room`, or withdraws the
  demand and then demands room alone; room that came below for the demand
  withdrawn is handed on to the next or asked for again (3.2).
- **`Finish`** sends `close_notify`, then finishes the stream below. It
  comes with no demand outstanding, unless a read that crossed `End`: a
  read the client is still reading below for would hold `close_notify`
  behind it, against a server that waits for the end of what it is sent
  before it answers. The side above may read on after it.
- **`Close` ends the client in any state.** Once the handshake is done,
  and unless the stream below was finished, it writes `close_notify`
  (unless it was written), sends what TLS owes below within room it
  demands, then answers `Closed`: it withdraws a demand that only reads,
  and keeps one for room, as an answer of room may be on its way and what
  TLS owes goes within it. Otherwise, or once the stream below fails, it
  withdraws what it demanded and answers `Closed` at once. It does not
  close the stream below, which its owner closes.
- **What it waits for**, `waiting()`: `Handshake` (not started),
  `Handshaking` (the server's records, or room for the client's: the
  connection arms its handshake deadline on `env.now`), `Room`, `Bytes`
  (the server's records, for a demand of the side above), `Above`, `Close`
  (it failed) and `Nothing`.

### 3.2 The side below

- **A record at a time:** a fill of its 5-byte header, then one of the
  length it gives, each appended to the records held and handed to
  rustls, which judges a header as it comes (its type, its version, a
  length within what TLS allows) and deciphers and joins records in
  place. `LARGEST_READ` is TLS 1.2's longest body, 2^14 + 2,048 bytes.
- **The plaintext carry-over** sits in the client's own `lib::Intake`,
  under a cap of `Limits::read` and one record's plaintext beside it
  (programming-model.md, 4.3): a demand the intake does not meet always
  leaves room for the next record, which is read only then. rustls holds
  no plaintext between steps.
- **It reads only what is needed:** the server's records while the
  handshake runs, and after it, only for a demand of the side above's the
  intake does not meet. A read stated alone, ahead of a demand, would
  hold a demand that room for the side above's next `Send` could not join
  (http.md, 3.1).
- **At the stream's end,** a read the client has outstanding crosses it
  and is never met, and io keeps the demand outstanding (lib.md, 7). The
  client reads nothing more: it withdraws a demand that only read at
  once, and one that asked room too stays until `Room` answers it. Either
  way it is then free to demand room after the end, for a peer that only
  half-closed still reads.
- **Room, in one demand with the read:** for what TLS owes (a flight of
  the handshake, `close_notify`), alone and first; or, once the handshake
  is done, for the side above's demand of `n` bytes of room, `room_for(n)`:
  its records, 29 bytes each at most (TLS 1.2's AES-GCM), and 54 bytes of
  slack for what may go before them: in TLS 1.3, the key update rustls
  asks for as its keys reach their limit and a second, as the first's own
  record reaches it, 27 bytes each (the one that answers the server's
  comes alone, as it starts the keys over); in TLS 1.2, the refusal of a
  renegotiation, 31. What TLS owes that arose
  while the side above held its grant goes in the same `Send`, in front:
  records go in the order rustls sealed them. Room that comes for a
  demand the side above withdrew meanwhile is held for no one: handed on
  at its next demand of room if it takes that demand's records, or given
  up, so that room is asked for again, as a grant not sent within gives
  way to the next (io.md, 3.3).
- **The ciphertext held** is one box of `Limits::records`, not an intake,
  as rustls reads and writes one slice: a record at least, and the
  records of a handshake message that spans several, the server's
  certificates among them. A message longer than that fails, `TooLong`.

### 3.3 Ends and failures

| What happens | The plaintext stream hears | `Failed` |
|---|---|---|
| the server's `close_notify` | `End`, once what is held meets no demand | |
| the stream below ends without it, after the handshake | what is held, then `Failed(Invalid)` | `Truncated` |
| it ends before or during the handshake | `Failed(Invalid)` | `Truncated` |
| a record fails to decrypt | `Failed(Invalid)` | `Decrypt` |
| the server's certificate is refused | `Failed(Invalid)` | `Certificate(why)` |
| anything else wrong with the server's records | `Failed(Invalid)` | `Protocol`, `TooLong` |
| a `HelloRetryRequest`'s cookie too long to echo within `FLIGHT` | `Failed(Invalid)` | `FlightTooLong` |
| the server sends a fatal alert | `Failed(Reset)` | `Alert(description)` |
| the stream below fails | `Failed(fault)`, at once | `Stream(fault)` |
| rustls fails for a reason of its own | `Failed(Other)` | `Other` |

- **A truncation is never taken for an end** (RFC 8446, 6.1): it fails the
  stream as invalid, the peer broken or hostile, so that a body read to
  the end of the stream is not cut short unseen. What was deciphered
  before it was authenticated, and still meets demands first.
- **Why a certificate is refused:** `Expired` and `NotYetValid` at the
  wall time the handshake started at, `Name`, `Issuer` (no root trusted),
  or `Invalid`.
- **After a failure,** nothing more is sent, not even rustls's alert; the
  client waits for its close, which withdraws what it demanded.

### 3.4 Configuration

Configuration is data (shell.md, 6). The shell reads the roots a service
trusts, and hands them over with the protocols it offers by ALPN, in
order of preference: a configuration every connection shares, rustls's,
with both versions of TLS, no client certificate, and no session
resumed, as a session cache would be state shared between connections.
It refuses roots of none, and protocols empty, longer than 255 bytes, or
of more than `ALPN` (256) bytes in all. Each connection gets the server's
`Name`: a DNS name, sent by SNI, or an IP address. Whoever holds a
name reads it back with `Name::server`, as what it is, borrowed:
`Server::Dns`, the DNS name's text as it was given, or `Server::Ip`,
core's `IpAddr`. A name is one or the other, and reading it allocates
nothing; an address is never handed over as text, so that its reader
writes it in its own form.

- **From DER bytes.** The shell reads the roots as bytes and needs no TLS
  crate (shell.md, 6.2): the certificates of the machine's bundle, each
  taken out of its PEM as DER, or the one of a DER file. `Config::from_der`
  takes them as they came, and builds rustls's root store from them with
  its own parser. A certificate it cannot parse, or whose key it does not
  support, is skipped and counted, never a failure: a distribution's
  bundle may hold one, and the rest still serve. It answers `Parsed`, how
  many it took and how many it skipped, for the shell to say once on
  standard error, and refuses with `Refusal::Roots` when it took none.
- **`Config::new`** takes a root store already built, for tests that make
  their own.
- **The roots are counted once.** Every connection shares the
  configuration, so its root store is in no connection's worst case. The
  service counts it once, from the bounds the shell read the roots within
  (shell.md, 6.2): their count and each one's bytes, at a factor measured
  as rustls's other parts are (section 5).

### 3.5 Limits and the worst case

```rust
pub struct Limits {
    pub read: u32,      // the most the side above demands at once; the intake holds it and a record's plaintext
    pub send: u32,      // the most room it demands at once, in plaintext
    pub records: u32,   // the ciphertext held: a record at least (MAX_RECORD), and a message that spans several
}
```

- **`worst_case(&limits)`** is the client's own: the intake, the records
  held, `FLIGHT` (2,048 bytes) for what TLS owes, and the delivery it
  reads, at most `LARGEST_READ`, which a step that encrypts holds as much
  of rustls's instead; and rustls's, measured (section 5): 16 KB of its
  own state, and the decoded form of the longest handshake message,
  20 bytes for each of its bytes, which the fewer of `Limits::records` and
  rustls's 64 KB (`MAX_HANDSHAKE`, 65,539 with its header) bound: a server
  sends a message whole within the records held. The factor is a hostile
  server's: a message of the shortest entries a list allows, 3 bytes each
  in TLS 1.2 and 5 in TLS 1.3 (empty certificates, names of a byte),
  decodes to an element of 24 or 48 bytes each, in a list that grows by
  doubling and is copied once, and a chain is kept. `None` for a read or
  room of nothing, or records shorter than one.
- **`largest_room`** is the larger of `room_for(send)` and `FLIGHT`.
- **`UP_MAX_OUT`** is two events and three requests: `Ready` or an answer,
  or the stream told it failed and `Failed`; below, what TLS owes sent or
  a read the end crossed withdrawn, the stream finished, and the next
  demand. **`DOWN_MAX_OUT`** is two and two.

### 3.6 The native face

`client::native` is the client for a side above that must write while its
read waits: a framed connection over TLS, whose writing never waits for
its reading (channel.md, 7; io.md, 3.3). It keeps lib.md, 7.1's
independent output on both its sides: on the plaintext above, and on the
ciphertext below, read-only demands beside output rights. It is built on
the classic client's engine: its session, its buffers, its reading of
records (3.2) and its answers to reads (3.1). The two faces differ in how
room is asked for and granted, how they finish and how they close.
Whoever stacks the client picks the face by its constructor, and it stays
fixed. The stream below must keep lib.md, 7.1 itself: no adapter over a
stream that takes only the classic joined demand gives this
independence.

- **Requests and events.** Above, `Request::Client` carries the classic
  `Request`, whose stream demands only read: a demand of room or a `Send`
  is a bug, asserted before it has any effect. `Request::Output` carries
  an output right's requests (lib.md, 7.1). `Event::Client` carries the
  classic `Event`, `Event::Output` an output right's terminal, and
  `TransportClosing` and `TransportClosed` say that the stream below is
  closing or closed. Below, `LowerRequest` and `LowerEvent` carry the
  same pair on the ciphertext, and `LowerEvent::Closed` says that the
  owner closed the stream below.
- **Four cells,** each with at most one obligation: the read above, the
  output right above, the read below and the output right below. The
  client makes the tokens of its rights below: checked, never reused and
  never wrapped. When it needs a token and none is left, the connection
  fails, `Other`; while it closes, it drops what it owes instead. It
  keeps no history of rights settled.
- **An output right above** is admitted only when its bytes are positive
  and within `Limits::send`, no other right above is pending or granted,
  and the client can still write; any other is a bug, asserted. It may be
  asked for during the handshake, and waits: it is granted only after
  `Ready`, and only once a right below for its records was granted,
  backed by bytes and one `Send` slot. That right below stays reserved
  for it until the side above's matching `Send`, empty or not, its
  `Release`, a `Finish` that is allowed, or the connection's close or
  failure spends it.
- **Identity before size.** A `Cancel` that names no pending right, or a
  `Send` or `Release` that names no granted right, does nothing; one that
  names the granted right with more bytes than it was granted is a bug.
- **Room below.** The right below for the side above's `n` bytes asks for
  `room_for(n) + FLIGHT` (`native::room_for`): their records, and room
  for everything TLS may owe in front of them. `native::largest_room` is
  that for `Limits::send`. What TLS owes may grow while the side above
  holds its grant (a key update, the refusal of a renegotiation): it goes
  in front of the side above's records, within the grant held, never on a
  right of its own that would wait behind that grant. A right below that
  TLS asked for its own output sends as much of what it owes as fits, in
  order, and asks for a new right for the rest.
- **`LowerLimits`** are the stream below's configured caps: `read`, at
  least `LARGEST_READ`; `output`, at least `native::largest_room`;
  `sends`, its queued `Send` slots, at least one beside the one in flight
  or stalled. `Client::new(config, name, limits, lower)` checks them
  before it allocates or clones anything, and answers `None` when the
  stream below cannot honour them. Credit short at a given moment is not
  refused: the right waits for it. Bytes and slots are checked apart;
  independence needs no second queued slot.
- **Settling.** A `Cancel` from the side above settles its pending right
  `Cancelled`, once. A right below that the client cancelled keeps its
  token until the stream below's own terminal for it comes. A `Granted`
  that went up stays the right's terminal, whatever follows. On a
  failure, a pending right above settles `Failed` before the plaintext
  stream's `Failed` and the client's `Failed`, and once, even when the
  right below's `Failed` and the stream below's `Failed` both arrive. A
  `Ready` that went up stays, even when running out of tokens fails the
  connection later in the same step.
- **The most an entry point emits:** `native::UP_MAX_OUT` is four events
  and three requests, `native::DOWN_MAX_OUT` three and three. The classic
  face's are section 3.5's.
- **The owner closes the stream below.** The client keeps no timer and no
  descriptor: its owner closes or aborts the stream below, says so with
  `LowerEvent::Closed`, and checks that it is for this client's stream
  below, not an earlier one.
  - A `Cancelled` from below that the client did not ask for makes the
    stream below unusable: no new demand or right goes below, a pending
    right above settles `Cancelled`, once, and `TransportClosing` goes
    up, unless a `Close` is pending. The owner then closes the stream
    below, and asks the client to `Close`. A cancel the client asked for
    spends only its own right.
  - `LowerEvent::Closed` comes once every right below has settled; one
    not settled then is the owner's bug. It first settles a pending right
    above `Cancelled`, before `Ready` too, and for a right asked for after
    the client cancelled its own. Without a `Close` pending,
    `TransportClosed` goes up, and ends the handshake, the reads and the
    connection. With a `Close` pending, `Closed` answers it, once, after
    every right below settled. A `Close` that comes after
    `TransportClosed`, such as one the owner sent on `TransportClosing`,
    does nothing, and no `Closed` goes up; on the classic face, a `Close`
    after `Closed` is a bug.
  - The client never makes up what the stream below did not say: no
    `Failed`, no `End`, no bytes and no error of the server's.
- **`Finish`** comes only after `Ready`, with no read above but one that
  crossed `End`, and no output right pending; it spends a grant held.
  After it, the side above reads no more. This is narrower than the
  classic face, which takes a `Finish` before `Ready` and reads after it.
- **A read withdrawn after `Ready`,** by the side above or by its
  `Finish`, may already be answered below. The client keeps the size of
  the `Fill` withdrawn, and drops the `Bytes` that answer it, unread,
  before it deciphers or seals anything; `Bytes` of another size is a
  bug. The withdrawal needs no answer of its own, and the `Bytes` may
  never come. No read is admitted after it, and `LowerEvent::Closed`
  forgets the size. A read withdrawn above during the handshake leaves
  the handshake's own reads below going, until `Ready`.
- **What it waits for**, `waiting()`, is section 3.1's, with `Close` also
  after `TransportClosing`, and once a close has sent all it owes and
  waits for the owner's `Closed`.
- **Memory.** `native::worst_case` is the classic `worst_case`, with its
  delivery of `LARGEST_READ` replaced by the larger of `LARGEST_READ` and
  `native::room_for(Limits::send)`, since the bytes read are dropped
  before a record is deciphered or sealed, plus the client's own size,
  its cells included. What TLS owes and what it sealed are counted
  together, before they go below and on an error before they do;
  allocations that overlap are counted as their sum. The plaintext the
  side above sends stays the caller's, counted there. The buffers,
  rustls's scratch, io's queues, the boxes in flight and the step's
  queues are each counted once. The whole envelope reserved and
  `FLIGHT`'s box stay counted even when less is sealed or owed.

## 4. The exception

- **rustls and its dependencies,** ring's among them, are the only step
  code from outside skein and the service: rustls-webpki,
  rustls-pki-types, ring, untrusted, getrandom, libc, cfg-if, subtle,
  zeroize and once_cell. ring builds with `cc`, without cmake and without
  std; aws-lc-rs, rustls's default, would bring a C build skein does not
  need. rustls is pinned (0.23.41), the rest by `Cargo.lock` (ring
  0.17.14).
- **Not deterministic:** ring draws its randoms and keys from the kernel,
  through `getrandom`: the only kernel calls step code makes, when the
  handshake starts and as it runs, and when it signs (below). That is the documented exception;
  nothing else impure comes in.
- **`getrandom` on Linux** makes the getrandom(2) system call. Where it
  cannot, a kernel older than 3.17 (below the shell's floor, shell.md, 4)
  or a seccomp filter that refuses it with `EPERM`, it falls back to
  `/dev/urandom`, once a poll of `/dev/random` says the pool is ready: it
  opens the file the first time it draws, under a lock of its own, and
  keeps the descriptor open for the life of the process, a file io knows
  nothing of, read with plain `read` calls. getrandom's
  `linux_disable_fallback` feature would make that an error instead;
  skein does not set it.
- **No clock:** rustls asks a time provider for the time it checks
  certificates against. Each connection's configuration carries one that
  answers the `env.wall` of the step that started its handshake, read
  once; the handshake deadline bounds how far it lags the wall.
- **An ID token's signature:** ring's RSA verification also checks an
  OAuth ID token's RS256 signature (oauth.md, section 2.2), inside
  skein-tls; it draws no randomness and reads no clock. The fake issuer's
  signing draws ring's blinding randomness, and its signatures do not
  depend on it.
- **An App's signature:** `sign_rs256(key, message) -> Option<[u8;
  256]>` signs a GitHub App's JSON Web Token (forge-connection.md,
  section 5.2; forge-github.md, section 8.1) with ring's RSA PKCS#1 v1.5
  over SHA-256, inside skein-tls, so ring stays behind one crate. The
  key is PKCS#1 `RSAPrivateKey` DER of 2048 bits alone, at most
  `RS256_KEY_BYTES`, the largest such key's DER; another, or one ring
  refuses, is none. It reads no clock: the caller gives the message,
  times and all. It draws ring's blinding randomness from the kernel, as
  a handshake does, and the signature does not depend on it, so a fake
  forge checks it with `verify_rs256`. A key is parsed at each signing,
  never kept by skein-tls.
- **What rustls's API makes the crate use:** an `Arc` of the shared
  configuration, a trait object for the time, a hand-written `Debug`
  for its connection, `Vec`s in the configuration, and arms for the
  states rustls may add to its `non_exhaustive` enums, unreachable for
  0.23's; each in place, with a scoped `expect` where a lint objects.

So the simulator and the protocol worlds run in plaintext, and nothing
else in step code may follow it.

## 5. Testing

TLS is tested on its own (testing-strategy.md, 4.4), against rustls's own
server, in memory: rustls's buffered connection over byte slices. Its
certificates are fixtures (`tests/tls/fixtures`, made by `make.sh` with
OpenSSL): a root, an intermediate, the server's certificate for
`skein.test` and `127.0.0.1`, one of 40 KB for 1,500 names, one that a
root no one trusts signed, one signed by itself, one of a CA, and one
for client authentication only, all valid from 2026 to 2036, checked at
a wall time each test chooses.

- **Step tests** (`crates/skein-tls/src/tests/`): the limits and what they
  price, the configuration's refusals, names, the faults, the bytes held
  for rustls; and the machine fed by hand: its first flight, records no
  server writes (not TLS, past TLS's lengths, an alert, a message longer
  than the records held, a retry whose cookie no flight holds), the
  stream ending and failing before and during the handshake, closes, and
  the bugs it asserts: limits it cannot honour, a second handshake, the
  stream before it, a second demand, a read or room past the limits, a
  `Send` without room, a `Finish` with room or a read outstanding, a
  close after close, and an answer no demand asked for.
- **Machine worlds** (`tests/tls`, `skein-tls-world`): one client from a
  seed between the server's ciphertext, cut at random, room granted late,
  ended or failed, and a user that reads with demands of every shape,
  slowly, writes within the room granted, finishes, and closes as the
  client waits for the handshake, room, bytes, the side above, or its
  close after a failure. They check both streams' contracts as they go,
  and each run against its scenario: the plaintext each side received,
  the server's ending (`close_notify`, a truncation, a corrupted record,
  nothing), the server's key update in TLS 1.3, certificates refused at
  the wall time handed in, a chain longer than the records held,
  `close_notify` sent on a finish or a close, the server reading every
  record the client sends, and, left alone, the end the server makes
  reached and the request all sent, so that a client that waits for ever
  fails. Focused tests aim at one
  outcome each: each version, a retry, ALPN, an alert, a certificate
  refused for each reason, every split of the ciphertext a byte at a
  time, a slow reader that fills the stream below, the same closes, a
  read that crosses the end held until withdrawn, room held for a demand
  withdrawn, a corrupted record read after `close_notify` went, the key
  updates rustls starts at its keys' limit, and a renegotiation request,
  sealed with the server's keys taken out, refused alone or in front of
  the side above's data. Nothing reaches `Other`, rustls failing for a
  reason of its own.
- **Not replayed:** what rustls draws from the kernel changes a record's
  length, and with it where the pieces fall. The worlds assert only what
  does not depend on it.
- **The machines stacked** (`tests/tls/tests/stack.rs`): the HTTP client
  over the TLS client, as a connection routes between them, a call made
  on `Ready`, a body uploaded in records, a response read by length and
  by chunks, and one read to the end of the stream: whole once
  `close_notify` ends it, failed `Stream(Invalid)` when the stream is cut
  without it.
- **Memory** (`tests/tls/tests/memory.rs`, with the counting allocator):
  every call of an entry point a step of the meter. The server runs on
  the same thread, between the client's steps; its heap and the
  harness's are measured with a span around what is done between steps,
  and each step is checked against the bound plus that heap. This is how
  rustls's part was measured, past the client's buffers: 6 KB for a
  handshake, 10 KB with the longest ALPN list, 21 KB in a step that
  deciphers or encrypts a record of 16 KB, and 82 KB for the 39 KB chain,
  held twice. A hostile server's messages cost more: sent first, in the
  clear, and decoded whole before rustls judges them, a chain of empty
  certificates and a request of names of a byte reach 18 times their
  length (298 and 303 KB for messages of 16 KB, 1.18 and 1.20 MB for
  64 KB), unknown extensions 3.5 times; a chain padded with empty
  certificates, which rustls accepts and keeps, 20 times in TLS 1.2 (1.30
  MB past the buffers) and 18 in TLS 1.3. That chain reaches 97% of the
  worst case. Dropped, the client frees what it held.
- **The fuzzy suite** (`tests/tls/tests/fuzzy_world.rs`): 400 runs of
  scenarios and neighbours drawn from each seed: each version, retries,
  ALPN, the big chain, the server's key updates, each ending and each
  certificate refused. It asserts that what it draws fell: no failure,
  the stream's own failure, a truncation, a record that fails to
  decrypt, among them one read after `close_notify` went, a message
  longer than the records held, each reason a certificate is refused,
  each version, a protocol agreed, a retry, records read past a key
  update, a close in each wait the worlds reach, and each oddity of the
  neighbours. It draws no alert, no protocol error and no cookie too long
  to echo: the focused tests reach those. It stands in for the fuzz
  target (`fuzz/`, fed records under every demand), which waits for a
  nightly toolchain.
- **The native face** (section 3.6): step tests in
  `crates/skein-tls/src/client/native/tests.rs`; a pair against a rustls
  server in memory (`tests/tls/tests/native_output.rs`), the same over
  io's sockets on the simulator, and memory against the worst case
  (`native_memory.rs`); a fuzzy run of short and reordered reads and
  writes (`fuzzy_native.rs`). rustls 0.23.41 folds the TLS 1.3 key
  updates asked for while the side above holds its grant into one: the
  tests ask for 32, and see one key update in front of the largest
  plaintext `Send`. A TLS 1.2 `HelloRequest` while the grant is held is
  refused in a sealed record, and that refusal and a later `close_notify`
  go below in order, across an older, smaller grant of TLS's own. The
  memory tests tell the client's bounds apart from the limits of what
  rustls's server lets a peer send. Nothing is replayed (above).

## 6. Decisions

- **The handshake starts with a request,** `Handshake`, not with the
  client: `Client::new` draws nothing from the kernel, and the step that
  starts it reads `env.wall`.
- **Certificates are checked at the wall time of that step,** carried by
  a time provider of each connection's configuration, not one read at the
  moment rustls verifies: that would take a cell the step writes `env.wall`
  into, interior mutability the model rules out, to gain no more than the
  handshake's duration.
- **ring, not aws-lc-rs:** it builds without cmake or std, and is pure
  enough to pin.
- **No read ahead of a demand,** and room for the side above's demand
  joined to the read: otherwise a read stated alone holds the demand that
  room for an upload needs, and the client waits for a server that waits
  for it.
- **What TLS owes goes first and alone,** within room it demands for it,
  one `Send` a grant, as io grants one; what arose while the side above
  held its grant goes in front of the side above's `Send`, within
  `room_for`'s slack.
- **No coalescing.** Each `Send` of the side above is sealed into records
  in the step that takes it and goes below in one `Send`; a small `Send`
  costs a record of its own. The client never holds plaintext back to
  fill a record: holding it would need a moment to flush, a timer the
  machine does not keep (programming-model.md, 4), or a flush the side
  above would have to ask for, and rustls would hold plaintext between
  steps (3.2). A side above that wants full records sends pieces of a
  record's plaintext, as skein-llm-connection's owner does
  (llm-connection.md, 7).
- **A read that crosses the end stays outstanding,** both ways, as
  lib.md, 7 says and io does, until `Room` answers it or it is withdrawn:
  the client withdraws its own, as it reads no more after the end, unless
  it asked room, and holds the side above to the same, so that a side
  above that works over TLS works over a socket.
- **`Finish` comes with no read outstanding,** but one that crossed the
  end, rather than `close_notify` waiting behind a read below.
- **The length of what rustls encrypts is asked until two answers
  agree:** a query that finds the keys at their limit schedules a key
  update that the next call writes before the data, and that update's own
  record, sealed at the limit, schedules a second. A length that never
  settles, or a write that does not fit it, fails the connection, Other.
- **A truncation fails the stream as invalid,** never ends it, once what
  was deciphered is delivered; a peer's fatal alert is a reset.
- **No session resumed,** and no alert sent after a failure.
- **A name reads back as what it is,** not as text. rustls's `to_str`
  writes an IP address with std's `Display`, which a crate without std
  lacks, allocates to do it, and writes IPv6 compressed (`::1`), a form
  no reader chose: an owner that writes addresses as it does a socket's
  (forge.md, 5.1) would hold two forms of one address.
- **rustls is built as it ships in every dev build,** not only the
  tests', without its debug assertions (`[profile.dev.package.rustls]` in
  the workspace's `Cargo.toml`): one holds any alert after `close_notify`
  to be a bug, and aborts a half-closed connection that reads a corrupted
  record (`send_fatal_alert`'s debug assertion, once
  `send_close_notify` has run); release builds compile it out.
  Cargo honours profiles only in the root manifest, so a workspace that
  depends on skein-tls copies the override into its own: until it does,
  its debug builds abort on a peer's error after the client's
  `close_notify`.

## 7. Open questions

- **A deterministic TLS** (notes.md): rustls's ring provider draws its
  key shares from ring's own randomness, not from the provider's, so a
  provider that draws from the seed would need key exchanges of its own.
- **Session resumption,** which would save a round trip on each new
  connection to the same server, at the price of a cache shared between
  connections.
- **Requests after a failure on the native face.** The classic face
  drops a stream request that comes after `Failed`. The native face drops
  a read then, but asserts on an output right asked for after a failure
  or after `TransportClosing`, on a `Finish` after a failure, and on a
  read after `TransportClosing`. Which is right waits for its first user.

## 8. Not built yet

What of this document is not built is `STATUS.md`'s (its row in section 2,
and section 3).

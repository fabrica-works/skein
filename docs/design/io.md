# io

Provisional, 2026-10-03, revised 2026-10-19. The design of `skein-io`'s
step layer: the lowest step layer of every service. It owns sockets,
pipes, files and child processes, the operations in flight on them, and
the receive and send queues, and it is the only layer that sees a
descriptor or a kernel error. Below it is the kernel boundary
(kernel.md); above it, a service's protocol layer.

## 1. In one page

- **Entities, and streams.** io's vocabulary has two parts: entities
  (listen, accept, connect, spawn, a root, close) and bytes, which are the
  stream vocabulary of lib (lib.md, 7), the same for a socket and a pipe.
- **Every completion goes through io first.** io turns the kernel's
  completions into events about its entities, and requests about its
  entities into kernel operations, and absorbs the mechanics in between:
  buffers in flight, short transfers, cancellation, settling, graceful
  close.
- **Flow control is explicit.** A receive is in flight only while the
  intake has room, a send only for queued output under its cap, and an
  accept only while the listener's owner has room.
- **Paths stay beneath a root,** spawned children are pidfds from the
  start, each in a process group of its own, and termination signals
  arrive as events.
- **Trees, in cgroups.** A child may start in a cgroup io made, as
  another user, and contained: in namespaces of its own, seeing the file
  system through a view the owner composes. A cgroup is stopped as a
  whole, and proved empty by the kernel (6.2 to 6.4).
- **Files are replaced whole, or appended to.** A record is replaced
  durably, privately when it is a secret (5.2, 5.3); a trace or a log is
  a write stream on a file opened to append, with a write deadline for a
  filesystem that stalls (5.1).
- **Addresses, never host names.** io connects to an address: a socket
  address, the host's name resolved before it, or an in-process name,
  which needs no resolving (section 4).
- **Joined sockets in one process.** A connect to an in-process name is
  joined in memory to a socket its listener accepts, with no kernel
  between them, through the same requests and events as TCP (3.5).

## 2. In skein

`skein-io` depends on lib. A service's protocol layer depends on it, and
so do the backends, for the kernel records. io's limits are counted in
its worst case like any layer's: the shared entity slots, the operations
in flight, each stream's intake and queued output, children's pipe
lists, and the in-process names (3.5). The command buffers supplied to
`Spawn` belong to the caller's memory bound.

```rust
// skein-io, the protocol side, for sockets
pub enum Request {
    Listen  { owner: Token, addr: Addr },
    Connect { owner: Token, addr: Addr },
    Bind    { socket: Token, owner: Token },          // attach to an accepted socket
    Reject  { socket: Token },
    Stream  { stream: Token, down: stream::Down },    // sockets and pipes alike
    Output  { stream: Token, down: stream::OutputDown }, // independent output reservation
    Spawn   { owner: Token, spawn: Spawn },
    Signal  { child: Token, signal: Signal, to: Target }, // the child, or its group (section 6)
    Usage   { owner: Token },                         // the process's resource usage (6.1)
    Cgroup  { owner: Token, op: cgroup::Op },         // cgroups (6.2)
    // File { owner, root, op }; Window { owner, stream } (section 7)
    Close   { entity: Token },                        // graceful (section 3); one Closed follows
    Abort   { entity: Token },
}

pub enum Event {
    Listening  { owner: Token, listener: Token, addr: Addr },
    Accepted   { owner: Token, socket: Token, peer: Addr },  // to the listener's owner
    Connecting { owner: Token, socket: Token },
    Connected  { owner: Token },
    Stream     { owner: Token, up: stream::Up },
    Output     { owner: Token, up: stream::OutputUp },
    Spawned   { owner: Token, child: Token, pipes: Box<[Token]> },
    Exited    { owner: Token, exit: Exit },
    Usage     { owner: Token, usage: Measured },      // for a Usage: its one terminal
    Cgroup    { owner: Token, event: cgroup::Event }, // 6.2
    Shutdown  { signal: ServiceSignal, from: Option<Token> }, // section 7
    // File { owner, result }; Resized, Window { owner, size } (section 7)
    Failed     { owner: Token, error: Error },
    Closed     { owner: Token },                      // terminal
}

pub enum Error { Refused, Unreachable, TimedOut, Reset, Busy, Other }

pub enum Addr { Ip(SocketAddr), InProcess(Name) }     // section 4
```

An accepted socket is announced to the listener's owner, which binds to
it or rejects it (programming-model.md, 4.2). Files have requests and
events of their own, `skein_io::file`'s (section 5).

The loop drives io through four entry points, each declaring its `MAX_OUT`
(programming-model.md, section 2): `resume` takes one entry of the ready
list, `up` one completion, `fire` one expired deadline (a graceful
close's, or a retry's), and `down` one request. io's stage in the up pass
is `resume` until the ready list is empty, then `up` for each completion,
then `fire` while a deadline is due; the loop hands io no completion while
the ready list holds something, so what the down pass made is told first.
Requests to the kernel go out as `Submit` records into the queue the loop
hands the kernel.

**Decisions.**

- **Every event names its owner first:** the token the layer above gave
  for the entity concerned. `Accepted` names the listener's owner, which
  answers it.
- **`Listening` carries the bound address,** so that a listener on port 0
  is usable.
- **`Connecting` hands over the socket at once,** in the up pass after the
  `Connect`, so that its owner can close or abort a connect before it
  connects. A listener's setup waits on nothing outside the host, so its
  owner closes it once it hears `Listening`.
- **io has its own `Error`,** carrying only what the layer above can act
  on: `Refused` (nothing listened there), `Unreachable`, `TimedOut` (the
  kernel gave up on the peer), `Reset`, and `Busy`, a refusal at io's
  entrance: no socket slot, no in-process name free (3.5), or the kernel
  out of descriptors, local ports or buffers. Every other kernel error is
  `Other`, among them an address in use or not this host's; one the
  contract rules out is an assertion.
  A stream's failure is lib's `Fault`: `Reset` when the peer is gone
  (reset, timed out, unreachable, the pipe broken), `Other` otherwise.
- **io has no connect timeout.** That is policy above
  (programming-model.md, 4): the connection's deadline lives in the
  protocol layer, which aborts the connect. io keeps only the mechanics'
  deadlines: a graceful close's, and a retry's; and, on files, the
  deadline an owner states for an operation a stalled filesystem may hold
  (5, 5.1).
- **What found the kernel out of buffers or descriptors is retried after
  `Limits::retry`,** not at once: a receive, a send, a half-close or an
  accept that failed for want of a buffer, an accept that failed with an
  error the kernel gave no name, and an accept out of descriptors, which
  may be the system's (`ENFILE`), given back by another process. Retried
  at once, each would spin the loop for as long as the shortage lasts.
  The retry deadline sits in io's own table beside the close deadlines,
  one of each per entity.
- **Listeners, streams, children and pipes share one entity slab,** whose
  capacity is `Limits::sockets`: one namespace of tokens, so that `Close`
  and `Abort` name any of them. A spawn reserves one slot for its child
  and one per requested pipe before it goes to the kernel.
- **A refusal is held until the next up pass.** A `Listen`, `Connect` or
  `Spawn` refused for want of entity slots has no entity to hold its `Failed`
  and `Closed`, so io holds the owner's token in a list of its own,
  bounded by `Limits::refusals`, and `resume` tells it. The loop hands io
  a request only while io can hold a refusal (`Io::takes`), as it
  reserves room in the submissions.
- **The startup check.** Whoever stacks a machine on a stream checks that
  the machine's largest exact read (a fill or a scan) is no more than
  `Limits::largest_read` (the intake cap), and its largest room no more
  than `Limits::largest_room` (the output cap). A demand past either
  could never be met, and io asserts it, as lib's intake does (lib.md,
  7). A read of what has arrived is met whatever its maximum.

## 3. Sockets and pipes

- **The entities** are listeners, sockets, children and their chosen pipes.
  Sockets and pipes are both streams; above io, they differ only in how
  they were made.
- **One receive is in flight** per stream while its intake has room,
  whether or not anything has been demanded yet. What arrives goes into
  the stream's `Intake`, which meets the demand from above.
- **One send is in flight** per stream, in order. The rest of the output
  waits in a queue under the output cap. A short send continues from an
  offset into the same `Box`; the remainder is never copied.
- **Accepts are re-armed one at a time** while the listener's owner has
  room, up to a configured number per iteration. A flood of connections
  then waits in the kernel's backlog.
- **Close is graceful:**
  1. flush the queued output;
  2. half-close;
  3. read and discard until the peer ends or the close deadline passes;
  4. close.

  This matters when a response goes out while the peer is still sending,
  such as a refusal in the middle of an upload: the response reaches the
  peer instead of being lost to a reset. Abort closes at once.
- **Cancellation is io's to settle.** An operation cancelled by a close, an
  abort or a deadline stays *settling* until its completion arrives, and
  the entity above sees one event (programming-model.md, 5.3).

### 3.1 Entities and owners

| Entity | Made by | Owned by | Named above by | Ends with |
|---|---|---|---|---|
| listener | `Listen` | the owner of the `Listen` | `Listening.listener` | `Closed` |
| listener, held | `adopt_listener` (3.2) | io, until a `Listen` for its address claims it; then that `Listen`'s owner | the token `adopt_listener` returns, which `Listening.listener` names once claimed | `Closed` |
| socket, connecting | `Connect` | the owner of the `Connect` | `Connecting.socket` | `Closed` |
| socket, accepted | a listener's accept | io until answered; then the owner of the `Bind` | `Accepted.socket` | `Closed` once bound; nothing once rejected |
| operation | io, for an entity | that entity, until its completion | its `Submit` token, below | its completion |

- **A listener and a socket are one kind of entity** in one slab,
  `Limits::sockets` of them, each running the machine of its kind. The
  operation table holds the operations in flight, each for its entity. An
  entity is retired only once nothing of its is in flight, its cancels
  included, so a completion always finds it.
- **One `Closed` per entity with an owner,** the last event naming that
  owner. A `Listen` or `Connect` that fails is told `Failed`, and io
  closes what it made by itself, since its owner may hold no token for
  it. A listener whose accept stops is told `Failed` too, but stays: its
  owner, which holds its token, closes it. A rejected socket has no
  owner, and is closed without one.
- **`Failed` and `End` are told once each,** and never after `Closed`.
  After its owner's `Close` or `Abort`, an entity tells only `Closed`.
- **Stale handles.** A request naming an entity that is gone, or closing,
  is dropped. One naming an entity of the wrong kind, or an announced
  socket before its answer, is the layer above's bug, asserted. A
  completion always finds its operation and its entity, which is
  asserted.

### 3.2 The listener

| State | Holds | In flight |
|---|---|---|
| Socket | the address | `Socket` |
| Binding | fd | `Bind` |
| Arming | fd, the bound address | `Listen` |
| Listening | fd, the accept, discards | `Accept` when armed; the closes of discarded sockets |
| Settling | fd, the accept if in flight, cancels, discards | those |
| Releasing | | `Close` |
| Closed | | |

The accept, in `Listening`, is `Armed` (an `Accept` in flight),
`Answering` (a socket announced and not yet bound or rejected: the owner
has no room until it answers, and holds its answer while it has none),
`Idle` (on the ready list for the next iteration's accept batch, among
the starved for a socket slot or a descriptor, or waiting for its retry
deadline), or `Stopped` (an error io cannot retry, told as `Failed`). Arming it takes a free socket slot, no
discard in flight, and room in the accept batch, which counts the accepts
armed in an iteration over every listener. The starved are woken when io
retires an entity or closes a discarded socket.

| State | Event | Next, and what it does |
|---|---|---|
| Socket | `Socket` ok / error | Binding, `Bind` / Closed: `Failed`, `Closed` |
| Binding | `Bind` ok / error | Arming, `Listen` / Releasing: `Failed`, `Close` |
| Arming | `Listen` ok / error | Listening: `Listening`, arm / Releasing: `Failed`, `Close` |
| Listening | `Accept` ok | Answering: `Accepted`; with no slot left, the socket discarded and Idle, starved |
| Listening | `Accept`: no descriptor | Idle, starved, and its retry deadline armed |
| Listening | `Accept`: no buffer, or an error with no name | Idle, its retry deadline armed |
| Listening | `Accept`: the connection's own error | Idle, retried in the next iteration |
| Listening | the retry deadline | arm |
| Listening | `Accept`: any other error | Stopped: `Failed` |
| Listening | a discard done | the starved woken |
| Listening | its socket answered; resumed or woken while Idle | arm |
| Listening | `Close`, `Abort` | Settling: `Cancel` of the armed accept; Settling for the discards; else Releasing: `Close` |
| Settling | `Accept` done | an accepted socket discarded |
| Settling | `Cancel` done | retried, unsubmitted, while the accept waits |
| Settling | nothing left in flight | Releasing: `Close` |
| Releasing | `Close` done | Closed: `Closed` |
| Settling, Releasing, Closed | `Close`, `Abort` | ignored |
| any but Listening and Idle | the retry deadline | impossible: cancelled once the accept leaves Idle |
| Socket, Binding, Arming | `Close`, `Abort` | impossible: the owner has no token yet |
| any | `Bind`, `Reject`, `Stream` | impossible: the layer above's bug |

A socket accepted after the owner closed the listener is discarded by
io, unannounced: the owner asked for no more.

**Accept errors.** Out of descriptors, the listener starves until io
gives one back. Out of buffers, or with the network error of the
connection it took (a reset, a timeout, no route, or an error the kernel
gave no name: accept(2) has these retried), it tries again in the next
iteration. Any other error says the socket no longer listens, and stops
it.

**A held listener.** A listening socket opened before the loop, by the
service's startup or inherited from its parent (shell.md, 6.6), is
adopted with the address it is bound to,
`Io::adopt_listener(fd: Fd, addr: SocketAddr) -> Result<Token, Fd>`, and
held: io arms no accept on it, and the kernel queues the connections it
takes in its backlog until someone accepts.

| State | Event | Next, and what it does |
|---|---|---|
| Held | a `Listen` for its address | Listening, the `Listen`'s owner its owner: `Listening`, arm |
| Held | `Close`, `Abort` on its token | Releasing: `Close` |
| Held | given to a child (6.3) | Held: io keeps its descriptor |

- **Claimed by its address.** A `Listen { owner, addr }` whose address
  equals a held listener's, its IP and its port, takes that listener in
  place of making a socket: no `Socket`, `Bind` or `Listen` is
  submitted, and it is the owner's from then on as if it had bound,
  `Listening` naming the held listener's token. A `Listen` for any other
  address, port 0 among them, makes its socket as before. So the
  components of a service never know whether their listener was
  inherited: the address their configuration gives is the one they
  listen on, and the one `Listening` tells them.
- **Held for giving.** A held listener's token may be given to a child,
  which inherits the same socket at the descriptor its `Way::Given`
  entry names (6.3).
  io keeps its own descriptor, so that a child started again can be
  given it again, and nothing else can bind the address meanwhile. A
  parent that gives never claims: it accepts nothing.
- **Refused,** the caller keeping the descriptor (`Err(fd)`) as every
  adoption does: on a full slab, which counts it among
  `Limits::sockets`, and when a listener at the same address is already
  held. io checks nothing of the socket itself: startup has (shell.md,
  6.6).
- **Its events.** Unclaimed, the held listener's token is its own
  owner, as an adopted pipe's is: its `Closed` names it. Claimed, its
  events are the claimant's. One never claimed is closed at io's close
  as any entity is.
- **Not taken: a short reservation.** The smaller design frees a
  reserved port just before the step that binds it starts, and needs
  nothing of io. It leaves a window, from the free to the bind, in which
  another process takes the port, and the step then fails at its bind:
  the address it reports is no longer certainly its own.

### 3.3 The stream

Sockets and write pipes also expose lib.md, section 7.1's independent
output reservations through `Request::Output` and `Event::Output`. They
keep one pending or granted output token beside the classic demand; a
read-only demand remains live while that output progresses. The native
reservation checks both byte capacity and a Send record slot, including
the write in flight. A matching Send spends the full grant and enters the
same bounded output queue and kernel write path as classic Send. Read
pipes cannot admit output reservations.

Classic room ownership and independent output ownership are mutually
exclusive. Stale Cancel/Send/Release tokens do not affect a current
reservation. Room requests require an open, connected or bound stream;
they are not admitted after failure or closing. Cancelling an admitted
pending reservation stages its exact terminal for the next up pass.
Close/Abort likewise stages Cancelled before Closed; a genuine stream
failure emits the pending reservation's Failed terminal before classic
Failed. A previously emitted Granted is never answered again. Close and
Abort's promise of only Closed applies to the classic face; the
independent face also owes its already admitted pending terminal.

The loop reserves the declared maximum for all simultaneous events,
including Bytes, End and an independent output terminal. The extra cell,
staged terminal and actual enum layouts enter io's checked worst-case
bound. Over TLS, a framed connection uses TLS's native face (tls.md, 3.6),
which implements it at both its plaintext and ciphertext boundaries.

| State | Holds | In flight | Serves | Deadline |
|---|---|---|---|---|
| Opening | owner, address | `Socket` | | |
| Socket | owner, address | `Socket` | | |
| Connecting | owner, fd | `Connect` | | |
| Announced | fd, its listener | | | |
| Open | owner, fd, intake, demand, reader, writer, output | a `Recv` while the intake has room; a `Send` or a `Shutdown` | the demand | |
| Broken | owner, fd, what is in flight | what was, finishing | | |
| Closing | owner, fd, drain, flush, output | a `Recv`, discarding; a `Send` or a `Shutdown` | | close |
| Settling | owner if bound, fd if made, what it waits for, cancels | those | | |
| Releasing | owner if bound | `Close` | | |
| Closed | | | | |

- **Opening** is a connect whose `Connecting` is not told yet. It is on
  the ready list, which the loop drains before it hands io completions.
- **Open's reader** is `Receiving` (a `Recv` in flight), `Full` (the
  intake is), `Stalled` (no buffer: waiting for the retry deadline),
  `Ended` (the peer ended, `End` not told yet) or `Told`. **Its writer**
  is `Idle`, `Sending` (finishing or not: the half-close follows the last
  of the output), `Stalled` (holding the send's box and offset), `Shutting`,
  `Unshut` (a half-close that found no buffer) or `Shut`. **Closing's
  drain** is a `Recv` in flight, `Stalled`, or done; **its flush** is
  `Sending`, `Stalled`, `Shutting`, `Unshut`, or done.

| State | Event | Next, and what it does |
|---|---|---|
| Opening | resumed | Socket: `Connecting` |
| Socket | `Socket` ok / error | Connecting, `Connect` / Closed: `Failed`, `Closed` |
| Connecting | `Connect` ok / error | Open: `Connected`, `Recv` / Releasing: `Failed`, `Close` |
| Announced | `Bind` / `Reject` | Open, `Recv` / Releasing, `Close`; the listener re-arms |
| Open | `Recv` of n > 0 | the intake appended; delivered |
| Open | `Recv` of 0 | reader Ended; delivered |
| Open | `Send` of n | the same box from its new offset, or the next, or the half-close when finishing; delivered |
| Open | `Shutdown` ok | writer Shut |
| Open | `Recv`, `Send`, `Shutdown`: no buffer | that side stalls, the same again at the retry deadline |
| Open | the retry deadline | what stalled submitted again |
| Open | `Recv`, `Send`, `Shutdown`: any other error | Broken: `Failed` |
| Open | `Demand` | stored; on the ready list |
| Open | `Send` | queued; sent at once by an idle writer |
| Open | `Finish` | the half-close, once flushed |
| Open | resumed | delivered |
| Open | `Close` | Closing, its deadline armed; Releasing if nothing is left to flush or drain |
| Broken | a completion | that operation is done |
| Closing | `Recv` | discarded; again, until 0 or an error |
| Closing | `Send`, `Shutdown` | flushed, then half-closed; an error ends the flush |
| Closing | drained and flushed | Releasing: `Close`, the deadline cancelled |
| Closing | `Recv`, `Send`, `Shutdown`: no buffer | that side stalls, the same again at the retry deadline |
| Closing | the retry deadline | what stalled submitted again |
| Closing | the close deadline | Settling: `Cancel`s; what stalled is dropped |
| Socket, Connecting, Broken | `Close` | as `Abort`: there is nothing to flush |
| Socket, Connecting, Open, Broken, Closing | `Abort` | Settling: `Cancel`s; Releasing if nothing is in flight |
| Closing | `Close` | ignored |
| Settling | a completion | done; an unsubmitted cancel retried while its target waits; nothing left: Releasing, `Close`; or Closed, `Closed`, if no fd was made |
| Releasing | `Close` done | Closed: `Closed` if bound |
| Broken, Closing, Settling, Releasing, Closed | `Stream` | dropped: the stream failed, or its owner closed it |
| Settling, Releasing, Closed | `Close`, `Abort` | ignored |
| Opening, Announced | `Close`, `Abort`, `Stream` | impossible: no token yet, or no answer yet |
| Socket, Connecting | `Stream` | impossible: a stream before it is connected |
| any | `Bind`, `Reject` but Announced | impossible: the layer above's bug |
| any but Settling | a completion `Cancelled` | impossible: io cancels only when settling |

**Delivered** is one function of Open, applied once after every
transition in the up pass (a completion, a deadline), whatever cell made
it, and by `resume` after a request in the down pass
(programming-model.md, 2 and 5.4):

1. the demand's answer, if there is one: `Bytes`, if the intake meets its
   read (never after `End`); or else `Room`, if it asks for room, the
   writer takes sends, and the output has that many bytes and one more
   `Send` free. Either answer ends the demand, read and room;
2. `End`, once, if the peer ended and a read is outstanding that can
   never be met, or none is and the intake is empty;
3. a `Recv`, if none is in flight and the intake has room again.

io keeps the contract of a stream (lib.md, 7) as the side below:

- **A demand is answered at most once:** by `Bytes`, what its read asks
  for (lib.md, 7: exactly, or for a read of what has arrived, what the
  intake holds up to its maximum), or by `Room`, whichever io can give
  first; either answer ends it, and nothing is outstanding until the side
  above states its next. The side above states its next demand only after
  an answer, never in place of one outstanding, which io asserts.
- **`Read::Nothing` with no room withdraws** the outstanding demand, and
  only when the side above will read no more: it is closing, or its read
  crossed `End`. An answer
  already on its way may still arrive, and the side above drops it. No
  other `Bytes` or `Room` come without a demand.
- **`End` comes once nothing io holds can meet a demand:** with one
  outstanding, when it can never be met; with none, only when nothing is
  held. It comes once, and ends reading only. A read that crosses it is
  never met, and `End` does not end the demand: it stays outstanding
  until `Room` answers it, if it asked for room, or until the side above
  withdraws it. Room may still be granted after `End`, as the stream can
  still send to a peer that only half-closed. A read larger than what is
  left before the end is never met; a side above that must see every
  byte reads by its framing, or reads what has arrived.
- **`Failed` may come at any time,** a demand outstanding or not, and
  nothing follows it but `Closed`. After `End` it says only that the
  stream can no longer send: what was read stands.
- **A scan that meets no delimiter within its maximum delivers exactly
  the maximum,** and the side above decides what that means.
- **A read of what has arrived is met by the first delivered that finds
  a byte in the intake:** in the up pass after the down pass that stated
  it, if the intake holds one, or else in the pass its next receive
  lands. A receive asks for the room the intake has (at most
  `Limits::receive`), so a stream read so costs one delivery per receive,
  not one per byte. Pipes meet it as sockets do (section 3).
- **Room grants one more `Send`** of up to that many bytes, and the
  output counts a `Send` in flight until all of it is sent. A `Send` past
  the output cap or `Limits::sends`, or after `Finish`, is the layer
  above's bug, asserted; so is room demanded after `Finish`.
- **A `Send` is within the room granted** (lib.md, 7), and io holds the
  layer above to it: each open stream keeps what its last `Room` granted,
  less what was sent since. `Room` sets it to the room asked for, each
  `Send` must fit within it and spends it, and a demand for no room leaves
  it as it is. A `Send` past it is the layer above's bug, asserted, in
  every world: it is what keeps the output under its cap by the layer
  above's asking, not by io's refusing.
- **`Failed` is told at once,** and drops what the intake held and the
  output queued: the peer is gone.
- **Discarding starts with the close,** not after the half-close, so that
  a peer blocked on an upload drains, then reads the response.
- **Only operations that wait are cancelled:** `Accept`, `Connect`,
  `Recv` and `Send`. `Socket`, `Bind`, `Listen`, `Shutdown` and `Close`
  are waited for.
- **No buffer is not a failure.** A `Recv`, `Send` or `Shutdown` that
  fails for want of kernel buffers did nothing: its side *stalls*, a
  stalled send keeping its box and offset, and is submitted again once
  the retry deadline passes. The deadlines a state implies are one
  function of it, applied after every transition: the close deadline runs
  only while closing, the retry deadline only while a side stalls.

### 3.4 Memory

Per entity: a slot in the slab; for each stream, an intake of `Limits::intake`; a
receive buffer of at most `Limits::receive`; the output, at most
`Limits::output` bytes in at most `Limits::sends` boxes; and a slot in the
ready lists, and two in the deadlines. An entity has at most four
operations in flight (a `Recv`, a `Send` and a cancel of each). A pipe
has at most one read or write and its cancel; a child waits and may have
one signal in flight. Thus `operations(limits)` is four per entity slot,
and one for a `Usage` (6.1), the ring's size (kernel.md, 5);
the operation table holds twice that, as an operation retired in an
iteration keeps its slot until the reclaim point. `worst_case` adds up
what the containers report (programming-model.md, 6.3), including a
child's pipe IDs and the temporary arrays on spawn completion. The
caller budgets the command buffers it supplies to `Spawn`.

### 3.5 Joined sockets

A listener on an in-process name (section 4) accepts sockets joined in
memory to those that connect to it, with no kernel between them. Their
owners make the same requests and hear the same events, in the same
orders, as over TCP, so a component listens and connects where its
configuration says, and its stack runs over either. jig's one-process
shape joins its roots this way.

| On a socket | On a joined socket |
|---|---|
| `Socket`, `Bind`, `Listen` | the name taken, in `down` |
| `Accept` | the first connector waiting on the name, joined |
| `Connect` | waiting on the listener until it accepts |
| `Recv` | none: the peer's moves fill the intake |
| `Send` | a move into the peer's intake, as far as its room allows |
| `Shutdown` | the peer's end, after the last byte moved |
| `Close` | the pair unjoined |
| `Cancel` | none: nothing waits on the kernel |

The machines are 3.2's and 3.3's, each kernel operation replaced as the
table says. The states that wait only on the kernel (a listener's
Socket, Binding and Arming; a stream's Socket, Settling and Releasing)
are passed through at once, and no retry deadline is ever armed.

- **The name.** `Listen` takes the name for its listener until the
  listener's `Closed`, which frees it. A name already taken, or the empty
  name, is answered `Failed` with `Other`, as an address in use is. With
  all `Limits::names` held, a `Listen` on a name is refused at the
  entrance (`Busy`), as one without a slot is.
- **Connecting.** `Connecting` comes as over TCP, and io looks the name
  up then. A name unbound, or held by a listener that is closing, is told
  `Failed` with `Refused`, then `Closed`. Otherwise the connector waits on
  the listener, in its own slot and in the order connectors came, until
  it is accepted or its owner aborts it (io has no connect timeout,
  section 2). A listener that closes refuses those still waiting.
- **Accepting** follows 3.2: armed one at a time while the listener's
  owner has room, with a free slot and room in the accept batch, starved
  without a slot. Armed, it takes the first connector waiting: io makes
  the accepted end, joins the two, tells the listener's owner `Accepted`,
  its `peer` the empty name, and the connector `Connected`. The owner
  binds or rejects it as over TCP. Nothing moves into an end until it is
  bound.
- **Moving bytes.** Each end has an intake and an output under section
  3's caps, and no receive buffer. One move is in flight per end, in
  order: the head of its output, copied into the peer's intake as far as
  that has room, a short move continuing from the same box's offset, the
  box dropped once all of it is in. A full intake stops the peer's moves,
  so the peer's output fills and its owner's room waits: the reader's
  intake is the backpressure. Demands are met by 3.3's delivered, and
  independent output reservations (3.3) are offered as on a socket, the
  move counting as the send in flight.
- **Ends.** A half-close reaches the peer after the last byte moved, and
  the peer hears `End` as 3.3 tells it. A graceful close keeps section 3's
  four steps: what is queued moves; the peer is half-closed; what the
  peer moves meanwhile is discarded as it comes, until its own end or the
  close deadline; then the pair is unjoined. An end whose peer has closed
  fails as `Reset` at its next move, its bytes lost, as after TCP's late
  reset. One whose peer aborts, is rejected, or meets its close deadline
  before its flush is done fails as `Reset` at once, or, if closing, ends
  its close. 3.1's rules hold as they stand; an end never settles, as it
  has nothing in the kernel, so a close with nothing to flush or drain,
  or an abort, is told `Closed` in the next up pass.
- **Nothing happens inside a request.** `down` records a request and puts
  its end on the ready list, telling nothing (section 2). io moves bytes,
  joins, unjoins and tells what came of each when it resumes an end, in
  the next up pass, as early as a kernel's completion could come; a move,
  or room made in an intake, wakes the peer through the same list. A
  joined end submits nothing and holds no slot of the operation table:
  its move in flight is state, the head box and its offset. Boxes come
  only from a down pass's `Send`s, and an intake drains only to a demand
  stated there or to a close, so each up pass ends.
- **Memory.** A pair holds two slots of `Limits::sockets`, one per end,
  as a loopback connection within one process does. An end holds no more
  than a socket's stream, so 3.4's bound covers it; `worst_case` adds the
  names table, `Limits::names` entries of a name and its listener's id.
- **In the simulator** joined sockets run as in the real loop: they are
  io's own, never the kernel's records, so the simulator's faults and
  latency never reach them and its trace does not show them; a cut
  (simulator.md, 3.3) drops both ends with their process. What an end
  meets comes from the owners at both ends and from the limits: short
  moves under small intakes, a peer that stops reading, closes early,
  never ends, or aborts.
- **No TLS.** The bytes never leave the process: TLS is not stacked on a
  joined socket, and no world checks it there.

## 4. Addresses and names

- **io connects to addresses only:** an `Addr` is a socket address, IPv4
  or IPv6 with a port, or an in-process name. The kernel's records carry
  only socket addresses (kernel.md, 3).
- **To start:** the shell resolves the configured peer host names at
  startup and hands in the socket addresses as configuration (shell.md,
  6).
- **An in-process name** is at most `Name::MAX` bytes, compared as bytes
  and held inline, so an `Addr` stays a plain value that allocates
  nothing. It lives in io's own namespace: the names its listeners hold,
  which no other process, and no other io, sees. Nothing resolves it, at
  startup or later: configuration gives it as it is. A listener on one
  accepts joined sockets (3.5); the empty name is a connector's, which
  has none.
- **Later:** when a service must resolve host names while it runs, the
  next protocol machine is a DNS client over UDP sockets. It reads the
  hosts file and the resolver configuration at startup. io gains
  datagram sockets for it.

## 5. Files

Files are a machine of their own beside io's: `FileIo`
(`skein_io::file_layer`), its requests and events in `skein_io::file`. A
service holds both, submits the records of each to the one kernel, and
hands each completion back to the machine whose operation it was.

```rust
// skein_io::file
pub enum Request {
    // whole files, beneath a root's token
    Load  { owner, root: Token, path, max: u32, no_follow },                       // Loaded, TooLarge
    Scan  { owner, root: Token, path, max: u32, max_bytes: u64, no_follow },       // Scanned
    Store { owner, root: Token, path, bytes, expected: Expect, no_follow }, // Stored, Conflict
    MakeDirectory { owner, root: Token, path, mode },                            // Made
    OpenPrivate { owner, root: Token, path },                                    // Opened, Refused
    // open files, at offsets
    Create, CreateNoFollow { owner, root: Token, name, mode },                     // Opened
    OpenRead, OpenReadNoFollow { owner, root: Token, name },                       // Opened
    OpenWrite, OpenWriteNoFollow { owner, root: Token, name },                     // Opened
    OpenDirectory { owner, root: Token, name, no_follow },                         // Opened: a root
    ReadAt  { owner, file: Token, offset: u64, max: u32 },                         // Read
    WriteAt { owner, file: Token, offset: u64, bytes },                            // Written
    Cut { owner, file: Token, length: u64 },                                       // Cut
    Stat { owner, file }, Sync { owner, file }, Close { owner, file },
    // the entries of a directory
    SyncDirectory { owner, root: Token }, List { owner, root: Token },
    ListPage { owner, directory: Token, max: u32 },                                // Listed
    Rename { owner, from_root: Token, from, to_root: Token, to },
    Remove { owner, root: Token, name, directory },
    // a lock
    Lock { owner, root: Token },                                                   // Locked, Busy
    // a tree's metadata, by name in an open directory (kernel.md, 6.4)
    StatEntry { owner, directory: Token, name },                                   // Stated
    ReadLink { owner, directory: Token, name },                                    // LinkRead { target }
    MakeLink { owner, directory: Token, name, target },                            // Linked
    HardLink { owner, from: Token, from_name, to: Token, to_name },                // Linked
    SetMode { owner, directory: Token, name, mode },                               // Changed
    SetOwner { owner, directory: Token, name, user, group },                       // Changed
    ListAttributes { owner, directory: Token, name },                              // AttributesListed { names }
    GetAttribute { owner, directory: Token, name, attribute },                     // AttributeRead { value }
    SetAttribute { owner, directory: Token, name, attribute, value },              // Changed
    RemoveAttribute { owner, directory: Token, name, attribute },                  // Changed
    // the filesystem, and btrfs's subvolumes (kernel.md, 6.5)
    FilesystemStatus { owner, file: Token },                                       // FilesystemStated { filesystem }
    SyncFilesystem { owner, file: Token },                                         // Synced
    SubvolumeCreate { owner, directory: Token, name },                             // Made
    Snapshot { owner, source: Token, directory: Token, name },                     // Made
    SubvolumeDestroy { owner, directory: Token, name },                            // Removed
    SubvolumeReadOnly { owner, subvolume: Token, read_only },                      // Changed
    SubvolumeInfo { owner, subvolume: Token },                                     // SubvolumeStated { subvolume }
    SubvolumeChildren { owner, subvolume: Token, from: u64 },                      // Children { children }
    SubvolumePath { owner, subvolume: Token, directory: u64, tree: u64 },          // PathRead { path }
}
// each request ends in one event: its own, Failed { error } or Cancelled
```

- **Paths resolve beneath a root.** A root is an open directory. Every
  request names one, and a path relative to it that the kernel resolves
  beneath it (`openat2` with `RESOLVE_BENEATH`), so neither `..` nor a
  symbolic link leaves it. `no_follow` refuses a symbolic link in any
  part of the path.
- **Where roots come from.** The shell opens the first at startup, from
  configuration (`open_root`), and the service gives each to
  `FileIo::adopt_root` for its token. `OpenDirectory` opens another
  beneath one, and its `Opened` gives that root's token. Every request
  names a root by its token, never by its descriptor, which io alone
  sees.
- **Whole files** are one request each; io runs the opens, the reads or
  writes and the closes beneath it:
  - `Load` reads a file of at most `max` bytes; a larger one is
    `TooLarge`, with its size and none of its bytes;
  - `Scan` lists a directory, keeping its first entries in name order
    within `max` entries and `max_bytes` of names, and counts those left
    out;
  - `Store` replaces a file, conditionally and atomically (below).
- **Open files** stay open across requests, behind tokens, up to the
  number the driver is made with. `ReadAt` and `WriteAt` work at an
  offset, and io continues a short transfer until the request is met or
  a read reaches the end. `Sync` makes a file's writes durable,
  `SyncDirectory` a directory's entries. These are what kv.md's log
  segments need (kv.md, section 8).
- **What a request makes by a path, io makes durable** before it
  answers: `Store` after its directory's `Sync` (5.2); `MakeDirectory`
  after a `Sync` of the directory it made the new one in; `OpenPrivate`,
  when it made its directory, after the same, before it opens it. io
  holds that directory open, having opened it to make the name (below),
  and the owner, which named a root and a path, has no descriptor for
  it. A failed `Sync` there answers `Failed` with `committed`: the
  directory is made, its durability unknown. What an owner makes
  beneath a root it holds open (`Create`, `Rename`, `Remove`, and the
  entries of a directory io made) is durable once it syncs that root
  (`SyncDirectory`), as kv's segments are.
- **Listing** comes whole or by pages. `List` reads a whole directory,
  and fails with `NoBufferSpace` past the driver's `max_entries`. A
  directory that may hold more is listed by pages: the owner opens it
  with `OpenDirectory`, and each `ListPage` answers `Listed` with at most
  `max` of its entries (at least 1 and at most `max_entries`: 0, or
  more, is `InvalidArgument`, before any record), from where the last
  page of that open directory stopped. io fills each page until `max` or
  the directory's end, so a page shorter than `max` is the last, and an
  empty one says the end was reached already. The position is the open
  directory's own (kernel.md, 6.1's `List`), so the open directory is
  the cursor: two owners listing one directory each open it, and the
  owner closes it when done. Entries come in the
  directory's order, `.` and `..` left out, each once while the
  directory is unchanged; one added or removed between pages may appear
  or not, as `readdir` allows. A page's entries are counted in
  `worst_case` as a `List`'s are. A page that fails, with a kernel's
  error or stopped by its deadline or a cancel (below), may have taken
  entries it never answered: the position is past them. An owner that
  must see every entry closes the directory and lists it from a new
  `OpenDirectory`.
- **Renaming** moves one name: `Rename` takes the directory the entry
  leaves and the one it lands in, each a root, the same one for a rename
  in place, and a single name in each. Both lie on one filesystem;
  across two, the kernel's error is `Other(EXDEV)`. A rename between two
  directories survives a crash once the owner has synced the directory it
  lands in (kernel.md, 6.1).
- **Locking** is `Lock` on a root: an exclusive lock, taken without
  waiting (kernel.md, 6.1). It answers `Locked` when the lock is taken or
  already held through this root, or `Busy` when another open of the
  directory holds it, in this process or another; `Failed` with the
  kernel's error otherwise. io never waits for a lock and keeps no
  timer for one: an owner that must wait asks again on a timer of its
  own, at the interval it chooses. The lock lasts as long as the root's
  descriptor: until its `Close`, for a root `OpenDirectory` made, and
  until the process ends, for one the shell opened. There is no unlock.
  A spawned child never holds it.
- **A tree's metadata** is read and written by name, in a directory the
  owner holds open: a root, or one `OpenDirectory` made, named by its
  token, as `ListPage` names one. Each request is one record (kernel.md,
  6.4), so it acts on the entry itself, never a link's target, and io
  opens nothing for it. `StatEntry` answers `Stated` with the entry's
  whole stat; `ReadLink` answers the link's target; `ListAttributes`
  answers each attribute's name, unpacked from the kernel's list;
  `GetAttribute` answers one value. `MakeLink` and `HardLink` answer
  `Linked`, and the requests that set a mode, an owner or an attribute,
  or remove one, answer `Changed`. io checks each request's names, mode,
  target, attribute name and value against the record's bounds, and
  answers one outside them `InvalidArgument`, before any record. What
  they write is durable as kernel.md, 6.4 says: a link's name once the
  owner syncs its directory (`SyncDirectory`), the rest once it syncs the
  entry's file or its filesystem (`SyncFilesystem`). A file is made with
  `Create`, a directory with `MakeDirectory`, and cut with `Cut`.
- **The filesystem** of an open file or directory: `FilesystemStatus`
  answers its kind and its space, and `SyncFilesystem` makes everything
  written to it durable, answering `Synced`. **btrfs's subvolumes**
  (kernel.md, 6.5): `SubvolumeCreate` and `Snapshot` make one at a name
  in an open directory (`Made`), and `SubvolumeDestroy` destroys one
  (`Removed`); `SubvolumeReadOnly`, `SubvolumeInfo`, `SubvolumeChildren`
  and `SubvolumePath` take a subvolume's root, held open as a directory.
  `SubvolumeChildren` answers one page, at most 255 children, from tree
  ID `from` up: a page shorter than 255 is the last, and after a full one
  the owner asks again from the last tree ID plus one. Off btrfs each
  fails `NotSupported`.
- **These requests stall the loop.** Every one but `StatEntry`,
  `MakeLink` and `HardLink` is synchronous (kernel.md, 6): it runs in the
  loop, at submit, and nothing stops it. A request's deadline cannot cut
  one short, and a stalled filesystem stalls the service. The writing
  subvolume requests and `SyncFilesystem` are long even on a healthy
  disk (kernel.md, 6.5). A service that must not stall makes them in a
  process of its own.
- **Removing** takes one name. `Remove` removes a file, or, with
  `directory`, an empty directory, never what lies inside one: a
  directory that is not empty fails with the kernel's error, as
  `unlinkat` with `AT_REMOVEDIR` does (kernel.md, 6.1). io never
  traverses to remove a tree. Its owner removes the names it knows, then
  the directory, so nothing it did not write is removed.
- **Writing a file that exists:** `OpenWrite` opens an existing regular
  file to read and write, never creating it (`Missing` if it is absent),
  so a writer that resumes a file after a restart writes at the offsets
  it chooses; `Cut` sets an open file's length, dropping what lies past
  it, durable once the file is synced. They are what a file of records
  appended at a known committed length needs after a crash: open it, cut
  it to that length, sync it, and write on from there (kv.md, section 6's
  payload logs; a transcript's file in an application's store).
- **What the records allow** (kernel.md, 6.1). Only an `Open` resolves a
  path beneath a root, so io opens the directory a name lies in, beneath
  the root, before it renames or removes that name or makes a directory
  there, and it stats what it has opened: a stat needs the file readable,
  but for the path-only open that names why a private open was denied
  (5.3). Reads and writes may be short,
  and io continues them; a listing comes an entry count at a time, and
  io's stated count is a limit it checks as the entries come.
- **Replacing a file, in order:** `Stat` the old file, opened to read, for
  its permission bits; `Create` the temporary, beside the target in its
  directory, with those bits; `Write` it all; `Sync` it; `Close` it;
  `Rename` it over the target; `Sync` the directory. A reader sees the old
  file or the new, never part of either, and after the last `Sync` the
  new one survives a crash (kernel.md, 6.1). `Absent` uses an atomic
  no-replace rename: any entry at the target makes it fail with `Conflict`.
  `Any` skips the conditional recheck and uses a replacing rename.
  `Digest` rechecks the target before a replacing rename (5.2); a recheck
  is not a lock, so an outside writer can still race with that rename.
  `Stored` carries the new content's digest.
  - The temporary is named `<target>.skein-<16 hex>`: the target's name
    (its first 230 bytes), `.skein-`, and 16 lowercase hex digits drawn
    from io's seeded randomness. The name's shape is part of the
    contract, so that an owner's scan can tell io's temporaries from its
    own names (5.2). A `Create` that finds the name taken
    (`Exists`) draws another.
  - The old file's permission bits are kept, never its setuid, setgid
    or sticky bits, which `Stat` also answers. A target that does not
    exist, or cannot be opened to read, is made with `0o666` less the
    umask.
  - A symbolic link at the target is replaced by the file, not written
    through: the rename gives the link's own name the new file.
- **One request at a time, each with a deadline:** the driver's timeout
  from when it starts, or the owner's own (`down_until`). At the
  deadline, or when the owner cancels, io cancels the operation in
  flight and holds the request until it settles; then, once every
  descriptor the request opened is closed, it sends the one terminal:
  `Failed` with `TimedOut`, or `Cancelled`, if the stop won, and the
  request's own if the operation finished first. A store gives up only
  before its rename: once the new file is in place, it runs to the commit
  terminal described in section 5.2. An operation that completes without
  a `Cancel` (kernel.md, 6.1: a `Stat`, `StatEntry`, `Rename`, `Remove`,
  `MakeDirectory`, `List` or `Lock`, and every operation of its sections
  6.4 and 6.5) always finishes first, so a request
  whose answer it decides answers its own, whatever its deadline: a
  `Lock` past it still says `Locked` or `Busy`, and a `Rename` that
  landed says so. A request with more to do after it, such as a page of
  a listing with entries still to read, stops at that completion, unless
  the completion finished it.
- **Memory:** `FileIo::worst_case` counts the open files' table, the
  driver's own buffers, and an admitted `Load`, `Scan` or `Store` at its
  bounds. The buffers an owner hands in, `WriteAt`'s bytes and paths,
  are the owner's. The records of a tree's metadata and of btrfs take
  buffers at the kernel's largest (kernel.md, 6.4 and 6.5), which io
  makes for the request in flight and hands up cut to their length, so
  `worst_case` counts 65,536 bytes for an attribute's list or value and
  4,096 for a link's target or a subvolume's path, the largest one
  request holds.
- **Beyond whole files:** a file appended to as a stream (5.1); the
  durable replace that services keep their records with (5.2); and
  private files, for secrets (5.3). A file read as a stream, by demand,
  comes when a user needs to read one too large to hold.

### 5.1 Append streams

A file written as the service goes, a trace or a log, is a stream like a
write pipe's: lib's stream vocabulary (lib.md, 7), output only, driven by
the ring like any stream. It replaces a shell thread that writes the
file beside the loop (programming-model.md, 2.1).

- **Made by adoption.** The shell opens the file at startup, beneath one
  of its roots (shell.md, 6), to write at its end, creating it with the
  mode configuration gives if it is absent, and io adopts the descriptor
  as a write stream, as it adopts an inherited pipe. A standard output
  that is a regular file, as when a person redirects it, is adopted the
  same way. Opening one beneath a root while the service runs, as a file
  request, comes when a user needs it.
- **Writes go at the end,** one in flight, in order, each an `Append`
  (kernel.md, 6.1), from the output queue under its cap, as for a pipe:
  room, a `Send` within it, `Finish`. A short write continues from its
  offset in the same box. Each write
  lands at the file's end, so another process appending to the same file
  may land between two pieces of one `Send` cut short: a file has one
  appending writer, or its writers accept that.
- **The kernel's copy, not the disk's.** A write is done once the kernel
  took its bytes, which then outlive the process but not a crash of the
  machine. The stream does not sync; what must survive a crash is
  replaced whole (5.2).
- **A write deadline gives up on a stalled filesystem.** A regular file
  has no peer to stop reading, but its filesystem may stall (kernel.md,
  6.1), and a write that never completes would hold the stream's owner,
  and every step that waits on its room, for good. So the owner states,
  when it adopts the stream, the most one write may stay in flight. io
  arms that deadline with each write and, at it, cancels the write, keeps
  the stream settling until the write's completion arrives, drops what
  is queued, and tells the owner the stream failed (`Failed`), once. The
  owner decides what an abandoned stream means: a trace's sink records
  it where it still can. As for a whole-file operation (section 5), the
  deadline is the owner's policy and io's mechanism.
- **An inherited write pipe takes the same deadline when its owner
  states one.** A standard output that is a pipe or a terminal, adopted
  to carry such a stream, as an event stream's, stalls when its reader
  stops reading, as a file stalls on its filesystem. Its owner states
  the write deadline when it adopts the pipe, and io arms, cancels and
  tells it the same way. Without one, a write pipe waits on its reader,
  as a child's pipe does.
- **Close flushes.** `Close` writes what is queued, then closes the
  descriptor: there is no half-close, and nothing to drain. While the
  stream closes, its close deadline (section 3) replaces the write
  deadline, as a function of its state, so a filesystem that stalls at
  the end costs the stream its last bytes, never the process its end
  (programming-model.md, 5.2). `Abort` drops what is queued and cancels
  the write in flight.
- **Backpressure is the owner's choice.** An owner that must lose
  nothing asks for room before it encodes (programming-model.md, 7), so a
  slow disk holds its queue, and through it the step that fills it. One
  that may lose drops and counts when the room does not come
  (programming-model.md, section 3).

### 5.2 Durable replace

Replacing a file whole, in the order above, is how a service keeps a
record it must neither lose nor tear: a store's file, a configuration it
writes, a token record. It is the idiom of kernel.md, 6.1 (`Create`,
`Write`, `Sync`, `Rename`, then `Sync` of the directory) as one request,
run by io.

- **One request, one terminal,** answered `Stored` only after the
  directory's `Sync`: the new content then survives a crash. A reader
  sees the old file or the new, never part of either.
- **The rename is the commit.**
  - Before it, a failure, a deadline, a cancel or a `Conflict` found at
    the recheck removes the temporary before the one terminal, and the
    old file is as it was. So does a rename that does not commit: an
    `Absent` one that finds the target taken, or one whose flag the
    filesystem rejects. If the temporary cannot be removed, as on a file
    system that has turned read-only, the terminal says so and names it,
    and nothing waits on it. The old file is still as it was, the next
    replace of the target does not depend on the temporary, and a scan
    may remove it.
  - After it, the new file is in place. A failure of the directory's
    `Sync`, or its deadline, answers `Failed` saying that the replace
    committed: the new content is what a reader sees, and its durability
    is unknown. Nothing is rolled back, and the same replace done again
    makes it durable.
  - A cancel after the rename is not honoured: the request runs to its
    terminal.
  - Rolling back would need a second copy of the old file, and would
    still leave a window a crash could fall in, so none is kept.
- **What a crash leaves:** the old file or the new, whole, and at most a
  temporary beside the target, named from it. The next replace of the
  target does not depend on it, and a scan may remove it.
- **What the request expects of the target,** one of three, always
  stated:
  - `Any`: replace whatever is there, or nothing. No read of the old file
    is needed, so a target io cannot read is replaced all the same;
  - `Absent`: there must be no target at the final rename. Any entry
    there, including an unreadable file, a dangling link or a directory,
    is answered `Conflict` without replacing it;
  - `Digest`: the target must hold exactly the content with this digest.

  `Any` skips the conditional recheck; opening the old file for its mode
  still applies. For `Absent` and `Digest`, io checks once more before
  the rename. A failed expectation, or a target io cannot read for
  `Digest`, is answered with nothing replaced: `Conflict` or the read's
  fault. `Absent` also commits with `RENAME_NOREPLACE`: even when two
  creators both rechecked a missing target, only one can create it; the
  other answers `Conflict` and cleans its temporary before that terminal.
  A filesystem that rejects the flag with `EINVAL` answers `Failed` with
  `InvalidArgument`, uncommitted, after temporary cleanup; io never
  retries with a replacing rename. `Digest` uses a replacing rename after
  its optimistic recheck: a recheck is not a lock, and an outside writer
  can change the target between that check and the rename.
- **The mode** is the old file's, or `0o600` for a private file (5.3).
- **Its deadline** is the owner's, as for every whole-file operation. A
  replace that fails or times out before its rename removes its
  temporary before its one terminal, and leaves the old file as it was.
  After its rename, it answers as the commit says.
- **Not here:** updating part of a file, or a set of records larger than
  one write. Those are skein-kv's (kv.md).

### 5.3 Private files

A secret a service keeps between runs, such as a token record or a key,
lives in a private directory, readable by the service's user alone. io's
part is the modes and the checks; what the files hold, and their names,
are the owner's (oauth.md, 6).

- **A private root.** The directory is made `0o700` if it is absent
  (`MakeDirectory` takes a mode, as `Create` does; kernel.md, 6.1), the
  directory it lies in synced (section 5), and opened beneath its root
  as a root of its own, without following a link at its name. io stats
  what it opened and refuses it unless it is a directory with no group
  or other permission bits and the expected owner, saying what it found
  unsafe (a link, a non-directory, group or other bits, another owner),
  so an owner tells a planted file from a failed read.
- **Private files are made `0o600`,** less the umask, which only takes
  bits away, and are replaced whole with that mode (5.2), never with the
  old file's: a file found with group or other bits is not copied
  forward.
- **Reading one checks what was opened:** a regular file, reached
  without following a link, with no group or other permission bits, one
  link, and the directory's owner. Anything else is refused unread. It is
  read whole within a stated maximum (section 5).
- **What io does not do:** encrypt, lock, or keep a secret out of memory.
  A secret's bytes are its owner's, which keeps them out of logs, traces
  and `Debug` (oauth.md, 4).

The checks of owner and links need the kernel's `Stat` to answer both
(kernel.md, 6.1). The owner they expect is the process's own effective
user: the shell reads it once at startup (shell.md, 6) and gives it to
io's file configuration, never inferred from a path or its parents. A
private root or file owned by anyone else is refused, saying so.

- **A denied open is still named.** A private root or file of another
  user is usually unreadable to this one (`0o700`, `0o600`), so its open
  fails with `Permission` before there is anything to stat. io then opens
  the same path again, beneath the same root, for its path only
  (`OpenHow::PathNoFollow`, kernel.md, 6.1), which needs no permission on
  the entry itself, stats that, closes it, and answers the refusal it
  found: another owner, a link, a non-directory, group or other bits, in
  the order the checks above take. Only when that stat finds nothing
  unsafe (a `0o000` directory of this user, say), or the path-only open
  fails too, is the answer the first open's `Failed`, unchanged.
- **The path-only descriptor decides nothing.** It is never read and
  never makes a file acceptable: every check that admits a root or a
  file is made on the descriptor that will be read. It only names a
  refusal, so a swap between the two opens can change which refusal is
  told, never let an unsafe file be read. This is why a user who once ran
  the service as root hears that the directory is root's, not that
  permission was denied.

## 6. Processes

- **Spawn** takes a program, its arguments, its environment, a working
  directory beneath a root, and which pipes to make. It answers with a
  child and its pipes. The backend also returns an opaque `kernel::Sender`,
  kept by io beside the child's token until its `Exited` event. It is
  compared only for equality with signal senders (section 7); no numeric PID
  enters io's events or its child entity.
  - The ring adapter uses glibc's `pidfd_spawn`, so the child is a pidfd from
    the start. Its file actions install requested pipes at chosen child
    descriptors and close everything else on exec. The kernel's own check
    then stops a reused PID from being signalled. `std::process` is not used.
  - The child starts in a process group of its own, so a terminal's
    interrupt reaches the service alone, which decides what its children
    hear (section 7).
  - A given descriptor is an entry of the descriptor table whose way is
    `Given(token)` (6.3): one io holds, a held listener (3.2) so far,
    which the child inherits at the entry's descriptor; io keeps its own,
    and `Spawned` names no token for it.
  - A requested pipe has a chosen child descriptor and direction. Its
    parent end is a one-way stream token in the `Spawned` event, in request
    order. Each pipe consumes one entity slot alongside the child.
- **Exit** is a wait on the pidfd, through the ring (`waitid`). It
  observes the exit without reaping the child (`WNOWAIT`): the child
  stays unreaped, holding its PID, until io closes it.
- **Signals** (`Interrupt`, `Terminate`, `Kill`, `Stop`, or `Continue`) go
  to the child alone, or to its process group: `Signal` names which (`Target::Child` or
  `Target::Group`). Both go through the child's pidfd,
  `pidfd_send_signal`, the group's with the kernel's
  process-group scope, which reaches the group the child leads from its
  spawn (kernel.md, 6.2). The group's ID is the child's PID, which the
  kernel cannot give another process while the child is unreaped. A
  signal to the group therefore reaches the child and whatever it started
  that stayed in its group, and never a stranger, even once the child
  itself has exited.
  - **One signal in flight.** A child has at most one signal in flight
    (3.4). A `Signal` sent while one is in flight, or once its owner has
    begun to close the child, is dropped, and nothing tells the owner. An
    owner that must be sure its signal went closes the child instead:
    closing kills its group.
- **A child is *closed*** once it has exited and its pipes and pidfd are
  closed. It has one terminal event, which comes after all of those.
  Closing a child sends `Kill` to its group, whether or not the child
  itself has exited, then reaps it: what it started in its group ends
  with it. The exit and pipe closures still settle before its `Closed`
  event.
- **A timeout is the owner's** (programming-model.md, 4). An owner that
  gives a child a deadline signals its group when it passes, then closes
  it. Whatever the child started that holds its pipes then ends too, and
  the pipes with it, so a command that timed out is answered within its
  deadline and io's close deadline, not when the last of its descendants
  chooses to exit.
- **A group is not a tree.** A descendant that leaves the group
  (`setsid`, `setpgid`) escapes its signals and outlives its child, and
  nothing bounds or counts what a group may use: a descendant reparented
  away from the child is in no one's resource usage (6.1). A child
  spawned into a cgroup of its own is a tree (6.3): stopped as a whole,
  proved empty, bounded and counted.

### 6.1 Resource usage

A service may report what it used, at its end: CPU time and peak
resident size, its own and its children's. io reads them from the kernel
for it.

- **One request, one terminal.** `Usage` answers `Usage`, or that the
  kernel would not read it (`Unread`, with its error), with two parts:
  the process's own, and its reaped children's. Each holds user and
  system CPU time, and the peak resident size in bytes. The children's
  CPU is the sum over the children reaped so far, and their peak is the
  largest one of them reached, as the kernel counts them.
- **It holds no entity.** The kernel reads it at submit (kernel.md, 6.3).
  One may be in flight at a time, in an operation slot of its own; a
  second while one is in flight is the layer above's bug, asserted.
- **A child counts once io has reaped it,** at its close (section 6),
  not when it exits: a service that wants every child in its report asks
  after its last child's `Closed`. What a child started counts only if
  the child itself reaped it: a descendant reparented away, as one the
  group's `Kill` ends after the child exited, is not counted (section 6).
- **When to ask is the service's.** A service that writes a report at its
  end asks once its children have closed and before the stream it writes
  the report to closes (shell.md, 13).

### 6.2 Cgroups

A cgroup is an io entity, beside children: a directory of the kernel's
cgroup v2 hierarchy, which holds processes, counts what they use and
bounds it, and is stopped and proved empty as a whole. A service makes
its cgroups beneath its own, and spawns children into them (6.3).

```rust
// skein-io's cgroups, the protocol side; in io's Request as
// Cgroup { owner: Token, op: cgroup::Op }, answered by cgroup::Event
pub enum Op {
    Make    { parent: Token, name: Box<[u8]>, limits: Limits, delegate: Option<Owner> },
    Enable  { cgroup: Token, controllers: Controllers }, // for its children
    Enter   { cgroup: Token },                           // the service moves itself in
    Counts  { cgroup: Token },
    Signal  { cgroup: Token, signal: Signal },           // every process in it (6.4)
    Kill    { cgroup: Token },                           // every process, at once (6.4)
    Empty   { cgroup: Token },                           // answered once none is left (6.4)
}
pub struct Limits     { pub memory: Option<u64>, pub processes: Option<u32> }
pub struct Owner      { pub user: u32, pub group: u32 }
pub struct Controllers { pub cpu: bool, pub memory: bool, pub processes: bool }
pub struct Counts {
    pub user: Duration, pub system: Duration,         // CPU, of every process it held
    pub peak: Option<u64>, pub oom_kills: Option<u64>, // with the memory controller
}
pub enum Event {
    Made { owner: Token, cgroup: Token }, Entered { owner: Token },
    Enabled { owner: Token }, Counts { owner: Token, counts: Counts },
    Signalled { owner: Token }, Killed { owner: Token }, Empty { owner: Token },
}
// a refusal is io's Failed; a cgroup's close, its Closed
```

- **The service's own cgroup** is opened by the shell at startup
  (shell.md, 6.4), and io adopts it as it adopts a root. io never kills
  or removes an adopted cgroup: closing it closes its descriptors.
- **`Make`** makes a cgroup by name beneath another io holds, and opens
  it. It sets its limits (`memory.max`, `pids.max`), whose controllers
  must be enabled in the parent, and always `memory.oom.group` where the
  memory controller is: the kernel's out-of-memory kill then takes the
  whole cgroup, not one process of it. With `delegate`, it gives the
  cgroup to another user, as the kernel's delegation does: the directory
  and the files the kernel names delegable (`cgroup.procs`,
  `cgroup.subtree_control` and the rest) change owner, which needs
  privilege, and its depth and number of descendants are bounded
  (`Limits::cgroup_depth`, `Limits::cgroup_descendants`), so that what
  the other user makes beneath it stays within what io removes.
- **`Enable`** enables controllers for a cgroup's children. The kernel
  refuses it while the cgroup itself holds a process, so a service enables
  controllers on its own cgroup only after `Enter`ing one beneath it.
  The usual start: make `self` beneath the service's cgroup, enter it,
  enable the controllers, then make the trees' cgroups beside `self`.
- **`Enter`** moves the service's own process into a cgroup. The kernel
  allows it where the service may write both that cgroup's `cgroup.procs`
  and their common ancestor's.
- **`Counts`** reads the CPU time of every process the cgroup and those
  beneath it held (`cpu.stat`), and, with the memory controller, the
  peak of their memory together (`memory.peak`) and how many the kernel
  killed for want of it (`memory.events`). Read once the cgroup is empty,
  they are its final counts. A cgroup's processes are counted here, not
  in the service's `Usage` (6.1), whatever reparents them.
- **Closing a cgroup io made** kills what it holds, if anything, waits
  until it is empty (6.4), removes the cgroups beneath it, deepest first,
  then removes it. Its `Closed` comes after all of that, and is the proof
  that the cgroup and everything in it are gone. A process the kernel
  cannot kill (one in uninterruptible sleep on a filesystem that stalls)
  holds the close open, and io waits with it (section 10). `Abort` of a
  cgroup is its close.
- **Bounds.** A cgroup takes one entity slot and holds two descriptors,
  its directory and its `cgroup.events`, and the name it was made with.
  The operations on it are synchronous but `Empty` (kernel.md, 6.6); one
  `Empty` may be in flight on a cgroup, and a second is the layer above's
  bug, asserted.

### 6.3 Spawning a tree

A *tree* is a child spawned into a cgroup of its own, and everything it
starts. `Spawn` gains what trees need; a spawn that asks for none of it
is a child as in section 6.

```rust
pub struct Spawn {
    pub program: Box<[u8]>, pub args: Box<[Box<[u8]>]>, pub env: Box<[Box<[u8]>]>,
    pub dir: Dir,                       // beneath an io root, or a path in the view
    pub pipes: Box<[Pipe]>,             // the child's descriptor table: pipes and given
    pub cgroup: Option<Token>,          // a cgroup io holds; the service's own if none
    pub ids: Option<Ids>,               // the user and groups the command runs as
    pub contain: Option<Box<Contain>>,  // namespaces, and a view of the file system
}
pub enum Dir    { Beneath { root: Token, path: Box<[u8]> }, InView(Box<[u8]>) }
pub struct Ids  { pub user: u32, pub group: u32, pub groups: Box<[u32]> }
pub enum Way   { In, Out, Given(Token) }  // Given: a held listener (3.2) or an io root
pub struct Contain {
    pub users: Box<[Map]>, pub groups: Box<[Map]>,  // the user namespace's maps
    pub pid: bool,                                  // a pid namespace of its own
    pub network: Network,
    pub hostname: Option<Box<[u8]>>,                // a uts namespace with this name
    pub view: Option<View>,
    pub rlimits: Box<[Rlimit]>,                     // lowered, never raised
}
pub struct Map { pub inside: u32, pub outside: u32, pub count: u32 }
pub enum Network { Loopback, Host }
pub struct View {
    pub base: Base,                     // the view's root
    pub binds: Box<[Bind]>,             // io roots, where the tree sees them
    pub read_only: Box<[Box<[u8]>]>,    // paths within writable binds
    pub scratch: Box<[Scratch]>,        // empty, writable, in memory
}
pub enum Base    { Empty, Root { root: Token, path: Box<[u8]>, writable: bool } }
pub struct Bind  { pub root: Token, pub path: Box<[u8]>, pub at: Box<[u8]>, pub writable: bool }
pub struct Scratch { pub at: Box<[u8]>, pub size: u64 }
pub struct Rlimit { pub resource: Resource, pub soft: u64, pub hard: u64 }
```

- **Into a cgroup.** A child spawned with `cgroup` is in it from its
  first instruction (the kernel's `CLONE_INTO_CGROUP`), so nothing it
  starts is ever outside it, a double fork included. One cgroup may hold
  several children: smith's run, whose commands share a cgroup beneath
  its host's, is one cgroup, or one with a cgroup per command beneath it
  (section 10's trees in one process are this).
- **As other ids.** `ids` runs the command as that user, with that group
  and those supplementary groups, which replace the service's; an
  activator running as root starts a service as its user so. Outside a
  user namespace this needs privilege; inside one, `ids` are the
  namespace's, root of the namespace if absent.
- **Given descriptors.** A descriptor table entry whose way is
  `Given(token)` names a descriptor io holds, a held listener (3.2) or
  an io root, an open directory, at the entry's child descriptor; with
  the pipes, they are all the child inherits. It is a way of the table's
  entries, not a field of its own, so that it is added without changing
  a `Spawn` anyone writes. Built first for held listeners, on every
  spawn, a tree's or not: each backend's `posix_spawn` file actions,
  Linux's and macOS's, `dup2` io's descriptor to the child's number,
  which leaves it open across the exec, and io keeps its own. The
  caller leaves its `parent` empty, as a pipe's; io fills it with its own
  descriptor for the backend, and `Spawned` names tokens for the pipes
  alone, in request order. A spawn whose table names one child descriptor
  twice, or a token which is neither, is refused before anything is
  made. The simulator puts the same listener in the child's socket
  table, so that the child's claim (3.2) accepts what the parent's
  backlog held. A helper that works beneath a store the
  service opened takes it so (hearth's domain/store.md, 5.2).
- **Contained.** A contained child has a user namespace of its own,
  mapped as `users` and `groups` say, and with it mount, ipc and cgroup
  namespaces of its own; a pid namespace if `pid`; a network namespace
  with only loopback, up, unless `Host`; a uts namespace, with its
  hostname, if one is given. It runs with `no_new_privs`, with
  `rlimits` lowered, and without the service's supplementary groups.
  - **The maps** may name the service's subordinate ids
    (`/etc/subuid`, `/etc/subgid`), which `newuidmap` and `newgidmap`
    map, and the service's own ids. The service's own ids need not be
    mapped: unmapped, they are the overflow id inside, and the tree
    reaches the service's files only as any other user would. A map that
    names only the service's own group leaves the command the service's
    supplementary groups, which the namespace cannot change then
    (kernel.md, 6.6). A tree with a view must map the namespace's 0.
  - **No fallback.** A machine without user namespaces, or without
    `newuidmap` where the maps need it, fails a contained spawn at its
    namespaces, and so does a service that runs with `no_new_privs`
    where the maps need `newuidmap`, which the shell checks at startup
    (shell.md, 6.4). A service whose own `/proc` or `/sys` has parts
    mounted over, as a unit hardened by its supervisor may, fails a view
    for its new ones: the kernel lets a user namespace mount them only
    where the host's are whole. A service checks at startup, by spawning
    a tree with a view, when it must not start without them.
- **The view** is the file system a contained tree sees, composed in its
  mount namespace before the command starts:
  - **its root:** an empty one in memory (`Empty`), or a directory io
    holds, writable or not, given as the io root and its path on the
    host;
  - **binds:** directories io holds, each where the tree sees it,
    writable or not; within writable ones, `read_only` paths, such as a
    repository's git directory; and `scratch`, writable directories in
    memory, each bounded;
  - **always:** a new `/proc`; `/sys`, new and read-only, or with `Host`
    the host's, read-only; a minimal `/dev` (`null`, `zero`, `full`,
    `random`, `urandom`, `tty`, its own `pts` and `ptmx`, `shm`, and the
    `fd`, `stdin`, `stdout` and `stderr` links); a private `/tmp`.
  - **Paths stay beneath the view.** Each place a mount goes is resolved
    beneath the view's root, following no link; one missing is made only
    where the root is writable or `Empty`. A bind is found by its path
    on the host, as the service's own user, and checked to be the
    directory io holds (its device and inode); what was checked is what
    is mounted, so a path changed under it fails the view instead of
    binding something else. Once composed, the root is read-only unless it was
    given writable, and the command starts at `dir` in the view.
- **Spawned, or failed.** io answers `Spawned` once the command is
  executing, after the whole of the setup, or `Failed` naming the stage
  that failed (`Spawn(Stage)`: the cgroup, the ids, the namespaces and
  their maps, the view, or the command itself) and the kernel's error,
  then `Closed` once the child is reaped. A helper killed during the
  setup, by its owner's deadline or for want of memory in its cgroup,
  reports nothing: io answers `Spawned`, and the child's exit then says
  the signal.
- **The child io holds** is the tree's first process. When the spawn
  asks for ids or containment, it is a helper (kernel.md, 6.6) that
  relays the command: what it is signalled it passes on, and it exits
  as the command did, with its status, or by its signal. In a pid
  namespace of its own, the command's end ends the tree: the kernel kills
  what the command left in the namespace.
- **Bounds.** A spawn's maps, binds, read-only paths, scratches, rlimits
  and the bytes of their paths are bounded (`Limits::contain`), so that
  the whole of it fits the helper's 64 KiB (kernel.md, 6.6); past them, the
  spawn is refused at io's entrance. A given root takes no slot of its
  own. The helper's memory is its own process's, outside the service's
  worst case.

### 6.4 Stopping a tree, and its end

| Step | The request | What reaches the tree |
|---|---|---|
| polite | `Signal { child, Terminate, Child }` | the first process, which passes it to the command |
| terminate | cgroup `Signal { cgroup, Terminate }` | every process in the cgroup and beneath it |
| kill | cgroup `Kill` | every process in it and beneath it, at once (`cgroup.kill`) |

- **Each step, and its deadline, is the owner's,** as everywhere in io
  (programming-model.md, 4): a tree's owner signals politely, gives the
  command its time, terminates, kills.
- **Terminate is one pass,** over the processes the cgroup and those
  beneath it list. Each is opened as a pidfd and checked to be in the
  tree still before it is signalled, so a PID reused meanwhile is never
  signalled. A process forked during the pass may be missed: kill is the
  step that leaves none. The command may hear the signal twice, from the
  pass and relayed by the first process.
- **Empty** answers once the cgroup and those beneath it hold no live
  process (`cgroup.events`, `populated 0`). A process that has exited
  but is not reaped is not live, so a tree's first process, which io
  keeps unreaped until its close, does not hold the tree open. io reads
  the state, and while it is populated waits for the kernel's next
  change and reads it again, so no end is missed. `Empty` may come
  before the first process's exit is observed. A cgroup that is empty
  is filled again only by a spawn into it, or by a process moved in.
- **Escape.** A process leaves its cgroup only by moving itself, which
  needs the right to write the `cgroup.procs` of where it goes and of the
  common ancestor. A tree that runs as the service's own user, in no
  user namespace, can move into the service's other cgroups on purpose.
  A tree run as another user cannot, and nor can a contained one: it has
  a cgroup namespace of its own, and the shell checks that cgroup2 is
  mounted with `nsdelegate` (shell.md, 6.4), which forbids a move out of
  it. A service that runs a command it does not trust contains it.
- **Nesting.** A tree may make cgroups beneath its own where its ids may
  write there: when the service runs it as its own user, or when the
  cgroup was delegated to its user. Closing the cgroup removes them, and
  stops what is in them.
- **The end.** A tree has ended when its cgroup is empty: its pid
  namespace, if it had one, has ended, and the mounts of its view are
  gone with the last process that held them. Its owner then
  closes the child (its exit observed, its pipes settled, reaped) and the
  cgroup (removed), in either order, and hears each one's `Closed`. A
  cgroup closed while it holds processes is killed first.

## 7. Signals to the service

Termination signals are blocked at startup (shell.md, 6), read from a
signalfd through the ring, and arrive as `Shutdown` events. The domain
decides what shutting down means.

`Shutdown { signal: ServiceSignal, from: Option<Token> }` names the sender
only when it is a live child this io spawned: `from` is `Some(child)` after
that child's successful `Spawn` completion and before its `Exited` event.
The kernel boundary carries `Signalled::Termination { signal, sender:
Option<kernel::Sender> }`; io compares that opaque identity for equality
with the identities returned by `Done::Spawned { pidfd, sender }` (section 6).
`from` is `None` for the kernel, a terminal, this process itself, an
unrelated process, or a child whose exit io has reported, even before its
reap. Linux reads the identity from signalfd's `ssi_pid`. macOS's kqueue
signal source has no sender identity, so `from` is always `None` there.

A service at a terminal also blocks `SIGWINCH` at startup (shell.md,
6.3). It is read from the same signalfd and arrives as `Resized`, after
which the owner asks for the size: `Window { owner, stream }`, naming the
stream that reads the terminal, answered `Window { owner, size }`, or
`Gone` if the terminal has hung up, with rows
and columns, its one terminal (kernel.md, 6.3).

## 8. Testing

io's tier is io worlds (testing-strategy.md, 2.6): io over the
simulator, with a scripted owner as the step above it. It listens,
connects, streams, opens files and spawns, in an order the test chooses,
with closes and aborts in every state, under tiny limits and every fault
the simulator injects. The worlds check what io adds: buffers in flight,
cancellation and settling, graceful close, the receive and send queues
under their caps, and the accept budget; and every contract and
invariant of testing-strategy.md, 6.

Files and processes go to skein's minimal fake machine: a few files
beneath a root, a program that echoes its input, one that exits with a
given status, one that never exits. Its files lie behind the
simulator's machine seam (testing.md, 4).

- **Step tests** (`crates/skein-io/src/tests/`): each cell of both
  machines driven by hand, the refusals at a full slab, stale tokens
  dropped going down and asserted going up, every outcome of a cancel in
  every order, each call with exactly its `MAX_OUT` of room; sends within
  the room granted, and one past it, or with none granted, asserted.
- **io worlds** (`tests/io`, `skein-io-world`): a loop over the
  simulator drives each process's io and a scripted owner, which runs
  each connection by a plan of sends within the room granted and demands
  of every kind; a referee holds each scenario's expectations; the harness
  checks `MAX_OUT` and the accept batch at each call, both halves of the
  stream contract in a ledger (io's, and the owner's as the side above),
  the invariants once settled, and replay. Thirteen scenarios: accept,
  bind and reject; connects made, refused for a slot and by the peer;
  connects waiting on a full backlog, cancelled; descriptors run out, so
  a socket is refused and a listener starves; two listeners under one
  accept batch; a socket discarded for want of a slot; a burst of
  connects past the slab and the refusals; an exchange both ways;
  backpressure through io; a refusal mid-upload that still reaches the
  peer, which hears the end and no reset; abort; the close deadline;
  closes and aborts at random moments. Each admission point's scenario
  checks its trace for evidence the point was reached. A few seeds each,
  calm and chaotic, in the focused suite; 150 of each in the fuzzy one,
  which asserts that every fault fell and that a cancel of each operation
  io cancels was seen to stop it, to come too late, and to go
  unsubmitted.
- **One exchange over the real ring**, in the focused suite.
- **Memory:** io driven by hand to its limits and back, every call
  checked against `worst_case` with the counting allocator, at four sets
  of limits, one of them with receive buffers that dwarf the rest; and a
  listener's life, its sockets announced, rejected, bound and discarded.
- **Processes and pipes:** step tests exercise spawn, wait, signal, the
  one-way pipe streams and child closure after its pipes. A separate
  counting-allocator test fills the entity slab with a child and its
  pipes, checks the spawn completion and arms a read on every pipe
  against `worst_case`.
- **Files:** step tests drive `FileIo` by hand
  (`crates/skein-io/src/tests/file.rs`, `store.rs`): short transfers,
  bounds refused after the close, a scan's prefix across batches, each
  request's deadline, cancels that win and that come too late, and every
  step of a store, its conflicts, a taken temporary name, and a rename
  that settles at its deadline. The kernel records for files are held to
  the ring by the conformance suite (kernel.md, 8), and kv's tests run
  the driver over the simulator, on a fake machine that crashes at every
  step of a commit and a snapshot (kv.md, section 9). A world that cuts
  a power loss lays nothing its requests make: its private root and its
  directories start absent, so a cut after each make and before its
  answer finds the directory there only if the request answered it.
- **Process groups:** on the real kernel, a child that starts a
  descendant holding its pipe and exits, then a signal to its group,
  which ends the descendant and the pipe; a close of a child whose group
  still runs; a signal to a group after its leader exited, before its
  close. In the simulator, groups the minimal machine's programs make,
  and the same cases.
- **Cgroups and trees:** step tests of each cgroup request and its
  refusals, of a close that kills, waits, and removes deepest first, and
  of a contained spawn's every stage failed in turn. io worlds over the
  simulator: trees whose first process exits before the rest, whose
  processes ignore the polite signal or the termination, that fork
  during a terminate, that escape their group but not their cgroup, and
  whose end the kernel reports late. On the real kernel, the conformance
  suite's (kernel.md, 8).
- **A tree's metadata and its filesystem:** step tests of each request,
  its bounds refused before any record, each record's errors, a deadline
  that passes while a synchronous record runs (the request still answers
  its own), and `SubvolumeChildren`'s pages; the records themselves are
  the conformance suite's, btrfs's on the ring alone (kernel.md, 8). A
  world over the simulator, on a fake machine that cuts a power loss,
  shows a link's name kept only once its directory was synced, and a
  mode, an owner and an attribute only once the filesystem was.
- **Append streams:** io worlds over the simulator with sends within the
  room granted, short writes, a close that flushes, and the simulator's
  hung `Write`, met by the write deadline while open and by the close
  deadline while closing; the conformance suite, on the ring and the
  simulator, for a write at the end of a file opened to append.
- **Durable replace and private files:** every step of the replace
  failed in turn, the temporary removed each time, and a conflict; a
  private directory and file refused for each bit, link and kind; and a
  cut at each `Sync` and `Rename` (simulator.md, 3.3), after which the
  target is old or new and whole.
- **Resource usage:** in the simulator, a usage asked before and after a
  child's close, its part joining the children's only at the close; on
  the real kernel, the conformance suite's (kernel.md, 8).
- **Joined sockets** (3.5): step tests of each cell the table changes,
  and a check at every call that a joined end submits nothing. io worlds
  with scripted owners at both ends of each pair, in one process: accept,
  bind and reject, the connector then reset; connects refused by an
  unbound name and by a closing listener; connectors waiting on a
  listener with no room or no slot, one aborted as it waits; two
  listeners under one accept batch; a name taken, then taken again after
  its listener's `Closed`; every name held; an exchange both ways under
  intakes small enough to cut every move; backpressure through io; the
  refusal mid-upload that still reaches the peer, which hears the end and
  no reset; abort, heard as a reset; the close deadline against a peer
  that never ends; closes and aborts at random moments. The ledger holds
  both halves of the stream contract as for sockets. A few seeds each in
  the focused suite, the random one in the fuzzy suite.
- **The same owners over either:** a calm world whose owners run the
  same plans over a loopback pair and over a joined pair, and read the
  same bytes, ends and failures over each.
- **Memory:** every name held and every slot a joined end, against
  `worst_case`.
- **One process, on the real ring:** a listener on loopback and one on an
  in-process name in one `drive`, an exchange over each.

## 9. Remaining work

What of this document is not built is `STATUS.md`'s (its row in section 2,
and section 3).

## 10. Open questions

- **A stream's fault for a stall.** A write deadline fails an append
  stream with lib's `Fault::Other`, as a full disk does. Whether `Fault`
  should name a stall, so its owner can tell a filesystem that stopped
  from one that refused.
- **Operations the kernel cannot interrupt.** A cancelled write on a
  filesystem in uninterruptible sleep keeps its stream settling, and its
  process from ending (notes.md). So does a cgroup whose process sleeps
  so, which its close waits for (6.2).
- **Filters on a contained tree's calls.** A contained tree has no
  seccomp filter, and keeps the capabilities its ids have in its
  namespace. Whether io offers a filter, when a service needs one.
- **A terminal for a tree.** A contained tree's command has pipes, never
  a terminal; whether a tree may be given one, when a service needs it.
- **Faults between joined ends.** The simulator's faults never reach a
  joined pair (3.5), so an application's one-process worlds meet only
  what the owners at either end do. Whether io gives a world a way to
  reset a pair, or cut a move short, as a socket world can.

# The examples

Provisional, 2026-10-04, revised 2026-10-15. The design of skein's
example services (testing.md, 3): what each one is, the steps README.md
asks for before code (the wire protocol, the limits, the entities, the
state machines), its fakes, the harness its worlds run on, and how it is
tested; and of the UI example, an application on skein-ui (section 9).
The examples exist to be tested and to be copied: a service starts from
them. The echo server is built; the HTTP server and its client are not,
nor is the UI example (section 8).

## 1. In one page

- **Laid out as a service is** (README.md, "Building a service on
  skein"): a domain, a protocol layer, a service with its `iterate` and
  the sum of its worst cases, and a shell with its `main`, each a crate.
  The step crates are in the model's subset, under the workspace's lints.
- **Their fakes are step machines** (testing-strategy.md, 4), so that the
  simulator and the real loop host them as they host the service.
- **One harness hosts them all:** `skein-world`, the loop of a simulated
  world over the simulator, with its trace, its memory check and its
  referee; and the same loop over the shell's kernels, on the real clock.
- **Tested in simulated worlds and on the real loop,** under tiny limits
  and every fault the simulator injects.
- **An application with a face, too.** The UI example (section 9), a
  small notes application on skein-ui, with its browser shell and a
  server on skein's HTTP, is the reference an application's browser
  shell is copied from, and is tested in the browser tier (ui-web.md,
  section 14).

## 2. Layout

```
examples/echo/domain        skein-echo-domain     sessions, and the answer to each line
examples/echo/protocol      skein-echo-protocol   the listener and the connections: lines to calls and back
examples/echo/service       skein-echo-service    iterate, the limits, the sum of the worst cases
examples/echo/shell         skein-echo-shell      main: configuration, startup, the loop; the binary skein-echo
testing/skein-echo-client   skein-echo-client     the fake echo client, a step machine
testing/skein-world         skein-world           the world harness: simulated worlds and the real loop
tests/echo                  skein-echo-world      the echo's simulated worlds, and its real loop
```

The crate graph is README.md's: the domain depends on lib; the protocol
layer on lib, io and the domain; the service on all three, and re-exports
the domain's and the protocol layer's crates; the shell on lib, io, the
service and `skein-shell`; the worlds on lib, io, the service, the fake
client and skein's testing crates. Every crate may name lib, and every
one but the domain's may name io, as the role graph has it
(programming-model.md, 4): the shell names io's limits and records, and
reaches the domain and the protocol layer only through the service. The
fake client shares no type with the echo (testing-strategy.md, 4): it
depends on lib and io only.

The UI example's crates (section 9):

```
examples/ui/view        skein-ui-example-view       the notes' state, view, drawing code, phrasebook, codec for places and records: step code
examples/ui/shell-web   skein-ui-example-shell-web  the reference shell: its start, its loop, its look and stylesheet, its component and its snippet; wasm32 only
examples/ui/server      skein-ui-example-server     its server on skein's HTTP: the bundle, its exchanges, its stream; the binary skein-ui-example
tests/ui-web            skein-ui-web-tests          the browser parts' host tests, and the UI example's browser tier
```

The view depends on lib, the codec and skein-ui, and builds for the host
and for `wasm32-unknown-unknown`; the browser shell on the view and
`skein-ui-web`, with the glue; the server on lib, io, HTTP, the shell
kit and the view, whose records it exchanges. Neither the view nor the
server depends on the browser shell, which the server serves as bytes in
the bundle.

## 3. The echo, before code

### 3.1 The wire protocol

A foreign-style line protocol, so scanned framing (programming-model.md,
8):

- **A request is a line:** any bytes but `\n`, then `\n`, at most
  `Limits::line` bytes with it.
- **The answer to a line is the same line,** its `\n` included. One
  request is in flight per connection: the server asks for room for the
  next answer before it reads the next line.
- **Refusals are small and fixed.** `busy\n`, as the first and only
  answer, when the domain has no session for the peer; then the server
  closes. `too long\n`, when a line passes `line` bytes without its `\n`;
  then the server closes, and io's graceful close discards the rest.
- **Idle:** a connection that sends no line for `idle` while the server
  waits for one, or reads nothing for as long while the server waits for
  room for its answer, is closed, gracefully and without a word. Each
  deadline is spread by up to `spread`, drawn from the seed, so that
  connections opened together do not all expire in one iteration.
- **End:** a peer that half-closes has every complete line answered, then
  the server closes gracefully. A last piece of a line, with no `\n`, is
  dropped.
- **Past the protocol's slab** of connections, an accepted socket is
  rejected: io closes it without a word.

### 3.2 Limits and the worst case

| Layer | Limit | What |
|---|---|---|
| domain | `sessions` | sessions at once: an `Open` past it is answered `Busy` |
| protocol | `conns` | connections at once: an accepted socket past it is rejected |
| protocol | `line` | the longest line, `\n` included, and the room each answer asks for: at least the longest refusal |
| protocol | `idle`, `spread` | the idle deadline, and how far it is spread |
| protocol | `retry` | how long the listener waits to listen again after io refused it for want of resources |
| io | io.md, 2 | `intake` and `output` at least `line`: the protocol's largest read and room |
| service | `queue` | each queue between the stages, at least the largest `MAX_OUT` |

```
worst case = io::worst_case(io)
           + domain:   the slab of sessions
           + protocol: the slab of connections, its deadlines and ready list,
                       conns × line (one line in flight per connection:
                       the bytes read, the domain's text, or the answer
                       before io takes it), and one line more (a step's
                       copy while it decodes or encodes)
           + service:  its queues' containers
```

Startup refuses a worst case past the configured memory, and io limits
that cannot meet the protocol's largest demand (io.md, 2): `line` past
`intake` or `output`.

### 3.3 Entities

| Layer | Entity | Made by | Owned by | Named by | Ends |
|---|---|---|---|---|---|
| protocol | the listener | startup | the protocol layer | a fixed owner token below; io's listener token | io's `Closed` |
| protocol | a connection | an accepted socket it binds | the protocol layer, which owns io's socket | `Id<Conn>`: io's owner token, and the `ReplyTo` of its calls | io's `Closed`, with no call out and its session told gone |
| domain | a session | an `Open` admitted | the domain | `Id<Session>`, as a token the connection holds | `Gone` |

The domain's vocabulary (programming-model.md, 4.4): calls up, `Open {
reply_to }` and `Line { session, reply_to, text }`, answered by `Reply {
to, reply }` with `Admitted { session }`, `Busy` or `Echo(text)`; and
events up, `Gone { session }` and `Shutdown`, the latter answered by the
request `Stop`: admit no one more. The text of a line is its bytes without
the `\n`: framing stays in the protocol layer, which copies the text out
when it decodes and writes the answer at its length when it encodes.

The protocol layer tells the domain `Shutdown`, from its ready list,
when io's `Shutdown` event arrives (io.md, 7), or when the service's
`shutdown` asks it to, which worlds below the binary call. The domain
decides what it means: it admits no one more, and asks for the listener
to stop. The connections it has drain (programming-model.md, 5.2): one
waiting for the peer's next line, or for its greeting's room, is idle and
closes at once; one with a line out answers it, then closes. None waits
for its idle deadline, so the echo ends without one firing
(testing-strategy.md, 6).

### 3.4 State machines

**The connection.**

| State | Holds | Demand | Deadline |
|---|---|---|---|
| Greeting | io's socket | room for an answer | idle |
| Admitting | socket; whether the peer ended | | |
| Reading | socket, session | a scan to `\n`, at most `line` | idle |
| Answering | socket, session; whether the peer ended | | |
| Draining | socket, session | room for an answer | idle |
| Closing | whether io told `Closed`; whether a call is out; the session to tell `Gone` | | |
| Closed | | | |

The demand and the deadline are one function of the state, applied after
every transition: entering Greeting, Reading or Draining states its demand
and arms the idle deadline afresh, so that each line read and each room
granted is progress (programming-model.md, 7).

| State | Event | Next, and what it does |
|---|---|---|
| Greeting | `Room` | Admitting: `Open` |
| Admitting | `Admitted` | Reading; or, if the peer ended, Closing: `Close`, `Gone` owed |
| Admitting | `Busy` | Closing: `busy\n`, `Close` |
| Reading | `Bytes`, a line | Answering: `Line` |
| Reading | `Bytes`, no `\n` | Closing: `too long\n`, `Close`, `Gone` |
| Answering | `Echo` | Draining: the answer sent; or, if the peer ended, Closing: the answer, `Close`, `Gone` owed |
| Draining | `Room` | Reading |
| Admitting, Answering | `End` | the same, the end noted |
| Greeting, Reading, Draining | `End` | Closing: `Close`, `Gone` if admitted |
| Greeting, Reading, Draining | the idle deadline | Closing: `Close`, `Gone` if admitted |
| any open state | `Failed` | Closing: `Close`, and `Gone` if the session is known; in Admitting, once the domain answers |
| Closing | the reply to its call | `Admitted`: its session owed `Gone`; `Busy`, `Echo`: dropped |
| Closing | `Bytes`, `Room`, `End`, `Failed` | ignored: told before io took the close |
| Closing | `Closed` | retired once no call is out and nothing is owed |
| Closing | resumed | `Gone` told |
| any but Closing | `Closed` | impossible: io tells it only after the close |
| any without the demand | `Bytes`, `Room` | impossible: io answers only a demand |
| any without a call out | a reply | impossible: one reply per call |
| any without the deadline | the idle deadline | impossible: cancelled on leaving |

A `Gone` owed by a down-pass cell (a reply that finds the connection
closing) is up from the down pass (programming-model.md, 2): the
connection goes on the protocol's ready list, and `resume` tells it.
`Gone` always comes after the session's last call in the domain's queue,
which is first in, first out: a stream that fails while its line is out
tells `Gone` at once, behind the line, which the domain answers first. So
the domain never answers for a session it has retired. A connection
retires once io told `Closed`, no call is out, and its session was told
gone: the bindings above and below have both ended (programming-model.md,
5.2).

**The listener.** The address it listens at is the layer's, from its
making.

| State | Holds | Deadline |
|---|---|---|
| Unopened | (on the ready list) | |
| Opening | whether the domain asked it to stop | |
| Listening | io's listener token, the address bound | |
| Closing | its failure, if one came | |
| Failed | its failure; whether the domain asked it to stop | |
| Backoff | when it listens again; the shortage that refused it, for `main` to report | `retry` |
| Closed | its failure, if one came | |

| State | Event | Next, and what it does |
|---|---|---|
| Unopened | resumed | Opening: `Listen` |
| Opening | `Listening` | Listening, its address kept; or, if stopped, Closing: `Close` |
| Opening | `Failed` | Failed: io closes it |
| Opening | `Stop` | Opening, stopped |
| Listening | `Accepted` | a connection: `Bind`, Greeting; or, the slab full, `Reject` |
| Listening | `Failed` (its accept stopped) | Closing: `Close` |
| Listening | `Stop` | Closing: `Close` |
| Closing | `Accepted` | `Reject`: announced before io took the close |
| Closing | `Failed` | its failure kept: told before io took the close |
| Failed | `Closed` | `Busy`, and not stopped: Backoff, its deadline `retry` on; `Busy` and stopped: Closed; otherwise Closed, the failure kept |
| Backoff | its deadline | Opening: `Listen` |
| Closing | `Closed` | Closed |
| Unopened, Backoff | `Stop` | Closed |
| Failed | `Stop` | Failed, stopped |
| Closing, Closed | `Stop` | ignored |

io's `Busy` says that nothing was made and the request may be made again
later (io.md, 2): a listen refused for want of descriptors or buffers is a
shortage, not a failure, so the listener listens again after `retry`.
Any other failure stops it for good, and the service with it: `main` then
says why and exits. A failure is kept whether a stop came meanwhile or
not; a shortage never counts as one, but Backoff keeps it, and `main`
says once that the listen is refused and retried, so that a shortage that
lasts is seen. Retrying is the protocol layer's policy, kept where the
listener is.

**The session** is active from `Open` admitted to `Gone`, and counts the
lines it answered. **The domain** admits until `Shutdown`, then answers
every `Open` with `Busy` and asks once for `Stop`.

## 4. The echo's main

`main` reads its configuration from its arguments (the address to listen
on, and the memory it may take), runs startup (shell.md, 6): the worst
case against the memory, io's caps against the protocol's largest demand,
the termination signals blocked and their signalfd opened
(`open_termination_signals`) for io to adopt, then the seed and the
kernel; and then the loop of programming-model.md, section 2, over the
shell's `Kernel` and `Clock`.

- **It ends by itself.** `SIGINT` or `SIGTERM` arrives as io's `Shutdown`:
  the echo admits no one more, its connections end (section 3.3), and the
  loop exits once the service holds nothing, with a success (shell.md,
  13). A listener that fails, as when the address is in use, ends it too,
  with the failure on standard error.
- **Its loop is `drive`** (shell.md, 12), over the echo's `Host`, from
  skein-shell. Its hook says once that the echo listens, and at
  what address, and once that a listen refused for want of resources is
  being retried: the startup facts emitted between `iterate` and the
  submit.

## 5. The fake echo client

`skein-echo-client` is a step machine (testing-strategy.md, 4): io and
one layer above it, which plays the line protocol over io's stream
vocabulary, with an `iterate` of its own and its own limits and worst
case. It shares no type with the echo. Each connection follows a plan:

- when it connects, how many lines it sends and their lengths, drawn
  from its seed, one of them past the server's limit if the plan says;
  how many it sends ahead of their answers, in pieces of what size;
  from when it reads, and after how many bytes it stops sending;
- what it does after its last answer: half-close and wait for the
  server's end, close, abort, or linger until the server closes; or an
  abort at a given time, whatever it is doing;
- how often it tries again, after a refusal or a broken stream, and how
  long it waits first.

It holds no copy of what it sent: a line's bytes are drawn from the
seed, and drawn again, from the same seed, to check its answer. It checks
its peer as it goes, failing the world on the first breach: each answer
is the oldest line unanswered, byte for byte; `busy\n` comes only first;
`too long\n` only to the line past the limit; nothing comes after either
but the end. What it saw (answers, refusals, the server's end, failures,
the bytes it handed io, when it closed) is kept for the referee.

## 6. The world harness

`skein-world` (testing-strategy.md, 7) is skein's: its examples are the
second user of a world. It is ordinary Rust, generic over a scenario's
processes, each a host of an `iterate` (a service, or a fake client):

- **One loop** drives every process over the simulator: each reaps,
  iterates and submits, in turn, and the referee observes each.
- **Time** moves only when the world is idle, to the earlier of what the
  simulator has due and the processes' and the referee's earliest
  deadline (simulator.md, 3).
- **Trace and replay:** a run returns the simulator's trace, which the
  same seed replays.
- **Memory** (simulator.md, 5): under the counting allocator, each
  process's heap, what grew within its own calls (building it, and each
  `iterate`), at its peak within each call, against its own worst case.
  What grew within the simulator's calls and the harness's is left out,
  by metering around the processes' calls rather than theirs; and as
  nothing one process allocates is freed by another or by the simulator,
  each process's growth is its own. A process frees in one call what it
  allocated in another, so the heap falls below where a call began: the
  counting allocator's span measures a call's growth signed, where its
  meter, which checks a step from a base it never falls below, cannot. A
  process's worst case counts the box the harness keeps its state in.
  Once settled, dropping the run's outcome drops each process under a
  span: it must free exactly what was metered as its own, which finds a
  leak, and heap made or freed outside its calls.
- **Contracts:** each process keeps its own as it goes, as its loop is
  its own: the echo's and the fake client's `iterate` assert each call
  within its `MAX_OUT`, the protocol layer one reply per call, the fake
  client the echo's contract; the simulator, the kernel boundary's.
- **Settled:** once the referee passed and nothing is busy, every process
  holds nothing, the simulator has nothing in flight, and every
  descriptor is closed.
- **The shell's own `Host`** (shell.md, 12), which `skein-world`
  re-exports: the harness calls each process's hook where `drive` does,
  after its `iterate`, and checks the teardown invariant
  (testing-strategy.md, 6): after the scenario's last word, no deadline
  but io's own may fire before the world settles. The echo's idle
  deadline runs at its shipped value or beyond the world's horizon,
  unless the scenario is about it.
- **The referee** holds a scenario's expectations, each with a deadline:
  safety on every observation, liveness as the deadline; it may also
  inject what belongs to no fake. The echo's tells the fake clients the
  echo's address once it listens, as a directory would, and shuts the
  echo down once the clients are done, or at a given time, so that the
  world settles. Both reach into the service where the referee should
  only watch from outside (testing-strategy.md, 7): its reading of
  `svc.listening()` stands in for the "listening at" line `main` prints,
  which the service does not emit as a fact; and its call of
  `svc.shutdown()` for the termination signal, which the end-to-end test
  sends the binary instead.
- **The real loop** runs the same processes and referee over one ring, in
  one thread on loopback, with deadlines on the real clock. An idle loop
  blocks on the ring until the next completion or deadline.
  - **Each process's operations stay its own.** The harness renames each
    token to one unique in the world, and back as its completion is
    reaped. It holds each process to the operations in flight its own
    ring would allow, and the ring has room for all of them.
  - **A spawned service is hosted, not run.** Its spawn never reaches the
    kernel: the harness makes the child's pipes, hands the parent its
    ends, and starts the child in the same loop with the other ends, as
    the simulator does (simulator.md, 3). The parent's wait ends once the
    child has finished and the harness has closed what it still held, as
    the kernel would at its exit. A kill drops the child and closes
    everything it held.
  - **Signals.** One service reads the process's signalfd, as it would
    alone, and the referee sends it real signals. A process's signals
    cannot be aimed at one of the services it hosts, so every other
    service, a hosted child among them, reads its signals from a pipe the
    harness gives it. A parent's signal to a hosted child, or the
    referee's, arrives there as a signal record.
- **End to end** (testing-strategy.md, 2.9), the harness starts a
  service's binary as a child, as it ships, on pipes or under a
  pseudo-terminal. The test's ends of the pipes, or the terminal's other
  side, join the test's loop as streams of its scripted processes. The
  child's exit arrives in the loop as an event, and the referee may send
  the child a signal. Nothing of the child is hosted: it runs its own
  loop.
  - **Its tree is the kit's to settle and to count.** A binary may start
    processes of its own, which may leave its group or its session. None
    may outlive the test, and a test that measures what the binary used,
    a benchmark's harness among them, wants all of it: CPU time and peak
    memory. The kit settles and counts the binary's whole tree, one of two
    ways, and the test's result says which ran.
  - **A cgroup, where the machine delegates one.** When the test's user
    holds a delegated cgroup v2 subtree (a systemd user scope, a
    container's), the kit makes a cgroup per binary and starts the binary
    in it (`CLONE_INTO_CGROUP`), so every descendant is in it, whatever
    group or session it makes. The tree has settled once the cgroup is
    empty: `cgroup.events` says it is not populated, and the kit is told
    when that file changes. Its counts are the cgroup's: user and
    system CPU from `cpu.stat`, and the tree's peak memory, all its
    processes at once, from `memory.peak`.

    This is the first use of the kernel's cgroup facilities that
    contained trees build on (draft/process.md). It is written as a tree
    in that draft's terms (spawned into a cgroup, empty, killed, counted)
    so that it can move into io whole. When io gains trees, the kit
    starts its binary as an io tree and drops its own cgroup code; the
    walk below stays only for machines with no delegated subtree.
  - **A walk of pidfds, otherwise.** The test process is made a
    subreaper (`PR_SET_CHILD_SUBREAPER`), so a descendant whose parent
    exits is reparented to it rather than to init. The kit holds a pidfd
    for the binary from its spawn, and opens one (`pidfd_open`) for each
    process it finds among its own children (`/proc/self/task/*/children`):
    when the binary exits, whenever one it holds exits, and at a short
    period while any runs. It waits on each pidfd, never on any child, so
    it takes no other part's children. The tree has settled once it has
    reaped every one. Its counts are the kernel's count of reaped
    children (`Usage`, kernel.md, 6.3), taken before the binary starts and
    after the tree settles: CPU summed over the tree, and a peak that is
    the largest one process reached, not the tree's at once, which only a
    cgroup gives. The kernel's peak is the largest over every child the
    test process ever reaped, so it is this tree's only where nothing
    larger was reaped before: a test that measures by the walk starts one
    binary per process. A process orphaned below a parent the kit does not
    hold is reparented to the test process all the same, and the next
    listing finds it.
  - **The end.** Once the binary has exited and the test is done with
    it, or at the test's deadline, the kit kills what is left of the tree
    (`cgroup.kill`, or each pidfd it holds and the binary's group), waits
    for it to settle, then reads the counts and removes the cgroup. A
    test that expects the tree to end with its binary fails on a process
    still running at its exit, naming it; one that measures a binary it
    does not own only kills and counts.

## 7. Testing

- **Step tests** in each example crate: every cell of the connection and
  the listener, the domain's admission and refusal, `iterate` with its
  `MAX_OUT` reserved at each stage, the worst case and the startup
  checks; and the fake client's own cells.
- **Simulated worlds** (`tests/echo`): the echo and fake clients as
  processes of the simulator, under tiny limits, calm and chaotic: many
  clients; a line too long; a busy refusal at the entrance, and a
  rejection; an idle timeout; a client that stops reading, stopped at the
  server by backpressure and then idled out, its close discarding the
  rest; closes and resets in every state; a shutdown; a half-close with
  lines unanswered, every whole line answered before the end and a
  trailing piece of a line dropped (section 3.1's end, tested); and the
  echo driven to its worst case. Calm, each scenario holds whole; under
  faults, a stream may break, so each expects only that every connection
  finishes, while the fake clients still check every answer they get, and
  the idle deadline's "not before" is calm's alone: a server whose side of
  a stream broke ends it at once, which the client cannot tell from an
  early idle. So that the faulted sweep still shows the echo at work,
  every outcome a client can see must appear over it: served, busy,
  rejected, broken, too long, idled out. Each admission point's scenario
  checks what the clients saw for evidence that it was reached; the
  protocol layer's rejection shows as the end without a word, or as a
  reset when the client's first line was already there unread. A rare
  cell, a stream failing while its line is out with the domain, is pinned
  by its seed. Focused seeds in the default suite; sweeps in the fuzzy
  one, asserting that every fault fell. Every binary checks memory at
  every iteration, and that each process frees what it held once
  settled; io holds the echo to the room it was granted in every world.
- **The real loop** (`tests/echo/tests/real.rs`): the echo and its fake
  clients in one loop over the shell's ring, on loopback, quick, failing
  clearly where `io_uring` is unusable.
- **End to end** (`tests/echo/tests/end_to_end.rs`, built with the
  shell's package): the shipped binary against two fake clients, seen
  only through loopback, its stderr and its exit, stopped by SIGTERM.

## 8. Not built yet

- **The HTTP server and its client,** with `skein-http`.
- **The echo's `main` over `drive`,** with its hook (section 4), and its
  worlds under the teardown check (section 6), once `skein-shell` has
  both (shell.md, 11); with them, its connections draining at a shutdown
  rather than running to their end (section 3.3).
- **Domain worlds** for the echo's domain: it is one slab and a counter,
  and its step tests cover it.
- **The UI example** (section 9): its three crates and its browser
  tier.

## 9. The UI example

A small notes application on skein-ui: the reference an application
with a face starts from, as the echo is a service's. It is an
application's three pieces, a crate each: its view (step code), its
browser shell (the **reference shell**, which applications copy), and a
server on skein's HTTP. It carries one of each thing ui.md and ui-web.md
let an application write, so that the browser tier tests each once: a
form with entry kinds and a select, a dialog, a focus request, a drawing
acted on, an element of its own, a component, one JavaScript snippet,
two places, the small store, exchanges and a stream.

### 9.1 What a person sees

- **One owner,** the example's (ui.md, 3.3), with the look's prefix
  `notes`: its style tokens and region names start with `notes-` (ui.md,
  section 8). Its words are English, from its phrasebook (ui.md,
  section 9).
- **The list's page,** at `/`, under the outline (ui.md, section 14):
  - *the banner:* the example's name, and a status saying whether the
    page is live with its server;
  - *the main:* a level-1 heading; the notes, a list keyed by note, each
    item its title, its due date as a time, its label, and a link to the
    note's page; and **the form for a new note**: a title (a text field,
    required, submitting on enter), a body (a text area, the
    component's: 9.4), a due date (the date entry kind), the minutes it
    takes (the number entry kind), a label (a select of three options:
    idea, task, reference), a button that adds it, and one that clears
    it. A field the view finds wrong is invalid, its description saying
    why (ui.md, 3.2). Adding a note, or discarding a draft, moves the
    focus to the title by a focus request (ui.md, 3.2);
  - *a complementary landmark:* the chart (9.4), and beside it a choice
    group showing all notes or one label's;
  - *a dialog,* while the person clears a form holding words: whether to
    discard the draft, with a button that discards it and one that keeps
    it, which names Escape (ui.md, 6.1).
- **A note's page,** at `/notes/<n>`: in the main, a region named by the
  note's title and carrying `notes-card` (9.4), holding its title as the
  level-1 heading, its body, its due date, its minutes and its label;
  and a link back to the list. Any other path shows a page saying there
  is no such note.
- **Its places** are printed and parsed by its own codec, in its view
  (9.2; ui-web.md, section 9).

### 9.2 Its view

`skein-ui-example-view`, step code under the subset, built for the host
and for `wasm32-unknown-unknown`:

- **Its state:** the notes the page knows, the form's values with the
  view's count of its own writes to each (ui.md, 3.4), the label shown,
  the place, the adds pending, and whether the stream is live.
- **Its view,** built through skein-ui's builder from the state, with
  its bindings decoded into its actions (ui.md, sections 5 and 6), its
  drawing code (ui.md, 11.1), its codec for places, its limits from
  skein-ui's and its worst case (ui.md, section 15).
- **Its records** are a codec family of its own (codec.md), a note, an
  add and the list, which the server names too. An exchange carries one
  record as `application/octet-stream`; the stream carries one in each
  event's data, as base64 (lib.md, 6.1), read with HTTP's event-stream
  reader (http.md, section 4).
- **An add is written to the small store first,** under a key drawn from
  the seed, then sent, in the transport's order (ui-web.md, section 10).
  The server makes an add once per key, so an add a reload sends again
  makes one note; its record leaves the small store once answered.
- **Live:** at start, and when a hidden tab comes back (ui-web.md, 5.1),
  the view opens the stream, then reads the list, and applies each note
  by its number, so a note made between the two shows once.

### 9.3 Its server

`skein-ui-example-server`, the binary `skein-ui-example`: one process, a
loop over `drive` (shell.md, 12), on skein's HTTP server and its
event-stream writer (http.md, sections 4.3 and 5). Its notes are kept in
memory, and its modules keep the domain, which names no io, apart from
the routes; the echo shows a service's layers as crates (section 2), and
a service copies that.

| Request | Answer |
|---|---|
| `GET /api/notes` | the list, one record |
| `POST /api/notes` | an add, one record with its key: the note made, or the one made before under the same key |
| `GET /api/events` | the stream: an event for each note made while it is open |
| `GET /bundle/<digest>/<file>` | the bundle's file, as ui-web.md, 4.4, serves it; a digest not the bundle's, not found |
| any other `GET` | the page |

- **The bundle,** written by `skein-ui-bundle` with `/bundle/` as the
  bundle's prefix (ui-web.md, 4.2), is read from the directory its
  arguments name, at startup (shell.md, 6.1), and served under
  ui-web.md, 4.4: the digest directory immutable, the page with an
  `ETag` and `no-cache`.
- **The strict content security policy** of ui-web.md, 4.4, on every
  answer, so the browser tier sees any violation of it.
- **Bounded:** the notes it keeps, a record's bytes, and the streams
  open at once are limits; past one, it refuses at the entrance
  (programming-model.md, section 7), and the page says so in words.
- **It listens at the address its arguments name,** and says where,
  once, from its hook (shell.md, 12): the line the harness finds by its
  prefix (ui-web.md, 14.2), which can stop it and start it again there.

### 9.4 Its browser shell, its component and its drawing

- **The reference shell,** `skein-ui-example-shell-web`, built for
  wasm32 only: its start makes skein's browser parts with its look and
  starts its view, and its loop is the turn (ui-web.md, 3.1 and 5.2).
  About 200 lines (a guide), commented to be copied: an application's
  browser shell starts as a copy of it.
- **Its look** (ui-web.md, 3.2): the prefix `notes`, its tokens' and
  region names' names, its drawing code and its component table; and
  its stylesheet, loaded after `skein-ui.css`, styling its tokens and
  placing its regions (ui-web.md, section 12). Its page is the
  bundler's or its own, the plan's choice (ui-web.md, 4.3).
- **An element of its own** (ui-web.md, 6.1): a region carrying
  `notes-card` is an `<article>`, which keeps the region's role and
  name.
- **A component** (ui.md, section 12; ui-web.md, 6.1): the body's text
  area carries the token `notes-grow`, which the look's component table
  maps, for a text area, to a text area that grows with its words. It
  keeps its node's role and name, reports only the input acts a text
  area takes, on its node, and is removed with it. Its one JavaScript
  snippet, at most 10 lines, sets the element's height from its
  content's, through `wasm-bindgen`'s snippets, so the bundle and the
  policy carry an application's JavaScript. The text shell, the fake
  person and a screen reader see the text area.
- **A drawing acted on** (ui.md, section 11): a bar chart of the notes
  by label, of the example's one drawing kind, its data the counts and
  their words. Its drawing code draws a bar and a text per label, with
  the token `notes-bar`, as SVG in a browser by default (ui-web.md,
  section 8). Its fallback is a list saying each label's count in
  words. Each bar's mark carries the act that shows its label's notes,
  which the choice group beside the chart does too (ui.md, 11.3).
- **Its module's size,** built with the `web` profile (ui-web.md, 4.1),
  is kept beside the shell as a guide; past it, what grew is reported
  (ui-web.md, 4.2).
- **What it gives skein's stories for a browser shell** (ui-web.md,
  14.2): its title field, typed in while a second person adds a note,
  and that note, which the first person's list then shows; `/` and a
  note's address, and what each shows; and, in its test build alone
  (debug assertions on, as in the browser tier's module, never in one
  built with `web`), an address that makes its module panic at start,
  with its phrasebook's words for "the application failed", which its
  page shows past the restart limit, and one that bounds its small
  store below an add's record, so that the add's write is refused, with
  the note that add would have made. Those addresses are the plan's.

### 9.5 How it is tested

- **Step tests** of the view: its pages printed through skein-ui-text
  and kept as golden text (ui-text.md, section 8); the fake person's
  tree face adding a note, a required title refused in words, a label
  chosen, a note opened and left (ui.md, 17.1); golden scenes of the
  chart (ui.md, section 11); its places printed and parsed back; its
  limits and worst case.
- **Step tests** of the server: each route, an add made once per key,
  the bundle's headers and the policy on every answer, the page at any
  other path, a digest not the bundle's not found, a limit refused.
- **The wasm32 builds** of the view and the browser shell, with clippy's
  lints as errors, in skein's gate (testing.md, section 6).
- **The browser tier** (ui-web.md, 14.1 and 14.3), in `tests/ui-web`:
  the server's binary and Chromium through `skein-fake-person-browser`'s
  harness and face, failing on any trouble a story did not expect, a
  policy violation among it. It calls skein's stories for a browser
  shell with the example's binary, bundle and codec (9.4; ui-web.md,
  14.2), and adds its own: a person opens the example and sees its
  notes, with no trouble; a field of each entry kind it uses, and a
  label chosen from its select; Escape in its dialog pressing the keep
  button, and a focus request moving the focus to the title; a press on
  a bar, through the harness, showing its label's notes, as the choice
  beside it does, and the chart's fallback read by its name; the
  component growing with its words and reporting them; a note added in
  one person's page showing in another's, through the stream; a reload
  keeping a pending add, which the server makes once.

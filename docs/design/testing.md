# Testing skein

Provisional, 2026-10-03, revised 2026-10-19. How testing-strategy.md
applies to skein itself: which tiers skein has, what they run, the
example services and the minimal fake machine they need, where the tests
live, what skein supplies for a service's own tiers, and where things
stand. How each part is tested in its own tier is in that part's
document.

## 1. In one page

- **skein tests the kit.** Its tiers stop at io under small example
  services: an echo server, an HTTP server, and a client of it; and at a
  browser under the UI example, a small application on skein-ui. A
  service's domain, protocol layer and fakes are tested in the service's
  own tiers, which stand on skein's and do not retest them.
- **Each part has its tier:** lib in step tests, each machine in machine
  worlds, the machines stacked in protocol worlds, io in io worlds, the
  loop in simulated worlds of the examples, and the ring in the real loop
  and the conformance suite.
- **The examples are the template.** skein's simulated worlds and real
  loop run them, and a service's worlds copy them.
- **skein ships no fake machine.** Its own io and simulated worlds use a
  minimal one, which stays that small.

## 2. skein's tiers

| Tier | In skein | Its design |
|---|---|---|
| step tests | lib's containers and value types; each crate's step functions | lib.md, 10 |
| machine worlds | HTTP, server-sent events, JSON, each side alone; the TLS client, against rustls's server in memory | http.md, 6; json.md, 6; tls.md, 5 |
| protocol worlds | an LLM client's HTTP, server-sent events and JSON against a server's; skein-llm's subscription client against independent wire scenarios | http.md, 6; llm.md, Verification |
| io worlds | io over the simulator, with a scripted owner | io.md, 8 |
| simulated worlds | the examples' `iterate`, each a process of the simulator | section 3 |
| real loop | the examples under the shell, on the real kernel | section 3 |
| beside them | the conformance suite, against the simulator and the ring | kernel.md, 8 |
| beside them | the simulator's own tests; the ring's own tests | simulator.md, 6; shell.md, 9 |
| durable store | `skein-kv` step tests; `tests/kv` over the crashing fake disk and the real ring | kv.md, 9 |
| browser tier | the browser kit against real Chromium; the UI example's bundle in Chromium, through the person's browser face and harness | browser.md, 9; ui-web.md, 14.1 |

TLS is tested on its own (tls.md, 5), and the replaying tiers run in
plaintext.

## 3. The examples

Small services in `examples/`: an echo server, an HTTP server, and a
client of it. They exist to be tested, and to be copied.

- **In simulated worlds,** each runs as a process of the simulator, with
  every layer real. It is the first tier where the loop runs, so it tests
  what the loop adds: the two passes, `MAX_OUT` reserved at each stage,
  deadlines across layers, and requests made in the down pass reaped in a
  later iteration.
- **In the real loop,** they run under the shell on the real kernel: the
  ring, loopback sockets, a scratch directory as the root, real child
  processes, the real clock, and TLS. It shows what only the real kernel
  can: the ring adapter's `unsafe`, the probe at startup, signals read
  from a signalfd, and a process tree that ends.
- **As a template,** a service's simulated worlds start from theirs.

The UI example (examples.md, section 9) is an application's rather than
a service's: a view on skein-ui, its browser shell and its server. Its
view and its server are tested in step tests, and the three together in
the browser tier (ui-web.md, section 14).

Their design, the steps before code of each, their fakes and the harness
their worlds run on are examples.md. The echo has a domain, a protocol
layer, a service and a `main`, its fake client, and its worlds in
`tests/echo`, over the world harness, `skein-world` (examples.md, 6).

## 4. The minimal fake machine

The simulator plays the kernel, not what a program does: that is the
embedder's fake machine (simulator.md, 3). skein's own io and simulated
worlds need a minimal one:

- a few files beneath a root;
- a program that echoes its input, one that exits with a given status,
  and one that never exits.

It lives with skein's tests, and stays that small. A service's fake
machine is the service's.

It is `skein-fake-machine`, in `testing/`. A world lays a
root in it from a scenario's items (files, directories, symbolic links,
each with its mode) and gives its handle to a process as the shell would
a root opened at startup; after each submit, the world takes the
simulator's calls and has the machine answer each (simulator.md, 3.1).
The machine resolves paths beneath a root as `openat2` with
`RESOLVE_BENEATH` does, follows the links that stay beneath it, keeps its
owner's permissions, and refuses what a real filesystem refuses, in the
order Linux checks; the conformance suite holds it, through the
simulator, to the real kernel in a scratch directory. It keeps a step
machine's shape, a call in and an answer out, in a vocabulary of its own
that a face translates to and from the simulator's. Its programs are
built too, the fixture of the conformance suite's processes, and so is
its crash model, which keeps what a crash keeps (simulator.md, 3.3). It
keeps each file's lock with the handle that took it (simulator.md, 3.1).
For a tree's metadata (kernel.md, 6.4) it keeps each entry's inode,
group, mode up to `0o7777`, times and attributes, and hard links, and
lays FIFOs; it has no devices, no sockets and no btrfs. It answers every
subvolume call `NotSupported`, and its status a filesystem that is not
btrfs, as Linux does off btrfs, so skein's btrfs scenarios run on the
ring alone (kernel.md, 8). A service whose fake machine models btrfs,
such as hearth's, holds that machine to those scenarios itself.

For trees (io.md, 6.2 to 6.4; simulator.md, 3.4) its programs add: one
that starts a descendant which leaves its group (`setsid`) and outlives
it; one that forks a child each time it is signalled; and, on the ring,
one that writes what it finds of itself, its ids and groups, its
hostname, its network's interfaces, the processes in its `/proc`, its
`no_new_privs`, its rlimits, and whether chosen paths can be read and
written, so that a conformance scenario checks a contained tree from
inside it (kernel.md, 8).

A separate dependency-free kit, `skein-fake-checkout`, supplies generic
byte-path files, scripted commands and local git mechanics for domain
worlds (fake-checkout.md). It has no simulated-kernel adapter, inode or
permission model. Services retain their policy, remote implementations,
clock, cancellation and delivery ledgers; the kit answers synchronous
mechanical operations only. Its focused leaf tests and small replay sweep
stay beside the crate in `testing/skein-fake-checkout/tests`.

A code forge has fakes of its own (fake-forge.md). `skein-fake-forge-domain`,
a step crate on skein-lib alone, plays a forge's state, CI, merges and git,
and `skein-fake-forge-protocol` serves Forgejo's API over it on skein's HTTP
server. The forge peer in `skein-fake-peers` hosts that layer on loopback
io. `skein-throwaway-forgejo` runs the pinned Forgejo in a scratch
directory, for the tools and process tiers that need a real one, and a
conformance tool outside the suites holds the dialect to it. A service
keeps its fake of its own git component, its worlds and its cost ceilings.

## 5. What skein supplies for a service's tiers

| Service tier | Real | What skein supplies |
|---|---|---|
| step tests | one of its step functions | lib |
| domain worlds | one domain or child domain | lib, the counting allocator; for an application on skein-ui, the fake person (ui.md, 17.1) |
| system worlds | several of its domains, or several services' domains | lib, the counting allocator |
| protocol worlds | its protocol layer, over skein's machines | the machines, tested in skein's tiers; the counting allocator |
| simulated worlds | every layer, `iterate` per process | the simulator, the world harness, the counting allocator, the examples as a template |
| real loop | the service as it ships | the shell kit, the world harness's real loop; for a service with a web, the browser kit (browser.md); for an application on skein-ui, the person's browser face and harness (ui-web.md, 14.2) |
| end to end | the service's binaries as they ship | the world harness's end-to-end kit: binaries started on pipes or a pseudo-terminal, and each binary's process tree settled and counted, its CPU and peak memory (examples.md, 6) |

The service supplies its fakes, the fake machine that plugs into the
simulator among them, and the scenarios its worlds run. A failure in a
service's world that comes from io, a machine or the simulator is
reproduced in skein's own tier, and fixed there.

A service that works on a forge takes the forge's fakes from skein: the
domain and the protocol layer in its protocol worlds, the forge peer in
its real loop, and the throwaway Forgejo in its tools and process tiers
(fake-forge.md, sections 2 to 6). The peer serves plaintext on loopback
over a forge the service may share with its own fake of git. Its
transcripts of git stay its own.

**The counting allocator** is `skein-heap`, in `testing/` (section 6): a
global allocator that counts each thread's heap, and a meter that checks a
step at the most it held of its own, less what it handed out in its
requests, against its worst case (programming-model.md, 6.3). Every world
that checks memory uses it, whatever its tier (testing-strategy.md, 6), in
a test binary of its own that declares it. It measures; it runs no world.
In a simulated world, the check at every iteration is the world harness's:
each hosted service's heap, grown within its own calls, against its own
worst case (simulator.md, 5), measured with the allocator's span, the
heap's growth around one call, signed.

**Who counts a payload handed to a step.** A step's bound covers what
it keeps and what it allocates; what it hands out in its requests is the
next step's input, and the meter takes it off the step's own. An input
moved into a step is counted by whoever made it, with one exception: a
delivery from the side below is made to the reading machine's own
demand, within its largest demand, so the reader's bound covers the one
it reads, for the step that reads it. A request whose size the receiving
step's limits do not bound, such as an HTTP call's target and fields or
a piece of a request body, is counted in the sender's worst case (the
protocol layer's), and the memory test checks the step that takes it
against its bound plus that input's size.

**The world harness** is `skein-world`, in `testing/` (examples.md, 6):
one loop over the processes' `iterate`, generic over a scenario's
processes, with the referee's expectations, the trace, the heap at every
iteration, and the settled world's invariants; and the same loop over the
shell's shared ring, on the real clock. The examples are its first user and its
second, which is why it is skein's (testing-strategy.md, 7).

- **It hosts the shell's `Host`.** `Host` and `drive` are `skein-shell`'s
  (shell.md, 12), and `skein-world` re-exports both, so a service's worlds
  host the same `Host` its `main` drives, and a shipped binary links no
  testing crate.
- **It calls each process's hook** where `drive` does: once per
  iteration, after the process's `iterate` and before its submit. The
  drains a world runs are then the ones that ship.
- **It checks the teardown invariant** (testing-strategy.md, 6) in every
  world, at every tier it runs, for every service: after the scenario's
  last word, by default the referee's last expectation met, it fails the
  world when time must reach a process's `next_policy_deadline()` before
  the world settles, naming the process and the deadline. In the
  replaying tiers that is a jump of the clock to it; in the real loop, a
  wake for it. No service writes the check, and none opts out.
- **It carries state digests** (lib.md, 5.1). `Host` (shell.md, 12) and
  `Machine`, the seam a world's fake machine plugs into, each give
  `state_digest() -> Option<[u8; 32]>`, `None` unless the process or the
  fake gives one. What a host hashes is its own choice, its domains'
  state at least; a fake machine hashes its own state.
  - **At the end of a run:** a world reads them from its `Outcome`'s
    processes and machine, and returns them beside its trace, so
    `assert_replays` compares them with the rest of what must come out
    the same. Its signature is unchanged.
  - **After each iteration, on request:** with the world's `digests`
    option, the harness writes each process's and the machine's digest
    into the trace after each iteration, so two runs that part are found
    at the first iteration whose digests differ, before anything crosses
    a boundary. A process's is taken where its hook runs, after its
    `iterate` and its drain and before its submit; the machine's once
    every process has had its turn in the iteration. Each is an entry of
    the trace that simulator.md, section 3 names. Without it, the trace is
    as before.

## 6. Layout

```
crates/*/src/tests.rs           step tests; lib's, io's, JSON's, HTTP's and TLS's in a module per area, under src/tests/
crates/skein-ui                 the UI kit (ui.md): its step tests, and its golden scenes beside them
crates/skein-ui-text            the text shell (ui-text.md): its step tests, and its golden text beside them in src/tests/golden/
crates/skein-ui-web             the browser parts (ui-web.md): each part's pure half tested on the host in its step tests
crates/skein-ui-bundle          the bundler, a tool built optimized (ui-web.md, 4.2)
testing/skein-conformance       the conformance suite: the backend interface, the scenarios, the driver, the checks
testing/skein-heap              the counting allocator, the meter that checks a step against its worst case, and the span a world meters its processes with
testing/skein-scratch           a scratch directory beneath the system's temporary one, removed when dropped, for tests of files on the real kernel
testing/skein-world             the world harness: processes' iterate over the simulator or the real ring, the referee, the trace, the heap
testing/skein-echo-client       the fake echo client, a step machine
testing/skein-fake-machine      the minimal fake machine: files beneath a root, its programs, and its face behind the simulator
testing/skein-fake-checkout     files, scripted commands and local git for a service's domain worlds (fake-checkout.md)
testing/skein-fake-llm-*        the shared scripted LLM peer: its domain and its byte peer (fake-llm.md)
testing/skein-fake-forge-*      the shared fake forge: its domain and its protocol layer (fake-forge.md, 2 to 4)
testing/skein-throwaway-forgejo the pinned Forgejo run on loopback in a scratch directory (fake-forge.md, 6)
testing/skein-fake-oauth        an independent rotating OAuth issuer (oauth.md, 5)
testing/skein-fake-peers        the LLM, OAuth and forge peers hosted on loopback io (fake-forge.md, 5)
testing/skein-fake-channel      a scripted framed peer for a service's protocol worlds (channel.md)
testing/skein-codegen           the codec generator, run by a consumer's tests (codec.md)
testing/skein-browser           the browser kit, a step machine: headless Chromium over the DevTools protocol on a pipe (browser.md)
testing/skein-fake-person       the person: targets by role, name and text, steps, the tree face, the random person (ui.md, 17.1)
testing/skein-fake-person-browser the person's face over the browser kit, and the harness: an application's server binary, Chromium and the peers a test lends in one loop; skein's stories for a browser shell (ui-web.md, 14.2)
tests/heap                      the counting allocator's own tests, skein-heap-tests
tests/echo                      the echo's simulated worlds and its real loop, skein-echo-world
tests/io                        io worlds: io over the simulator, a scripted owner, a referee, skein-io-world
tests/lib                       lib's comparisons with naive functions, run long, and its worst cases against the counting allocator, skein-lib-tests
tests/sim                       the simulator's own tests, skein-sim-tests
tests/world                     the world harness's own tests, over scripted processes of raw records, skein-world-tests
tests/ring                      the ring adapter's own tests, skein-ring-tests
tests/conformance/sim           the suite against the simulator, skein-conformance-sim
tests/conformance/ring          the suite against the ring, skein-conformance-ring
tests/json                      the JSON tokenizer's machine worlds, the writer against it, and their worst cases against the counting allocator, skein-json-world
tests/json/transcripts          its transcripts, each with what it must decode to
tests/http                      the HTTP client's and server's and the event stream reader's and writer's machine worlds, a reference reader of each, the client and the reader stacked with JSON, and their worst cases against the counting allocator, skein-http-world
tests/http/transcripts          its transcripts, responses and, in requests/, requests, each with what it must decode to
tests/protocol                  the protocol worlds: an LLM client's stack against a server's, joined by bytes cut at random, skein-protocol-world
tests/llm                       skein-llm's subscription protocol worlds, fragmentation and fault sweeps, and bounds against the counting allocator, skein-llm-world
tests/llm-connection            the LLM connection component's worlds, skein-llm-connection-world
tests/oauth                     the OAuth client against the independent issuer, skein-oauth-world
tests/forge/machine             the forge component's machine world and the listener's, against a scripted HTTP peer and the fake's GitHub layer on its clock, and their worst case against the counting allocator, every holder filled (forge-connection.md, 10)
tests/forge/protocol            the protocol world: the client and the component against the fake forge's protocol layer and domain, joined by streams cut at random, under a scripted durable owner that restarts; its fuzzy runs take seeds (forge-connection.md, 10)
tests/forge/client              the client world: the client alone under a scripted parent, over the fake forge's domain (forge-client.md, 10)
tests/forge/setup               the setup world: the host component over a store of items and the fake forge's setup routes and GitHub layer, through HTTP bytes, and its worst case against the counting allocator (forge-setup.md, 7)
tests/forge/conformance         the conformance tool, outside the suites: a throwaway Forgejo driven, each fact the dialect relies on recorded (fake-forge.md, 7)
tests/channel                   the framed channel's machine and protocol worlds, skein-channel-world
tests/codec                     a generated sample codec and its worlds, skein-codec-tests
tests/kv                        the store over file io, the simulated kernel and a crashing fake disk, skein-kv-tests
tests/browser                   the browser kit's machine world against a fake browser, its tests against real Chromium, and its worst case against the counting allocator, skein-browser-world
tests/tls                       the TLS client's machine worlds against a rustls server in memory, the HTTP client stacked on it, and its worst case against the counting allocator, skein-tls-world
tests/tls/fixtures              its certificates and key, and the script that makes them
tests/ui                        skein-ui's worst cases against the counting allocator, and its comparisons with naive functions, skein-ui-tests (ui.md, 17)
tests/ui-text                   the text shell's terminal on the simulated machine, skein-ui-text-tests (ui-text.md, 8)
tests/ui-web                    the browser parts' host tests that need more than a crate's step tests, and the UI example's browser tier, skein-ui-web-tests (ui-web.md, 14)
tests/**/tests/*.rs             a crate's focused tests
tests/**/tests/fuzzy_*.rs       its fuzzy tests: sweeps over many seeds
tests/clippy.toml               what the crates under tests/ may not use
examples/echo/*                 the echo: its domain, protocol layer, service and shell, a crate each (examples.md, 2)
examples/ui/*                   the UI example: its view, its browser shell and its server, a crate each (examples.md, 9)
```

No crate under `crates/` has a `tests/` directory: its step tests are
in `src/`, and whatever drives it from outside is a crate under `tests/`,
named by its path. Such a crate is ordinary Rust (programming-model.md,
10.2): what its test binaries share is its library, in `src/` (the
simulator's harness and its scripted exchange, a backend of the suite,
the naive functions lib is compared with, a machine's world and the
reference parser it is checked against),
and each file in its `tests/` is a test binary of its own, a fuzzy one
when its name starts with `fuzzy_`. `tests/clippy.toml` bans what would
make a run unrepeatable: hash maps with a random seed, the system clocks,
threads. Each backend of the conformance suite is implemented beside the
tests that run the suite against it. A crate's memory tests are binaries
of their own, as the global allocator a binary declares is the whole
binary's: `tests/memory.rs`, and `tests/fuzzy_memory.rs` for a sweep
against the worst case (testing-strategy.md, 8). The crate's other tests
run on the system allocator. A simulated world checks memory at every
iteration, so each binary of `tests/echo` that runs one declares the
counting allocator; its real loop runs on the system allocator.

What a crate under `testing/` holds is shared by the tests of more than
one crate, and is ordinary Rust held to the step crates' lints, each with
a `clippy.toml` of its own. `skein-heap` holds the one `unsafe` in skein
beside the ring adapter: a global allocator is an `unsafe impl`.
programming-model.md (1, 2.1, 10.2) confines `unsafe` to the ring adapter
in code a service runs, and in tests to this `unsafe impl GlobalAlloc`: a
test-only crate, never linked into a service. It allows its `unsafe` in
place, with a scoped
`#[expect(unsafe_code, reason = "…")]` and a `SAFETY` comment on each
block. A fake that runs as a step machine, as `skein-echo-client` does,
is held to the subset instead (testing-strategy.md, 4): it has no
`clippy.toml` of its own, so the step crates' applies.

A machine's worlds are a crate of their own, `tests/<machine>`, with its
transcripts in `transcripts/` beside its tests. The minimal machine joins
the io worlds with files and processes; the protocol worlds are a crate
of their own, `tests/protocol`. `scripts/check.sh` runs what CI runs:
formatting, the lints as errors, a build of `skein-lib`, `skein-codec`
and `skein-http` alone for `wasm32-unknown-unknown` (lib.md, section 2),
and clippy's lints as errors on that target for `skein-ui`,
`skein-ui-web` and the UI example's view and browser shell, since the
parts' platform halves compile only there (ui-web.md, section 14); then
the focused suite and the fuzzy suite, with nextest (section 7); then
the UI example's bundle, made with `skein-ui-bundle` from a module built
with the dev profile (ui-web.md, 4.1), and the browser suite, which runs
the browser kit's tests against Chromium and the UI example's tier on
that bundle (ui-web.md, 14.1). The builds for that target need only the
target installed: they run nothing, since lib, the codec, HTTP,
skein-ui and the example's view hold no io and their tests run on the
host, and skein-ui-web's platform half runs in the browser tier.

## 7. Where things stand

How each part is tested, and what its worlds cover, is in the part's own
document; the echo's worlds are examples.md, 7.

The two suites of testing-strategy.md, section 8, are
`.config/nextest.toml`'s profiles, each with its budget as a global
timeout: the focused suite by default, within 15 seconds, and the fuzzy
suite, the `fuzzy_*` binaries, with `--profile fuzzy`, within a minute.
Two profiles stay out of both: `browser`, the browser kit against real
Chromium and the UI example's browser tier, within two minutes for both
(`cargo nextest run --workspace --profile browser`; browser.md, section
9; ui-web.md, 14.1), and `live`,
skein-llm against real endpoints, with a key or a plan's token from the
environment. `measure` runs a suite with no budget, to time each test.

Replay: a seed replays to the same trace of submissions and completions,
and a JSON world, an HTTP world, an io world and an echo world to the
same run; a TLS world does not, as rustls draws from the kernel. The
state digests replay with the trace (lib.md, 5.1; section 5).

Memory: each part's worst case is checked against the counting allocator
in its own tier, a call of an entry point at a time; the echo's simulated
worlds check it at every iteration (examples.md, 6). A world's scenarios
peak below the worst case, and a part that held more than it should
within that slack would pass unseen; and the worst cases of io's and the
protocol layer's B-tree tables (deadlines, ready lists) count a full
table's nodes, while a world arms a few timers. So a focused test drives
the echo to its limits (every connection's intake, receive and output
full at once) and holds it to at least half its worst case, so that it
keeps reaching them.

The forge's worlds (section 6) keep within a budget of their own: a step
test under 20 ms, a focused world test under half a second, and a fuzzy
run within 5 seconds, its seeds included, so that `tests/forge` stays
within half of each suite's.

## 8. Not built yet

What of this document is not built is `STATUS.md`'s (its row in section 2,
and section 3).

## 9. Open questions

- **Heap handed between services in one thread.** A world checks each
  service against its own worst case by metering around its own calls
  (simulator.md, 5), which holds while nothing one service allocates is
  freed by another: true over the kernel boundary, whose backend hands
  every buffer back. A world that joins two services' layers in-process,
  passing a box from one to the other, would need the hand-off counted.
- The real loop in CI and sanitizers on the ring adapter are the shell's
  (shell.md, 10).

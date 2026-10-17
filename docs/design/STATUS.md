# Status

Updated 2026-10-17. How far skein's code is from its design, and which
plans take it there. The designer keeps this file (development.md,
section 3.3).

## 1. Plans

| Plan | State | Takes the design to | Where |
|---|---|---|---|
| temper's client-api | active on carbon-forge since 2026-10-16 | ui-web.md 14.2: the harness opens the printed URL whole, serves other origins, awaits trouble (skein 01); ui-text.md 3 and 4: the application's lines setting and `Terminal::room`, its own lines' room kept (skein-01.s1); the line bytes setting, `Terminal::worst_case` and `Terminal::operations` (skein-01.s2); lib.md 12: a request not kept, retired by its key (skein-01.s3) | temper's `docs/development/plans/client-api/` |
| temper's forge-setup | active on x1 since 2026-10-16 | lib.md 6.2: SHA-256, `sha256::Hmac` and `sha256::equal`, with skein's three copies (io's digest, PKCE, the bundler) moved onto it (skein 01) | temper's `docs/development/plans/forge-setup/` |
| hearth's render | ready | json.md 3.6: positions on skein-json's tokens and errors (skein 02); lib.md 5.1, testing.md 5 and 8, simulator.md 7: state digests, lib's types' `Hash`, replay and the trace (skein 03); io.md 5 and simulator.md 3.3: `Rename` between two directories, `ListPage` (skein 04); kernel.md 6.1, io.md 5 and simulator.md 3.1: `Lock`, its conformance scenario, and the answer of a `Lock` or a page past its deadline (skein 04, 05) | hearth's `docs/development/plans/render/` |
| temper's forgejo-17 | done 2026-10-16 | ui-text.md 3 and 4: a print's room from its own tree, never the limits' worst case; the output in pieces | temper's `docs/development/plans/forgejo-17/` |
| jig's skein-ui | done 2026-10-15 | `ui.md`, `ui-text.md`, `ui-web.md`; `examples.md` 9; `testing.md` 6 (the wasm32 builds, `tests/ui`, `tests/ui-text`, `tests/ui-web`, the example's bundle and browser tier in the gate); `browser.md` 1, 3, 4, 5, 6, 7 and 9 (a name matched by words, a press at a point, a go's closing warning and the native dialogs, `keys` for a native date or time field, a find's roots in the current document); `lib.md` 2 and `programming-model.md` 2.1 (the wasm32 build, the glue the only `unsafe`): this file's "skein-ui" in section 3, steps (1) to (3) but jig's new primitives | jig's `docs/development/plans/skein-ui/` |
| jig's test-budgets | done 2026-10-14 | lib.md 7, io.md 2 and 3.3, http.md 3.3, 4 and 6, tls.md 3.1, programming-model.md 4.3, testing-strategy.md 2.4: a read of what has arrived; testing-strategy.md 8: nextest without its double spawn, the focused suite back within 15 s; codec.md 4: generated worst cases checked once | jig's `docs/development/plans/test-budgets/` |
| jig's jig-protocol | done 2026-10-13 | io.md 3.5 (joined sockets), section 4 (in-process addresses), 5 (`OpenWrite`, `Cut`); kernel.md 6.1 (the `Write` open, `Truncate`); simulator.md 3.1 (a cut kept until synced); shell.md 5 (the cryptographic seed), 6.1 (`read_private_file`); lib.md 6 (`Secret`), 6.1 (`base64`), 8 (`CryptoRng`); codec.md 3–4 (`secret N`, `boxed` variants), 6 (lints); oauth.md 6.4 (a record dropped on sign in again) | jig's `docs/development/plans/jig-protocol/` |
| tls-cleanup | done 2026-10-12 | tls.md 3.6 (the code in its words, one engine for both faces); the crate's and its world's tests, one behaviour each | `docs/development/plans/tls-cleanup/` |
| smith's parity | done 2026-10-15: ended short of parity at `87a8d1e8` on RT (benchmarks.md 16.5) | fake-llm.md 2.1: a changed tool choice keeps an automatic query's prefix | smith's `docs/development/plans/parity/` |
| smith's reliability | done 2026-10-14 | kernel.md 6.3, io.md 7 and shell.md 6.3: terminal modes (skein 02.6). Its first run (2026-10-09 to 2026-10-12) built io.md 5–7; kernel.md 6; simulator.md 3; shell.md 6, 12–13; tls.md 3.4; json.md 3, 5; http.md 4; llm.md 2, 4; llm-connection.md 4–7; fake-llm.md 2–3; oauth.md 1, 6; examples.md 3.3, 6; testing-strategy.md 6 | smith's `docs/development/plans/reliability/` |
| smith-testing | done 2026-10-08 | simulator.md 3 (hosted spawns); llm-connection.md and oauth.md, plaintext to loopback; fake-llm.md 3 (peers on loopback); examples.md 6, 8; testing-strategy.md 2.8–2.9 | `~/src/rust/plans/smith-testing/` (not versioned) |
| smith-protocol-layer | done 2026-10-08 | codec.md; channel.md; llm-connection.md 3–6, 8 | `~/src/rust/plans/smith-protocol-layer/` (not versioned) |
| domain-completion-plan | done 2026-10-07 | fake-checkout.md 4 (commit messages); oauth.md 1–5 | `~/src/rust/plans/domain-completion-plan/` (not versioned) |
| browser-implementation | done 2026-10-06 | browser.md; io.md 6 (pipes at chosen descriptors); lib.md 7 (a held buffer read as a stream) | `docs/plans/browser-implementation.md` |
| kv-implementation | done 2026-10-05 | kv.md 1–5, 7–9; io.md 5 (files held open); simulator.md 3.1 (a disk that crashes) | `docs/plans/kv-implementation.md` |

Section 2 names an increment of smith's reliability plan as
`reliability <session>.<n>`, from that plan's `skein/` session files.

## 2. The design

| Document | Built | Left |
|---|---|---|
| `foundation/programming-model.md` | partly | 4.3: a read of what has arrived (jig's test-budgets, skein 01); 10.3: rules held in review only, with no checker: not planned |
| `foundation/testing-strategy.md` | partly | 2.4: machine worlds reading what has arrived (jig's test-budgets, skein 01.2); 8: nextest without its double spawn in the gates and the queue, the suites back within 15 s on carbon-forge (jig's test-budgets); 6: state digests, transition coverage, `cargo fuzz`: not planned |
| `lib.md` | partly | 12: `RequestTable::not_kept`, a request whose record was not stored retired by its key: temper's client-api (skein-01.s3); 6.2: SHA-256, with skein's copies moved onto it, its HMAC and its comparison in constant time: temper's forge-setup; 5.1: state digests, the hasher and lib's types' `Hash`: hearth's render (skein 03); 13: `Slab::get2_mut`: not planned |
| `kernel.md` | partly | 6.3: `Window` (reliability 02.6); 6.1: the `Write` open and `Truncate` (jig's jig-protocol); 6.1: `Lock`, and its conformance scenario: hearth's render (skein 05); 9: fixed files, 10: the ring's descriptor limit: not planned |
| `io.md` | partly | 2, 3.3: a read of what has arrived on sockets, joined ends and pipes (jig's test-budgets, skein 01.1); 5: `Remove` of an empty directory (smith's reliability plan, a step its supervisor names); 5.1: a write pipe's stated write deadline (smith's reliability plan, a step its supervisor names); 7: `Resized`, `Window` (reliability 02.6); 3.5: joined sockets, 4: in-process addresses, 5: `OpenWrite` and `Cut`, and a made directory synced into its parent, with a world that cuts after each make (jig's jig-protocol, skein-03.s1); 5: `Lock`, `ListPage` and `Rename` between two directories, and the answer of a `Lock` or a page that completes past its deadline: hearth's render (skein 04, 05); 5: a `Stat`, `Rename`, `Remove` or `MakeDirectory` that completes past its deadline answers `TimedOut` (`file_layer.rs`'s `stopped`), not its own: not planned; 4: DNS and datagram sockets, 9: streams that read files, 6: contained trees: not planned |
| `simulator.md` | partly | 7: the state digest in the trace (lib.md, 5.1; testing.md, 5); 3.1: locks, and 3.3: a rename between two directories in the crash model: hearth's render (skein 03, 05, 04) |
| `shell.md` | partly | 5: the cryptographic seed, 6.1: `read_private_file` (jig's jig-protocol); 6: `make_root` (jig's jig-protocol, skein-03.s1); 6.1: `directory_access` (smith's reliability plan, a step its supervisor names); 6.3: terminal modes (reliability 02.6); 6: names at startup, 7: the readiness backend, 8: optimisations: not planned |
| `examples.md` | partly | 8: the HTTP example, the echo's domain worlds: not planned |
| `testing.md` | partly | 5, 8: state digests in the world harness: hearth's render (skein 03); 8: TLS in the real loop, the HTTP example's worlds, coverage, fuzz targets, the referee's purity: not planned. Section 7 is as of 2026-10-04 |
| `json.md` | partly | 3.6: positions, with 3.3's `length` below `u32::MAX`, and the reference's (section 6): hearth's render (skein 02); 8–9: writing in pieces, several documents in a stream, codecs from schemas, fuzz target: not planned |
| `http.md` | partly | 3.3, 4: a body and an event's data read as they arrive; 6: the worlds reading so (jig's test-budgets, skein 01.1, 01.2); 2: the build for `wasm32-unknown-unknown`: not built; 8–9: chunked uploads, codings, trailers, reconnecting event streams, HTTP/2, a pool per peer, fuzz targets: not planned |
| `tls.md` | partly | 3.1: a read of what has arrived from the plaintext intake (jig's test-budgets, skein 01.1); 8: a user of the native face, the server side, client certificates, the real loop, kernel TLS, fuzz target: not planned |
| `codec.md` | partly | 4: generated worst cases that check a family's limits once (jig's test-budgets, skein 02.2); 2–4: `secret N`, `boxed` variants, pieces; 6: generated files that pass the lints with no allowances, table tests, bounds at small test limits, a generator linear in its schema (jig's jig-protocol); 2: the build for `wasm32-unknown-unknown`, 7: maps and sets, smaller integers, other languages: not planned |
| `channel.md` | yes | 13: compression, bodies across frames, named calls, reconnecting: not planned |
| `kv.md` | partly | 6: payload logs, 12: a shared sync, values on disk: not planned |
| `llm.md` | partly | 2, 4.2: a run of consecutive reasoning items as one block; replay values charged at their total (smith's reliability plan, a step its supervisor names). 6: Codex routing state, WebSocket: not planned |
| `llm-connection.md` | partly | 9: names while running, proxies, HTTP/2, pool fairness: not planned |
| `fake-llm.md` | partly | 2.1: a changed tool choice keeps an automatic query's prefix (smith's parity); 7: providers' cache granularity: not planned |
| `oauth.md` | partly | 6.4: a record dropped when its refresh fails as sign in again (jig's jig-protocol); 7: IPv6 loopback, device authorization, a credential helper: not planned |
| `browser.md` | partly | 6, 7: a test answering a native dialog itself, and the kit's `browser.rs` split by concern (jig's skein-ui `later.md`); 11: open questions: not planned |
| `fake-checkout.md` | yes | Nothing. A process face for real-loop worlds was proposed, not designed |
| `ui.md` | partly | 4.2, 18, 19: the vocabulary that waits (a combo box, a slider and a meter, a switch, media, a feed, a list box choosing several, a tree grid) and right-to-left text, each by the light path (4.3) when an application needs it; 19: a markup macro for the builder, a server keeping the previous bundle's digest directory; 16, 18: parts for a phone or a desktop: not planned (jig's skein-ui `later.md`) |
| `ui-text.md` | partly | 3, 4, 7: the settings' lines, `Terminal::room`, the terminal's own lines' room kept apart from the application's, no room once closing, the settings' line bytes, the terminal's worst case and operations: temper's client-api; 3, 4: a print's room from its own tree, never the limits' worst case, a page past a u32 printed as one line, the output in pieces of 64 KiB: temper's forgejo-17; 9: the text shell's own words in another language, a secret typed without echo: not planned (jig's skein-ui `later.md`) |
| `ui-web.md` | partly | 11, 15: the file picker, and parts for other effects; 15: the bundle served compressed, tabs of one person sharing one stream, HTTP/2; 16: a server keeping the previous bundle's digest directory; 14.2: the face typing into a field holding words: not planned (jig's skein-ui `later.md`); 14.2: the harness opening a printed line as a whole URL from either output, a page of another origin it serves, a printed URL again, and awaiting trouble and a request's status with its own `Host`: temper's client-api |

Several "Not built yet" sections still list parts that `main` has:
`io.md`, 9 (group signals, reaping at close, append streams);
`kernel.md`, 10 (all but `Window`); `shell.md`, 11 (`Host` and `drive`,
append files, io's records); `simulator.md`, 7 (groups, usage, appends,
`Stat`); `json.md`, 9 (the collector, `Text` and `Skip`, compact
documents); `http.md`, 9 (the reader's data face); `examples.md`, 8 (the
echo over `drive`, draining); `testing.md`, 8 (the harness on `Host`,
process trees in the end-to-end kit).

## 3. Not planned

- **Contained process trees** (`io.md`, section 6): a cgroup and a view
  of the file system per tree, stopped whole and proved empty, in place
  of process groups. Waits for `draft/process.md` to become a design, then
  a plan of its own; smith's containment checks wait on it.
- **Joined sockets** (`io.md`, sections 3.5 and 4): in-process
  addresses, and sockets joined in memory within one process, for jig's
  one-process shape. Waits for its plan, which follows.
- **Names while running** (`io.md`, section 4; `llm-connection.md`,
  section 9): a DNS client, and the datagram sockets it needs. Waits for a
  service that must resolve while it runs.
- **Names at startup in the kit** (`shell.md`, sections 6 and 11):
  services resolve their peers with the standard library. Waits for a
  decision whether the kit does it.
- **Streams that read files** (`io.md`, section 9): waits for a user.
- **The readiness backend and the deferred optimisations** (`shell.md`,
  sections 7 and 8; `kernel.md`, section 9): wait for a deployment
  without io_uring, and for measurements.
- **The ring's descriptor limit** (`kernel.md`, section 10): waits for a
  way to lower it without `unsafe` outside the ring adapter.
- **State digests in smith's worlds** (`testing-strategy.md`, section 6):
  wait for skein's harness to have them (hearth's render, skein 03).
- **Transition coverage and fuzz targets** (`testing-strategy.md`,
  section 6; the "Not built yet" sections of `json.md`, `http.md` and
  `tls.md`): wait for `cargo llvm-cov` and a nightly toolchain.
- **`Slab::get2_mut`** (`lib.md`, section 13): waits for its first user.
- **HTTP's remaining features** (`http.md`, sections 8 and 9): chunked
  uploads, content codings, trailers, reconnecting event streams and their
  `retry` block, HTTP/2 and upgrades, a request body read while
  answering, a pool per peer, `Expect: 100-continue`, the server's
  deadlines. Each waits for a peer that needs it; temper's forge and
  webhooks pull next.
- **The HTTP example** (`examples.md`, section 8; `testing.md`, section
  8): its service, worlds and real loop. Waits for a plan.
- **JSON's remaining parts** (`json.md`, sections 8 and 9): writing in
  pieces, several documents in one stream, codecs from schemas, text that
  is not UTF-8. Each waits for a user; temper asks for the first.
- **TLS's server side and client certificates** (`tls.md`, section 8;
  `channel.md`, section 13): wait for temper's engine, or smith's
  connected agent's domain contract.
- **TLS in the real loop, and kernel TLS** (`tls.md`, sections 7 and 8;
  `testing.md`, section 8): a loopback exchange through the shell. Waits
  for a plan; a TLS that replays and session resumption are open
  (`notes.md`). The drafted rustls issue waits for the user to file it.
- **Channel features** (`channel.md`, section 13): compression, bodies
  across frames, named calls, reconnecting. Named calls wait for smith's
  and temper's channels both to have them.
- **Codec extensions** (`codec.md`, section 7): maps and sets,
  variable-width integers, generators for other languages. Each waits for
  a record or a peer that needs it.
- **LLM routing and transport** (`llm.md`, sections 6 and 7): Codex's
  turn state and `reasoning.context`, Responses over WebSocket, and the
  open questions on captures, Anthropic's lookback and lifetime. Wait for
  smith's `affinity` and `cache-reuse` measurements.
- **LLM connections' later work** (`llm-connection.md`, section 9):
  proxies, HTTP/2, fairness in the memory pool. Wait for a provider that
  needs them, or a measurement under load.
- **OAuth's extras** (`oauth.md`, section 7): an IPv6 loopback listener,
  device authorization or a pasted redirect, a credential helper. Wait for
  a decision.
- **kv's payload logs and later work** (`kv.md`, sections 6 and 12):
  sketched in `kv-implementation.md`, sections 6 and 9, not scheduled.
  Payload logs wait for temper's transcripts; values on disk and a shared
  sync, for a dataset or a second store.
- **The fake LLM's cache granularity** (`fake-llm.md`, section 7): waits
  for a world that needs providers' minimum lengths and steps.
- **The echo's worlds made pure** (`examples.md`, section 8; `testing.md`,
  section 8): the referee reads `listening()` rather than the fact `main`
  prints, and the echo has no domain worlds. Waits for a plan.
- **Heap handed between services in one thread** (`testing.md`, section
  9; `http.md`, section 9): the protocol worlds unmetered. Waits for a way
  to count the hand-off.
- **Rules held by a checker** (`programming-model.md`, section 10.3;
  `notes.md`): a `syn` checker and two more lints. Waits for review to
  prove not enough.
- **io's open questions** (`io.md`, section 10; `notes.md`): a fault that
  names a stall, and operations the kernel cannot interrupt. Wait for a
  decision.
- **The browser's open questions** (`browser.md`, section 11): Chromium's
  sandbox under containment, other browsers, text by a pattern beyond
  words. The first waits for contained trees.
- **skein-ui's later work** (`ui.md`, 4.2, 16, 18 and 19;
  `ui-text.md`, section 9; `ui-web.md`, sections 11, 14.2, 15 and 16;
  `browser.md`, sections 6 and 7; jig's
  `docs/development/plans/skein-ui/later.md`): the vocabulary that
  waits, right-to-left text, parts for a phone or a desktop, the file
  picker and other effects' parts, the bundle served compressed, a
  server keeping the previous digest directory, a markup macro, the text
  shell's words in another language and a secret typed without echo, the
  face typing into a field holding words, a test answering a native
  dialog, the kit's `browser.rs` split. Each waits for the application
  or the plan its row names.
- **The client's generic half** (jig's `README.md`, section 10, and
  `client/domain/client.md`, section 18): keyed requests sent again
  until durable, level-triggered watches, the link and generic guards,
  which jig's `jig-client` holds, may move to skein, as skein-ui moved,
  by a rename and a crate move. Waits for a second user, such as
  hearth's wire.

## 4. Drafts

| Draft | About | Next |
|---|---|---|
| `draft/process.md` | Contained process trees: a cgroup and a view of the file system per tree, stopped whole, proved empty, nested | Promotion into `io.md` by a design pass, then a plan of its own |
| `draft/redb.md` | redb as an adapter over skein's file records, at the boundary of a service and its shell | A decision to adopt it, once a store outgrows memory (`kv.md`, section 12); then its section 7 checks |

# Status

Updated 2026-10-19. How far skein's code is from its design, and which
plans take it there. The designer keeps this file (development.md,
section 3.3).

## 1. Plans

| Plan | State | Takes the design to | Where |
|---|---|---|---|
| temper's forge-kit | active on carbon-forge since 2026-10-19 | forge.md 2 to 9, 11; forge-connection.md 2 to 10; forge-client.md 2 to 10; forge-setup.md 2 to 4, 6 to 8; fake-forge.md 2 to 8: temper's forge crates, made neutral, moved here: `skein-forge` with the plans' own tests (skein 01, 10), the client (02), the component (03), reconcile (04), the fakes and the forge peer (05), the throwaway Forgejo and the conformance (06), and the worlds (07 to 09) | temper's `docs/development/plans/forge-kit/` |
| temper's tools | active on debian-16gb-hel1-1 since 2026-10-19 | lib.md 12: keys given by the asker, `ask` and `restore` refusing as `Full` or `Held`, and the admission number they return, which `not_kept` names (skein 01) | temper's `docs/development/plans/tools/` |
| temper's client-api | done 2026-10-18 | ui-web.md 14.2: the harness opens the printed URL whole, serves other origins, awaits trouble (skein 01); ui-text.md 3 and 4: the application's lines setting and `Terminal::room`, its own lines' room kept (skein-01.s1); the line bytes setting, `Terminal::worst_case` and `Terminal::operations` (skein-01.s2); lib.md 12: a request not kept, retired by its key (skein-01.s3) | temper's `docs/development/plans/client-api/` |
| temper's forge-setup | done 2026-10-18 | lib.md 6.2: SHA-256, `sha256::Hmac` and `sha256::equal`, with skein's three copies (io's digest, PKCE, the bundler) moved onto it (skein 01) | temper's `docs/development/plans/forge-setup/` |
| hearth's render | done 2026-10-18 | json.md 3.6: positions on skein-json's tokens and errors (skein 02); lib.md 5.1, testing.md 5 and 8, simulator.md 7: state digests, lib's types' `Hash`, replay and the trace (skein 03); io.md 5 and simulator.md 3.3: `Rename` between two directories, `ListPage` (skein 04); kernel.md 6.1, io.md 5 and simulator.md 3.1: `Lock`, its conformance scenario, and the answer of a `Lock` or a page past its deadline (skein 04, 05) | hearth's `docs/development/plans/render/` |
| kit-batch | done 2026-10-18 | development.md 3.5, 3.6, 5.4, 7.3, 7.4, 7.6, 9.1, 10.3, 11, 14.2, 15.2, supervisor.md and implementer.md: the kit's changes adopted from the forgejo-17, skein-ui, protocol, client-composition, parity and wall-clock retrospectives, and the director's candidate 7: the machine file, `plan check`'s refusals, the boot unit, `heavy`'s row from its task, `plan watch`'s events with what each needs, the coordinator's commands, the metrics at each model's rates (skein 01 to 06) | `docs/development/plans/kit-batch/` |
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

## 2. The design

| Document | Built | Left |
|---|---|---|
| `foundation/programming-model.md` | partly | 10.3: rules held in review only, with no checker: not planned |
| `foundation/testing-strategy.md` | partly | 6: state digests in the worlds of skein's services but the echo's (the harness carries them, `testing.md`, 5), transition coverage, `cargo fuzz`: not planned |
| `lib.md` | partly | 12: `RequestTable::not_kept`, a request whose record was not stored retired by its key: temper's client-api (skein-01.s3); keys given by the asker, `ask` and `restore` refusing as `Full` or `Held`, and the admission number they return, which `not_kept` names: temper's tools; 3: `Slab::get2_mut`: not planned |
| `kernel.md` | partly | 8: the descriptor limit on the ring; 9: fixed files: not planned |
| `io.md` | partly | 5: a root named by its token in `Create`, `CreateNoFollow`, `OpenRead`, `OpenReadNoFollow`, `OpenWrite`, `OpenWriteNoFollow`, `OpenDirectory`, `SyncDirectory`, `List`, `Rename` and `Remove`, which take its descriptor, read from `FileIo::descriptor`; a `Stat`, `Rename`, `Remove` or `MakeDirectory` that completes past its deadline answers `TimedOut` (`file_layer.rs`'s `stopped`), not its own; streams that read files; 4: DNS and datagram sockets; 6: contained trees: not planned |
| `simulator.md` | yes | Nothing |
| `shell.md` | partly | 6: names at startup (step 5), and certificates and keys for TLS's server side (step 6); 7: the readiness backend, and its conformance; 8: the deferred optimisations: not planned |
| `examples.md` | partly | 6: the referee takes the echo's address from `svc.listening()`, not from its startup fact; 7: the echo's domain worlds (testing-strategy.md, 2.2); the HTTP server and its client (testing.md, 3), not yet designed here: not planned |
| `testing.md` | partly | 3: the HTTP server and its client, and TLS in the real loop: not planned |
| `json.md` | partly | 4: writing in pieces, a body encoded a piece at a time as io grants room; 6: the fuzz target; 7: several documents in one stream; 8: codecs from schemas: not planned |
| `http.md` | partly | 3: chunked uploads; 3 and 5: content codings, trailer fields, HTTP/2 and upgrades; 4: reconnecting an event stream, and a reconnection time in a block of its own; 5: a request's body read while answering it; 6: the selective collector on each event's data in the protocol worlds, their heap metered, the fuzz targets; 8: a pool per peer, `Expect: 100-continue` waited for, the server's deadlines: not planned |
| `tls.md` | partly | 1: the server side, with client certificates; 3.6: a user of the native face, a framed connection over TLS (channel.md, 7); 5: the real loop, a loopback exchange through the shell against a local rustls server, and the fuzz target; kernel TLS (shell.md, 8): not planned |
| `codec.md` | partly | 7: maps and sets, smaller integers, other languages: not planned |
| `channel.md` | yes | 12: compression, bodies across frames, named calls, reconnecting: not planned |
| `kv.md` | partly | 6: payload logs, 12: a shared sync, values on disk: not planned |
| `llm.md` | partly | 6: Codex routing state, WebSocket: not planned |
| `llm-connection.md` | partly | 9: names while running, proxies, HTTP/2, pool fairness: not planned |
| `fake-llm.md` | partly | 2.2: a reply in pieces and a hold: designed, waits for a downstream repository's live-text plan; 7: providers' cache granularity: not planned |
| `oauth.md` | partly | 7: IPv6 loopback, device authorization, a credential helper: not planned |
| `browser.md` | partly | 6, 7: a test answering a native dialog itself, and the kit's `browser.rs` split by concern (jig's skein-ui `later.md`); 11: open questions: not planned |
| `fake-checkout.md` | yes | Nothing. A process face for real-loop worlds was proposed, not designed |
| `ui.md` | partly | 4.2, 18, 19: the vocabulary that waits (a combo box, a slider and a meter, a switch, media, a feed, a list box choosing several, a tree grid) and right-to-left text, each by the light path (4.3) when an application needs it; 3: the builder's assertion that a named button, tab or menu item holds no node: not planned (temper's client-api `later.md`); 19: a markup macro for the builder, a server keeping the previous bundle's digest directory; 16, 18: parts for a phone or a desktop: not planned (jig's skein-ui `later.md`) |
| `ui-text.md` | partly | 3, 4, 7: the settings' lines, `Terminal::room`, the terminal's own lines' room kept apart from the application's, no room once closing, the settings' line bytes, the terminal's worst case and operations: temper's client-api; 9: the text shell's own words in another language, a secret typed without echo: not planned (jig's skein-ui `later.md`) |
| `ui-web.md` | partly | 6: a click inside a button or a menu item finding it (`page/web.rs` finds a link's alone): not planned (temper's client-api `later.md`); 11, 15: the file picker, and parts for other effects; 15: the bundle served compressed, tabs of one person sharing one stream, HTTP/2; 16: a server keeping the previous bundle's digest directory; 14.2: the face typing into a field holding words: not planned (jig's skein-ui `later.md`); 14.2: the harness opening a printed line as a whole URL from either output, a page of another origin it serves, a printed URL again, and awaiting trouble and a request's status with its own `Host`: temper's client-api |
| `forge.md` | no | 2 to 9, 11: `skein-forge`, the vocabulary, Forgejo's dialect and its call plans, the markers, deliveries, credentials and the pin, held today by temper's `temper-forge-forgejo` and `temper-engine-domain-forge-client`: temper's forge-kit; 9: a recorded fact for a job no runner took: not planned (temper's forgejo-17 `later.md`); 10: GitHub's dialect: not planned |
| `forge-connection.md` | no | 2 to 10: the component, held today by temper's `temper-engine-protocol-forge`: temper's forge-kit; 10: the protocol world's late, reordered and lost deliveries, its stories run with hints and without; 5.1: renewal; 6: a listener over TLS, for a forge on another host (`tls.md`, 1): not planned |
| `forge-client.md` | no | 2 to 10: the working set, keeping up, fresh reads, the budget, effects' keys, lanes and recovery, restart, held today by temper's `temper-engine-domain-forge-client`: temper's forge-kit |
| `forge-setup.md` | no | 2 to 4, 6 to 8: the access, the three modes and Forgejo's reconcile, held today by temper's `temper-forge-setup`: temper's forge-kit; 5: GitHub's plan: not planned |
| `fake-forge.md` | no | 2 to 8: the fake forge's domain and protocol layer, the forge peer, the throwaway Forgejo and the conformance, held today by temper's fakes, `ForgeHost` and tools: temper's forge-kit; 4: deliveries to an organization's hook, in a world: not planned; 3: the real-git bridge: a design of its own |

## 3. Not planned

- **Contained process trees** (`io.md`, section 6): a cgroup and a view
  of the file system per tree, stopped whole and proved empty, in place
  of process groups. Waits for `draft/process.md` to become a design, then
  a plan of its own; smith's containment checks wait on it.
- **Names while running** (`io.md`, section 4; `llm-connection.md`,
  section 9): a DNS client, and the datagram sockets it needs. Waits for a
  service that must resolve while it runs.
- **Names at startup in the kit** (`shell.md`, section 6): services
  resolve their peers with the standard library. Waits for a decision
  whether the kit does it.
- **Roots by token** (`io.md`, section 5): the requests on open files and
  directories name a root by its descriptor, which the owner reads from
  `FileIo::descriptor`, though io alone should see one; `Load`, `Scan`,
  `Store`, `MakeDirectory`, `OpenPrivate` and `Lock` take its token.
  Waits for a plan.
- **Streams that read files** (`io.md`, section 5): waits for a user.
- **The readiness backend and the deferred optimisations** (`shell.md`,
  sections 7 and 8; `kernel.md`, section 9): wait for a deployment
  without io_uring, and for measurements. The backend's conformance
  comes with it.
- **The ring's descriptor limit** (`kernel.md`, section 8): waits for a
  way to lower it without `unsafe` outside the ring adapter.
- **State digests in the worlds** (`testing-strategy.md`, section 6):
  skein's harness carries them (`testing.md`, section 5) and the echo
  gives one; skein's other services and smith's worlds give none. Each
  waits for a plan of its own.
- **Transition coverage and fuzz targets** (`testing-strategy.md`,
  section 6; `json.md` and `http.md`, section 6; `tls.md`, section 5):
  wait for `cargo llvm-cov` and a nightly toolchain.
- **`Slab::get2_mut`** (`lib.md`, section 3): waits for its first user.
- **HTTP's remaining features** (`http.md`, sections 3, 4, 5 and 8): chunked
  uploads, content codings, trailers, reconnecting event streams and their
  `retry` block, HTTP/2 and upgrades, a request body read while
  answering, a pool per peer, `Expect: 100-continue`, the server's
  deadlines. Each waits for a peer that needs it; temper's forge and
  webhooks pull next.
- **The HTTP example** (`testing.md`, section 3): its design in
  `examples.md`, its service, worlds and real loop. Waits for a plan.
- **JSON's remaining parts** (`json.md`, sections 4, 7 and 8): writing in
  pieces, several documents in one stream, codecs from schemas, text that
  is not UTF-8. Each waits for a user; temper asks for the first.
- **The protocol worlds' collector** (`http.md`, section 6): the
  client's end stacks a tokenizer on each event's data, where an LLM
  client's stack has the selective collector (`http.md`, section 2).
  Waits for a plan.
- **TLS's server side and client certificates** (`tls.md`, section 1;
  `shell.md`, section 6; `channel.md`, section 12; `forge-connection.md`,
  section 6): wait for temper's engine, a forge's webhooks from another
  host, or smith's connected agent's domain contract.
- **A user of TLS's native face** (`tls.md`, 3.6): a framed connection
  over TLS (`channel.md`, section 7). Until one, the native face meets
  only its own tests (`tls.md`, section 5). Waits for a service that
  runs a channel over TLS.
- **TLS in the real loop, and kernel TLS** (`tls.md`, sections 5 and 7;
  `testing.md`, section 3; `shell.md`, section 8): a loopback exchange
  through the shell against a local rustls server. Waits for a plan; a
  TLS that replays and session resumption are open (`notes.md`).
- **The rustls issue** (`tls.md`, section 6): drafted against 0.23.41
  and not filed, so that the override of `[profile.dev.package.rustls]`
  can go. Waits for the user to file it:

  > **`send_fatal_alert` panics in debug builds after `send_close_notify`**
  >
  > `CommonState::send_close_notify` sets `sent_fatal_alert`, so that no
  > alert follows `close_notify`; `send_fatal_alert` begins with
  > `debug_assert!(!self.sent_fatal_alert)` (common_state.rs:567). A
  > client that sends `close_notify` and reads on, as a half-closed
  > connection lets it (RFC 8446, 6.1: each side closes its own
  > direction), reaches it on the first fatal error in what it reads: a
  > record that fails to decrypt (conn.rs:1133, `BadRecordMac`), a message
  > that does not decode, one that comes out of turn. A debug build
  > panics; a release build queues a fatal alert behind the
  > `close_notify`.
  >
  > To reproduce with `UnbufferedClientConnection`: complete a handshake,
  > `queue_close_notify` and send it, then hand `process_tls_records` the
  > server's next application-data record with a byte of it changed.
  > Expected: `Error::DecryptError`. With debug assertions: a panic at
  > common_state.rs:567.
  >
  > Suggested fix: once `close_notify` was sent, `send_fatal_alert` sends
  > no alert and returns the error, as `send_close_notify` sends nothing
  > after a fatal alert; the assertion then guards only against a second
  > fatal alert.
- **Channel features** (`channel.md`, section 12): compression, bodies
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
- **The fake LLM's pieces and holds** (`fake-llm.md`, section 2.2): a
  scripted reply sent in pieces, and a hold released by its owner.
  Designed, and waits for the downstream repository's live-text plan,
  which needs it to read a reply partway.
- **The fake LLM's cache granularity** (`fake-llm.md`, section 7): waits
  for a world that needs providers' minimum lengths and steps.
- **The echo's worlds made pure** (`examples.md`, sections 6 and 7): the
  referee reads the echo's address from `svc.listening()` rather than
  from its startup fact, and the echo has no domain worlds. Waits for a
  plan.
- **Heap handed between services in one thread** (`testing.md`, section
  9; `http.md`, section 6): the protocol worlds unmetered. Waits for a way
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
- **GitHub** (`forge.md`, section 10; `forge-setup.md`, section 5): its
  dialect and its setup's plan. Waits for a service on GitHub.
- **The real-git bridge** (`fake-forge.md`, section 3): real git
  against the fake forge, for real-loop worlds. Waits for a design
  session of its own.
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

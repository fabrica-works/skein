# Status

Updated 2026-10-20. How far skein's code is from its design, and which
plans take it there. The designer keeps this file (development.md,
section 3.3).

## 1. Plans

| Plan | State | Takes the design to | Where |
|---|---|---|---|
| smith's perf | active on hetzner2 since 2026-10-20, owned by o/smith-perf | llm.md 2.5: a cut HTTP exchange is `Unavailable`, its detail naming the cut (e880245); http.md 3 and llm.md 4.8: the gzip encoder, and the Codex route sending its request body gzip; llm-connection.md 4: `request_bytes` the bytes sent; fake-llm.md 3.1: the fake inflating a gzip body | `~/src/rust/tmp/owners/smith-perf/` (not versioned) |
| temper's first-run | designing since 2026-10-20, owned by o/first-run | fake-llm.md 2.4, 3: a script cued by the person's request, an answer for anything else, a peer that keeps no records; fake-forge.md 1, 4, 5: the forge peer's setup store lent to its owner, a setup grant reaching the domain, a peer that keeps no records; fake-forge.md 4, 5, 8: deliveries following the setup's hooks; fake-llm.md 3: the test root exported; fake-forge.md 1, 3, 4, 5, 8: the git route, a fake of a service's git reaching the store from another process (temper's `draft/qa-loop.md`, 11); testing.md 6, forge.md 2, fake-forge.md 1, fake-checkout.md 1: the fake LLM, the fake forge's domain and protocol layer, the fake issuer, the peers and the fake checkout moved to `crates/`, their tests to `tests/fake-forge`, `tests/fake-peers` and `tests/fake-checkout` (temper's `draft/first-run.md`, 3.5) | `~/src/rust/tmp/owners/first-run/` (not versioned) |
| temper's forge-kit | done 2026-10-20 | forge.md 2 to 9, 11; forge-connection.md 2 to 9; forge-client.md 2 to 10; forge-setup.md 2 to 4, 6, 8; fake-forge.md 2 to 8: temper's forge crates, made neutral, moved here: `skein-forge` with the plans' own tests (skein 01, 10), the client (02), the component (03), reconcile (04), the fakes and the forge peer (05), the throwaway Forgejo and the conformance (06), and the worlds (07 to 09) | temper's `docs/development/plans/forge-kit/` |
| hearth's M2 files | designed 2026-10-19, owned by o/hearth-skein-files | kernel.md 6.1, 6.4, 6.5, io.md 5, simulator.md 3.1, 3.3, 4, testing.md 4: the full stat by name, links, modes, owners, extended attributes, the filesystem's status and sync, btrfs's subvolumes, for hearth's tree helper and store (hearth's `README.md`, 8.1) | `~/src/rust/tmp/owners/hearth-skein-files/` (not versioned) |
| hearth's M2 trees | designed 2026-10-19, owned by o/hearth-skein-trees | io.md 6, 6.2 to 6.4, kernel.md 6.2, 6.6, 7, 8, shell.md 6.4, simulator.md 3.4, testing.md 4: cgroups made, delegated, entered, counted, signalled and killed, proved empty; a child spawned into one, as other ids, given descriptors, contained (a user namespace mapped by `newuidmap`, the pid, network, ipc, uts and cgroup namespaces, a composed view, `no_new_privs`, rlimits) through the helper `skein-contain`; their conformance on the ring; the end-to-end kit's cgroup code moved onto io (examples.md 6) | `~/src/rust/tmp/owners/hearth-skein-trees/` (not versioned) |
| temper's browser-1 | done 2026-10-20; skein 01 done 2026-10-19 | browser.md 5 and 6: the kit's `Address` read of the history's current entry (skein 01.1) and its `Key` request with `shift` (skein 01.3); ui-web.md 14.2: the face's read of the current address and a step checking it (skein 01.1), `back_and_forward` awaiting a page that holds its server's state (skein 01.2), the face's key with Shift held (skein 01.3); ui-web.md 6: a click inside a button or a menu item finding it (skein 01.4); ui-web.md 5.1: the fetch part's stream refused, with its status (its QA, 3cd6428) | temper's `docs/development/plans/browser-1/` |
| temper's tools | done 2026-10-19 | lib.md 12: keys given by the asker, `ask` and `restore` refusing as `Full` or `Held`, and the admission number they return, which `not_kept` names (skein 01) | temper's `docs/development/plans/tools/` |
| smith's live-text | done 2026-10-19 | fake-llm.md 2.2: a reply in pieces, its holds and their release by the domain's event and by the byte peer (skein 01) | smith's `docs/development/plans/live-text/` |
| smith's accounts | done 2026-10-20 | the first part of LLM accounts: the MIT licence; llm.md 1, 2.5, 2.6, 4 to 4.4, 4.7, 4.8, 5 and llm-connection.md 1, 3, 4, 4.2, 8: the public Responses route, Anthropic with a key, a credential's scheme or none, the failure classes and their report, limits per route (skein 01, 02); fake-llm.md 2.3, 3, 3.1, 5, 6, 7: scripted failures, several credentials, observations of route, scheme and index (skein 03, 04, 11); oauth.md 1 to 5: an issued client id, `nonce` and `resource`, the ID token checked, granted scopes, revocation, the record's version 3, the fake issuer's additions (skein 05 to 07, 11); oauth.md 6.1 to 6.8: the driver's accounts while running, signing out, the shared record under the owner's lock (skein 08 to 10) | smith's `docs/development/plans/accounts/` |
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
| `README.md` | yes | Nothing: the index of the parts, their crates and what is not in skein |
| `lib.md` | partly | 12: `RequestTable::not_kept`, a request whose record was not stored retired by its key: temper's client-api (skein-01.s3); keys given by the asker, `ask` and `restore` refusing as `Full` or `Held`, and the admission number they return, which `not_kept` names: temper's tools; 3: `Slab::get2_mut`: not planned |
| `kernel.md` | partly | 6.1, 6.4, 6.5: `StatEntry`, `Stat`'s whole metadata and `Kind`'s special files, a tree's metadata (`ReadLink`, `MakeLink`, `HardLink`, `SetMode`, `SetOwner`, the attributes), `FilesystemStatus`, `SyncFilesystem` and btrfs's subvolumes, each with its conformance scenario (8), btrfs's on the ring alone: hearth's M2 files; 6.2, 6.6, 7, 8: cgroups, a spawn into one, the helper and its setup, with their conformance: hearth's M2 trees; 6.4: setting an entry's times: not planned (hearth's M3); 8: the descriptor limit on the ring; 9: fixed files: not planned |
| `io.md` | partly | 5: the requests of a tree's metadata, the filesystem and btrfs's subvolumes, with their step tests and a world that cuts a power loss after each (8): hearth's M2 files; 5: a root named by its token in `Create`, `CreateNoFollow`, `OpenRead`, `OpenReadNoFollow`, `OpenWrite`, `OpenWriteNoFollow`, `OpenDirectory`, `SyncDirectory`, `List`, `Rename` and `Remove`, which take its descriptor, read from `FileIo::descriptor`; a `Stat`, `Rename`, `Remove` or `MakeDirectory` that completes past its deadline answers `TimedOut` (`file_layer.rs`'s `stopped`), not its own; streams that read files; 4: DNS and datagram sockets: not planned; 6, 6.2 to 6.4: cgroups, a tree's spawn, its stopping and its proof of empty: hearth's M2 trees; 10: filters on a contained tree's calls, a terminal for a tree: not planned |
| `simulator.md` | partly | 3.1, 3.3, 4: a tree's metadata and the filesystem through the seam, their faults, and what a power loss keeps of them: hearth's M2 files; 3.4: cgroups, and a contained spawn with its stage faults: hearth's M2 trees |
| `shell.md` | partly | 6: names at startup (step 5), and certificates and keys for TLS's server side (step 6); 7: the readiness backend, and its conformance; 8: the deferred optimisations: not planned; 6.4: the helper's dispatch and the service's own cgroup at startup: hearth's M2 trees |
| `examples.md` | partly | 6: the referee takes the echo's address from `svc.listening()`, not from its startup fact; 7: the echo's domain worlds (testing-strategy.md, 2.2); the HTTP server and its client (testing.md, 3), not yet designed here: not planned |
| `testing.md` | partly | 6: the fakes in `crates/`, their tests under `tests/`: temper's first-run; 4: the minimal machine's inodes, groups, modes, times, attributes, hard links and FIFOs: hearth's M2 files; 4: the trees' fixture programs: hearth's M2 trees; 3: the HTTP server and its client, and TLS in the real loop: not planned |
| `json.md` | partly | 4: writing in pieces, a body encoded a piece at a time as io grants room; 6: the fuzz target; 7: several documents in one stream; 8: codecs from schemas: not planned |
| `http.md` | partly | 3: chunked uploads; 3 and 5: content codings beyond the gzip encoder (smith's perf), trailer fields, HTTP/2 and upgrades; 4: reconnecting an event stream, and a reconnection time in a block of its own; 5: a request's body read while answering it; 6: the selective collector on each event's data in the protocol worlds, their heap metered, the fuzz targets; 8: a pool per peer, `Expect: 100-continue` waited for, the server's deadlines: not planned |
| `tls.md` | partly | 1: the server side, with client certificates; 3.6: a user of the native face, a framed connection over TLS (channel.md, 7); 5: the real loop, a loopback exchange through the shell against a local rustls server, and the fuzz target; kernel TLS (shell.md, 8): not planned; 3.4: `Name::server`, a name read back as what it is: temper's forge-kit |
| `codec.md` | partly | 7: maps and sets, smaller integers, other languages: not planned |
| `channel.md` | yes | 12: compression, bodies across frames, named calls, reconnecting: not planned |
| `kv.md` | partly | 6: payload logs, 12: a shared sync, values on disk: not planned |
| `llm.md` | partly | 1, 2.5, 2.6, 4 to 4.4, 4.7, 4.8, 5: the public Responses route, Anthropic with a key, a credential's scheme, or none, the failure classes (exhausted with a reset only where given, refused, unsupported, a stream's failure with an unnamed code classed unavailable) and their report, limits per route: smith's accounts; 4.6, 4.7: the Codex route, and Anthropic's bearer route with its identity profile, withdrawn and removed once their last caller has moved: not planned; 6: Codex routing state, WebSocket: not planned |
| `llm-connection.md` | partly | 1, 3, 4, 4.2, 8: a credential with its scheme handed with each call that has one, the provider's report on a failure, several accounts' credentials on one pool: smith's accounts; 9: names while running, proxies, HTTP/2, pool fairness: not planned |
| `fake-llm.md` | partly | 2.4, 3: a script cued by a request and an answer otherwise, a peer without records, the test root exported: temper's first-run; 2.2: a reply in pieces and a hold: smith's live-text; 2.2: the byte peer as a process of its own, refusing a script with a hold at its start: not planned (smith's live-text `later.md`); 2.3, 3, 3.1, 5, 6, 7: scripted failures, their resets and rows per route, several credentials each with its scheme, or none, the public route and the key, observations of the route, the scheme and the credential's index, the real loop's use with the fake issuer's tokens: smith's accounts; 7: providers' cache granularity: not planned |
| `oauth.md` | partly | 1, 2, 2.1 to 2.3, 3, 4, 5: an issued client id, `nonce` and `resource`, the ID token checked, granted scopes, revocation, the added error names, the record's version 3, the fake issuer's additions: smith's accounts; 6.1 to 6.8: accounts added and removed while running, signing out, a wanted account refreshed with no grant, a shared record re-read under the owner's lock, a redirect port that varies: smith's accounts; 7: IPv6 loopback, device authorization, a credential helper: not planned |
| `browser.md` | partly | 6, 7: a test answering a native dialog itself, and the kit's `browser.rs` split by concern (jig's skein-ui `later.md`); 11: open questions: not planned |
| `fake-checkout.md` | partly | 1: the kit in `crates/`, its tests in `tests/fake-checkout`, shipped in temper's `temper fake git` over the fake forge's git route: temper's first-run |
| `ui.md` | partly | 4.2, 18, 19: the vocabulary that waits (a combo box, a slider and a meter, a switch, media, a feed, a list box choosing several, a tree grid) and right-to-left text, each by the light path (4.3) when an application needs it; 3: the builder's assertion that a named button, tab or menu item holds no node: not planned (temper's client-api `later.md`); 19: a markup macro for the builder, a server keeping the previous bundle's digest directory; 16, 18: parts for a phone or a desktop: not planned (jig's skein-ui `later.md`) |
| `ui-text.md` | partly | 3, 4, 7: the settings' lines, `Terminal::room`, the terminal's own lines' room kept apart from the application's, no room once closing, the settings' line bytes, the terminal's worst case and operations: temper's client-api; 9: the text shell's own words in another language, a secret typed without echo: not planned (jig's skein-ui `later.md`) |
| `ui-web.md` | partly | 11, 15: the file picker, and parts for other effects; 15: the bundle served compressed, tabs of one person sharing one stream, HTTP/2; 16: a server keeping the previous bundle's digest directory; 14.2: the face typing into a field holding words: not planned (jig's skein-ui `later.md`); 14.2: the harness opening a printed line as a whole URL from either output, a page of another origin it serves, a printed URL again, and awaiting trouble and a request's status with its own `Host`: temper's client-api; 14.2: the harness hosting the peers a test lends it, running commands (the server's held link on one's standard input, the link one prints opened), and keeping a page's value for the test: o/github's GitHub work |
| `forge.md` | partly | built: 3's protection queue, signatures, linear history, code owners and the App's own `Bypass`, with Forgejo answering false and `Never`; 4.3: the binaries' digests and `digest(binary, arch)`: temper's first-run; built: 2's fakes in `crates/`, and 3, 8 and 10's first vocabulary for GitHub (`Validator`, `Rate`, `Answer::Queued`, `Pull`'s `queued` and `dequeued`, `Credential::App`), each refused or `None` until the task that gives it meaning; 2 to 9, 11: `skein-forge`, the vocabulary, Forgejo's dialect and its call plans, the markers, deliveries, credentials and the pin: temper's forge-kit; 9: a recorded fact for a job no runner took: not planned (temper's forgejo-17 `later.md`); 3, 8, 10: the rest of GitHub's additions to the vocabulary (`NotConnected`, `NotInstalled`): o/github's GitHub work, after forge-kit's skein steps 07.3, 08.1 to 08.4, 09.1 and 10.5 |
| `forge-github.md` | partly | built: 5.3's protection documents, server encoders and read plan, joining paged applicable rules, each distinct ruleset's bypass and classic protection within `inner`; 2, 3, 6, 7 and 12, the module's foundation (its requests, heads, walker, failures' map, pin and the server's side of its routes), 4.1's reads' requests and documents, and the plans of Locate, Settings, Branch, Branches, Items, Item, Pull, PullFor and Statuses (2's tables and head, 7's failures' map in a plan; the issue listing and an issue with its comments, paged by Link; a pull with its CI folded from the combined status and the check runs, the open-then-oldest pull for a head and base, and a commit's statuses, paged); without the other reads' plans and every write's wait; 8.1's token bytes (`github::jwt`); 8.2 to 8.4's requests, documents and plans (the mint, installations, reachable repositories, the identity read: `github::plans::start`'s Listing, Minting, Reachable and Identity); 9's deliveries (the heads, the signature's check and sign, the payload's decoder and its forge hint, the encoder `server::encode` and `Sent`), 9.3's delivery log pages and trusted whole reads (`github::log`, `github::plans::start::{LogPage, LogDelivery}`); 10's job metadata and log redirect, with tail retention; 9.2's guid, 10's `Redirect`, 11's manifest, form and install page, the App's reads, its conversion and bot user; 4 to 14 else: `skein_forge::github`, GitHub's dialect at its pin, the App's documents, deliveries and the delivery log, the job log's redirect, the setup's documents: o/github's GitHub work; 15: its open questions |
| `forge-connection.md` | partly | built: 3's access handed at runtime (`Request::Connect` with an `Access`, `Event::Connected`, `Stopped::Invalid` naming the refused `Value`) and a forge without it answering `NotConnected` at no cost; 3.5, 6.1, 7.2, 7.4: `Kind::GitHub` in the settings and the checks (an App's key per GitHub forge, the other dialect's credential refused, `key_bytes`, `installations` and `renew_before` checked); 3.4, 5.2, 5.3, 8.3: GitHub's start, listing installations on the signed and reused `Jwt`, minting Full and Contents tokens for accepted unsuspended installations, checking the bot with Viewer on Full, ready without an identity read when none is accepted, and `Event::GitToken` handing Contents to the owner; 2.1, 4.4: `Request::Installations`, the bounded table reconciled page by page, accepted rows kept first, missing accepted ids reread once, `Event::Installation` with `Account` and `Change`, and `Fact::Unkept` once per read; 3.5, 5.1, 5.3, 8.6: eligible installations minted while Ready, tokens dropped and stale answers invalidated on loss, deferred Identity without a second `Connected`, at most one later `Stopped`, `Stopped::{KeyRefused, TokenRefused}`, each scope's renewal due fixed at mint, private capped backoff including already-due mints, a usable Full kept until expiry and renewed Contents handed up once; 5 and 8.1: one private HTTP/TLS/JSON connection carrying Forgejo and GitHub exchanges, the component supplying `Authorization`, each dialect encoding its request, reading its head and decoding its answer, the added read rows and write results, GraphQL pulls and App permissions/events included; 7.5: the heap bound and fixture accounting for those decoders, installation reads, token loss and renewal; 10: the three protocol invariants over Forgejo and GitHub (one terminal per call through cuts and restarts, costs equal to first-byte writes, no handed secret in a record, request line or fact: forge-kit's 09.4), over GitHub with the App's key, every JWT and every installation token as the secrets, through renewals and a key replaced while calls run, and its stories of a lost creation found after a restart and listings following another party's changes, the machine world's GitHub peer and its start, installation and renewal stories, including discarding the reply of a read still undecoded; 4.1 to 4.4: GitHub's calls routed by installation account, an account's accepted installation selected at admission, `NotInstalled` without one, the per-dialect tables (`Component::forgejo_tables`, `github_tables`) and their heap in 7.5; 4.1 to 4.4, 7.5: GitHub's File, Compare and PullFiles calls, File's head and whole bodies kept to the answer bound, PullFiles' before sides read at the merge base, one boxed exchange per connection in the heap bound, and an unknown repository's installation found by searching across installations; 7.5: the bound priced per kind of forge (`limits::Forges`, `worst_case` taking each kind's count), a dialect's tables, decoder share and per-forge App terms paid only for a kind configured, each call slot at the larger configured dialect's terms; 7.5: a GitHub call slot's files state, the read containers and Reviewers' lists priced and the largest Compare and PullFiles pages measured against it; 4.1 to 4.4, 7.5: GitHub's Merge and SetReviewers, the queue's 405 going on to GraphQL's pull and enqueue under the one selected installation, a merge's 401 renewing Full without replay, a sent enqueue never replayed, and their state priced in the slot; 4.1: a call's terminal carrying `free`, its requests answered `304`, and `rate`, the latest window GitHub reported across its answers (`Answered`'s `free` and `rate`, passed through to the client's event); 7.5, 10: the full holders of Forgejo, GitHub and mixed forges measured against the declared worst case (forge-kit's 08.5); 4.5, 3.2, 7.1, 7.2, 7.5: job logs followed once to a configured resolved origin without a credential or Range, origins pooled separately, unconfigured origins diagnosed once per remembered origin per connect, and configured/said storage priced for GitHub alone; 6.5, 7.2, 7.5, 8.7: GitHub's delivery-log puller, conditional first pages and cursor catch-up, bounded age/page gaps, once-per-poll installations reads, whole repository-selection deliveries, held hints and JWT reads sharing the own connection at no call cost, its four limits and checked heap; the dialect caps log pages at `min(inner, 100)`; 4.2: a delivery read whole renaming its kept repository row, or dropping it for a new owner; 4.5, 10: the machine world's job-log stories on the GitHub fake's signed blob listener (followed whole or as a tail, no credential or `Range`, an unconfigured origin said once per connect, a second redirect and an expired signature refused); 4.5, 6.5, 10: the worlds' GitHub log stories on the fake: the machine world's delivery-log puller (a first poll keeping the newest guid alone, catch-up to the kept guid hinting each delivery once, page and age bounds saying one `Gap` and rereading installations, a `304` keeping the validator and guid, redeliveries skipped and never kept, a `401` stopping `KeyRefused`), an unconfigured origin said again after a connect, and the protocol world's deliveries reaching the client's owner through GitHub's log before its own polling (forge-kit's 09.3's log half); the machine world's joint serving GitHub beside Forgejo; 4.1: GitHub's `304`s; 10: the remaining GitHub cases: o/github's GitHub work; 6: a listener over TLS, for a forge on another host (`tls.md`, 1): not planned |
| `forge-client.md` | partly | built: 2 to 10: the working set, keeping up, fresh reads, the budget, effects' keys, lanes and recovery, restart (temper's forge-kit); 7.1 and 8's writer given at runtime and kept as a row (`writer`, `Stored::Writer`, `Key::Writer`); 5, 7.4 and 7.5: `NotConnected` and `NotInstalled`, a write failing at once, an uncertain find tried again, a fresh read ended once; 7.7: a merge answered `Queued` kept (`Entry`'s `queued`, `Outcome::Queued`), its lane held and its pull request looked at until the queue decides; 6: a call settled by its cost less the requests the forge answered `304` (`Event::Answered`'s `free`), and the forge's rate window bounding the budget (`Event::Answered`'s `rate`), at 0 holding all four classes until its reset, refunds never lifting the window above its last figure, the last unit of a capped window held until the forge's reset |
| `forge-setup.md` | yes | built: 2 to 4, 6.1, 6.2, 8: the access, the three modes and Forgejo's reconcile (temper's forge-kit); 6.3's host component (`host::Host`, `Plan::Reconcile`, `Check`, `TakeOver`, `Adopt`, `Reachable`, `Event::Done` carrying `Outcome`): one writing run and one picker per forge, `runs` counting both, `Busy` past them, each `Send` on a connection of its own, waiting as 4.4 says; 4.3's narrowed reconcile (no team, no organization hook, a given org made when absent, none read on a managed forge); 4.6's check of connected's two tokens (`check::Check`, one user kept, `OtherUser`) and a plan's `Done` outcome (`Need::Done(Outcome)`); 4.7's take-over of an owned forge's admin's token, checked, kept as `AdminToken`, the admin's other tokens deleted; 4.1, 4.5, 6.3: adopted repositories given and checked (`Given`'s `adopted`, `given::check_adopted`, at most `Limits`' `repositories`), `adopt::Adopt` and reconcile repairing the bot's collaborator access and each loopback repository hook; 4.8: `reachable::Reachable`, the admin's repository search or connected's API-token listing (`Request::UserRepos`, `document::Reachable` with its `push`), only writable connected rows kept and `more` counted before the drop, the fake's private repositories and grants and the listing's conformance facts; 6.4: `User` as eight bytes, little-endian, and GitHub's `Item::{AppId, AppSlug, AppKey, Accepted, Pasted}`, their total decoders and `item::Bounds`; 4.2: `Need::Send` with `call::{Call, Auth}`, `Reply::{GithubDocument, GithubRows, Unsigned}`, `Stage::{App, State}` and `Why::{KeyForm, Unknown, OtherApp}`; 5.1 to 5.7, 6.1, 6.3: `github::given`'s `Given`, `Place`, `Refused` and `check`, `github::limits`' `Limits` and `check`, used by the plans and host; `host::Kind::{Forgejo, GitHub}` and `Forge`, GitHub requests encoded and decoded at the pin, an App JWT signed per request, an installation bearer or no auth, an unsignable key never sent, and `host::github` with `Unusable::GitHubDocuments` checking document limits; 4.8, 5.9, 6.3: `Reachable::github` and the host's GitHub picker, checking `Accepted`, minting once per page, returning `Outcome::Reachable`, keeping neither key nor token past the run; 5.1, 5.7, 6.3: `github::manifest::{Target, Manifest}`, `Plan::Manifest`, `Outcome::Manifest` and the bounded host-wide one-time state table, binding states to forge and purpose, expiring and evicting them, taking each once, forgotten on close or restart; 5.2, 5.3: `github::convert::Convert`, converting without auth, decoding PEM to DER, proving the App and bot and writing `AppId`, `AppSlug`, `AppKey`, `User`, each acknowledged before `Done`; 5.6, 5.8: `github::checks`' `Lacking` and `lacks`, the App's missing wanted permissions and events in manifest order, and `Outcome::Lacking`; 7: the setup world (forge-kit's 07.4) and the host's GitHub picker against the fake over HTTP bytes; 5.4, 5.7, 6.3: `github::install` and `github::accept`, `Plan::Install`, `Plan::Accept` and `Outcome::Install`, the install link with an Accept-purpose state and an installation accepted through its own state; 5.2, 6.3: `Plan::Convert`, the host keeping the conversion's items before answering the browser; 5.6: the installation sweep, the accepted set pruned only after a complete listing, the strays seen uninstalled in id order, a listing past `pages` ending `Why::Crowded`; 5.5, 5.6, 6.3: GitHub's reconcile (`github::reconcile::Reconcile`), reading the kept App and `Accepted`, proving the App, checking its wanted permissions and events, repairing its hook, sweeping installations and answering `Lacking` after every write is acknowledged, run by `Plan::Reconcile` on a GitHub forge with an empty adopted set, and `Unusable::GitHubMetadata`; 7: the setup world's GitHub stories over HTTP (an installation accepted by its state, the accepted set pruned after a complete listing, strays removed by age, the hook repaired, a second reconcile writing nothing, Install and Accept over fixed seeds), and `github::worst_case`, the heap of the pure plans and the states table; 5.8, 6.3, 10: GitHub's paste (`github::paste::Paste`, `Plan::Paste`), a pasted App connected or replaced without changing the forge's identity; 6.3: the host's `Transport::Tls`, skein-tls's client under HTTP, a TLS forge at any IP address; 6.5, 7: `host::worst_case`, the host's whole heap over Forgejo and GitHub, plaintext and TLS, a slot holding a run and its settling connection until io's close deadline frees it, and the setup world's counting allocator checking it at every iteration; 4.4, 6.5: each connection's rows reserved at its request's page, a Forgejo listing's page lowered to `inner`, and a call past the priced page refused at the connection's opening; 9: a connected GitHub's fine-grained token and an App's hook at a public address: not planned |
| `fake-forge.md` | partly | built: 10.1, 10.4 and 10.5's person credentials, GET /user, installation token revocation and scope matrix, account-limited repository reach, byte faults, and full-store service/peer allocator binaries; GH013 push refusal deferred until P10 and a GitHub push boundary exist; 10.1, 10.4 and 10.5's CI check runs and job reads, signed 600-second job-log redirects, the peer's bounded blob listener and request record, third-origin and blob-redirect scenarios, and seeded head/base mergeability settle deadlines (GitHub null, Forgejo false while checking); 1, the fakes in `crates/`; 2.5's promises' judge (`promises`), a client's once, forced, foreign and order promises over the decoded observations, undecided under loss, truncation, a full table, missing decisions or a move without `forward`; 10.2's protection bypass list, linear and code-owner facts, and approval checks at queue admission; 10.1's ruleset, classic and combined protection presentations, signatures from scenario configuration, with the enqueue approval refusal text awaiting a recording; 10.1's start of GitHub's layer (`testing/skein-fake-forge-github`), the App's public test keys, the check of its JSON Web Token, installations and their minted tokens, and its Service: the App's routes (the App, its hook, the installations, the mint, the uninstall), the token's routes (the reachable repositories, paged, and the viewer) and the repository, its branch and its branches through the shared domain, and the browser's pages (the manifest form and its states, the conversion of a code into the App, the install page and its reach), without their hooks; 4, 5, 8: a setup grant reaching the domain, deliveries following the setup's hooks, the forge peer's setup store lent, a peer without records: temper's first-run; 1, 3, 4, 5, 8: the git route, its repository's calls and its store's, served on the forge peer's listener: temper's first-run; 2 to 8: the fake forge's domain and protocol layer, the forge peer, the throwaway Forgejo and the conformance, held today by temper's fakes, `ForgeHost` and tools: temper's forge-kit; 9: deliveries to an organization's hook: not planned; 3: the real-git bridge: a design of its own; 10, 11: the rest of GitHub's layer and GitHub's conformance on it and live: o/github's GitHub work, the domain's gains in 10.2 after forge-kit; 9: speculative merges in the queue, recorded live transcripts: not planned |
| `lab.md` | no | 2 to 10: the records, the runner in `scripts/lab/` (Python's standard library, for now) and its two test tiers: temper's QA loop, owned by o/qa (`~/src/rust/tmp/owners/qa/`, not versioned); 10.3: the unit tier in the gate and the process tier after landing, wired by the kit's owner once the commands are ready; 5.2, 10.4: the host on skein's contained trees, and the runner in Rust, under a jig lab application: not planned |

## 3. Not planned

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
- **The byte peer as a process of its own** (`fake-llm.md`, section
  2.2): one that refuses a script with a hold at its start, since no
  owner can release it. No such process exists: the worlds and the real
  loop host `skein-fake-peers`' `llm::Peer` in their own process, and
  release through it (smith's live-text plan, its `decisions.md`, design
  question 2). Waits for such a process; the designer rewords 2.2 and 6
  first.
- **The fake LLM's cache granularity** (`fake-llm.md`, section 7): waits
  for a world that needs providers' minimum lengths and steps.
- **The first part of LLM accounts**, the rows here that name it and a
  downstream repository's, is two plans, by layer, run after smith's
  live-text plan that builds `fake-llm.md`, section 2.2: the fakes'
  scripted failures and credentials (2.3, 3) stand beside its pieces and
  holds. The first plan, its grants, smith's `accounts` (section 1),
  takes every row here that names it; the other, its sources, runs from
  the first's main and changes a downstream repository only. The
  downstream repositories' parts follow, as the second part.
- **The withdrawn LLM routes removed** (`llm.md`, sections 4.6 and 4.7):
  the Codex route, with the account id and its identity headers, and
  Anthropic's bearer route, with its beta and identity profile. Waits for
  their last caller to move to the public route and to a key.
- **The echo's worlds made pure** (`examples.md`, sections 6 and 7): the
  referee reads the echo's address from `svc.listening()` rather than
  from its startup fact, and the echo has no domain worlds. Waits for a
  plan.
- **Heap handed between services in one thread** (`testing.md`, section
  9; `http.md`, section 6): the protocol worlds unmetered. Waits for a way
  to count the hand-off.
- **A checked world that knows a shared handle's owner**
  (`testing-strategy.md`, section 6; `fake-forge.md`, section 5): memory a
  hosted program allocates in a peer's shared handle outlives the
  program, as temper's fake git's does in the forge peer's `Forge`, and
  skein-world's checked world (`World::new_checked`, its `Heap::release`)
  fails it. temper's real-loop forge
  stories run on `World::new_controlled` until it knows the owner
  (temper's real-loop plan, its `later.md`). Waits for a plan of its own.
- **Rules held by a checker** (`programming-model.md`, section 10.3;
  `notes.md`): a `syn` checker and two more lints. Waits for review to
  prove not enough.
- **io's open questions** (`io.md`, section 10; `notes.md`): a fault that
  names a stall, operations the kernel cannot interrupt, filters on a
  contained tree's calls, and a terminal for a tree. Wait for a
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
- **GitHub's later** (`forge-github.md`, sections 4.3 and 15;
  `fake-forge.md`, section 9): reads batched into one GraphQL query,
  conditional reads beyond the delivery log, speculative merges in the
  fake's queue, live transcripts recorded for replay. Each waits for a
  budget or a world that needs it.
- **A connected GitHub's fine-grained token, and an App's hook at a
  public address** (`forge-setup.md`, section 9): a connected GitHub is
  an App the person made, and every App's hook is at no listener. Each
  waits for a service that asks.
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
| `draft/redb.md` | redb as an adapter over skein's file records, at the boundary of a service and its shell | A decision to adopt it, once a store outgrows memory (`kv.md`, section 12); then its section 7 checks |

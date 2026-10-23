# skein's design

Provisional, 2026-10-19. The index of skein's parts: what each holds,
its crates and its design document, how the crates depend on one
another, and what skein leaves to the services built on it. It was the
repository's `README.md` until 2026-10-19. The rules every part follows
are skein's foundation documents: `programming-model.md`,
`testing-strategy.md` and `notes.md`. How a service is built on skein is
`examples.md`, section 10. `STATUS.md` says how far the code is from
this design.

## 1. In one page

- **A kit, not a framework.** skein has no service trait, no generic loop,
  no scheduler and no callbacks. A service calls the parts by name, in
  its own loop of about ten lines. Nothing in skein calls into a service,
  not even the simulator.
- **io_uring is the kernel interface.** io talks to the kernel in owned
  records: an operation goes down carrying its memory, and its completion
  comes back carrying the same memory. The ring backend maps each record
  onto one submission; the simulator implements the same records; the
  readiness backend (epoll, kqueue), for hosts without io_uring and
  macOS, implements them a third time. Nothing above the records changes.
- **Counted and bounded throughout.** Every part exports its `Limits` and
  its `worst_case`, and a service adds them up.
- **Built when pulled.** A part is built when its first user needs it,
  and that user is its first test.
- **Laid out by role** (testing.md, section 6): step crates in
  `crates/`, shared test harnesses and fakes in `testing/`, worlds in
  `tests/`, the example services in `examples/`.

## 2. The parts

| Part | Crate | What | Design |
|---|---|---|---|
| lib | `skein-lib` | handles, slabs, bounded containers, cursors, streams, deadlines, time | [lib.md](lib.md) |
| the kernel boundary | `skein-io` (`kernel`) | the records io submits to a backend and the completions it gets back | [kernel.md](kernel.md) |
| io | `skein-io` | sockets, pipes, files, processes, signals to the service | [io.md](io.md) |
| the shell kit | `skein-shell` | the io_uring backend, the clock, the seed, startup | [shell.md](shell.md) |
| the simulator | `skein-sim` | the simulated kernel and its faults; beside it, the conformance suite (`skein-conformance`) | [simulator.md](simulator.md) |
| the fake LLM | `skein-fake-llm-domain`, `skein-fake-llm-protocol` | shared bounded scripts and independent byte peers; application worlds supply tools and policy | [fake-llm.md](fake-llm.md) |
| the key-value store | `skein-kv` | ordered byte keys and values in memory, made durable by a commit log and snapshots on io's files | [kv.md](kv.md) |
| the fake checkout | `skein-fake-checkout` | for tests: deterministic files, scripted commands and local git mechanics; worlds own policy and delivery | [fake-checkout.md](fake-checkout.md) |
| the counting allocator | `skein-heap` | for tests: the heap counted, and each step checked against its worst case | [testing.md](testing.md) |
| the world harness | `skein-world` | for tests: each process's `iterate` in one loop, over the simulator or the ring, with a referee | [examples.md](examples.md) |
| HTTP | `skein-http` | HTTP/1.1 client and server, server-sent events | [http.md](http.md) |
| JSON | `skein-json` | a bounded tokenizer, a sized writer | [json.md](json.md) |
| framed channels | `skein-channel` | a framed channel between two peers over a stream: an opening that agrees a version, bounds, flow control | [channel.md](channel.md) |
| codecs | `skein-codec`, `skein-codegen` | records described by schemas, and their codecs, worst cases and golden bytes generated from them | [codec.md](codec.md) |
| TLS | `skein-tls` | the TLS client, a stream over rustls's unbuffered connection, with ring | [tls.md](tls.md) |
| LLM | `skein-llm` | provider-neutral calls, streaming deltas, concrete replay and completions; subscription wire dialects | [llm.md](llm.md) |
| LLM connections | `skein-llm-connection` | a protocol layer's component that runs LLM calls end to end: connecting, TLS, deadlines, reuse | [llm-connection.md](llm-connection.md) |
| OAuth | `skein-oauth`, `skein-oauth-accounts`, `skein-fake-oauth` | bounded sign-in and refresh client; the driver that runs it end to end for its owner, listener, connection and keeping included; an independent rotating issuer for worlds | [oauth.md](oauth.md) |
| the OAuth issuer | `skein-oauth-issuer` | OpenID Connect's issuer, stateless: an authorization and a token request checked, an ID token signed, the discovery document, key set and userinfo answer | [oauth-issuer.md](oauth-issuer.md) |
| the forge's vocabulary and dialects | `skein-forge` | a code forge's calls and documents, deliveries and credentials, neutral and rendered by a dialect; Forgejo's built, GitHub's later | [forge.md](forge.md) |
| the forge component | `skein-forge-connection` | a protocol layer's component that runs a forge's calls end to end: connecting, TLS, credentials, deliveries | [forge-connection.md](forge-connection.md) |
| the forge client | `skein-forge-client` | a bounded view of a forge for its owner's domain, and each write made at most once | [forge-client.md](forge-client.md) |
| the forge setup | `skein-forge-setup` | makes a forge ready for a service: the identity, credentials, repositories and hooks it acts with | [forge-setup.md](forge-setup.md) |
| the fake forge | `skein-fake-forge-domain`, `skein-fake-forge-protocol`, `skein-throwaway-forgejo` | for tests: a forge's state and git, Forgejo's API over it, a forge peer on loopback, the pinned Forgejo run in a scratch, and the conformance that holds the dialect to it | [fake-forge.md](fake-forge.md) |
| the browser kit | `skein-browser` | for tests: a headless Chromium driven over the DevTools protocol on a pipe, by role and name, from the loop | [browser.md](browser.md) |
| the UI kit | `skein-ui` | a semantic tree of an application's state, by accessibility roles, built, diffed into patches, its acts bound to their owners; words, Markdown, scenes | [ui.md](ui.md) |
| the text shell | `skein-ui-text` | the tree printed as lines, a typed line read as an act, for a terminal or a pipe | [ui-text.md](ui-text.md) |
| the browser parts | `skein-ui-web`, `skein-ui-bundle` | parts an application's browser shell wires into its own loop, compiled to WebAssembly; the bundler | [ui-web.md](ui-web.md) |
| the fake person | `skein-fake-person`, `skein-fake-person-browser` | for tests: a person who finds nodes by role, name and text and acts on them, on the tree or in Chromium | [ui.md](ui.md), [ui-web.md](ui-web.md) |
| the lab | none: `scripts/lab/`, Python's standard library for now | for products' tests: experiments run as processes, driven by an agent, checked from outside, judged over seeds into verdicts and findings | [lab.md](lab.md) |
| the lab's browser | `skein-lab-browser`; `scripts/lab/look/` | for the lab: a headless Chromium on the browser kit answering calls on a closed network, and the look's capture of a page beside its mockup in three views | [lab-browser.md](lab-browser.md) |

## 3. The crate graph

```
crate          depends on
skein-lib      nothing
skein-io       lib
skein-http     lib
skein-json     lib
skein-channel  lib
skein-codec    lib
skein-codegen  nothing from skein (a generator, in consumers' tests)
skein-tls      lib, rustls (and ring beneath it)
skein-llm      lib, http, json
skein-llm-connection lib, io, tls, llm
skein-oauth    lib, json
skein-oauth-accounts lib, io, http, tls, oauth
skein-fake-oauth lib, json, oauth
skein-oauth-issuer lib, json, tls
skein-forge    lib, json
skein-forge-client lib, forge
skein-forge-connection lib, io, http, tls, json, forge
skein-forge-setup lib, io, http, json, forge
skein-fake-forge-domain lib
skein-fake-forge-protocol lib, http, json, forge, fake-forge-domain
skein-throwaway-forgejo lib, io, http, json, shell, forge
skein-browser  lib, json
skein-ui       lib
skein-ui-text  lib, io, ui
skein-ui-web   lib, ui; on wasm32, wasm-bindgen, js-sys, web-sys
skein-ui-bundle nothing from skein (a tool), wasm-bindgen-cli-support
skein-fake-person lib, ui
skein-fake-person-browser lib, io, ui-web, fake-person, browser
skein-shell    lib, io, io-uring, libc
skein-sim      lib, io
skein-kv       lib, io
```

The crates from outside skein are few, and each is an exception the
programming model names (its 2.1 and 3): `io-uring` and `libc`, the
shell's, for the ring adapter; and `rustls` and its dependencies, `ring`
among them for its cryptography, the TLS client's; and the browser
parts' glue, `wasm-bindgen`, `js-sys` and `web-sys`, with the bundler's
`wasm-bindgen-cli-support`, pinned to exact versions (ui-web.md, 4.1).

## 4. Not in skein

- **Domains, and protocols only one application speaks:** temper's
  protocol between worker and engine. They are built on skein's machines.
- **What a service does with an LLM:** its tools and their execution,
  when to sign in or refresh a credential, and whether to retry. The
  calls, their connections and OAuth's documents are skein's.
- **What a service does on a forge:** what it watches, its effects and
  their purposes, its policy, and what people see. A forge's calls, its
  component, client and setup, and the fakes of a forge are skein's.
- **A service's wiring:** its `iterate`, the sum of its worst cases, its
  `main`.
- **What a simulated program does.** The simulator plays the kernel; the
  files and programs a scenario needs come from the service's own fake
  machine, which plugs into the simulator.
- **Schedulers, async and threads.** There is one loop per process. A
  service that needs more cores runs more processes.

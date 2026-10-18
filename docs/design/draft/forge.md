# The forge kit

Draft, 2026-10-18. What skein would give a service that works on a code
forge, Forgejo now and GitHub later:

- a neutral vocabulary of forge calls, rendered by Forgejo's dialect;
- a component that runs those calls and receives the forge's webhooks;
- a client domain that keeps a bounded view of a forge and writes to it
  at most once;
- a setup machine that makes a forge ready for a service;
- the fake forge and the conformance tooling that a service's worlds and
  tests need.

All of it exists in temper today, written for temper but nearly all
neutral. This draft says:

- what moves to skein;
- what each crate becomes;
- what it leaves to the service;
- what GitHub will ask of it.

temper's side is temper's `docs/design/draft/forge-kit.md`.

## 1. In one page

- **What a connector stands on, not the connector.** skein gets the
  layers a forge connector is built on: the documents, the call plans,
  the component, the client's keep and write lanes, the setup's
  machines, and the fakes. It gets nothing of what a connector decides.
  The service keeps these (section 9):
  - what to watch;
  - which effects exist and what each is for;
  - when to write;
  - what people see.

  skein names no service, and jig is not involved.
- **Shaped like skein's LLM crates.** `skein-forge` is to forges what
  `skein-llm` is to providers: a neutral vocabulary rendered by dialects.
  Each call is answered once, and nothing is retried.
  `skein-forge-connection` is to it what `skein-llm-connection` is to
  `skein-llm`: a component that a protocol layer owns, and that connects,
  runs calls and keeps connections (llm.md; llm-connection.md).
- **One call, one answer, and its cost.** Each `Read` or `Write` is
  answered by exactly one `Answer` or `Error`, made of one or a few HTTP
  exchanges. The answer carries the count of requests the call cost.
  Budgeting the forge's rate is the client's job.
- **The service names its marks.** A creation's key travels in a marker
  whose word the service gives (`<!-- <word>:key … -->`). The user agent
  is the service's, and the setup marks its bot with the same word.
  temper gives `temper`, so whatever it has already written still reads.
- **Credentials have a kind, set by the dialect.** A token lasts until
  revoked (Forgejo's, or a GitHub fine-grained token). A GitHub App
  installation's token is renewed from the App's key every hour. The
  component holds the current credential, renews it when the dialect
  says, and gives it to no one.
- **Webhooks for latency, polling for liveness.** Deliveries are
  verified as they stream and become hints. The client's keep polls
  whether or not they arrive.
- **The client keeps and writes, and decides nothing.** Its working set,
  request budget and write lanes are bounded. Each write carries a key
  that its owner made, so a write whose answer was lost is found after a
  restart, not made twice. What an effect is for is opaque bytes of the
  owner's.
- **Setup brings a forge to one kind of result: an access.** On any
  forge that result is the same:
  - an identity;
  - its API and git credentials;
  - the repositories it may write;
  - a webhook secret.

  Each forge has its own plan to reach it. Forgejo's is reconcile, with
  a site admin's token. GitHub's is the App manifest flow, later. skein
  supplies only the machines; the host is the service's.
- **The fake forge is Forgejo-shaped and shared.** Its domain and
  protocol layer move with the dialect, and skein-fake-peers hosts them.
  The conformance runs against a throwaway Forgejo, and its facts and
  transcripts move with them.
- **GitHub is a second dialect.** The seams are designed for it now
  (section 8), and it is built later.

## 2. The crates

```
skein-forge                     the vocabulary (api); the dialects: Forgejo's documents, both sides,
│                                 its call plans, its deliveries, the setup's documents, the pin;
│                                 keys' markers; credentials' kinds
├── skein-lib, skein-json
skein-forge-client              the keep (working set), the request budget, the write lanes and their
│                                 recovery, keys
├── skein-lib, skein-forge (its vocabulary only)
skein-forge-connection          the component: calls, connections, each forge's start, repositories'
│                                 paths, users, credentials and their renewal; the webhooks' listener
├── skein-lib, skein-io, skein-http, skein-tls, skein-json, skein-forge
skein-forge-setup               the setup's result and each forge's plan: Forgejo's reconcile machine;
│                                 GitHub's manifest flow, later
├── skein-lib, skein-json, skein-forge
testing/skein-fake-forge-domain    the fake forge's domain
testing/skein-fake-forge-protocol  Forgejo's API over it on skein's HTTP server; the setup's routes;
                                     signed deliveries; the tally of requests served
testing/skein-throwaway-forgejo    a real Forgejo and its runner, at the pin, for the conformance
```

Where each comes from in temper:

| skein | temper's |
|---|---|
| `skein-forge` | `temper-forge-forgejo`; `api.rs` of `temper-engine-domain-forge-client`; the call plans and the marker of `temper-engine-protocol-forge` |
| `skein-forge-client` | the rest of `temper-engine-domain-forge-client` |
| `skein-forge-connection` | the rest of `temper-engine-protocol-forge` |
| `skein-forge-setup` | reconcile, its types and checks, of `temper-forge-setup` |
| `testing/skein-fake-forge-*` | `testing/temper-fake-forge-domain`, `testing/temper-fake-forge-protocol`, `tests/fake-forge` |
| `testing/skein-throwaway-forgejo`, the conformance | `testing/temper-throwaway-forgejo`, `tools/forgejo-conformance` |

- **What moves keeps its design.** The design moves with the code, under
  skein's names:
  - temper's `protocol/forge.md`, sections 3 to 5, 7 to 10 and 13;
  - reconcile in `protocol/setup.md`, sections 5 and 11;
  - the fake forge in `testing.md`, section 4.2;
  - the client in `domain/forge.md`, sections 5 and 6.

  Section 11 says where each goes.
- **What the move changes,** and nothing else:
  - the marker's word and the user agent become the owner's (3.3, 4);
  - an effect's identity becomes opaque bytes (5);
  - the setup reads no configuration, and is given values (6);
  - credentials gain kinds (3.5);
  - the call plans move from the component into the dialect (3.2).
- **The moved types keep their shapes.** temper's store keeps the
  client's rows, and each row decodes as before.

## 3. skein-forge

### 3.1 The vocabulary

The forge client's `api` module as temper has it:

- the calls: `Read`, `Write` and `Op`;
- their result: `Answer`, `Error` and `Kind`;
- `Hint`;
- the items: `Pull`, `Review`, `Comment`, `Status`, `JobAttempt`,
  `File` and the rest.

A repository is named by the forge's own id, which survives a rename or
a transfer. Adoption first locates a repository by its owner and name.
Every dialect answers every call of the vocabulary. A call that GitHub
cannot answer as Forgejo does is named when GitHub's dialect is designed
(section 8).

### 3.2 Dialects

- **Forgejo's, at its pin,** v17.0 and forgejo-runner v13.2, as temper's
  `protocol/forge.md`, section 3 has it. It holds both sides of each
  document:
  - each request, as a sized encoder;
  - each answer, as a decoder over JSON tokens that skips what it does
    not keep as it passes;
  - the error bodies;
  - the deliveries' headers and payloads;
  - the setup's documents.

  The client's side is for the component and the setup. The server's
  side is for the fake forge.
- **Call plans belong to the dialect.** A plan is a small machine for one
  call. It says its next request, reads each answer, and ends with the
  call's `Answer` or `Error` and the number of requests it made. The
  component runs any dialect's plans on its connections. The only
  dialect knowledge it keeps is in the plans it is handed, and in a
  repository's path, which a plan resolves.
- **The pin is a constant of the dialect** (`VERSION` and
  `RUNNER_VERSION`). The throwaway Forgejo, the conformance and a
  service's owned Forgejo read it there.

### 3.3 Keys' markers

- **A creation's key goes at the head of its body.** An issue's body, a
  comment and a review each carry `<!-- `, the owner's word, `:key `, the
  key in lowercase hexadecimal, then ` -->` and a line feed, then the
  text. The marker is an HTML comment, which forges' rendering hides.
- **The word is the owner's.** It is 1 to 16 bytes of lowercase ASCII
  letters, digits and `-`, checked at start. A body whose marker carries
  another word has no key.
- **The key is the owner's bytes.** This crate encodes and decodes the
  marker and never reads the key. Whether to believe a key is the
  client's call (section 5).
- **Bounded.** The marker is twice the key's length plus the word's
  length plus 15 bytes. With temper's word, that is twice the key plus
  21, as now.

### 3.4 Deliveries

- **Verified as the body streams.** The check is an HMAC-SHA256 of the
  body under the forge's webhook secret, compared with the dialect's
  header. A missing or wrong signature is `403`, and an oversize body
  `413`.
- **Each delivery becomes a hint.** The hint names what changed (a
  repository, an item, a branch, a commit or a job), as temper's
  `protocol/forge.md`, section 5 has it. An event the dialect does not
  hint is answered `204` and dropped.

### 3.5 Credentials

- **A credential has a kind, and each dialect names the kinds it
  takes:**
  - **a token,** which lasts until revoked: Forgejo's, or GitHub's
    fine-grained token;
  - **an App installation** (GitHub, later): the App's id, its private
    key and the installation's id. Its token is minted from a JSON Web
    Token signed with RS256, through `ring` as skein-tls has it, and
    expires within the hour.
- **The dialect says how a credential renders on a request.** For a
  renewed kind, it also gives the plan that renews the credential, and
  how long before expiry it is due.
- **A credential's value reaches no record, request line, fact or
  hint.**

## 4. skein-forge-connection

A component of a protocol layer, as `skein-llm-connection` is
(programming-model.md, sections 4 and 4.2). It has its own state,
limits, worst case, deadlines and maximum outputs per entry point. Its
owner's protocol layer routes io's events to it, reports its earliest
deadline, and closes it as part of its own end.

- **What its owner tells it at start:**
  - for each forge: its number, its dialect, its API's address and
    transport (TLS, or plaintext to loopback), its credential's value,
    the identity the credential must stand for, and its webhook secret
    when it has one;
  - the listener's address, when webhooks are configured;
  - the marker's word;
  - the user agent.

  It reads no file. Where each value comes from is the owner's concern.
- **Calls in, answers out.** A call names its forge, its repository and
  its `Op`. Its one terminal answer carries its cost and its result.
  Hints come from deliveries, and no request causes them.
- **Each forge's start** checks with the first exchange that the
  credential stands for the configured identity. A wrong identity, or
  another `4xx`, stops that forge's start and says so.
- **Repositories by id.** The component resolves an id to a path and
  keeps the path. It drops a path on `404` or a redirect, and resolves
  it again.
- **Credentials are renewed here,** for a renewed kind: the dialect's
  plan runs before the credential is due, on the component's own
  connections, and its requests are counted like any call's. A call that
  is waiting for a renewal waits within its own deadline.
- **Room, and the drain** (programming-model.md, sections 5.2 and 7):
  - An answer or a hint that the owner cannot take yet is held and
    offered again: at most one per call and one per webhook connection.
    Nothing more is read off that connection meanwhile.
  - One that the owner returns during its drain is never offered again.
    It goes when the component closes, and the next process reads
    afresh.
- **No retries, and no policy.** Whether to call again is the owner's
  domain's decision.
- **The listener** serves plaintext on loopback, as temper's does. A
  listener over TLS waits for skein's TLS server side.
- **Limits, deadlines, machines and worst case** are as temper's
  `protocol/forge.md`, sections 7 and 8 have them.

## 5. skein-forge-client

A domain crate, `no_std` and lib-only like its source, driven by `step`,
`fire` and `resume`. Its owner is a domain: a service's forge connector.
The client tells the owner what to store (its `Stored` rows). The owner
commits them before it releases the calls that the same decision made.

- **The keep: a bounded working set.** It watches repositories and live
  items, and keeps them fresh three ways:
  - by listings since a time, read first where hints point;
  - by polling every `poll`;
  - by a slow pass every `slow`.

  What each answer changes is a fact, which goes to the owner.
- **The request budget.** Calls queue by priority: `Fresh`, `Write`,
  `Keep` and `Slow`. Each is charged the cost its answer reports, within
  a window. A rate-limited answer makes the budget wait until the reset.
- **Write lanes, and recovery.** Each effect has a key, a recovery class
  and a lane. A write whose answer was lost, or which timed out, is
  looked for by its key before it is made again, so each effect is made
  at most once. Its clock survives a restart.
- **Keys.** `effect_key(namespace, identity, max)` gives:
  - version 1;
  - the namespace's length, as two bytes big-endian;
  - the namespace;
  - the identity.

  The namespace is the owner's. temper derives it from the deployment's
  id. The identity is the owner's bytes. temper's `EffectPurpose` (goals,
  tasks, attempts and projections) stays temper's, and the client takes
  its encoding. Whether a key is believed depends on three things: its
  version, its namespace, and whether its author is among the
  configured writers.
- **What it never knows:** what an effect is for, which effects exist,
  policy, grants, subscribers, and the owner's durable outbox.
- **Its words are a connector's.** Working set, effect, key and recovery
  class are what a connector that keeps an outbox needs. jig's
  connectors are its first owner (jig's `domain/connectors.md`, sections
  4 and 5), and nothing in the crate names them.

## 6. skein-forge-setup

### 6.1 The result: an access

The setup ends with what the component needs (section 4), and with
nothing more:

- the identity the service acts as, by the forge's id;
- its API credential and its git credential;
- its reach: the repositories it may write;
- the webhooks' secret, and hooks that deliver to the service's
  listener.

### 6.2 Three modes, named for any forge

- **Connected:** a person makes the access and gives it.
- **Managed:** the service makes the access, with an authority that the
  person gives: a Forgejo site admin's token, or the person's approval
  of a GitHub App.
- **Owned:** the service runs the forge, and then sets it up as managed.
  Only Forgejo can be owned, and the service does it (section 12).

### 6.3 Forgejo's plan: reconcile

It is the machine of temper's `protocol/setup.md`, section 5, with the
same steps in the same order. Each step reads first, and writes only
where the forge differs:

1. check the admin;
2. the bot, marked `<word>:bot`;
3. its two tokens;
4. its id;
5. each org's team with write access, and the bot in it;
6. each org's webhook.

- **Given values, not configuration:**
  - the API's address;
  - the admin's token;
  - the bot's login and email;
  - the tokens' names (temper's are `temper-api` and `temper-git`);
  - the orgs;
  - the listener's URL;
  - the word;
  - the webhooks' secret, or a request to draw one;
  - its limits: the orgs at most, the deadline and the first backoff.
- **It says each request,** and its host runs it and hands back the
  outcome. It ends with the access, or fails naming the step.
- **Its host is the service's:** the command, the files, the forge's
  lock, and the exit status.

### 6.4 GitHub's plan, later

- **Managed is the App manifest flow:**
  1. The machine drafts the manifest. It holds the App's name and URL,
     its webhook URL and events, and its permissions: contents, issues
     and pull requests to write; checks, statuses, actions and metadata
     to read.
  2. The person creates the App on GitHub. GitHub redirects their
     browser to the host's loopback listener with a code, like
     skein-oauth's redirect (oauth.md, section 6.5).
  3. The machine exchanges the code for the App's id, its private key
     and its webhook secret.
  4. The person installs the App on their orgs or repositories.
  5. The machine finds the installations, and the repositories each one
     reaches.
- **Reconcile then checks and reports.** It checks the App and its
  installations. No API makes an installation, so it repairs nothing,
  and it names what the person must do.
- **The access's credentials** are of the installation kind (3.5). The
  identity is the App's bot user, and git authenticates with the
  installation's token.
- **Connected** is a fine-grained token, or an App that the person makes
  and installs.

## 7. The fake forge and the conformance

- **The domain,** `testing/skein-fake-forge-domain`, follows temper's
  `testing.md`, section 4.2, with its own vocabulary. It is
  Forgejo-shaped, and no better than Forgejo:
  - its listings, its refusals and its updated times;
  - its timing, faults and skewed clock;
  - CI drawn or cued by content;
  - merges as squashes at an exact head;
  - a bounded queue of what it changed and refused, which a referee
    drains.
- **Its git** lives in the same store, with commits named by a count so
  that a seed replays to the same names. A working tree reaches it
  through a typed transport: the fake checkout's remote boundary
  (fake-checkout.md, section 4). A bridge for real git is designed
  separately, and lands here.
- **The protocol layer,** `testing/skein-fake-forge-protocol`, serves
  Forgejo's API over the domain on skein's HTTP server, from the
  server's side of `skein-forge`, as temper's `protocol/forge.md`,
  section 9 has it:
  - the routes, with Forgejo's quirks;
  - the setup's routes, from a store of its own;
  - signed deliveries;
  - the tally of requests by route and by user;
  - faults moved down to bytes.

  skein-fake-peers hosts it on loopback, beside the LLM and OAuth
  peers.
- **The conformance:**
  - `testing/skein-throwaway-forgejo` runs Forgejo and its runner at the
    pin;
  - a tool outside the suites, at `tests/forge/conformance`, records the
    dialect's facts as `holds`, `fails` or `unchecked`, with their
    transcripts, masked;
  - temper's `protocol/forge.md`, section 13 becomes the dialect's
    facts. These are what `skein-forge` relies on of Forgejo.
- **A service's own stories,** such as temper's forge connector world
  and its cost ceilings, stay the service's, and run on these fakes.

## 8. GitHub

What its dialect must give, and what the seams above already hold for
it. Its documents, plans, facts and fake come in a plan of their own.

- **The vocabulary may gain three things** (temper's `protocol/forge.md`,
  section 14):
  - validators for conditional requests, whose `304` GitHub does not
    count against the rate;
  - the rate's window, reported on every answer, which the budget takes
    in place of its configured one;
  - reads batched into one query.

  Each addition is optional, and Forgejo's dialect ignores it.
- **Credentials:** the installation kind (3.5); git over HTTPS with the
  installation's token.
- **Deliveries:**
  - They carry `X-Hub-Signature-256`, `X-GitHub-Event` and
    `X-GitHub-Delivery`.
  - An App's deliveries all arrive at one URL, and each names its
    installation.
  - GitHub delivers from the internet, so its listener needs skein's TLS
    server side, or a relay. Until then, a GitHub forge polls.
- **CI:** commit statuses and check runs, and Actions' jobs, whose logs
  come as a redirect to a download.
- **Setup:** 6.4. There is no owned mode.
- **The fake:** a GitHub protocol layer over the same domain, with its
  quirks set per dialect (section 12).

## 9. What stays with the service

- **The connector's top:**
  - what it watches;
  - which effects exist, and their purposes;
  - policy: gates, merges and landings;
  - adoption's checks;
  - what people see.
- **The root's translation,** and the store's records of the client's
  rows.
- **The configuration and its files;** the setup's commands and their
  host; owned mode and its box.
- **Its git component,** and that component's fakes.
- **Its worlds' stories,** and its cost ceilings.

## 10. Verification in skein

- **Step tests:**
  - the documents against the conformance's transcripts, against answers
    generated from the seed, and against mutations of both;
  - golden bytes for each encoder, bounds at the limit and one past,
    and a fuzz target per decoder;
  - each plan against canned answers;
  - the marker with several words;
  - the setup's machine against answers.
- **The component's machine world:**
  - against a scripted HTTP peer, with faults at every byte and every
    deadline;
  - a wrong identity stopping a start;
  - a renewal due while calls wait;
  - the listener against scripted deliveries.
- **The protocol world:** the client and the component against the fake
  forge's protocol layer and domain, joined by in-memory streams cut at
  random, and driven by a scripted owner.
  - Its stories:
    - a creation whose answer was lost is found after a restart, and
      not made twice;
    - listings keep up with another party's changes;
    - deliveries arrive late, out of order or never.
  - Its invariants:
    - each call is answered once;
    - the requests the fake served equal the costs the answers
      reported;
    - no credential reaches a record, a request line or a fact.
- **The setup's world:** Forgejo's reconcile against the fake's setup
  routes, converging from each partial state.
- **The conformance** runs outside the suites (section 7).

## 11. Promotion

When this draft becomes design, it is written into five skein design
documents, each from the temper sections that it replaces:

- **`forge.md`** (the vocabulary, the dialects, the marks, deliveries and
  credentials) comes from temper's `protocol/forge.md`, sections 1, 3 to
  5 and 13, with the vocabulary from the client's `api.rs`, which no
  document states yet.
- **`forge-connection.md`** comes from `protocol/forge.md`, sections 2,
  7, 8 and 10.
- **`forge-client.md`** comes from `domain/forge.md`, section 5, and the
  client's parts of its sections 6 (keys, write lanes and recovery) and
  16 (restart).
- **`forge-setup.md`** comes from `protocol/setup.md`, sections 3, 5 and
  11, and `13.1` to `13.3`.
- **`fake-forge.md`** comes from temper's `testing.md`, section 4.2, and
  `protocol/forge.md`, section 9.

temper's documents then cite these. temper's `draft/forge-kit.md`,
section 3 says what each of its documents keeps.

## 12. Open questions

- **Where the call plans live.** Default: in the dialect (3.2). The
  alternative leaves them in the component as one module per dialect,
  which is a cheaper move but makes the component grow with each forge.
- **The throwaway's serde and blocking HTTP.** No skein crate uses
  serde. Default: rewrite the throwaway on skein's HTTP client and JSON.
- **Owned Forgejo's preparation:** `app.ini`, migrations, the runner's
  registration and upgrades' dumps (temper's `protocol/setup.md`,
  section 6). Default: it stays the service's, since only temper runs a
  forge. It moves into skein-forge-setup's Forgejo plan when a second
  service runs one.
- **One fake domain for both dialects.** Default: yes, with each
  dialect's quirks set by its protocol layer.
- **The connector's top.** Default: it stays temper's until a second
  application that works on a forge is designed (temper's
  `draft/factory.md`, sections 11 and 12, the lab among them). Its home
  is decided then: a repository of shared connectors, building on skein
  and, if it must, on jig.
- **Several services on one forge.** Default: each has its own word, its
  own bot and its own keys' namespace.

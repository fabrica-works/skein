# The forge component

Provisional, 2026-10-18. `skein-forge-connection` runs a service's calls
to a forge end to end for the protocol layer that owns it. For each call
it:

- connects through io, over TLS or in plaintext to loopback;
- runs the dialect's plan (forge.md, section 4.2) and decodes each
  answer as it streams;
- counts the requests the call cost, and reuses idle connections.

It also checks each forge at start, holds the credential, resolves
repositories and users, and turns verified webhook deliveries into
hints. When its owner closes it, it closes everything it keeps and says
so. A service that works on a forge writes no connection code.

## 1. In one page

- **A component of a protocol layer:** a step machine with its own
  state, limits, worst case, deadlines and most outputs per entry point,
  which a service's protocol layer owns and routes to
  (programming-model.md, sections 4 and 4.2).
- **Calls in, answers out.** A call goes out once; its one terminal is
  an `Answer` or an `Error`, with the requests it cost.
- **Each forge starts alone:** the first exchange checks that the
  credential stands for the user the owner configured, and a wrong user,
  or another `4xx`, stops that forge's start (3.3).
- **Credentials have a kind** (forge.md, section 8): a token is never
  renewed, a renewed kind is renewed before it is due (section 5,
  later), and none leaves the component.
- **Webhooks for latency, polling for liveness:** a listener of its own
  verifies each delivery as it streams, answers it first, then offers
  its hint (section 6).
- **It reads no file,** and its memory is bounded: the owner hands it
  every value (3.1), and nothing grows with a forge's history (7.5).

## 2. In its owner's protocol layer

- **`skein-forge-connection`** depends on lib, io's vocabulary,
  skein-http (client, and server for the listener), skein-tls's client,
  skein-json and `skein-forge`. It knows no service's configuration,
  files or domain. One component serves every configured forge.
- **The owner's protocol layer:**
  - gives io its own tokens for the component's entities, keeping the
    component's token in each, so io's events reach the component that
    asked. The component's tokens are below 2^60, so 60 bits keep one:
    its API connections from 1, its listener 2^59, and its webhook
    connections above it;
  - reports the component's earliest deadline, and fires its deadlines
    where it fires its own;
  - routes what passes between the component, io and the owner's
    domain, and decides nothing;
  - closes the component as part of its own end, and counts it settled
    only on the component's `closed` (programming-model.md, section
    5.2).

### 2.1 What crosses

Down, from the owner: a **call** (token, forge, repository, `Op`);
**refused** and **refused hint**, an answer or a hint its domain did not
take; **returned**, an answer handed back in the domain's drain; an
**offer**, when the domain has room; and the **close**, its last act.

Up, to the owner: **answered**, a call's one terminal with its token,
cost and result; a **hint**, after its delivery's response is queued
(6.2); **listener lost** (6.4); **stopped**, a forge's start refused
(3.3), at most once per forge; and **closed**, the last event (8.5).

An entry point emits at most 16 events up and 16 requests to io, and the
owner leaves room for them before it.

### 2.2 Room, and the drain

(programming-model.md, sections 5.2 and 7.) An answer or a hint the
owner cannot take is held and offered at its next offer: at most one per
call, and one per webhook connection, which reads nothing more while its
hint is held. One the domain returns during its drain is never offered
again: the next process reads afresh.

### 2.3 The rules it keeps

API only, with `page` and `limit` always and within the forge's cap
(7.3). Paths and users are cached by their change token (section 4).
Webhooks are hints, answered first (section 6). The rate is honoured: a
`429` is `RateLimited { after }`, and nothing retries. A request is a
call's step or a start read, with no heartbeat. The rest are the
client's and the plans' (forge-client.md, sections 4 and 6; forge.md,
section 5.2).

## 3. Start

### 3.1 What the owner hands it

The values of forge.md, section 2.2, from no file:

- **for each forge:** its number, unique among the forges; its dialect
  (Forgejo's; GitHub's later); its API's socket address; its transport,
  TLS with a server name and what to trust, or plaintext to loopback
  with the name the forge was configured by, when it was one; its
  credential, a kind and a `Secret`; the forge user id the
  credential must stand for, never 0; and its webhook secret, a
  `Secret`, when it has a hook;
- **the listener's address,** when webhooks are used;
- **the word** (forge.md, section 6) and **the user agent** of every
  request: temper's are `temper` and `temper/0.0.0`;
- **the limits** (7.2) and **the shared values** (7.3).

The component makes each request's `Host` from the address and the
transport's name (forge.md, section 5.1); the owner hands no `Host`.

### 3.2 What start checks

A refusal names the value, or both limits where two disagree (7.4).
Nothing starts and no io is requested. It refuses: more forges than
`forges`, or two with one number; an in-process name for an address, and
plaintext off loopback; a plaintext name that is empty or holds a byte
at or below a space, at or above DEL, or one of `/:[]`; a credential
past `token_bytes`, and a user id of 0; a credential or a user agent
that is empty or holds a byte below a space other than tab, or DEL,
which skein-http would refuse in every request's head; a secret for no
configured forge, empty or past `hook_secret_bytes`; a listener that is
not a loopback IP socket; a word that is not 1 to 16 bytes of
lowercase ASCII letters, digits and `-`; and the limits 7.4 checks.

### 3.3 Each forge's start

Calls for a forge wait until it is ready. Its start runs two of the
dialect's plans (forge.md, section 4.1): the identity read, which says
who the credential stands for, and the page cap read, which gives the
forge's page cap. Forgejo's are `GET /api/v1/user`, for the `id`, and
`GET /api/v1/settings/api`, for `max_response_items`. Each plan runs on a
connection of the forge's own within `deadline`, and is counted in no
call's cost. The component decides from what a plan heard (8.3).

- **Stopped:** a user other than the configured one, or any `4xx` but
  `429`. A wrong credential would make the owner act as someone else,
  and another `4xx` says the address names no such API; asking again
  would mend neither. The component says `stopped`, naming the forge and
  the id it heard, or the read and the status.
- **Asked again** for anything else (8.3), after `start_backoff`, then
  at double the wait up to `start_backoff_max`, or a `429`'s
  `Retry-After` when longer, however long: the rate is honoured (2.3),
  the sum saturates, and the forge's calls meanwhile end at their
  deadlines (`Unavailable`, forge.md, section 5.4).

## 4. Calls, repositories and users

### 4.1 Calls

- **Admission is the owner's.** At most `calls` calls are outstanding;
  the component never refuses one, and `Busy` is the owner's word
  (forge.md, section 3.4). A call for a forge no one configured ends
  `Unavailable` at once.
- **A call runs its plan:** each step sends one request and hears its
  answer. It waits, holding no connection, for its forge's start; one
  whose repository has no path kept resolves it (4.2), and one that
  needs a login not held looks it up (4.3). A locate, which finds a
  repository by owner and name, names repository 0 (forge.md, 3).
- **Cost** is one for each request whose first byte was written, the
  requests that resolve a path or a login included. It goes up with the
  one terminal (forge.md, section 5.4).
- **The deadline** is `deadline` from admission, at most the owner's
  `lifetime`, so no byte of a write goes out past the time the owner
  counts on. A call still waiting at it ends `Unavailable`. An exchange
  that has written no byte fails `Unseen`, and one that has, `Seen`; the
  plan gives the `Error` (forge.md, section 5.4). The connection is
  aborted.

### 4.2 Repositories

The component keeps a row for each repository, by forge and id: its path
(owner and name), object format, default merge style and `html_url`. The
plan reads a row and fills it from the answers it hears (forge.md, 4.2).

- **Filled** by the read of the repository by id, counted in the call
  that needed it; by a locate; and by a read of its settings.
- **Dropped** when a call by its path ends on a `404` or is answered a
  redirect, or a merge is refused its style (forge.md, 5.4); the next
  call resolves the id again. A `404` that is an answer (forge.md, 5.2)
  keeps it.
- **At most `repositories` rows;** the one used longest ago goes first.

### 4.3 Users

A table of logins by user id, filled from every user object an answer
carries (a user, a status's creator when not `null`, the requested
reviewers, a collaborators' listing). An id it lacks is looked up by the
dialect's search, counted in the call (Forgejo's:
`GET /api/v1/users/search?uid=<id>&limit=1`); a user not found fails the
call `Missing`. At most `users` rows; the one used longest ago goes
first. A lost row costs one request, and a restarted component starts
with none.

## 5. Credentials and their renewal

- **A kind** says how a credential is made and how long it lasts
  (forge.md, section 8): a token lasts until revoked (Forgejo's, or a
  GitHub fine-grained token), and a GitHub App installation (later)
  expires within the hour.
- **One per forge,** a `Secret` of at most `token_bytes` in the
  component's own table. The dialect says how it renders on a request:
  Forgejo's is `Authorization: token <t>`.
- **It reaches no one:** no record, request line, fact, hint or event
  carries it, nothing logs it, and no entry point returns it.

### 5.1 Renewal, later

A token is never renewed: Forgejo's is held until the process ends, and
a changed token is the owner's restart. A renewed kind exists only in a
later dialect (forge.md, section 10), and has this rule:

- **the dialect says what renews:** the plan that makes the new
  credential, and how long before expiry it is due;
- **the component runs the plan before the credential is due,** on its
  own connections, its requests counted in no call's cost, as a start's
  reads are (section 11);
- **a call that needs the credential while a renewal runs waits,**
  within its own deadline; the new value replaces the old, which is not
  kept.

## 6. The webhooks' listener

Deliveries cut polling's latency and decide nothing (forge.md, section
7). Polling stands with no webhook (forge-client.md, section 4) and
finds whatever a delivery lost, refused or never sent told of.

### 6.1 The listener, and each forge's path

- **The listener** is a socket of the component's own, on loopback:
  plaintext HTTP on skein-http's server, at the owner's address. A forge
  on another host polls alone until skein-tls has a server side (11).
- **One path per forge,** which the dialect gives (Forgejo's is
  `POST /hooks/<number>`, forge.md, section 7), and which setup's hook
  names (forge-setup.md, section 4). A connection takes one request at a
  time and is kept alive.
- **A forge's secret** comes only with the listener. A forge with none
  has no path: it is answered as a path that names no forge.

### 6.2 Verified as it streams, answered first

The component computes the HMAC-SHA256 as the body arrives, with
`sha256::Hmac` keyed by the forge's secret, and decodes the body at the
same time into the few fields the hint reads. Nothing of it is believed
until the whole body is read and the signature holds, compared with
`sha256::equal`, in constant time (lib.md, 6.2). The header, the events
and the fields are the dialect's (forge.md, section 7).

Once the body is read and verified, the `204` is queued, and only then
does the hint go up, so the owner never makes the forge wait on its work
(http.md, 5.4). A verified delivery that hints nothing, one without the
dialect's event header (`X-Forgejo-Event`) among them, is answered
`204` too. A hint names the forge's number and the payload's repository id;
one for a repository the owner has not adopted goes up all the same.

### 6.3 What it answers

The head is checked in this order. A refusal is answered, its body
discarded, and the connection closed.

| The request | Answer |
|---|---|
| a head the server itself refuses | skein-http's own answer: a `4xx`, or `501` or `505` |
| a path that names no forge, or a forge with no secret | `404` |
| a method other than `POST` on a forge's path | `405` |
| no signature, or one that is not 64 lowercase hexadecimal digits | `403`, body unread |

A body is read within `hook_body_bytes`. A body too long (`413`), a
wrong signature (`403`) and a verified body the decoder finds malformed
(`400`) are answered as forge.md, section 7, says; each closes the
connection and hints nothing. A `204` alone keeps it alive. A request
not read within `hook_deadline` closes with no answer.

### 6.4 Bounds, and a listener that fails

Connections (`hooks`), the head (`hook_head_bytes`), the body
(`hook_body_bytes`) and a request's time (`hook_deadline`) are capped; a
full listener accepts no more until one closes. A push payload's commit
list is skipped by the decoder, not held. A listener that fails while
the component runs is said once, as `listener lost`, and it and its
connections close. What the owner does is its own.

## 7. Entities, limits and the worst case

### 7.1 Entities

Each is in a slab of the component: a **call**, a **forge's start**, a
**forge connection** (bound to io's socket and one forge, with its
stack: TLS, HTTP client, JSON tokenizer, the answer's decoder), the
**tables** (repositories, users, page caps, credentials, secrets), the
**listener**, and a **webhook connection** (its stack: HTTP server,
tokenizer, the delivery's decoder; the HMAC state, the fields decoded
and the hint it holds until the owner has room).

### 7.2 Limits

The component's own; their values are the owner's.

- **Forges and tables:** `forges`; `token_bytes`, the largest
  credential; `repositories` and `users`, the two tables' rows.
- **Connections:** `connections`, equal to the owner's `calls`, so no
  call waits for one and the component never says `Busy`; an idle
  connection of one forge closes for a call to another when none is
  free. `idle` is how long one is kept: below the keep-alive time of
  whatever serves the forge's API by more than a request's round trip.
- **What is read:** `head_bytes`, the largest response head, and
  `headers`, the most fields of a head, a delivery's too; `body_bytes_in`,
  the body read, streamed, so it caps time and bytes, not memory;
  `depth`, the JSON nesting followed (Forgejo's documents need about 8);
  `chunk`, the most text of a string demanded at once, and the most one
  read of a delivery takes.
- **What is kept,** the dialect's (forge.md, section 4.1):
  `string_bytes`, `body_bytes`, `inner` and `target_bytes`.
- **Time:** `deadline`, a call's and each start read's, from when it is
  asked; `start_backoff` and `start_backoff_max`.
- **Webhooks:** `hooks`; `hook_head_bytes` and `hook_body_bytes`, the
  largest delivery's head and body; `hook_deadline`, a request's read
  and answer, from its connection's accept or its last answer;
  `hook_secret_bytes`, the largest secret, at least 32.
- `tls`, and what HTTP and JSON take as their own (tls.md, http.md,
  json.md).

### 7.3 Shared values

The owner hands the component, as plain numbers, what it shares with its
domain: `calls`, `rows` (the most rows of a listing it keeps),
`answer_bytes`, `op_bytes` (an operation's text and key together),
`lifetime` (the longest a sent write may still arrive), `name_bytes`
(the longest name its domain keeps), `repositories` (the most it
watches), and io's caps. A plan is handed only what it reads: a
listing's page size, `body_bytes`, `rows` and `answer_bytes`. A page
size is the least of `rows`, the forge's page cap, and `answer_bytes`
over the listing's largest row, measured from `string_bytes`,
`body_bytes` and `inner`.

### 7.4 Checked at start

Each refusal names both limits:

- every limit is above 0, and the worst case fits a `u64`;
- `connections` equals `calls`, and `deadline` is at most `lifetime`;
- `body_bytes` is at least 2 * `op_bytes` + the word's length + 15: a
  marker writes the key twice over, in hexadecimal (forge.md, 6);
- `string_bytes` is at least `name_bytes`, and `repositories` at most
  the owner's;
- each listing holds at least one of its largest rows in `answer_bytes`,
  by the measure of forge.md, section 3.7, so no page size is 0;
- the dialect's own `check` of its limits (forge.md, section 4.1);
- for each forge, the request head fits skein-http's `request`, the send
  cap of the stream below: `target_bytes` of target, its `Host`, its
  credential, the user agent (the owner's, of any length) and 192 bytes
  of framing;
- `hook_secret_bytes` is at least 32;
- each stacked machine's largest demand fits the cap of the stream
  below it: HTTP over TLS and io, TLS over io, JSON over HTTP, and the
  listener's server over io and JSON over it.

### 7.5 The worst case

It is the sum of:

- for each of `connections`: its stack, its intake and output caps, its
  decoder and the answer as it is built (twice `answer_bytes`, for a
  body kept raw while it grows), TLS priced on every connection, since
  the check knows the forges' number, not their transports;
- for each of `calls`: its record, plan and copy of the operation, and
  its held answer, at most `answer_bytes`;
- the two tables, full, and each forge's credential and secret;
- for each of `hooks`: its stack, HMAC state, decoded fields and held
  hint; and the listener.

A long thread or a large payload costs time, not memory. The owner
counts its io's worst case once.

## 8. State machines

### 8.1 A forge connection

| State | Holds, and demands | On |
|---|---|---|
| Closed | nothing | a call or a start read: Connecting |
| Connecting | the holder | connected: Handshaking, or Sending without TLS; failed: `Unseen`, Closed |
| Handshaking | the holder; TLS's demand | done: Sending; failed: `Unseen`, Closed |
| Sending | the holder, its request; room | sent: Reading; a fault: `Unseen` if skein-http says `Closed`, else `Seen`; Closed |
| Reading | the holder, the decoder; the head, then the body as demanded | answered: the next step's Sending, or Idle with the answer up; a fault: `Seen`, Closed; a malformed answer is heard as such (forge.md, 4.2), Closed |
| Idle | nothing; none | a call to its forge: Sending; `idle`, the forge closing, or a call to another forge with none free: Closed |

A connection the forge marked `Connection: close`, or that read a raw
body past its bound, goes to Closed after its answer, not to Idle.

The holder is a call, or one of a forge's start reads (3.3), whose
failures go to its start. Deadlines are the holder's: every phase but
Closed and Idle waits until it, so no exchange outlives what it serves.
At it the exchange fails `Unseen` if no byte of it was written, else
`Seen`, and the connection is aborted. Which deadline runs is a function
of the phase, set in one place after each transition
(programming-model.md, section 5.4). A closing connection is bounded by
io's close deadline.

### 8.2 A call

Waiting for its forge's start; Resolving a path or a login (4.2, 4.3);
running its plan's steps on its connection; then answered, once.

### 8.3 A forge's start

| State | On |
|---|---|
| Checking | the identity read: the configured user: Settings; another user, `401`, `403`, any other `4xx` but `429`: Stopped; unreachable, `429`, `5xx`, malformed: Waiting |
| Settings | the page-cap read: Ready; a `4xx` but `429`: Stopped; unreachable, `429`, `5xx`, malformed, a cap of 0: Waiting |
| Waiting | the backoff: Checking |
| Ready | serves calls |
| Stopped | says `stopped` once, naming the forge |

A renewed kind's credential (later) is Current, then Renewing while the
dialect's plan runs, then Current again. A token has only Current.

### 8.4 A webhook connection

The answers are 6.3's.

| State | Holds | Deadline | On |
|---|---|---|---|
| Head | the head so far | `hook_deadline` | a signed `POST` to a forge's path within `hook_body_bytes`: Body; any other head: its answer, Closing; past the deadline: Closing, no answer |
| Body | the HMAC state, the fields decoded | `hook_deadline` | read and verified: Answering; a wrong signature or a body that does not decode: its answer, Closing |
| Answering | the hint, or none | `hook_deadline` | `204` queued: the hint offered up, held while the owner has no room (2.2); then Head (kept alive) or Closing |
| Closing | nothing | io's close deadline | closed |

A fault or the deadline in any state closes the connection with no
answer and no hint.

### 8.5 Ending

- **The close is the owner's last act,** after its domain's drain
  (programming-model.md, section 5.2). By then the domain takes no
  answers.
- **It ends what is open.** Each call still open ends with its
  connection and has no answer: no terminal is sent for it. It drops
  every answer and hint it holds, and every answer the owner returned,
  and offers none again. The next process reads afresh (forge-client.md,
  section 8).
- **It closes what it keeps:** its connections, the listener and each
  webhook connection. **`closed`** goes up once, when none is left and
  io has settled every socket. It is the last event.

## 9. What its owner owes it

The owner's protocol layer owes the routing of section 2, **the values**
of 3.1 and 7.3 with limits that pass 7.4, **an end on `stopped`**,
**a decision on `listener lost`** and **the close after its domain's
drain** (8.5). The owner's domain owes:

- **at most `calls` calls outstanding,** one live call per token;
- **a locate,** `Read::Locate { owner, name }`, answered
  `Answer::Located { repository }`, addressed with repository 0, its
  answer checked as every other's, a located id never 0;
- **costs,** as the boundary carries them: an answer's `cost` is the
  requests the call wrote, cache fills included, 0 for none, which the
  owner's budget charges (forge-client.md, section 6).

## 10. Verification

- **Step tests:** the start's decision for every state and everything
  heard; the tables' fills, drops and eviction; each limit check, one
  below and at its bound; each listing's page size; the signature check
  against known HMACs, with a header of 63, 64 and 65 digits and one of 64
  uppercase; `Host` for a TLS forge and a named plaintext one, and for an
  IPv4 and an IPv6 address, at the scheme's default port and off it. The
  plans' own are `skein-forge`'s (forge.md, section 11).
- **The machine world:** the component against a scripted HTTP peer over
  in-memory streams, for a plaintext and a TLS forge: every state of
  section 8, with faults at every byte and each deadline passing; a
  start stopped, and asked again with its backoff doubling; a path
  dropped and resolved again; a body past `body_bytes`; an idle
  connection closed for a call to another forge; a held answer offered
  again, and a returned one never; the owner's close in every state,
  crossed with a terminal racing it, `closed` once and last, with no
  spin; a renewal due while calls wait, once a renewed kind exists.
- **The listener,** against scripted deliveries: a good delivery hinted,
  its `204` queued before its hint goes up; each answer of 6.3; an event
  the dialect does not hint; a hint held while the owner has no room;
  `hooks` connections full; faults at every byte.
- **The protocol world:** the client and the component against the fake
  forge's protocol layer and domain (fake-forge.md, section 4), joined
  by in-memory streams cut at random and driven by a scripted owner
  (forge-client.md, section 10). Its stories: a creation whose answer
  was lost is found after a restart and not made twice; listings keep up
  with another party's changes; deliveries arrive signed, late, out of
  order or never. Its invariants: each call is answered once, however
  its streams are cut; the requests the fake served equal the costs the
  answers reported, the starts' reads aside; no credential reaches a
  record, a request line or a fact.
- **The worst case,** against the counting allocator (skein-heap): every
  limit at its maximum, all `calls` outstanding, the tables full and
  every webhook connection holding a hint.
- **The conformance** drives the component against a real Forgejo
  (fake-forge.md, section 7).

## 11. Open questions

- **A renewal's cost and failure** (later). A renewal is no call and has
  no answer to carry its cost, so the owner's budget does not see its
  requests. Default: they are counted in no call's cost, as a start's
  reads are, and a failed renewal is asked again as a start's read is,
  calls waiting within their deadlines; an expired credential answers
  `401`, which a call reports as `Forbidden`. The dialect that renews
  (forge.md, section 10) says whether the budget must see them, and adds
  the limits a renewal needs (the key material's bytes, the lead, its
  deadline).
- **A listener over TLS,** for a forge on another host, waits for
  skein-tls's server side (tls.md). Default: that forge polls alone
  until then.

# Shared scripted LLM peer

Provisional, 2026-10-05, revised 2026-10-19. The scripted peer that worlds
and an application's real loop call in place of an LLM provider.

## 1. Role

Skein owns reusable provider-neutral LLM calls and their independent wire
peers. `skein-fake-llm-domain` depends only on `skein-lib`; it knows no HTTP,
JSON, provider client, application tools, checkout paths or credential policy.
`skein-fake-llm-protocol` depends on that domain and the shared HTTP/SSE/JSON
and LLM codecs; provider codecs are `skein-llm`'s (llm.md). Application
worlds supply their own tool schemas, inputs, invalid calls, scripts and
effect expectations, and interpret the answers themselves.

## 2. Script domain

`Config` supplies fixed pending-call and owned-byte limits, seeded failure
chances, injected latency and output limits, and the cache, usage and
tool-choice settings of section 2.1. `Domain::configured` consumes
scripts and `Menu` together under `script_bytes`. The menu supplies complete
argument bodies and deliberately invalid names/bodies. An empty menu produces
no random tool calls. No application body or unknown tool name is invented.
`Line::Opaque` carries complete caller-supplied provider replay bytes; the
domain counts and preserves them without parsing.

`Event::Call` transfers a query and one reply right. A request is refused or
produces one delayed `Request::Reply`; `fire` uses only injected time.
Conversation validation requires every result ID to answer exactly one prior
call ID; tool calls belong to assistant messages and results to user messages.
Generated provider-call names use checked counters; exhaustion
refuses rather than reusing an identity. Query, script/menu and generated
answer ownership is checked before effects or payload copies.

### 2.1 The cache, usage and tool choice

- **A query says how it may be cached,** in the domain's own terms, which
  the byte peer fills from the wire (section 3.1): automatically, by
  prefix, within a scope, the opaque key a provider routes by, or none;
  or only up to marks, the positions the request asked to cache.
- **The domain keeps what calls wrote:** a bounded table of prefixes, each
  with its scope, its length in tokens and a digest of its content,
  written at injected time and expired after the configured lifetime. The
  oldest entry gives way when the table is full.
- **A query's prefix** is its model, its tools and its system text,
  then, for a marked query alone, its tool choice, then its messages. A
  changed choice therefore keeps an automatic query's whole prefix, as
  the Responses wire does, and only a marked query's tools and system text,
  as Anthropic's does.
- **Reads.** An automatic query reads the longest prefix written in its
  scope that it begins with. A query without a scope reads only with the
  configured chance, as a provider's random routing does, so a world can
  tell an affinity that reaches the wire from one that does not. A marked
  query reads the longest prefix, ending at one of its marks, that an
  earlier call wrote, and never past its last mark.
- **Writes.** An automatic query writes its whole prompt. A marked query
  writes the prefix at each of its marks; what it writes past what it
  read is its cache write.
- **Usage is optional, field by field.** The domain's usage has the same
  five fields as skein-llm's `Usage` (llm.md, section 2.3), each present
  or not. The
  configuration says which the wire reports, per dialect; the peer omits
  the rest, so worlds meet usage not reported as well as usage of zero.
- **Tool choice.** A query carries its choice: with `None` the domain's
  random answer calls no tool, and with `Only` it calls only the tools
  named. With the configured chance it calls outside the choice anyway,
  as a provider may, so a world sees its caller answer such a call as not
  run. Scripted turns are the application's, and call what they name.
- **Cut and oversized calls** need nothing new: an answer longer than the
  query's output cap is cut short inside its first tool call, which the
  peer sends as its dialect's own truncation, and a menu body longer than
  the client's input limit is an oversized call.

### 2.2 A reply in pieces, and a hold

- **A scripted reply can come in pieces.** The script gives the reply's
  text as an ordered list of pieces, and the peer sends each as a delta
  of its own, in order, in the dialect's own streaming events, so the
  client under test sees the reply arrive by parts, as it does from a
  provider. A reply scripted whole is one piece. The pieces' bytes are
  the reply: concatenated, they are the text the committed answer holds.
- **A piece the script marks held is not sent until its owner releases
  it.** Pieces before it are sent at their latency; the held piece and
  those after it wait, and the stream stays open, with the dialect's
  keep-alive if it has one. A world therefore observes a reply partway:
  what reached the client is exactly the pieces before the hold.
- **The owner releases by name.** The script gives a hold a name, and
  the world that owns the domain releases it by that name, at any time,
  in its own iteration (the domain stays pure: a release is an event,
  not a timer or a callback). Releasing sends the held piece and the
  pieces after it up to the next hold, each at its latency. A release
  of a name nothing holds changes nothing. A reply cut or refused by
  a fault while held ends as the fault says; the hold is dropped with
  the reply.
- **Holds and the peers.** The domain keeps the holds, so the byte peer
  and the worlds that host it (the one-process world and the real loop:
  testing-strategy.md, 4 and 4.1) take the release as the domain's
  event, never from the wire. The peer as a process of its own has no
  owner to release a hold, so it refuses a script with one at its
  start. A world without a hold sees the peer as before.

### 2.3 Scripted failures

- **A turn may fail.** A failing line, `Line::Fail`, answers with a
  provider failure: its class, as the domain's error; the provider's
  error code; its request id; and the field it names, for `Unsupported`.
  The domain's error has `Refused` and `Unsupported`, and `Exhausted`'s
  reset is optional, as skein-llm's failure has them (llm.md, section
  2.5). Exhausted with a reset or without, refused, unsupported and
  unauthorized are its point; any class with a row on its route may be
  scripted (below).
- **Where it falls.** A failing line that names no credential ends its
  turn: a line after it is refused when the script is configured. One
  that names credentials (below) ends the turn for their calls alone,
  and may be followed by lines, which the calls that pass over it
  answer. Reached before any other line of its call's reply, a failing
  line answers before any stream, with its route's status and error
  body; after other lines, it ends the stream as its route fails one
  (`response.failed` on Responses, an `error` event on Anthropic), once
  their pieces are sent, a hold's included when released (2.2).
- **As the route says it.** The byte peer renders the class as its
  route's section names it (llm.md, sections 4.6 to 4.8): the status, a
  code the section maps to that class unless the script gives one, the
  reset as the next bullet says, and the field as `error.param`. Where
  the section reads the class from the error's message, as Anthropic's
  context too long (llm.md, section 4.7), the peer sends that message.
  So the client's own classification reads back the class the script
  gave.
- **A reset, per route.** On the public and the withdrawn Codex routes,
  `Exhausted`'s reset goes in the error's `resets_in_seconds`, in whole
  seconds rounded up, never in `retry-after`, before a stream as in
  `response.failed`. On Anthropic only the withdrawn bearer route
  carries one: the reply's head gives
  `anthropic-ratelimit-unified-status: rejected` and
  `anthropic-ratelimit-unified-reset`, the reply's wall time in whole
  seconds since the epoch plus the reset rounded up to whole seconds
  (llm.md, section 4.7). Such a line must answer before any stream and
  fail bearers alone: on Anthropic it is refused at configuration where
  a line of its turn that is not a failing line comes before it, or
  where it names no credential or a key's index, and `Peer::accept`
  refuses a key at an index such a line names.
- **A class needs its row.** A class has a row on a route where the
  route's section names an error for it, not a status alone; a failing
  line of a class without one on its peer's route is refused at
  configuration. `Unsupported` has none on Anthropic, whose refused
  fields are `Invalid`. `Overloaded` has one on every route: on
  Responses, a 503 whose error is `overloaded_error` (llm.md, section
  4.8).
- **Whose calls it fails.** A failing line may name the accepted
  credentials it answers, by index (section 3), which the byte peer puts
  in each query; it fails only their calls, and a call with another
  credential, or with none, passes over it to the rest of its turn.
  Naming none, it fails every call. A failure adds no assistant message,
  so the same turn answers the call made again: a story fails one
  account's calls, lets another's through, and so drives each account's
  state deterministically.
- **The evidence is the script's.** The code, the request id and the
  field are sent as given, past the client's bounds too, so that a story
  sees the client keep each or leave it empty (llm.md, section 2.5). An
  empty request id is the peer's own (section 3.1).
- **The chances stay,** for soak runs: a seeded failure carries its
  route's code for its class and the peer's own request id, and names no
  field.

## 3. Byte peer

`Service` owns bounded routing across connections; `Server` owns one HTTP
exchange and one SSE writer. Start/up/down/resume entrances reserve their
published output maxima. Lower room and reads are explicit. A complete
provider request becomes the neutral domain query, and an actual domain
terminal becomes a bounded native stream.
`Service::target` consumes a whole domain terminal and returns it alongside
the actual connection token; an unmatched/closed route returns the same
owned terminal. Looking up a route never discards its reply right.

**Credentials.** The peer accepts several credentials, each a
`skein_llm::Credential` with its scheme (llm.md, section 2.6), so that one
peer serves several accounts. `Peer::new` takes one, the index 0, and
`Peer::accept` adds another and returns its index. Every credential is
accepted before the peer serves its first request, and none after. Each
entry receives the accepted credentials, borrowed. A request is
authorized when it presents one of them as its route sends it: on
Responses, either scheme as `Authorization: Bearer`; on Anthropic, a key
as `x-api-key` and a bearer as `Authorization` (the withdrawn route,
llm.md, section 4.7), never both. A peer whose wire configuration takes
calls with no credential, as a local model's server does, also
authorizes a request that presents no credential field (llm.md, section
2.6); that configuration is the public route's or Anthropic's, and
refused on a peer of the Codex route. Anything else, a credential it
does not hold or no credential without that configuration, is answered
as unauthorized.
The fake compares bytes; it owns no sign-in, refresh, JWT parser,
credential generation or durable credential record. The caller remains
the credential owner. The peer's provider selection is wire
configuration, never application policy.

**On OpenAI's dialect the route follows the credential.** A credential
with an account id is the withdrawn Codex route (llm.md, section 4.6),
whose `chatgpt-account-id` must match it; one without is the public route
(llm.md, section 4.8). A peer's OpenAI credentials are all of one route, and its
configured path is that route's; a mix is refused at configuration.

**No observation holds a credential's value.** The head observation
leaves out the credential's fields (`Authorization`, `x-api-key`,
`chatgpt-account-id`). `Observation::Credential` records, for each
request, its route (`skein_llm::Provider`) and what it presented,
each distinct: an accepted credential, by its scheme and its index; a
credential that matched none, by the scheme its field shows (a key in
`x-api-key`, else a bearer) and no index; or no credential field at
all, as the scheme none and no index.

`Close` requests lower settlement; `closed` acknowledges actual settlement.
A lost connection cannot authorize effects or turn a withdrawal into a
provider answer. The independent script domain still produces its actual
terminal; old connection routing may no longer deliver it.

**On io,** the byte peer also runs as a process of its own. It listens on
loopback, in plaintext, or in TLS with skein's test certificates, so that
a simulated world and the real loop host it as they host a service
(testing-strategy.md, 4 and 4.1). The fake issuer runs the same way
(oauth.md, 5).

### 3.1 Request heads, affinity and echoes

- **The peer records each request's head:** its fields, names and values,
  within the observation caps, beside the decoded query, so a world can
  assert what reached the wire: that every request of a conversation
  carries the same affinity, and that two conversations' threads differ.
- **It reads the dialect's cache instructions into the query.** The
  public route: `prompt_cache_key` must be the key in its rendered form,
  and a `session-id` or `thread-id` header is refused. The withdrawn Codex
  route: `prompt_cache_key` and the `session-id` header must be the same
  key, and `thread-id` must be well formed. On both, the key becomes the
  query's scope. Anthropic: each `cache_control` marker becomes a
  mark; there may be at most four, never on thinking or redacted
  thinking. A request that breaks a rule the dialect promises is refused
  as invalid: the fake checks its client (testing-strategy.md, section 6).
- **It refuses what a plan's route refuses.** On the public route, a
  request carrying a field of llm.md, section 4.8's refused list is
  answered as unsupported, naming that field.
- **It answers with a request id,** in each response's head under its
  route's name (`x-request-id`, `x-oai-request-id` or `request-id`):
  numbered as its response ids are, unless a failing line gives one
  (section 2.3).
- **It echoes as a provider does.** Its Responses events can carry, on
  either route, the request's instructions and tools and a usage
  attribution entry per input item, sized by configuration, so worlds
  exercise a client that keeps only what it reads (llm.md, section 4.4).

## 4. Shared codec and replay

`skein-llm::DocumentLimits` is the neutral document-bound vocabulary. Native
Anthropic peer decode/encode entrances live in the shared codec, not an
application's adapter. Tool schemas remain whole bounded JSON values.
Signed/redacted thinking preserves unknown extension fields. Required known
fields and complete JSON shape/bounds are checked; thinking/signature deltas
replace only their own values in the retained provider head.
The native request decoders read the dialects' cache instructions and
tool choice into the neutral query (section 3.1), and the usage encoders
write only the fields the configuration reports (section 2.1).
Unknown nonempty assistant-native block kinds keep their complete object,
including nested proof fields, without inferred effects or deltas. Known
text/tool/thinking kinds cannot bypass their admission rules by masquerading
as the opaque variant. Duplicate queried known fields are refused; unknown
extension keys remain uninterpreted, including duplicates.

`Replay::to_bytes` and `Replay::from_bytes` provide a versioned opaque durable
envelope. Its seven-byte version/tag/length header is additional to the raw
`opaque_bytes` allowance. `replay_bytes` exports the complete receiver cap;
`replay_worst_case` includes temporary JSON and simultaneous raw/envelope
storage. A complete raw replay admitted at its cap still fits its envelope.
Provider compatibility is validated by the real Client when replay is used.
`Prompt::output_ceiling` applies configured dialect support: Codex omits its
unsupported wire field, the public route refuses an explicit cap at
admission (llm.md, section 4.8), and the caller still bounds local output.

## 5. Outside stories and bounds

`skein-llm-world::World::prepared` adopts one freshly prepared Client and
caller-owned literal HTTP/SSE response bytes. The caller supplies the same
limits used at preparation. Adoption has no stream entrance, send, read or
terminal effect and constructs no second Client. `World::new` prepares once
and delegates to this identical initialization. Demand and intake,
fragmentation, queue limits, terminal checks and lower settlement are shared;
the constructor neither parses fixtures nor controls application policy.
The original Client keeps its callback owner and prepared request bytes.
Its price is counted once; the world's source tape, request tape, terminal
observations, queues and intakes remain separate owned storage. Its stories
expect literal bytes, never the peer's response encoder's output, and
establish no live provider admission.

`skein-llm-world::fake::Exchange` connects the actual shared Client to the
independent byte peer and real script domain. It records requests and
their heads, queries, response bytes and actual terminals. Its transport uses bounded intakes and
checks every read/room/send; it owns no provider parser. Its stories
carry whole schemas, literal argument bytes, exact paired feedback and
continuation through both wire configurations, then corrupt provider IDs.
Cancellation has no terminal before actual lower settlement; repeated close
settlement produces no second terminal.

`Exchange::prepared` adopts an application's already prepared actual Client.
The caller moves the exact endpoint, credential, if any, and unchanged
receiving limits used at admission as independent peer metadata, released
after configuring the peer. It creates no second Client and performs no
second preparation; `Exchange::new` prepares once and delegates to the
same constructor. The Client is priced once. Peer credential bytes and
the retained target are separate ownership, as are the caller's retained
application declarations, decoded results and observation buffers.

`Exchange::at(now, wall)` installs the caller's one iteration snapshot in
the actual Client, independent script domain and byte peer before their
entries run. The caller supplies nondecreasing monotonic `Time`; `Wall` may
jump independently and never arms deadlines. Installing time neither starts
the exchange nor fires timers, delivers bytes or settles effects; the caller
still drives the existing entrances. It adds no retained state or memory
allowance.

`Exchange::observe(ObservationLimits)` opts a fresh, unstarted exchange into
fixed observation counts and whole-record byte caps. Events, decoded queries,
request heads, manual pending calls and both wire tapes reserve their exact
wrapper capacity.
Payload ownership is checked before cloning or appending, including public
native replay token wrappers and their owning bytes. Drained records become
caller ownership; a moved vector's replacement capacity is reserved at the next
physical entrance. Without `observe`, observation is unconstrained.

`extra_worst_case` prices the peer, script domain, routing service, queues,
intakes, delivery scratch, credential/target and configured observations. It
excludes the one actual Client: callers add the Client or adapter price once,
plus their own retained inputs, metadata clones and drained records. Checked
arithmetic refuses impossible products without allocating the proposed cap.
The driver reclaims retired service routes after draining their actual outputs;
`Service::calls()` reports allocated live and retired slots without content.

Memory stories compare the counting allocator's high water with production
`worst_case` values plus the separately priced world intakes, wire tapes and
observation copies: replay transit, extended opaque heads, a maximum-size
scripted response, the exact joint script/menu cap with every delayed-call
slot at its full answer cap (one over each cap is refused), and two
connections sharing one delayed script domain. They establish shared
Client/peer ownership only: application worlds add their schema, result and
replay copies and their outside effect ledger to this envelope.

**The accounts' stories,** through `Exchange`, which accepts further
credentials for its peer as `Peer::accept` does: a peer holding
credentials of both schemes on one endpoint answers each as its own, and
observes which by index; each scripted failure reaches the client's
terminal with its class and report, and a code, request id or field past
the report's bound arrives empty; a failing line naming one credential
fails its calls and lets another's through to the lines after it; a
scripted reset reaches the client's `Exhausted` in whole seconds, on
each route that carries one; a line after a failing line naming no
credential, a class without its row, and an Anthropic reset on a key or
after the stream has begun are each refused at configuration; a call
with no credential is observed as the scheme none, and answered by a
peer configured to take such calls and as unauthorized by another; a
credential the peer does not hold is observed apart from both, by its
scheme with no index, and answered as unauthorized; a public-route
request with a refused field is answered as unsupported, naming it; and
no observation holds a credential's value.

The redacted Anthropic captures beside the shared codec keep their
provenance. They flow through the actual Client under fragmentation and
prove wire preservation, not current live subscription admission.

## 6. The real loop

An application's real loop (testing-strategy.md, section 2.8) runs the
peer in its own process, on loopback, with no injected latency, as it runs
the fake forge (fake-forge.md) and the fake issuer (oauth.md, section 6.8).
Its script is the same `Script`, `Turn` and `Line`, and the referee reads
the same observations.

- **Most stories give an account a key** (llm.md, section 2.6), which the
  peer accepts: no sign-in is driven.
- **Stories of an account with no credential** configure the peer to
  take calls with none (section 3).
- **Stories that sign a person in** run the fake issuer beside the peer,
  and the test drives the redirect. The issuer mints no token: its
  script lists every access token it grants, each refresh's included
  (oauth.md, section 5). The world that runs both gives the peer that
  list, as bearers, when it starts it (`Peer::new` and `Peer::accept`,
  before it serves, section 3). So a signed-in account's calls are
  accepted without the test reading a token. A token the issuer revokes
  stays accepted by the peer: a story that wants it refused scripts an
  `Unauthorized` line naming its index (section 2.3).
- **Stories of a program that signs in by itself** use a fake program; the
  credential it hands over is one the peer accepts.
- **An account's state is the script's:** failing lines that name a
  credential exhaust, refuse or unauthorize one account and not another
  (section 2.3), so a story asserts what its application does with each.

## 7. Open questions

- **Cache granularity.** Providers cache only past a minimum length and in
  steps of tokens; the fake caches any prefix at a message's or a mark's
  end. Default: it stays so, until a world needs the providers'
  granularity to catch a regression.
- **Digests.** The cache table compares prefixes by a digest of their
  content, not the content itself, to keep its bound small. A collision
  would show as a false hit; with a 64-bit digest and a world's few
  prefixes, it is accepted rather than paid for in memory.
- **A failure's reset as a header.** Section 2.3 renders a reset on the
  public route as the Codex route's, in the error's `resets_in_seconds`,
  though the client also reads `retry-after` (llm.md, section 4.8). What
  stays open is which the public route sends. Default: the error's field
  alone, until a capture shows it.

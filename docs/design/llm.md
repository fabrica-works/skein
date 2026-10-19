# LLM calls

Provisional, 2026-10-09, revised 2026-10-19. `skein-llm` is a reusable
composition of HTTP, SSE and a provider's JSON dialect. It depends on
`skein-lib`, `skein-http` and `skein-json`, and knows nothing of a
service's domain, sockets, credentials store or tools. This is a
composition crate, rather than a primitive protocol machine: the lib-only
rule for the individual machines remains in force.

Providers are OpenAI's public Responses API and Anthropic's Messages API.
OpenAI's default endpoint is `api.openai.com/v1/responses`, called with an
API key or with the access token of a sign-in that pays with the
person's plan, both sent as a bearer (section 4.8). Anthropic's is
`api.anthropic.com/v1/messages`, called with an API key in `x-api-key`
and an API version (section 4.7). A credential is a value and how it is
sent (section 2.6): the caller obtains and renews it, and sign-in and
refresh are outside this crate (oauth.md). Transport connects to the
endpoint's authority and provides a secured plaintext stream. Endpoint
overrides support fakes and explicitly chosen deployment routes; the
caller must bind the stream to the endpoint it supplied. Client identity
strings are optional data, never installed as defaults. Version overrides
are explicit endpoint headers; reserved credential and framing headers
are refused. The caller selects the model and any required compatibility
headers.

**Withdrawn routes.** OpenAI's Codex route (section 4.6) and Anthropic's
bearer route (section 4.7) are withdrawn, each described in its own
section, which says what replaces it.

## 1. In one page

- **One call, one terminal.** A client prepares one call from a neutral
  prompt, measured before its body is allocated, drives it over a
  caller-owned plaintext stream, and ends it with exactly one terminal:
  completed, failed or cancelled. It keeps no timer and retries nothing.
- **A neutral vocabulary.** Prompts, messages, blocks, tools, tool choice,
  affinity and usage name no provider. Provider metadata travels as
  tagged opaque replay. Two dialects render the vocabulary: OpenAI's
  Responses, on its public route and on the withdrawn Codex route, and
  Anthropic's Messages.
- **A credential says how it is sent.** A bearer or a key, which the
  route puts where its provider takes it. A credential carries no
  account id, the withdrawn Codex route's aside. A call to an endpoint
  that takes none, such as a local model's server, carries none
  (section 2.6).
- **Per-call outcomes are not failures.** A tool call too large to keep
  arrives as an oversize block with its byte count; a tool call the
  provider cut at its output cap arrives marked as cut. The completion
  completes, and the caller tells the model.
- **Failures are typed.** A local limit names itself and its bound, an
  HTTP failure carries its status, and a timeout names the deadline that
  passed. A provider's answer is classed: exhausted, with a reset only
  where the provider gives one; refused; unsupported; unauthorized; and
  the rest. Its error code, request id and the field it names travel
  with it as bounded evidence (section 2.5).
- **Limits are derived.** `Limits::derive` turns a few declared
  quantities (window, output, reasoning item, tool payload, calls per
  response, conversations) into every limit of the client, per dialect,
  with checked arithmetic. Each relationship between them is checked at
  startup by name. skein ships the derivation, not a model catalogue.
- **Retained, not buffered.** Provider events are decoded as they
  stream, keeping only what the dialect reads; everything else is
  scanned and counted, never stored. Echoed instructions, tools and
  metadata cost a scan, not memory.
- **A stable prefix.** Encoding is byte-deterministic, so a conversation
  that only appends sends requests that only grow. OpenAI routes requests
  by the prompt's affinity; Anthropic's cache breakpoints sit at positions
  fixed by the prompt's shape.

## 2. Vocabulary and ownership

A `Prompt` holds instructions, a model, conversation messages, tool
definitions, a tool choice, reasoning effort, an affinity and an optional
output-token cap. A message holds ordered text, refusals, tool calls,
tool results and reasoning blocks; a completion may also hold the
per-call outcomes of section 2.4. Provider-owned
`Replay` metadata travels with the block it belongs to and is tagged with
its provider. `Replay` can be retained as a bounded durable envelope without
exposing provider fields to a domain. The exported seven-byte header is
additional to the raw opaque cap; `replay_bytes` and `replay_worst_case`
state complete receiver/transit bounds. A replay value for a different provider
is refused. Native tool calls keep the application call ID and provider item
ID in separate owned fields. Both are bounded opaque strings: pipes, quotes and backslashes
are admitted without imposing an additional identity grammar. The call ID
pairs the exact result; the item ID moves into complete replay metadata and
returns unchanged on a continued request. Neither field is joined or split.
The native answer budget charges both ID payloads, name and input. Its slot
bound uses the enlarged `Part`/`Opened` wrapper size, with active ID/kind bytes
and admitted ready payloads priced simultaneously.

Generic tool calls retain raw JSON argument text; the application validates and
translates it into its own tool types before execution. Responses history carries
the exact bounded UTF-8 argument string, including malformed JSON, so the
application can send an error result and let the model correct its call. The
Client does not interpret that feedback. Anthropic's native `tool_use.input`
is an embedded JSON object: replay refuses malformed or non-object input
rather than normalizing or discarding the original call. A call the
provider cut short never reaches history as a call (section 2.4), so this
refusal only meets a caller's own malformed history. Schemas are bounded JSON
values, independent of the application's tool vocabulary.

Every emitted replay also fits the complete serialized raw opaque cap, including
synthesized text/refusal item IDs, phase fields and tool item IDs. An oversized
native metadata value produces actual Client `Failure::Limit` before a completed
block or Completed terminal, with lower settlement still owed. An admitted value
at that exact cap fits its exported durable envelope without dropping metadata.
JSON admission measures the complete escaped metadata under the smaller document
and opaque cap, without allocating a serialized buffer just to count it. Actual
Client controls cover exact/one-over escaped text, refusal and tool metadata.
Replay metadata and reasoning have separate caps (section 4.2): a large
reasoning item never widens what a text block's metadata may hold.

`Json`, `DocumentLimits` and `DocumentError` expose bounded document admission
without naming a wire dialect. `Json` is a compact document (json.md,
section 5.2). `document_error` retains receiving overflow as
`Error::Limit` and malformed grammar, missing fields or wrong types as
`Error::Invalid`. It reuses the Client's classification; it starts no call,
emits no terminal and supplies no retry policy.

`Client::prepare` validates and measures the whole request before allocating
its exact body. Admission is fallible and does not touch the stream. An
accepted client has one call, named by the caller's opaque token. Every
accepted call has one terminal: `Completed`, `Failed` or `Cancelled`.
Completed blocks and deltas are separate events; completion includes all
completed blocks in provider order, its stop reason and token usage.
Delta indexes are the provider's output/content indexes, not the order in
which completed blocks are delivered. A part's index within an item, a
message's content part or a reasoning summary's part, is not an output
item: parts are streamed as deltas and their bytes are within the answer,
so no count bounds them, and a reasoning summary has as many parts as the
model writes. A run of consecutive reasoning items is one block: Responses
writes a reasoning item for each segment of its reasoning, many before one
call, and Anthropic may write thinking blocks back to back. The block
holds the run's items in order, each as it came, and replays them as they
came, so a completion's blocks follow its calls and texts, not its
reasoning's segments. Each item is within the reasoning limit; the run's
items together are within the completion's share of reasoning, output
items × reasoning, which the receiving reservation already holds, and
their replay values within one block's retained tokens (section 4.2). Tool arguments are incomplete JSON until the whole call
block arrives. A local size limit is a failure, not a
provider token-limit stop. A tool call past the input limit is the one
exception: a per-call outcome (section 2.4). A Responses reasoning item past
its limit is dropped instead only where the owner opted in (section 4.8).

### 2.1 Affinity

```rust
pub struct Affinity {
    pub key: [u8; 16],   // opaque: the tree of conversations that share a prefix
    pub thread: u32,     // one conversation within the key; the first is 0
}
```

- **`Prompt.affinity: Option<Affinity>`** says which conversation a
  request continues, so a provider that routes by it can serve the
  conversation's prefix from one cache. It is the only cache key: no
  endpoint carries one of its own.
- **The caller owns it.** The caller supplies the key, minted at random or
  from a seeded generator, never derived from credentials, paths or
  content, and puts the same affinity, unchanged, into every prompt of a
  conversation, resumed ones included. Conversations that share a prefix, such as a run and its
  children, share the key and differ in the thread.
- **The dialect renders it** (sections 4.6 to 4.8): OpenAI routes by it;
  Anthropic accepts it with no wire effect. A prompt without affinity
  renders none of it.
- **Fixed size:** it allocates nothing, and its rendering has a fixed
  length, which the request head's limit counts.

### 2.2 Tool choice

```rust
pub enum ToolChoice {
    Auto,                    // the model may call any offered tool, or none
    None,                    // the model may call no tool
    Only(Box<[Box<[u8]>]>),  // the model may call only these offered tools, or none
}
```

- **`Prompt.choice`** says which of the offered tools the model may call.
  The offered definitions (`Prompt.tools`) stay the same in every prompt
  of a conversation, whatever the choice: only the choice changes, so the
  definitions stay in the cached prefix.
- **`Only` names offered tools,** each once, at least one; admission
  refuses anything else as `Invalid`.
- **A dialect that cannot express `Only` renders `Auto`.** None does
  today (sections 4.6 to 4.8).
- **The client does not filter.** A call outside the choice arrives as any
  call does, and the caller answers it, as not run.

### 2.3 Usage

```rust
pub struct Usage {
    pub input: Option<u64>,        // prompt tokens neither read from nor written to a cache
    pub cache_read: Option<u64>,   // prompt tokens read from a cache
    pub cache_write: Option<u64>,  // prompt tokens written to a cache
    pub output: Option<u64>,       // completion tokens, reasoning included
    pub reasoning: Option<u64>,    // the part of output spent reasoning
}
```

- **`None` means not reported;** zero is a report of zero. A completion
  whose provider reported no usage has every field `None`.
- **The three prompt fields are disjoint.** When all three are reported,
  their sum is the prompt's size as the provider counted it.
- **Usage never fails a completion.** A dialect's arithmetic is checked.
  A report that contradicts it keeps what was reported, and the derived
  field is `None`; the completion completes. An example is a total below
  the sum of its parts. Usage is accounting: a mismatch never discards
  what the model said, and a consumer charges for a missing count as for
  an unreported one.
- **`reasoning` is part of `output`,** never added to it.
- Each dialect maps its own fields (sections 4.7 and 4.8). A cumulative
  patch replaces a field only when it reports that field.

### 2.4 Per-call outcomes

- **Oversize.** A tool call whose argument text passes the dialect's
  input limit (section 4.2) is a completed block of its own,
  `Block::Oversize { id, name, bytes }`: the call's ID, its name, and the
  byte count of its argument text as received. The arguments were scanned
  and counted, never stored (section 4.4), so the call's size changes no
  memory bound. The completion completes, and the answer budget charges
  only the block's ID and name. The caller tells the model that the call
  was too large, by how much, and how to make it smaller.
- **Cut.** A tool call the provider cut off at its output cap is a
  completed block of its own, `Block::Cut { id, name, arguments }`: the
  argument text received so far, which need not be JSON. The completion's
  stop says why (`Stop::MaxTokens`).
- **Dropped.** A Responses reasoning item past the reasoning limit, where
  the owner opted in, is a completed block of its own, `Block::Dropped {
  bytes }`: the item's byte count as received, scanned and never stored
  (section 4.8). The dialect sends nothing for it, so a history may keep
  it as it came.
- **Neither oversize nor cut is a call.** A caller that matches only
  `Block::ToolCall` never runs one. Neither can be replayed as a call: a
  history holding either is refused at admission as `Invalid`. The caller
  replaces each in its history, as its own policy decides, with a note
  and an error result for instance, so the history it sends stays
  replayable.
- **Deltas** of an oversized or cut call's arguments still go up as they
  come: they are for display, never a call.

### 2.5 Failures

```rust
pub enum Failure {
    Unauthorized,
    Exhausted { retry_after: Option<Duration> },  // the reset, only where the provider gives one
    RateLimited { retry_after: Duration },
    Refused,       // the provider refuses this account what was asked
    Unsupported,   // the provider will not take a field of the request
    Overloaded,
    Unavailable,
    ContextTooLong,
    Invalid,
    Limit { which: Cap, bound: u64 },   // a local limit fired: which one, and its value
    Protocol,
    Cancelled,
    TimedOut { phase: Phase },          // the owner's deadline passed: Connect, Handshake, Head, Idle or Whole
}

pub struct Report {
    pub code: Box<[u8]>,     // the provider's error code, else its error type
    pub request: Box<[u8]>,  // the provider's request id, from the response head
    pub field: Box<[u8]>,    // the field of the request the provider names
}
```

- **The provider's classes.** Each route maps its provider's answers
  onto these by the error's code first, and by the status where the code
  is absent or not one the route names (sections 4.6 to 4.8):
  - `Exhausted`: the account's usage or quota is spent, a plan's usage
    limit or a key's quota. `retry_after` is the reset the response
    gives, and `None` when it gives none: the client never guesses a
    reset and never reports an unknown one as zero.
  - `RateLimited`: a short-term limit; its delay is the provider's hint,
    zero when it gives none.
  - `Refused`: the provider refuses this account what was asked: not
    eligible, a scope not granted, a permission it lacks. Neither the
    same request again nor a refreshed credential changes the answer.
  - `Unsupported`: the provider will not take a field of the request,
    which the report names. It is the request's error: the same body is
    never sent again unchanged.
  - `Unauthorized`: the provider did not accept the credential.

  The client retries none of them; what each does to an account is its
  owner's policy.
- **The report is evidence.** `Event::Failed` carries a `Report` beside
  the evidence and the detail: the provider's error code, its request id
  and the field it names, each as the provider sent it and empty where it
  sent none. Each is within the route's report bound (section 4.2); a
  value past it is left empty, never clipped, since a clipped code or id
  names nothing, and the detail still holds the provider's text. The
  request id is read from the response head, under the field each route
  names, so a failure after a head carries it whether or not its body
  decoded. A failure before a response head has an empty report; a
  local failure after one (a limit, a timeout, a malformed stream)
  carries the request id alone.
- **`Limit { which, bound }`** names the local limit that fired and its
  value: the request, the response head or its field count, an event's
  skipped bytes, retained bytes or tokens, nesting depth, a string, the
  answer, a reasoning item, replay metadata, the completion's items, or
  an error body. Admission's `Error::Limit` names its limit the same way.
  The failure's text detail is derived from it; a limit is never reported
  as a malformed event.
- **HTTP status.** `Evidence::Response { status }`: a response head
  arrived, with this status. Every failure after it carries it, a
  non-JSON error body's included. The other evidence is unsent, or
  possibly sent.
- **`TimedOut { phase }`** names the deadline that passed
  (llm-connection.md, section 6). The client arms none: the owner aborts
  with it. Head and idle say the peer stalled; whole says the owner's own
  time ran out. That is the distinction an owner's retry policy needs.
- **Provider text** stays in the failure's detail, within the detail
  limit. It is data: what to show, and where, is the owner's policy.
  An error body that does not decode as the dialect's error document
  (another JSON shape, plain text, a proxy's page) is provider text
  too: its detail is the body itself, clipped to the detail limit, as a
  cut body's is. The client never puts a diagnostic of its own in its
  place; the status is in the evidence already.

### 2.6 Credentials

```rust
pub enum Scheme {
    Bearer,   // an access token, or a key the provider takes as one
    Key,      // a key, sent where its provider takes keys
}

pub struct Credential {
    pub value: Box<[u8]>,       // the secret
    pub scheme: Scheme,
    pub account_id: Box<[u8]>,  // withdrawn: the Codex route's account; empty elsewhere
}
```

- **A credential is a value with its scheme, `bearer` or `key`, and no
  account id.** The caller obtains it, a key from its holder or an access
  token from a sign-in (oauth.md), and hands it with each call that has
  one (llm-connection.md, section 4.2). The client keeps it no longer than
  its call, and no `Debug` shows it. `Credential::bearer` and
  `Credential::key` make one; the field `account_id` is the withdrawn
  Codex route's alone (below).
- **The route puts it where its provider takes it.** OpenAI's public
  route sends either scheme as `Authorization: Bearer`, and renders the
  same request for a key as for a plan's token (section 4.8). Anthropic
  takes a key in `x-api-key`, with no authorization field; a bearer there
  is the withdrawn subscription route (section 4.7).
- **Or none.** A call's credential is optional (`Option<Credential>`),
  for an endpoint that takes none, such as a local model's server on
  loopback (llm-connection.md, section 3). A call without one sends no
  credential field: no `Authorization` on the public route, and neither
  `x-api-key` nor `Authorization` on Anthropic, whose `anthropic-version`
  is sent. The withdrawn Codex route refuses a call without one at
  admission, as `Invalid`. The client never makes one up: a provider
  that wants one answers `Unauthorized`.
- **No account id.** The account id is the withdrawn Codex route's alone
  (section 4.6), made with `Credential::codex`. The other routes refuse a
  credential that carries one, and the Codex route one without, at
  admission, as `Invalid`.
- **Its bounds.** The value and the account id are each within the
  bounds the owner gives when the component is made (llm-connection.md,
  section 3), which the request head's measure counts; either past its
  bound is refused at admission, by name.
- **Reserved names.** `Authorization`, `x-api-key` and the Codex route's
  `chatgpt-account-id` are credential fields: an endpoint's configured
  headers may not use them, as they may not use the framing fields.

## 3. Driving the stack

`client::down` accepts `Start`, `Next`, `Cancel` and `Close`.
`client::up` accepts the plaintext stream's events. `resume` drains bounded
internal routing work while `has_work` says there is runnable work. Every
entry point declares maximum output counts; the owner reserves them first.

`Next` asks for one delta, completed block or completion. It is idempotent
while one demand is outstanding. JSON/SSE progress and unknown extension
events do not consume that demand. The stack reads the next SSE event only
while demanded and after buffered output has been delivered. Transport
faults and cancellations may end the call without a `Next`. A terminal
answers an outstanding demand and ends all future reads for that call.

The connection's states and transitions are:

| State | Holds / waits for | Input and result |
|---|---|---|
| Prepared | sized request head/body | Start -> Head; Cancel/Close -> Closing |
| Head | HTTP upload and final head | room -> next upload piece; response -> Streaming or ErrorBody |
| Streaming | SSE reader, selective decoder, completion, at most one demand | Next -> consume one output or demand a read; provider terminal -> Draining; fault -> Closing |
| ErrorBody | bounded HTTP error document | bytes/end -> classified Failed; past its bound -> classified by its status and head alone, its detail saying it was cut, and Closing |
| Draining | terminal already emitted, HTTP body discarded | HTTP Done(Keep) -> Idle; Done(Close) -> Closing |
| Idle | retained HTTP client | replace with admitted call -> Prepared; Close -> Closing |
| Closing | machines stopped, actual stream still owned below | caller's settled `closed` -> Closed |
| Closed | no running machines | late inputs ignored |

`Close` on an unfinished call is cancellation. `Close` is a lifecycle
notification asking the owner to close the actual lower stream; stopping
the protocol machines does not claim that socket/TLS ownership has settled.
Cancellation's terminal comes only after the owner calls `closed`, then
`Closed` follows. A previously completed/failed call emits no second
terminal while closing. Completion may precede HTTP drainage; `Reusable`
is emitted only once drainage finishes. Reuse preserves HTTP carry-over
and checks the next call's provider and authority against the binding.

There are no hidden retries or timers. `waiting()` describes the stack's
current demand, and the owning protocol layer arms the connect,
handshake, head, idle and whole deadlines (llm-connection.md, section 6).
It uses `abort` with `TimedOut { phase }` to report its timeout, or with
the transport's failure. Failures include evidence: unsent, possibly
sent, or a provider response received with its status. This does not
instruct a caller to retry.

## 4. Bounds and wire format

Every limit of the client is derived from declared quantities (sections
4.1 to 4.3). Limits are per route: `Limits` holds a set for OpenAI's
public route (`openai`), for Anthropic's (`anthropic`) and for the
withdrawn Codex route (`codex`), as their heads, events, replay and
output caps differ. Startup checks
ensure SSE demands fit HTTP's read cap and that each piece size fits the
transport's (llm-connection.md, section 7). JSON is decoded as each
event's data streams, selectively (section 4.4). HTTP read/room maxima are
exported for the stream owner to check.
`worst_case` adds the child machines, bounded routing queues, the
selective decoder's retained values and depth stack, request storage and
held completion; no size computation may wrap.

### 4.1 Declared quantities

```rust
pub struct Declared {
    pub window: u32,              // prompt tokens the largest model accepts
    pub output: u32,              // completion tokens, reasoning included
    pub reasoning_item: u32,      // bytes of one opaque reasoning item as the provider sends it
    pub tool_payload: u32,        // bytes of one tool call's arguments once unescaped
    pub calls_per_response: u32,  // tool calls in one completion
    pub conversations: u32,       // calls outstanding at once, across the owner
    pub tools: u32,               // tools one request offers, at most
    pub history_items: u32,       // messages and blocks one request carries, at most
}
```

- **Per model:** window, output and reasoning item. An endpoint serves the
  models its owner sends to it, and its declaration takes the largest of
  each. Head and idle, also per model, are deadlines the owner gives per
  call (llm-connection.md, section 6), not inputs to byte limits.
- **Per deployment:** tool payload, calls per response and conversations.
- **Counts, from the owner:** the tools one request offers and the history
  items it carries. The owner knows both from what it holds in memory:
  an agent's, for instance, are its offered tools, and its history's
  bytes over the smallest block it keeps.
- **skein ships no catalogue.** A model's window and output are the
  provider's facts and change too often for a kit. The application
  declares them, from its own presets and settings.
- **Configuration declares quantities, never byte limits.** `Limits`
  stays a plain value, so a test may build one by hand or lower any limit
  after deriving it (testing-strategy.md, section 3).

### 4.2 Derived limits

`Limits::derive(&Declared) -> Result<Limits, Violation>` computes every
limit, for every route, with checked arithmetic: `Limits` holds one set
per route, and a connection takes its endpoint's. An overflow, or a relationship of section
4.3 that does not hold, is a `Violation` naming it. Two factors are the
derivation's own:

- **`ESCAPE` = 6:** the most bytes JSON escaping makes of one byte
  (`\u00XX`).
- **`TOKEN_BYTES` = 8:** the bytes of text one token is allowed to stand
  for. Text averages about four bytes a token; the factor is generous
  because memory is bounded by admission (llm-connection.md, section 7),
  not by these caps.

| Limit | Rule | What it bounds |
|---|---|---|
| request | window × `TOKEN_BYTES` × `ESCAPE` | one encoded request; a prompt within its window meets it only if its text passes `TOKEN_BYTES` a token with every byte escaped |
| answer | output × `TOKEN_BYTES` | what the model writes in one completion: text, refusals, call IDs, names and arguments |
| input | tool payload × `ESCAPE` | one call's argument text as received; past it, an oversize block (2.4) |
| reasoning | reasoning item | one reasoning item: Responses' encrypted content, Anthropic's thinking and signature |
| metadata | the dialect's constant | one block's other replay metadata: item IDs, phase |
| output items | 2 × calls per response + 2 | blocks in one completion: each call with a reasoning block before it, a text and a refusal. A reasoning block is a run of consecutive reasoning items, and a part's index within an item is not an item (section 2) |
| strings | the larger of answer and reasoning | the longest single string the decoder keeps |
| retained tokens | the dialect's fixed fields + reasoning | tokens kept from one event: an opaque value takes at most a token per byte |
| retained text | the largest of answer, input and reasoning, + the dialect's constant for its short kept fields | bytes of text kept from one event, unescaped: the collector's `text` |
| receiving | answer + output items × reasoning + one event's retained values | what one call holds of its answer at once: its memory reservation, beside its measured request |
| skip | request + `ESCAPE` × receiving | bytes of one event the dialect scans without keeping: an echo of the request and of the whole answer, escaped once more |
| tools | the declared `tools` | tools in one request |
| history items | the declared `history_items` | messages and blocks in one request |

- **Counts are memory, so they are declared.** skein's tables allocate
  their capacity when made, so a count is memory. A count derived as the
  request over the smallest encoded item gives counts that only a
  request of empty items reaches, a million history items for one
  272,000-token window, and allocates them all. The owner declares them
  instead. A request within its byte limit that would carry more is
  refused by name, as any count past its bound is.
- **Replay values are charged at their total.** What a completion keeps
  for replay, each block's item IDs, phase and opaque values, is held at
  its own length and charged at the total its blocks keep, within the
  receiving reservation that already holds the answer and the reasoning
  share. It is never reserved as output items × one block's largest:
  output items each holding a reasoning item's worst case is memory no
  completion holds, and each call in flight would reserve it.
- **The encoder makes no copies.** A request is written from the values
  its owner holds, borrowed, straight into the request's bytes, which
  the memory pool reserves:
  - tool schemas, replayed reasoning values and other compact documents
    are serialized by reference, never cloned into a compact copy;
  - JSON text a request carries, such as a call's arguments that a
    dialect sends as an object, is checked by a streaming pass of the
    tokenizer, which writes each token in the compact serializer's
    spelling as it reads it. That means no insignificant whitespace, and
    escapes as the serializer writes them. So the bytes sent, and what
    is admitted against the request bound, are exactly those of parsing
    and serializing it, without a compact document. What fails the pass
    is handled as the dialect already handles invalid arguments.

  So the encoder's scratch is its fixed state and a stack bounded by the
  dialect's nesting depth, held once by the component. Dense JSON, many
  tokens in few bytes, costs no memory, and no count of JSON tokens is
  admitted or refused. Sizing the scratch from the encoded request's
  bytes, a token per byte, charged 577 MB for one 272,000-token window.
  A per-item token constant would undercount dense JSON, so neither is
  used.
- **Sent text has no cap of its own.** A string the caller sends is
  bounded by the request as a whole; what the caller renders into it is
  the caller's to bound.
- **The SSE reader's** line and event limits are the skip cap, counted
  and never held (http.md, section 4.2). Its chunk, and HTTP's read and
  send, follow the transport (llm-connection.md, section 7).
- **Heads.** The request head's limit is measured per endpoint
  (llm-connection.md, section 3). The response head and its field count
  are the route's constants, sized for its provider's header set with
  headroom (section 7): the provider and every proxy in front of it add
  headers without notice.
- **The report** (section 2.5) holds three values, each within the
  route's constant, 128 bytes (section 7). The failure event holds them,
  so the worst case counts them once per call.
- **The error body** and the failure detail are the route's constants.
  An error body is evidence for the failure's detail and its finer class
  (a context too long, a retry hint in the body). It never decides
  whether a failure is retryable: the status does. A body past its bound
  is kept up to the bound and not parsed. The call fails with the class
  its status and head give, as a non-JSON body's does, its detail says
  that the body was cut, and the connection closes rather than drain the
  rest. So a large proxy page in front of a 503 stays `Unavailable`, not
  a local `Limit`. `Cap::ErrorBody` still names a zero bound refused at
  admission.

### 4.3 Relationships checked at startup

Each is checked when the limits are derived or composed, and a violation
refuses the configuration and names the relationship:

- **input ≤ answer:** a call within the tool payload is never cut by the
  answer limit;
- **output items ≥ calls per response,** and strings ≥ input and
  reasoning, which the rules above give by construction and the check
  keeps true when a test lowers one;
- **receiving + request ≤ memory pool:** one largest call fits the pool
  (llm-connection.md, section 7);
- **connections ≥ conversations** (llm-connection.md, section 7);
- **demands fit their caps:** the SSE reader's chunk within HTTP's read,
  the tokenizer's largest demand within the reader's data face, HTTP's
  largest read and room within the transport's. These depend on the
  transport's piece sizes, so they are checked where the connection
  component is built (llm-connection.md, section 7), not in `derive`;
- **a model's output** is at most what its route accepts as `max_tokens`,
  per model, checked where an endpoint of that dialect is composed: a
  declaration for one dialect is never refused for another's route.

### 4.4 Selective, streamed decoding

- **Each event's data streams** from the SSE reader (http.md, section 4)
  into a JSON tokenizer and the selective collector (json.md, section
  5.1). The dialect's filter names the paths it reads. Every other value
  is scanned, checked as JSON, counted against the skip cap, and never
  stored.
- **Responses keeps** an event's `type`, its indexes, its item ID and delta;
  of an item, its type, IDs, name, status, phase, content parts' text,
  arguments (under the input limit) and encrypted reasoning (under the
  reasoning limit), and an unknown item whole, as opaque replay under the
  reasoning limit; of `response`, its status, the known usage fields,
  error and incomplete details. It never keeps `response.output`,
  `instructions`, `tools` or per-item usage attribution, which echo what
  it already has or does not read.
- **Of an error,** each dialect keeps its type, code, message, the field
  it names (`param`) and the reset it gives, and of the response head
  the route's request id and limit fields; the report (section 2.5) is
  copied from them.
- **Anthropic keeps** each event type's known fields; a thinking block
  whole, its extension fields included, under the reasoning limit, so
  that its replay is exact; tool input under the input limit; and an
  unknown native block whole. Its events are told apart by their SSE
  event name, so its filter's root is chosen per event, and need not be
  `Tagged`.
- **Items and blocks are tagged** (json.md, 5.1): a Responses output item by
  its `type`, an Anthropic content block by its `type`. A known type
  keeps its fields; an unknown one is kept whole under the reasoning cap,
  whatever the order of its fields. The input, reasoning and string caps
  are the filter's named caps, set from the derived limits.
- **Retained values are bounded** by the retained-token and string limits.
  A value past its limit is `Limit { which }`, except a tool call's
  arguments, which become an oversize block, and a Responses reasoning
  item under the opt-in of section 4.8, which becomes a dropped block.
- **Memory is one event's retained values,** the tokenizer's text buffer
  and depth stack, and the reader's chunk: never an event's wire bytes.
  The size of what a provider echoes changes no bound.
- **Arguments stay text.** Responses' arguments are a string holding JSON;
  the decoder keeps the string's text and never parses it. The caller
  does.

### 4.5 Byte-deterministic encoding

- **The same input encodes to the same bytes.** The same prompt,
  endpoint, credential and limits always give the same request head and
  body: fixed key order, compact JSON, no clock, no randomness, no
  iteration over unordered state. Per-call headers are a function of the
  prompt alone.
- **Replay goes back unchanged.** The dialect writes provider metadata,
  reasoning and arguments from what it kept, the same way every time, and
  never rewrites their content.
- **So a prefix that is kept is sent again unchanged.** When a caller
  keeps the model, endpoint, affinity, instructions and tools of a
  conversation fixed and only appends to its history, each request's
  instructions, tools and history items are the previous request's,
  followed by what was appended. Anthropic's breakpoints are a pure
  function of the prompt's shape (section 4.7): only the tail marker
  moves, and a marker is not part of the content the provider caches.

### 4.6 The Codex route, withdrawn

**Withdrawn.** ChatGPT's Codex subscription route stays for the callers
that use it and takes no new one. The public route of section 4.8
replaces it, and it is removed once its last caller has moved
(`STATUS.md`). Its requests, tool choice, usage, cut calls and reasoning
opt-in are section 4.8's, which holds the Responses dialect; it differs
in these alone:

- **The endpoint** is `Endpoint::codex()`: `chatgpt.com` and
  `/backend-api/codex/responses`, `Provider::OpenAiCodex`. Its replay has
  its own tag, so neither OpenAI route replays the other's metadata
  (section 2).
- **The credential** is a bearer with an account id (`Credential::codex`,
  section 2.6), which goes in a `chatgpt-account-id` header.
- **Affinity** renders in the body as `prompt_cache_key`, as on the
  public route, and also in a `session-id` header. The thread renders the
  same way, from the key with the thread number XORed, big-endian, into
  its last four bytes, in a `thread-id` header: thread 0 is the key
  itself, as a native Codex root thread's ID is its session's. A prompt
  without affinity sends none of the three. `session-id` and `thread-id`
  are skein's per-call headers.
- **Identity headers** are optional data (`openai::identity`), never
  installed by default.
- **Failures.** `usage_limit_reached`, in an HTTP error or in
  `response.failed`, is `Exhausted`, with the reset the error gives in
  `resets_in_seconds` or `resets_at`; another 429 is `RateLimited`; the
  rest is classed as on the public route. The request id is the head's
  `x-oai-request-id`.
- **Head bounds** are its own (section 7).

### 4.7 The Anthropic dialect

Anthropic sends `stream:true`, native `messages`, tool `input_schema`, and
`max_tokens`. Adaptive thinking maps explicit effort values.
Signed/redacted thinking is preserved
as native opaque replay, tool names/IDs remain unchanged, and tool results
retain their native `is_error`. The bounded decoder verifies message/block
ordering, delta types, cumulative usage patches and completion at `message_stop`.

- **A key.** `Endpoint::anthropic()` is `api.anthropic.com` and
  `/v1/messages`. A key goes in `x-api-key`, with no `Authorization`
  field, and `anthropic-version` goes with it (`2023-06-01` unless the
  endpoint's headers set it). No beta flag is sent unless the endpoint's
  headers name one.
- **The bearer route, withdrawn.** A bearer credential is Anthropic's
  OAuth subscription route: `Authorization: Bearer`, and the OAuth beta
  flag `anthropic-beta: oauth-2025-04-20` unless the endpoint's headers
  set the beta. Its archived Claude Code identity profile
  (`anthropic::identity`) is opt-in: identity headers, and a system
  instruction that goes as the first system block. The route and its
  profile stay for the callers that use them and take no new one; a key
  replaces them, and they are removed once their last caller has moved
  (`STATUS.md`). `Credential::anthropic`, a bearer, goes with them.
- **`max_tokens` comes from the declared output.** It is the prompt's
  output cap, which the caller sets from its model's declared output, or
  lower; a prompt without one sends the endpoint's declared output. There
  is no fixed default. A cap above the declared output is refused at
  admission.
- **Affinity** is accepted and has no wire effect: a prompt encodes to the
  same bytes with and without it.
- **Breakpoints.** `system` is a block array, the withdrawn identity
  profile's block first when a caller opts in, and its last block carries
  `cache_control: {"type":"ephemeral"}`. Anthropic orders its cache as
  tools, then system, then messages, so this marker caches the tools with
  the system text. A second marker, the tail, goes on the last block of
  the last message that may carry one: text, tool use or tool result,
  never thinking or redacted thinking. No `ttl` is written, so each entry
  lives the provider's default of five minutes. There are never more
  than four markers; these two leave room. Empty instructions have no
  system marker, and a last message without an eligible block has no
  tail.
- **The markers move as the conversation grows.** The system marker stays
  where it is; the tail moves to the new end, so each request reads up to
  the point the previous one wrote.
- **Tool choice.** `Auto` sends no `tool_choice`, which is the provider's
  default; `None` sends `{"type":"none"}`; `Only` renders `Auto`
  (section 2.2): Anthropic's `tool` and `any` force a call, which `Only`
  does not mean, and do not combine with thinking. A change of choice
  keeps the cached tools and system text but not the cached messages.
- **Usage.** `input_tokens` is input, `cache_read_input_tokens` cache
  read, `cache_creation_input_tokens` cache write and `output_tokens`
  output. Reasoning is `None`: the provider counts thinking within output
  and does not report it apart.
- **Cut calls.** A `tool_use` whose input is unfinished when the message
  stops at `max_tokens` arrives as `Block::Cut`, with the partial input
  text. It is never parsed into an object, so it cannot make a history
  unreplayable.
- **Thinking past the reasoning limit** is always `Limit`: Anthropic's
  thinking is replayed whole or not at all, so the Responses opt-in does
  not apply.
- **Failures, by the error's type before the status.**
  `authentication_error` is `Unauthorized`; `permission_error` is
  `Refused`; `rate_limit_error` is `RateLimited`, or `Exhausted` where
  the head's `anthropic-ratelimit-unified-status` is `rejected` (the
  withdrawn route's plan limit), its reset from
  `anthropic-ratelimit-unified-reset`; `billing_error` is `Exhausted`
  with no reset (section 7); `overloaded_error` is `Overloaded`;
  `api_error` is `Unavailable`; a context too long, an
  `invalid_request_error` whose message says `prompt is too long`, or a
  `request_too_large`, is `ContextTooLong`; any other
  `invalid_request_error` is `Invalid`. An `error` event in the
  stream, after a head that said 200, of a type this list does not name
  is `Unavailable`, its type kept in the report (section 2.5). Anthropic
  names no field, so its report's field is empty, and a field it will
  not take is `Invalid`. The request id is the head's `request-id`.

Bodies are uploaded in bounded pieces using HTTP room.
Responses must be identity-encoded SSE on success; non-success responses
are parsed as bounded JSON errors. Provider-defined retry hints are data
in structured failures, never automatic retries.

### 4.8 The public Responses route

OpenAI's public Responses API, `Provider::OpenAi`: `Endpoint::openai()` is
`api.openai.com` and `/v1/responses`. It is called with an API key, which
pays at API prices, or with the access token of a sign-in that pays with
the person's plan (oauth.md). Both go as `Authorization: Bearer`, and the
request is the same for both: one rendering serves a key and a plan.

Requests are measured JSON: `store:false` and `stream:true` on every
request, the whole context in `input` (never `previous_response_id`), the
caller's instructions in `instructions`, client-side function tools, and
encrypted reasoning included for replay. A request never carries a field
a plan's route refuses: `background`, `conversation`,
`max_output_tokens`, `max_tool_calls`, `metadata`, `moderation`,
`multi_agent`, `prompt`, `prompt_cache_retention`, `safety_identifier`,
`temperature`, `top_logprobs`, `top_p`, `truncation` or `user`. So
admission refuses an explicit output-token cap, and the local output
bounds still hold.

- **Affinity.** The key renders as 36 bytes: lowercase hexadecimal in the
  8-4-4-4-12 form of a UUID, in the body as `prompt_cache_key`. The
  thread is not rendered: conversations that share a key share its cache
  routing. No `session-id` or `thread-id` header is sent; both stay
  reserved names, which an endpoint's configured headers may not use. A
  prompt without affinity sends no cache key.
- **Tool choice.** `Auto` sends `"auto"`, `None` sends `"none"`, and
  `Only` sends `"auto"` (section 2.2). `parallel_tool_calls` stays true.
- **Usage.** `input_tokens_details.cached_tokens` is cache read;
  `input_tokens_details.cache_write_tokens`, when present, is cache write;
  `input_tokens` less the parts that are reported is input, and `None`
  when `input_tokens` is below their sum (section 2: usage never fails a
  completion). A part the wire does not carry stays `None` and is not
  separated: its tokens, if any, stay within input. Nothing is invented
  for it; `output_tokens` is output;
  `output_tokens_details.reasoning_tokens` is reasoning. A field absent
  from the wire is `None`.
- **Cut calls.** A response that ends incomplete for its output limit
  with a function call unfinished delivers that call as `Block::Cut`.
- **Reasoning past its limit** is a `Limit` failure naming the reasoning
  limit. An owner may opt in to dropping it instead: the item is scanned,
  never stored, and `Block::Dropped { bytes }` takes its place in the
  completion. The dialect sends nothing for a dropped block, so later
  requests go without the item. The route keeps no state (`store:false`),
  so a conversation continues with less of its reasoning, not wrongly.
- **Success is `response.completed`,** which completes with
  `Stop::EndTurn`. `response.incomplete` completes with the stop it
  names, `MaxTokens` or `Refusal`. `response.failed`, or an `error` event,
  fails as its error's code says, as an HTTP error does; a stream that
  ends without one of them fails as `Protocol`.
- **The request id** is the head's `x-request-id`.
- **Failures, by code before status.** The error's `code`, else its
  `type`, decides where the table names it. Otherwise the status does:
  401 is `Unauthorized`; 403, 404 and 422 `Invalid`; 400 and 413
  `ContextTooLong` where the error says so, else `Invalid`; 429
  `RateLimited`; a 5xx `Unavailable`, or `Overloaded` where it is 529 or
  its error's code, else type, is `overloaded_error`. In the stream,
  after a head that said 200, no status applies: a `response.failed` or
  `error` event whose code, else type, neither this section nor, on the
  Codex route, section 4.6 names is `Unavailable`, the code kept in the
  report (section 2.5). A well-formed failure event is the provider's
  answer, never `Protocol`.

| Code (status) | Failure |
|---|---|
| `subscription_sharing_usage_limit_exceeded` (429, or in `response.failed`) | `Exhausted`, its reset only where the response gives one: the error's `resets_at` or `resets_in_seconds`, or `retry-after` |
| `insufficient_quota` (429) | `Exhausted`, with no reset: a key's quota is spent |
| `rate_limit_exceeded` (429) | `RateLimited` |
| `subscription_sharing_user_not_eligible`, `chatpass_v2_scope_not_authorized`, `chatpass_v2_invalid_authorization_context` (403) | `Refused` |
| `subscription_sharing_usage_unavailable`, `subscription_sharing_user_unavailable` (503) | `Unavailable` |
| `subscription_sharing_unsupported_capability` (400) | `Unsupported`, the report's field from `error.param` |
| `subscription_sharing_route_not_supported` (403) | `Invalid`: the method or the endpoint is wrong |
| `subscription_sharing_invalid_user`, `invalid_api_key` (401) | `Unauthorized` |
| `context_length_exceeded` (400) | `ContextTooLong` |

- **A plan's request is never paid another way.** The client sends the
  request it is given with the credential it is given, and never moves a
  call from a plan's token to a key, or back, whatever it fails with.

## 5. Verification

Focused tests cover encoding, replay, concurrent output ordering, deltas,
limits, error classification and every lifecycle boundary. Protocol worlds
drive the real HTTP/SSE/JSON stack over a demand-checking in-memory stream,
with fragmentation, slow reads, early response, truncation, transport faults,
cancellation and reuse. Memory tests compare the counting allocator's high
water with `worst_case`. Fuzzy cases live in the separate nextest profile.
Archived provider transcripts retain their provenance; synthetic cases
are labelled as such. Live admission, with a key or a plan's token, is not
implied by offline tests and requires a caller-provided current credential. Worlds answer the
actual Client with the [shared scripted peer](fake-llm.md); applications
supply its schemas, body menus, invalid inputs and scripts.

The contracts of sections 2 and 4 add these, all in the focused suite
unless marked:

- **Affinity:** the public route carries the key in the body alone, with
  neither header; the Codex route carries it in the body and in both
  headers, each conversation's thread distinct and thread 0 equal to the
  key; no affinity sends none of them; an endpoint header named
  `session-id` or `thread-id` is refused on every route; Anthropic encodes
  byte-identical requests with and without affinity. Through the fake peer, every request of one
  conversation carries the same affinity (fake-llm.md, section 3.1).
- **Determinism:** each request of a growing conversation, encoded twice,
  gives the same bytes, and its instructions, tools and history are the
  previous request's, the moved tail marker aside, followed by what was
  appended.
- **Breakpoints:** markers at exactly the system's last block and the
  tail, never on thinking, at most four, the system marker unmoved as the
  conversation grows; the fake peer reports cache reads up to the
  previous request's tail.
- **Tool choice:** each choice's rendering per dialect; `Only` with an
  unknown, repeated or empty name refused.
- **Usage:** each field present, absent and zero, per dialect.
- **Per-call outcomes:** an oversized call next to a normal one completes
  with one oversize block and one call, the high water independent of the
  oversized call's size; a cut `tool_use` arrives as `Cut` and a history
  holding it is refused; the Responses reasoning opt-in drops an item at
  one byte over and fails without the opt-in.
- **Parts:** a Responses reasoning summary of more parts than output items,
  and a message of more content parts, complete, their deltas forwarded
  with their indexes, under limits whose output items are two.
- **Reasoning runs:** under limits whose output items are two, a Responses
  completion of more reasoning items than that before its final text,
  and one of more reasoning items than output items before one call,
  complete with one reasoning block per run; each block replays its
  items in order, byte for byte; an Anthropic completion of thinking
  blocks back to back does the same; a run whose items pass the
  completion's share of reasoning fails as `Limit`, naming it.
- **Typed failures:** each limit, at its edge and one over, names itself
  and its bound; a non-JSON error body, or JSON of another shape, carries
  its status and its own clipped text as the detail; each deadline's
  phase reaches the failure.
- **Failure classes:** each row of section 4.8's table, as an HTTP error
  and, for the plan's limit, inside `response.failed`, the Codex route's
  `usage_limit_reached` in both, and each of section 4.7's types; a code
  the table names wins over its status;
  `Exhausted` carries a reset where the response gives one and `None`
  where it does not, never zero; each route's request id reaches the
  report, with or without a decodable body; the report's code, request id
  and field at their bound are kept and one over are empty; a failure
  before a head has an empty report, and a local one after it the
  request id alone; in the stream after a 200 head, a `response.failed`
  or `error` event of a code neither section 4.8 nor, on the Codex
  route, section 4.6 names, and an Anthropic `error` event of a type
  section 4.7 does not list, are `Unavailable` with that code or type in
  the report.
- **Credentials:** a key on Anthropic goes as `x-api-key` with neither an
  authorization field nor a beta, and a bearer with both; the public
  route sends a key and a token alike as a bearer, the two requests'
  bodies byte-identical; an account id is refused on the public and
  Anthropic routes and required on the Codex route; an endpoint header
  named `x-api-key`, `Authorization` or `chatgpt-account-id` is refused;
  a value or account id past its bound is refused by name; a call
  without a credential sends no credential field on the public and
  Anthropic routes, and is refused on the Codex route.
- **The public route:** over drawn prompts, no request carries a field
  of section 4.8's refused list or `previous_response_id`, and every one
  carries `store:false` and `stream:true`; an explicit output cap is
  refused at admission; `response.completed` completes, `response.incomplete`
  completes with its stop, and a stream that ends without either fails
  as `Protocol`. Its cases are synthetic, and labelled so, until a
  capture of the route replaces them.
- **Derivation:** properties over drawn declarations: `derive` either
  refuses, naming a relationship, or returns limits for which every
  relationship of section 4.3 holds; no product wraps.
- **Selective decoding:** a protocol world under tiny limits, a handful of
  retained tokens and a few hundred bytes of strings, decodes events that
  echo instructions, tools and attribution far larger than both, with a
  high water independent of the echo's size. A real-structure Codex
  `response.completed` event is kept as a golden event. In the fuzzy
  suite, random extension fields never change what is decoded.

## 6. Later

- **Routing state.** The Codex route's private turn-state token, which a
  response returns and the client replays within a turn, and its
  `reasoning.context`, which keeps reasoning across user messages, are
  adopted on an OpenAI route that takes them only if a consumer's
  benchmarks show a material gain. Adopted, routing state is
  an opaque value the dialect returns with a completion and the caller
  hands back within the same turn, and the reasoning context is a dialect
  option chosen per model.
- **Incremental transport.** Responses over WebSocket send only the new
  items after a previous response on the same connection, which would
  decouple a request's size from its history's. It is on skein's roadmap
  as a transport of its own, with a full resend when a connection is lost.

## 7. Open questions

- **Codex's thread rendering.** The backend's use of `thread-id` is
  private. The UUID-shaped rendering of section 4.6 mirrors native Codex;
  whether the header matters at all is for a consumer's benchmarks to
  measure. It goes with the route.
- **Responses' cache writes.** Whether `cache_write_tokens` is counted
  within `input_tokens`, as cached tokens are, is not yet seen on the
  wire. Section 4.8 assumes it is; a capture that shows otherwise
  changes only the dialect's arithmetic.
- **`TOKEN_BYTES`** and the dialects' smallest tool and item are set
  from captures, and checked against fresh captures as models change,
  which a consumer's benchmarks can take.
- **A bound on what the provider sends has headroom over its captures.**
  The response head's bytes and fields are the provider's and its
  proxies' to grow. Each is at least four times the largest captured,
  and never below 16 KiB and 128 fields: 16 KiB and 168 fields for the
  Codex route, 16 KiB and 128 fields for Anthropic, and the floor, 16 KiB
  and 128 fields, for the public route until a capture of it sets its
  bound. Each capture must fit
  within a quarter of its bound. A fresh capture past a quarter is a
  warning to raise the bound, and is never met by a live failure first.
  On 2026-10-10, bounds set at exactly the largest capture (1,935 bytes,
  42 fields) failed every live Codex call.
- **The error body's bound is a policy value,** since a body past it
  changes no class (4.2): 16 KiB for each route. That holds any
  provider's JSON error with room to spare, at a worst case the memory
  pool counts per call. Named captures of non-success answers, where
  they exist, check that each fits within a quarter of it. The failure
  detail's bound is the same kind of value where no capture settles it.
- **The report's bound is a policy value:** 128 bytes for each of the
  code, the request id and the field, on each route. Codes, request ids
  and field names are short identifiers; a capture of one past a quarter
  of it raises the bound.
- **`Only` on OpenAI.** The public Responses API expresses `Only` as an
  allowed-tools choice; whether a plan's token may use it, rather than
  being answered `subscription_sharing_unsupported_capability`, waits for
  a live test. Until then both OpenAI routes render `Auto`.
- **The plan's reset.** Whether OpenAI gives a reset with
  `subscription_sharing_usage_limit_exceeded` is not yet seen; the class
  carries one only where the response does (section 4.8).
- **A key's spent credit on Anthropic.** `billing_error` is `Exhausted`
  with no reset. A spent credit answered with another type, such as an
  `invalid_request_error` naming the balance, stays `Invalid` until a
  capture shows its shape.
- **Anthropic's lookback.** The provider looks for a cached prefix only a
  bounded number of blocks before a marker. A turn that appends more
  blocks than that would miss the previous tail; a third marker on the
  previous tail would cure it within the limit of four. Whether a turn's
  calls per response can reach it decides whether the dialect adds it.
- **Anthropic's lifetime.** Five minutes until a consumer's benchmarks
  measure what a conversation loses when it pauses for longer, as one
  does while its caller waits on a long piece of work; a longer
  lifetime on the system marker alone is the candidate.
- **Calls past the declared count.** A completion with more calls than
  output items allow fails as `Limit`. Whether the calls past the count
  should instead be per-call outcomes, told as not run, waits for a
  completion that meets it.

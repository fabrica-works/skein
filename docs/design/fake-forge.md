# The fake forge

Provisional, 2026-10-18. The kit's fakes of a forge: a domain that plays a
forge's state and git, a protocol layer that serves Forgejo's API over it,
the forge peer that hosts it on loopback io, the throwaway Forgejo that runs
the real one, and the conformance that holds the dialect to what it does.
The dialect is forge.md's; the parts under test are the kit's others.

## 1. In one page

- **The domain** (`testing/skein-fake-forge-domain`, on skein-lib alone) is
  a step crate with a vocabulary of its own. It is Forgejo-shaped, and no
  better (section 2).
- **Its git** lives in the same store, so a branch pushed is the head the API
  reads. It is reached through a typed transport, never over HTTP (section 3).
- **The protocol layer** (`testing/skein-fake-forge-protocol`) serves
  Forgejo's API over the domain on skein's HTTP server, with its quirks, the
  setup's routes, signed deliveries and byte faults (section 4).
- **The forge peer** (in `testing/skein-fake-peers`, beside the LLM and OAuth
  peers) hosts the protocol layer on loopback, in plaintext, over a forge its
  owner hands it and may share with another fake in the process (section 5).
- **The throwaway Forgejo** (`testing/skein-throwaway-forgejo`) runs the
  pinned binary on loopback, on SQLite in a scratch directory (section 6).
- **The conformance** (`tests/forge/conformance`, outside the suites) drives
  the throwaway and records each fact the dialect relies on as `holds`,
  `fails` or `unchecked`, with masked transcripts (section 7).
- **No serde.** The throwaway and the conformance use skein's HTTP client,
  driven by `skein-shell`'s `drive` as a one-shot host, and skein-json.
- **The service's own:** its fake of its git component, its worlds, stories,
  cost ceilings and git transcripts, and an owned Forgejo's preparation
  (forge-setup.md, section 9).

## 2. The domain

It knows no HTTP, no JSON and no dialect. Its `Read`, `Write` and `Git` are
its own types, close to forge.md, section 3; the layers above translate.

### 2.1 The boundary

- **In:** `Event::Call { reply_to, user, repository, op }`: a user number,
  never a credential, and an `Op` of `Read`, `Write` or `Git`. **Out:**
  `Request::Reply`, exactly one per call; `Request::Hook`, best effort, with
  no terminal event.
- **Entry points:** `step` takes an event, `fire` the clock, the only way
  time moves. Each emits at most `MAX_OUT` = 1 request; `next_deadline` says
  when to fire.
- **A call is decided as it arrives:** refused for its rate, failed before
  it is made, made and then timed out, or made and answered after a drawn
  latency. One beyond `calls` is answered at once as unavailable; a change
  beyond the `hooks` in flight sends no hook.

### 2.2 What it holds and refuses

- **It holds** repositories with their users' permissions, labels, issues
  and pull requests on one numbering, comments, reviews, CI statuses,
  Actions' runs and jobs, merges, a wiki and webhooks. Twenty-one limits
  bound it: counts of each kind, bytes of a name, title, body and file, the
  page size, and the calls, hooks and observations held. Over one, a call is
  refused as too large (bytes) or full (counts).
- **It refuses what Forgejo refuses:** too little permission, an undefined
  label, an empty title, a stale head on merge, a merge its base's
  protection does not allow. It keeps what Forgejo keeps and no more.
- **Listings are Forgejo's:** items updated at or after a time, at the
  forge's resolution, least recently updated first, paged by number. A page
  is read from the order as it stands when its call arrives, so an item
  that changes between two pages can shift another onto a page already read.
- **An item's updated time** moves for a comment, a review, a label, a
  title or body, a push to its head, closing, reopening and merging, not for
  a status on its head or a comment edited or deleted (`status_updates` and
  `edit_updates` make either move it). A pending review is hidden until
  submitted. Stale approvals are dismissed only where the protection says so
  (`dismiss_stale`). Anyone with write permission may edit or delete
  another's comment.
- **Merges** are squashes at an exact head: one commit on the base's tip,
  three-way from the newest commit head and base share, conflicting where
  both changed a path differently. A repository has one protected branch,
  `Protection { branch, contexts, approvals, dismiss_stale }`: a merge needs
  the statuses and approvals asked; pushes, creation and deletion are
  refused. Deleting a branch closes the pull requests it headed or based.

### 2.3 Timing, faults and CI

Chances are per mille, in `Config`, and drawn from the domain's seed.

- **Latency:** `latency_min..=latency_max`, or with the `late` chance
  `late_min..=late_max`. `unavailable` fails a call having done nothing;
  `timeouts` makes it, then fails it; `landing` fails a write or a git call
  as timed out and makes it `land_min..=land_max` after the answer.
- **Rate:** a user may make `rate_limit` calls in a window of `rate_window`,
  which starts with their first call after the last one ended; zero is no
  limit. A call beyond is `limited`; one that finds every window kept
  running is `crowded`.
- **Clock:** the forge keeps its own, at `resolution` seconds, skewed from
  the world's by `Skew::{None, Ahead, Behind}`; every time it shows is on it.
  Hooks come after `hook_min..=hook_max`, late with the `hooks_late` chance
  (`late_min..=late_max`), never with `hooks_lost`.
- **A commit that becomes a head** gets a pending status per context of its
  repository (`Checks { contexts, latency_min, latency_max, silent, passes,
  reruns, cue }`), then a verdict after a drawn latency: passed with the
  `passes` chance, failed otherwise, never reported with the `silent`
  chance. CI runs once per commit and repository. `Cue { path, green }`
  replaces the chance: a commit whose file at `path` holds `green` passes,
  any other fails.
- **A rerun** happens once per context, with the `reruns` chance: pending
  again, then a new verdict, so a head's CI can move after it settled.
  People with write permission report statuses of their own.
- **Actions' identities** follow the statuses kept: one `Run` per commit and
  repository, numbered from 1, and one `Job` per context, at attempt 1 or,
  after a rerun, 2. A commit whose statuses are forgotten loses its run.

### 2.4 Seen from outside

- **Every change is observed,** content and all, and so is every write the
  forge refused, in a bounded queue a referee drains (`pop_observation`). A
  full queue drops and counts (`observations_lost`).
- **A world acts from outside the API** with free functions (grants,
  commits, branch moves, CI outcomes, reachability) and, at settle, reads the
  store (`inspect`), the `tally` of what the forge did and refused, and the
  `room` left, so a forge that filled up is told from one that works. Needing
  room, it forgets the statuses of the oldest commit no head shows, never a
  commit.

## 3. Its git, and the remote boundary

What a fake of a service's git component may rely on. That fake is the
service's own (temper's is the first); the domain is the remote it acts
against.

- **One store.** Commits are named by a count the store keeps, from 1, so a
  seed replays the names. A commit is `Object { parent, merge_parent, tree,
  message }`: a first parent, an optional second, a tree of paths to
  contents, and a message of opaque bytes, at most `title_bytes + body_bytes
  + 2`. Squash merges and `advance` carry an empty message.
- **The calls** are `Op::Git`. `Clone` (every branch, and the default) and
  `Fetch { Branch | Commit | Default }` (the commit and those before it, by
  both parents) need read permission. `Push { branch, commit, expected }`
  and `Create { branch, commit }` need write; a protected branch takes
  neither.
- **A push** moves a branch by a fast-forward, or creates it; any other move
  is `Rejected`. A given `expected` must be the branch's head, or the push
  is rejected. A creation answers `Created` or `Exists`.
- **Every move of a branch** is observed, starts CI on the new head, and
  moves the head of each open pull request that follows it. `advance` is
  the world's other party moving a branch under a run.
- **The scenario's commits:** `commit` returns `None` for an unchanged tree;
  `merge_commit` and `commit_tree` store one anyway. A parent's id is less
  than its child's, as `git::Remote` asks (fake-checkout.md, section 4).
- **Faults, time and identity.** A git call meets section 2.3's latency, rate
  and faults; a fake that needs an instant answer sets the latencies to
  zero. The caller is a user number, mapped from its credential by the
  protocol layer's tables (section 4), so a token for the API and one for git
  stand for the same user.
- **No git over HTTP.** A working tree reaches the store through
  `git::Remote`, answered without the network; commands, arguments and pack
  formats are the service's. A bridge for real git is designed separately,
  and lands here.

## 4. The protocol layer

It depends on skein-lib, skein-http, skein-json, `skein-forge` and the
domain, and holds no clock of its own.

- **A forge it is handed.** It serves a fake forge its owner hands it and
  builds none (section 5). The forge peer hosts it.
- **Types.** `Server` is one HTTP connection (`MAX_UP` = 3, `MAX_DOWN` =
  18). `Service::new(config, &limits, seed)` owns routing, the tally and the
  setup store; `Service::target` hands a domain reply to its connection, or
  returns it whole when that connection has gone. Its `Limits` sit beside
  the domain's, and its `worst_case` is checked.
- **The scenario's tables** (`Config`): the hooks' `Endpoint { forge, host,
  secret }`; `base`, an `http://` origin; `tokens`, secrets to user numbers;
  `users`, numbers to logins; each repository's `id`, full name and
  protected branch; `max_response_items`, `default_paging_num` and `faults`.
- **Routes.** Outside the setup's, each route is one domain call: decoded by
  the server side of `skein-forge` (forge.md, section 5.2), translated to a
  `Read` or `Write`, and its answer encoded as Forgejo encodes it. A commit
  is a 40-digit SHA-1 id with the domain's count big-endian in its first
  eight bytes and zeros after; any other id names none.
- **Credentials.** The header is `Authorization: token <value>`; an unknown
  token is `401`. Tokens stay in this layer: no domain call, observation or
  tally holds one. The tally counts requests at the entrance: total, by route
  (up to 64), by user, unrouted and unauthorized.
- **Forgejo's quirks are kept,** so a client that leans on Forgejo being
  better than it is fails here first: the default order is newest created
  first, and `sort=leastupdate` gives least recently updated first; a page is
  capped at `max_response_items`; a size without a page is ignored; an
  issue's comments, a review's comments and a comparison come unpaged;
  collaborators come without their permissions; a merge answers with no body;
  `pulls/{base}/{head}` names the oldest of several; a pull request whose
  head adds nothing is opened (`201`); deleting a branch that is not there
  answers `500`; a failed job's completion step reports `failure`; a push to
  a pull request's head or base starts a mergeability check. Forge.md,
  section 9 names the fact that records each, or lists those not yet.
- **Faults move down to bytes.** A call refused for its rate is `429` with
  `Retry-After` in whole seconds, rounded up; one failed unavailable is `503`;
  one made and then timed out is `500`. The layer adds per mille byte
  faults: `reset` closes the connection before the next request is read;
  `drop` closes it, and `server_error` answers `500`, after the domain
  answered, so the call is made and its answer lost.
- **Deliveries.** `Service::delivery(&hook, &limits)` renders a domain hook
  as a `Post { head, body }`: a `POST` to `/hooks/<number>` with the event
  header and an HMAC-SHA256 signature of the body in lowercase hexadecimal,
  by the endpoint's secret (forge.md, section 7). A status the fake's CI made
  is delivered as its job's end; a status anyone else reports sends nothing,
  as on Forgejo. Deliveries go to the configured endpoint only; a hook made
  through the setup routes is stored and routes nothing.
- **The setup's routes** (accounts, tokens, organizations, teams, members and
  organization hooks) are served from this layer's own bounded store, not the
  domain's (forge-setup.md, section 4).

## 5. The forge peer

`skein_fake_peers::forge::Peer` hosts the protocol layer as the LLM and OAuth
peers host theirs. The real loop hosts the same one.

- **Its shape is the other peers'.** `new(address, limits, ..)` listens on a
  loopback address, port 0 for the kernel's, and refuses a non-loopback
  address or unusable limits (`Error::Limits`) before any effect.
  `address()` is the bound address, known after io's listening event.
  `observations()` holds frozen records under the world's caps. `shutdown()`
  closes admission, then each connection once its client has settled. It
  implements `Host` (shell.md, section 12), and a checked `worst_case` sums
  the face, the protocol layer, the shared forge, the connections and the
  observations. It records connections, request heads, calls with their
  terminals, and deliveries with their outcomes.
- **Plaintext only.** Its `base` is `http://`; its face is the shared one
  with `Transport::Plaintext` and a `queue` of at least 64.
- **It hosts the layer over a forge it is handed.** The owner builds the
  forge once: its domain, its configuration, the time of the last pass and
  the replies not yet routed. It hands the peer a single-threaded shared
  handle, and may hand the same forge to any fake in its process that acts
  on it, such as a fake of a service's git component (section 3).
- **Rules of sharing:** (1) each entry point borrows the forge for one pass,
  so borrows never overlap; (2) the peer writes the world's time into the
  forge at the start of each pass, and another fake reads it; (3) whoever
  steps the domain leaves a reply or hook it did not ask for on the forge's
  pending list, and the peer routes the list with `Service::target` at the
  end of its pass; (4) each pass the peer fires the domain once, then
  reclaims the domain and the service; `next_deadline` is the earlier of
  io's and the domain's.
- **It writes the deliveries.** A hook the domain releases is rendered
  (section 4) and delivered to the configured endpoint over loopback.
  Delivery is best effort: one that cannot be written (no address, no room,
  the connection refused or reset) is dropped and observed, never resent.

## 6. The throwaway Forgejo

The throwaway runs the pinned Forgejo for the tools and process tiers that
need a real one. It never meets an existing Forgejo.

- **The binary.** `Forgejo::from_path` takes the path from its caller and
  reads no environment. It runs `--version` and refuses, naming it
  (`Refusal`), a binary that is unreadable or whose version line does not
  name the pin's `VERSION` (forge.md, section 4.3). It computes the SHA-256.
  The pin's `RUNNER_VERSION` and `names_runner` are re-exported.
- **The instance.** `start(scratch, &Settings)` writes an `app.ini` for
  loopback, SQLite and the page cap, makes the admin user and its token, and
  waits for the API. The scratch must be empty or absent and not under
  `/tmp`. The environment is cleared, with a home in the scratch, a fixed
  `PATH` and `TZ`, and no system or user git configuration.
  `Settings::small()` pages at 2, enables Actions and bounds an answer at
  16 MiB.
- **Provisioning:** users, tokens, repositories (`Init::{Readme, Empty,
  Sha256}`), collaborators and `fresh_secret`. `Forgejo::work_path` and
  `Instance::cli` run Forgejo's command line without and with a web
  process, keeping the exit code and both streams.
- **Exchanges.** `exchange` (a token, or none) and `exchange_basic` send one
  request and return what was sent and received, through skein's HTTP client
  driven by `drive` as a one-shot host (shell.md, section 12). A recording
  holds no credential: `token MASKED` or `Basic MASKED`. `secrets()` lists
  every secret the instance made, for a leak scan.
- **`stop`** removes the instance and its scratch; a start that fails cleans
  up. The conformance starts the runner (section 7), not the throwaway.

## 7. The conformance

`tests/forge/conformance` is a tool outside the suites. It drives a throwaway
Forgejo with a page cap of 2, so paging shows, and records what the dialect
relies on (forge.md, section 9). It makes its own admin user and tokens.

- **Inputs:** the Forgejo's path, `--scratch` (empty or absent, not under
  `/tmp`), `--out`, and optionally the runner's path, checked with the pin's
  `names_runner`. It reads no variable of a service's name.
- **Transcripts:** `transcripts/<case>/{request,response}` are raw HTTP/1.1
  bytes. A response is unframed, with chunking removed and `Content-Length`
  recomputed; JSON key order, whitespace and types stay as received. A case
  replaces only its own directory. Keys, nesting, types and status lines
  must match; ids, times, counts and text may vary. `transcripts/binary.txt`
  holds the binary's version line and SHA-256.
- **Deliveries:** `deliveries/<event>/{request,response}`, one per event of
  forge.md, section 7, with a mask map of their own.
- **Facts:** `facts.txt` has provenance lines, then a line per fact (forge.md,
  section 9): name, `holds`, `fails` or `unchecked`, reason.
- **Masking:** one map masks object ids and RFC 3339 times by first
  appearance, keeping width and offset; ports, `Date` and `Last-Modified`
  take fixed values; numeric resource ids stay (provisioning has a fixed
  order).
- **Secrets never reach a file.** Before masking, the tool registers every
  token's value and its eight-digit tail, and every password. It scans each
  response body before masking and every output before writing. A leak stops
  the run naming the case and the kind of secret, never its value.
- **The groups:** the dialect's cases; the setup (accounts, tokens,
  organizations, teams and hooks, under the least admin scopes); `cli-`
  facts (Forgejo's command line, outcomes only); `runner-` facts
  (registration, the runner's connection, a workflow that clones a private
  repository with the job's token); and the Actions facts, which run two
  inline host-shell jobs, one succeeding and one failing, with nothing
  downloaded. The runner groups are `unchecked` without the runner.
- **`--component`** runs the kit's component (forge-connection.md) through
  real io sockets on the shell's ring against a fresh fixture, under the
  scopes `read:user`, `write:issue` and `write:repository`. It replaces only
  the `component-` facts and records no transcripts. Each call's cost is
  compared with the request lines counted from socket sends.

## 8. Coverage and budgets

- **Step tests** sit in each crate. The domain's cover each call's decision,
  listings, merges, protection, CI and hooks. The protocol layer's cover
  routes, errors, faults, the tally, the setup routes and deliveries, each
  decoded by `skein-forge`, with a signature that holds for its body only.
- **Memory:** a forge filled to its limits, and a long merge history walked
  through both parents, stay within `worst_case`, in binaries of their own
  under the counting allocator (testing.md, section 6).
- **Replay and the peer:** a seed replays the names, replies, observations
  and deliveries. The peer's tests run a client over loopback against the
  protocol layer, with deliveries written, late, lost and refused, and a
  forge shared with a second fake.
- **The throwaway and the conformance:** tests of masking, facts, `app.ini`
  and refused versions and scratches need no binary and run in the focused
  suite; those that need one are ignored unless the caller names it. The
  tool itself runs outside both suites.
- **Budgets:** the focused suite's 15 seconds and the fuzzy suite's minute
  (testing.md, section 7) bind these tests.

## 9. Open questions

- **A bridge for real git** needs a protocol it speaks, over a listener of
  the peer's. It is designed separately and lands in section 3. Default:
  none; a working tree reaches the store through `git::Remote`.
- **GitHub's protocol layer** would serve GitHub's API over the same domain.
  What GitHub asks of the domain beyond Forgejo's is open (forge.md,
  section 10). Default: the domain stays as it is, and GitHub's quirks live
  in its layer.
- **Deliveries** go to the configured endpoint, not to the organization hook
  that reconcile makes. Whether they follow that hook is open. Default: they
  do not, so a world that wants the hook's route configures its endpoint.
- **The conformance against the fake.** Running its cases against the
  protocol layer would hold its quirks to the facts. Whether it does is open.
  Default: it does not; the protocol layer's step tests hold the quirks.
- **The runner's start.** The conformance starts the runner (section 7).
  Whether a second user moves that start into the throwaway is open.
  Default: it stays in the conformance until a second user needs it.

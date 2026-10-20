# The fake forge

Provisional, 2026-10-20. The kit's fakes of a forge: a domain that plays a
forge's state and git, a protocol layer that serves Forgejo's API over it,
GitHub's layer that serves github.com's (section 10),
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
- **GitHub's layer** (`testing/skein-fake-forge-github`) serves github.com's
  API at the dialect's pin over the same domain: the App, its installations
  and tokens, the browser's pages as plain redirects, the merge queue,
  rulesets, the delivery log and the job log's blob host. Where it differs
  from github.com is written down as contracts (section 10).
- **GitHub's conformance** (`tests/forge/github`) runs the same cases on
  that layer, in the focused suite, and, opt-in and from the owner alone,
  on github.com with the probe App's fixtures (section 11).
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
  the domain's, and `new` refuses limits whose `worst_case` is none.
- **The scenario's tables** (`Config`): the hooks' `Endpoint { forge, host,
  secret }`, `host` being the loopback `ip:port` the forge peer connects to
  and sends as `Host` (section 5); `base`, an `http://` origin; `tokens`,
  secrets to user numbers; `users`, numbers to logins; each repository's
  `id`, full name and protected branch; `max_response_items`,
  `default_paging_num` and `faults`.
- **Routes.** Outside the setup's, each route is one domain call: decoded by
  the server side of `skein-forge` (forge.md, section 5.2), translated to a
  `Read` or `Write`, and its answer encoded as Forgejo encodes it. A write
  Forgejo answers with what it made or changed (an issue, a pull request,
  a comment, a review, an edited issue, a status, a branch) makes one call
  more, the domain's read of that, on the same request. A commit is a
  40-digit SHA-1 id with the domain's count big-endian in its first eight
  bytes and zeros after; any other id names none.
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
- **Deliveries.** `Service::delivery(&hook, &limits)` renders a domain hook as
  a `Post { head, body }`: a `POST` to `/hooks/<number>` with the event header
  and an HMAC-SHA256 signature of the body in lowercase hexadecimal, by the
  endpoint's secret (forge.md, section 7). A status the fake's CI made is
  delivered as its job's end; a status anyone else reports sends nothing, as
  on Forgejo. Of forge.md's table (section 7) it renders `issues`,
  `pull_request`, `issue_comment`, `pull_request_approved` for every review
  whatever its verdict (the hint is the same), `push` of a branch, `delete` of
  a branch, and `workflow_job_success` and `workflow_job_failure`; never
  `pull_request_rejected`, `pull_request_comment`, `create`, a tag's events,
  or a job cancelled or skipped. Deliveries go to the configured endpoint
  only; a hook made through the setup routes is stored and routes nothing.
- **The setup's routes** (accounts, tokens, organizations, a repository's
  read by id, collaborators and hooks, and the admin's repository search,
  in id order and paged by number as Forgejo pages it) are served from this
  layer's own bounded store, not the domain's (forge-setup.md, section 4).

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
  terminals, and deliveries with their outcomes: two records for each
  connection (its opening and its close), two for each call (its head and
  its terminal) and one for each delivery. Nothing drains them, and none
  is dropped: a record past the world's caps fails the peer's assertion,
  "frozen record count fits" or "frozen payload admitted before copying",
  and with it the story. So a world's caps are a stated room for a whole
  story, each named for what it holds, with its reason, and a story that
  outgrows it fails by name rather than losing its history.
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
  (section 4) and delivered over loopback to the endpoint's `host`, the
  address it connects to and the request's `Host` alike, as a Forgejo posts
  to its hook's URL. An owner whose listener binds port 0 sets the endpoint
  in the forge it shares once that listener listens, its address known
  after io's listening event as the peer's own is, not at its bind, before
  a hook it means to have written is due. Until then, and for a `host` that
  is no loopback `ip:port`, there is no address. Delivery is best effort:
  one that cannot be written (no address, no room, the connection refused
  or reset) is dropped and observed, never resent. A hook due before the
  owner's listener listens is such a drop, and one due once the owner has
  begun to stop may be; neither the peer nor the owner holds a hook back
  for the listener. A world asserts a delivery written only for a hook due inside
  its listener's life: a story set up before any host runs loses the
  setup's hooks, and a story that asserts the hook its own end causes
  ends only once the peer holds a delivery's outcome for each hook the
  story's own changes cause, a count the story states. A hook is scheduled
  at its change and released later, so no count of the hooks the forge
  has released so far (its domain's tally) says what a story is still
  owed: the story, which made the changes, does. Its domain's tally is
  checked against that count at the end.

## 6. The throwaway Forgejo

The throwaway runs the pinned Forgejo for the tools and process tiers that
need a real one. It never meets an existing Forgejo.

- **The binary.** `Forgejo::from_path` takes the path from its caller and
  reads no environment. It runs `--version` and refuses, naming it
  (`Refusal`), a binary that is unreadable or whose version line does not
  name the pin's `VERSION` (forge.md, section 4.3). It computes the SHA-256.
  The pin's `RUNNER_VERSION` and `names_runner` are re-exported. The kit's
  tests that need the binaries are ignored by default; their test files
  read the paths from `SKEIN_FORGEJO` and `SKEIN_FORGEJO_RUNNER`, and fail
  naming the variable when it is unset. No library code reads a variable,
  and a service's tests read their own.
- **The instance.** `start(scratch, &Settings)` writes an `app.ini` for
  loopback, SQLite and the page cap, makes the admin user and its token, and
  waits for the API. The scratch must be empty or absent and not under
  `/tmp`. The environment is cleared, with a home in the scratch, a fixed
  `PATH` and `TZ`, and no system or user git configuration.
  `Settings::small()` pages at 2, enables Actions and bounds an answer at
  16 MiB.
- **Attaching.** `Instance::attach(child, port, scratch, admin_token,
  &Settings)` takes a Forgejo its caller started and keeps, such as a
  service's owned forge. It waits for the API and reads the admin's login
  and id with `GET /api/v1/user`, naming no login of its own. Its `stop`
  ends the child and leaves the scratch, which stays its caller's, and
  `cli` refuses, having no work path.
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

- **Inputs:** `--forgejo <path>`, `--scratch` (empty or absent, not under
  `/tmp`), `--out`, and optionally `--runner <path>`, checked with the pin's
  `names_runner`. It reads no variable.
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
  organizations, repositories' collaborators and hooks, the admin's
  repository search, under the least admin scopes); `cli-` facts
  (Forgejo's command line, outcomes only); `runner-` facts
  (registration, the runner's connection, a workflow that clones a
  private repository with the job's token); and the Actions facts, which
  run two inline host-shell jobs, one succeeding and one failing, with
  nothing downloaded. The runner groups are `unchecked` without the
  runner.
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
- **The GitHub layer's speculative merges.** A queue's group is built
  from the entry ahead, one at a time (10.2); github.com batches groups.
  Default: one at a time, until a story needs batches.
- **Deliveries** go to the configured endpoint, not to the organization hook
  that reconcile makes. Whether they follow that hook is open. Default: they
  do not, so a world that wants the hook's route configures its endpoint.
- **The conformance against the fake.** Forgejo's conformance does not run
  against its protocol layer; that layer's step tests hold its quirks.
  GitHub's runs against its layer (section 11). Default: Forgejo's stays
  as it is.
- **Recorded live transcripts.** A live run's masked transcripts would serve
  the dialect's decoder tests. Default: the owner reviews them for leaks
  before committing them; the fake's own transcripts serve until then.
- **The runner's start.** The conformance starts the runner (section 7).
  Whether a second user moves that start into the throwaway is open.
  Default: it stays in the conformance until a second user needs it.

## 10. GitHub's layer

`testing/skein-fake-forge-github` serves github.com's API, at the
dialect's pin (forge-github.md), over the same domain, beside the Forgejo
layer and in its shape (section 4): a `Server` per connection, a
`Service` that owns routing, the tally and the layer's own stores, a
`Config` of the scenario's tables, and `Limits` with a `worst_case`.
Each route is decoded by the server side of `skein_forge::github` and
answered as GitHub answers. GitHub's quirks live here. The domain gains
only what GitHub has and Forgejo lacks (10.2).

### 10.1 What it serves

- **REST at the pin.** Every route of forge-github.md, sections 4, 8, 9
  and 11, at github.com's paths, with no prefix. A request without
  `X-GitHub-Api-Version: 2026-03-10`, or without `User-Agent`, is
  refused as GitHub refuses it (`400`, `403`). Pages are GitHub's: up to
  `per_page` rows, at most 100, with `Link` next while more remain.
  Issues and pull requests share one numbering, and the issues' listing
  holds pull requests, each with its `pull_request` member.
- **GraphQL at `POST /graphql`.** The layer answers the dialect's three
  documents, matched byte for byte against its constants (`viewer`,
  `pull`, `enqueue`), each with its variables. Any other document is
  answered `200` with an `errors` entry. This is a difference from
  github.com, which answers any query.
- **Credentials.** `Authorization: Bearer <t>` names one of three kinds:
  - a person's token (the scenario's `tokens`, as in section 4);
  - an installation's token, which the layer mints;
  - the App's JSON Web Token, which the layer checks by signing the
    token's signing input again with the App's key: RS256 with PKCS#1
    v1.5 is deterministic, so it needs no verifying function. A token
    past its `exp`, or more than 600 seconds after its `iat`, or not
    yet valid by more than 60 seconds on the forge's clock, is `401`.

  The App's routes on another kind are `401`. `GET /user` on an
  installation's token is `403`. Tokens stay in the layer, as in
  section 4.
- **The App** is held in the layer's store: id, slug, owner, key,
  webhook secret, permissions, events and the hook's URL. Its bot is a
  domain user, `<slug>[bot]`, who acts on the domain. `GET /app`, `GET`
  and `PATCH /app/hook/config`, and `GET /users/<slug>%5Bbot%5D` read and
  change the store.
- **Installations and their tokens.** An installation is an account and
  either all its repositories or a list of them, and may be suspended.
  A repository's `owner.type` is its account's kind, so one an
  organization owns reads `Organization`, and its pulls take the queue
  (forge-github.md, 4.1 and 5.1).
  The mint answers a token that expires an hour later on the forge's
  clock, full, or contents-only when the body asks. The domain sees the
  bot's user number, and the layer refuses beforehand what the token
  does not reach:
  - a repository outside the installation is `404`;
  - a route beyond a contents-only token is `403` "Resource not
    accessible by integration";
  - an expired token is `401`.

  `GET /app/installations` lists them and `GET /app/installations/<id>`
  reads one, each row with its account's type and its creation time on
  the forge's clock. `DELETE /installation/token` revokes a token. The
  App's `DELETE /app/installations/<id>` removes an installation.
- **The browser's pages, as plain HTTP** (seams S4), at the `web`
  routes, which the same listener serves:
  - `POST /settings/apps/new?state=<s>`, and
    `/organizations/<org>/settings/apps/new?state=<s>`, take the form's
    `manifest` field. The answer is `200` with a small page holding one
    link, `Create GitHub App` (id `create`), to the manifest's
    `redirect_url?code=<c>&state=<s>`; a test follows it as a person
    clicks GitHub's own form, and no form's `form-action` governs it.
    The code is single-use and expires an hour later. The layer keeps
    each state's posted manifest, which a test reads through the
    layer's handle by state (its `redirect_url`, `setup_url`,
    `hook_attributes` and the rest), since the page's post is not
    otherwise in sight.
  - `POST /app-manifests/<code>/conversions` makes the App and answers
    as forge-github.md, section 11, says.
  - `GET /apps/<slug>/installations/new?state=<s>` takes `account=<login>`
    (by default the App's owner), and `repositories=all` (the default)
    or `repository_ids=<id>,...`. It installs and answers `302` to
    `setup_url?installation_id=<i>&setup_action=install&state=<s>`; for
    an account already installed, `setup_action=update`.
  - A malformed manifest, or an unknown account, is `422` with a
    message.

  The keys an App can get are the layer's fixed test keys (`KEYS`,
  2048-bit, made for the fake and public). The conversion hands them out
  in turn.
- **Checks.** The domain's CI statuses for a commit are rendered as the
  `github-actions` App's check runs, one per context. The run is the
  domain's `Run`, the job its `Job`, and the attempt the job's. Statuses
  that people report stay statuses. The combined status is `pending`
  with `total_count` 0 when there are none.
- **A job's log** is a `302` to the blob origin: a second listener of
  the peer, the origin `http://<blob>`. The target is a path signed for
  600 seconds on the forge's clock. The blob listener serves the job's
  log, wants no credential, and ignores `Range`. The scenario may name a
  third origin, one the component does not know, for the diagnostic's
  stories.
- **Mergeability, merges and the queue.** `mergeable` is null for a
  drawn `settle_min..=settle_max` after a push to the head or the base.
  A merge then decides as the domain does:
  - a stale head is `409`;
  - a merge refused for its checks or its approvals is `405` with the
    rule's lines;
  - a branch that queues refuses a direct merge with the queue's
    message, and takes `enqueue`.

  The layer's `pull` document answers `mergeQueueEntry` and
  `mergeCommit` from the domain. The REST pull of a pull request merged
  by the queue has `merge_commit_sha` null, as on github.com.
- **Rulesets and classic protection.** The scenario says how the
  domain's protection shows (`Shown::{Ruleset, Classic, Both}`):
  - as a ruleset, with its id and the App's `current_user_can_bypass`;
  - as classic protection, otherwise `404` "Branch not protected";
  - or both.
- **Rates.** Every answer on a token carries `x-ratelimit-*` from the
  domain's window: `rate_limit`, what remains of it, and its end. The
  App's own routes carry none. A call refused for the rate (`limited`)
  is `403` with `x-ratelimit-remaining: 0`. A crowded one is `403` "You
  have exceeded a secondary rate limit" with `Retry-After`. A request
  with an `If-None-Match` that matches the answer's `ETag` is `304`, and
  not counted.
- **Faults** are section 4's, at the bytes, with GitHub's statuses.

### 10.2 What the domain gains

These are small, and they land after forge-kit's steps on the fakes:

- **Protection** gains `queue`, `linear`, `code_owners` and a list of
  bypassing users. A bypassing user's merge skips the checks and the
  approvals. The domain does not sign, so it keeps no signatures rule:
  the layer shows `required_signatures` from its `Config` and refuses a
  push as `GH013` when the scenario asks for it.
- **The merge queue.** `Write::Enqueue { number, head }` refuses a moved
  head, as a merge does. An entry's group commit is the squash of its
  head onto the base's tip, or onto the entry ahead of it. That commit
  becomes the head of `gh-readonly-queue/<base>/pr-<n>-<head>`, so the
  domain's CI runs on it as on any head:
  - when it passes, the base moves to it and the pull request is merged;
  - when it fails, the entry leaves with the reason `CI_FAILURE`;
  - entries behind a failed one are built again.

  Each leaving is a hook (`dequeued`, and the group's `destroyed`).
- **Installations' events.** Hooks for an installation made, removed,
  suspended, given repositories or renamed, released by the layer's
  store.

### 10.3 Deliveries and their log

- **Every delivery is logged.** A domain hook is rendered as GitHub's
  payload, with `X-GitHub-Event`, `X-GitHub-Delivery` (a UUID drawn from
  the seed) and `X-Hub-Signature-256`, and is entered in the layer's log.
  Each entry has an id of 19 digits, starting at 10^18 plus a seeded
  offset, and increasing.
- **Pushing is the scenario's choice.** By default the hook's URL does
  not resolve, as temper's App's does not: no delivery is written, and
  each is logged as failed, `status_code` 502. Pushing writes it over
  loopback to the configured endpoint, as section 5 says, and logs the
  answer's status.
- **The log** is `GET /app/hook/deliveries`, newest first, paged by an
  opaque `cursor` in `Link`. The first page's `ETag` changes when an
  entry is added, so a first page asked with it unchanged is `304`.
  `GET /app/hook/deliveries/<id>` answers the delivery whole: the
  request's headers and the payload, re-encoded, and the response. An
  entry is listed `log_min..=log_max` after its change (default 1.5 to
  2.2 seconds, as on github.com), and kept for `log_keep` (default three
  days) or until `log_entries` is full, oldest first out.
- **No installation, no repository** in the list for `ping`,
  `installation` and `installation_repositories`, as on github.com.

### 10.4 Where it differs from github.com

These are contracts: a story that leans past one of them is wrong, not
the fake.

- **Only the dialect's routes and documents** are served. Others are
  `404`, and other GraphQL documents are `errors`.
- **Plaintext, on loopback,** at three origins: `api`, which also
  serves `web`'s routes, `blob`, and an optional third one.
- **Messages** are the conformance's recorded texts, one per refusal of
  forge-github.md, section 7. The layer makes no other.
- **Merges** follow the domain: squashes, whatever the method asked,
  and a queue's group is built from the entry ahead, not batched.
- **Ids** are the domain's numbers, small and sequential. Deliveries'
  ids are 19 digits, so `u64` is held exactly.
- **What is not modelled:** GitHub Apps other than the one App and
  `github-actions`; forks; organizations' members and teams; signed
  commits; workflows; any secondary limit beyond `crowded`.

### 10.5 The peer and its tests

- **The forge peer hosts it** (section 5) with
  `Peer::github(address, blob, limits, ..)`. It works as the Forgejo
  face does, over a forge it is handed, with the same rules of sharing.
- **Step tests** cover each route, each refusal and its message, the
  mint and the expiry, the JWT's window, the browser's routes, the queue
  through to landing and to ejection, the log's paging and its `304`,
  and a delivery decoded by `skein_forge::github` alike pushed and
  logged.
- **Memory** is checked as section 8 says, with the stores full.
- **The budget** is the focused suite's, as section 8 says. The
  conformance against the fake (section 11) runs there as one test,
  within 3 seconds. The fake's clock carries the CI's and the queue's
  waits, so nothing sleeps.

## 11. GitHub's conformance

`tests/forge/github` is a tool beside section 7's. It runs the same
cases against two targets and records each fact of forge-github.md,
section 13, as `holds`, `fails` or `unchecked`. Its transcripts, facts,
masking and refusal of leaked secrets are section 7's.

- **The fake** (`--fake`, the default) is the GitHub layer in-process,
  hosted by the forge peer. It also runs as one test in the focused
  suite, which fails on any fact that does not hold.
- **github.com** (`--live <directory>`) is opt-in. It runs from the
  owner's session alone, and only with the director's word. It is never
  run by a suite, a gate or an implementer, and never from CI.
- **A fact that holds on github.com and fails on the fake** is a finding
  for the fake. One that fails on github.com is a finding for the
  dialect, and its pin moves only after it is understood (forge-github.md,
  section 12).
- **The fixtures' directory** is the person's
  (`~/src/rust/tmp/github-conformance/`, by its README), named by
  `--live`. The tool reads no variable, and refuses a directory that is
  not `0700` or a file that is not `0600`. It holds:
  - `app.json`: the probe App's id, slug, permissions, events and
    installations;
  - `app.pem`: the App's key;
  - `webhook_secret`: checks logged deliveries' signatures;
  - `person-<owner>.token`: one fine-grained token per owner, for any
    number of owners, found by name. Each is limited to that owner's
    probe repositories.
- **The fixtures:** the probe App `temper-probe-d0fc06`, installed on
  `freeekanayaka/temper-probe` and `fabrica-works/temper-probe`. The spare,
  `freeekanayaka/temper-probe-2`, is not granted: a run adds it to the
  installation and removes it again. Each probe repository's `ci`
  workflow fails a head whose message holds `FAIL`.
- **Who does what:**
  - The App's key and minted tokens drive the dialect's cases.
  - The person's tokens drive the person's part: rulesets, approvals,
    and an installation's repositories added or removed. They cannot
    list an installation's repositories, so the App's token reads them.
- **Expiry.** Each run reads each person's token's
  `github-authentication-token-expiration` and names, in `facts.txt`'s
  provenance, any token that expires within 30 days.
- **Secrets** never leave the directory. The tool registers the key,
  every token, every JWT, every minted token and the webhook secret
  before the first request. It copies none, and it masks
  `Authorization`, signed blob addresses, guids, node ids and ETags as
  well as section 7's. Numeric ids are masked by first appearance, since
  github.com's do not come in a fixed order.
- **The App is read only, live.** The fixture installations are
  accepted by no setup state, so setup's reconcile would take them for
  strays (forge-setup.md, section 5). A live run therefore never sends
  `DELETE /app/installations/<id>` or `PATCH /app/hook/config`, makes
  or converts no manifest, and changes nothing of the App: it reads
  the App's documents alone. The tool refuses those routes on the live
  target before any byte is written. Strays, the uninstall and the
  hook's address being set are checked on the fake alone, and their
  facts are `unchecked` live (forge-github.md, section 13).
- **Access is put back.** A repository the run adds to an installation
  or removes from it, with the person's token's `204`, is restored at
  the run's end. Each such change is written to `made.txt` before its
  request, so `--clean` restores it too.
- **A run cleans up what it made:** rulesets, pull requests (closed),
  issues (closed), branches, the spare's grant, and minted tokens
  (revoked). Each thing made is first written to `made.txt` in `--out`,
  and `--clean <out>` finishes a run that stopped partway.
- **Calls are few.** A full live run stays under 400 requests and about
  ten minutes, most of it waiting for the probe workflow's 45 seconds.

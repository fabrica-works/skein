# The forge client

Provisional, 2026-10-18.

`skein-forge-client` keeps a bounded view of a forge for the domain that
owns it, and makes each write the owner commits at most once. It is
`no_std`, lib-only, and depends on `skein-lib` and on `skein-forge`'s
vocabulary (forge.md, section 3). It makes no HTTP call and knows no
policy: the owner runs its calls (forge-connection.md) and decides the
rest.

## 1. In one page

- **A capability, not a connector.** It keeps, reads and writes. What to
  watch, which effects exist, when to write and what people see are the
  owner's (section 9).
- **The owner commits, then releases** (section 2.1): a decision's saved
  rows, progress and outcomes first, its calls after.
- **A bounded working set** (section 3), refused whole past capacity,
  **kept up by listing,** sooner on a hint, which is a reason to read and
  never a fact (section 4). **Fresh reads go first** (section 5).
- **One request budget** (section 6): four classes share `rate` requests
  per `window`. A rate refusal holds them all until its reset.
- **Each effect is made at most once** (section 7). Its key, lane and
  recovery class decide how a lost answer is found. The attempt is saved
  before the write starts, so the search survives a restart (section 8).
- **Everything is bounded:** `worst_case` (section 2.2).

## 2. Its owner, and how it is driven

The owner is a domain: a service's forge connector, which keeps the
service's durable records. The first is temper's. The client is its child
domain (programming-model.md, section 4.5), with its own vocabulary,
limits, worst case and tests; the owner translates between the two
vocabularies with total functions and routes the client's output.

- **Entry points,** each given `Env<Limits>`: `step(d, env, event, out)`
  decides one `Event`; `fire` expires one deadline; `resume` sends at
  most one eligible call. Questions (`next_deadline`, `is_due`,
  `is_ready`, `has_room`, `cached`) read state; `pop_fact` gives
  diagnostics, which decide nothing.
- **Construction:** `Domain::new(limits)`, `seeded(limits, seed)` and
  `configured(limits, seed, config)` with the owner's namespace and
  writers (section 7.1). **The namespace is given before any restored
  entry or live resource is handed over** (section 8). An owner that
  learns it after construction sets it with `bind_deployment(id, limits)`,
  a 16-byte id, before that restore. It returns false, changing nothing,
  once the client has taken any `Restore`, `Keep`, `Make` or `Remake`,
  with no configuration, or when the result breaks section 7.1.
- **Calls are the owner's to run.** `Request::Call` carries a token, a
  repository and an `Op` (forge.md, section 3); the owner returns exactly
  one `Event::Answered { call, cost, result }` per token, a cost of 0
  included.
- **The client keeps no store.** It emits `Save` of a `Stored` row (`Live`
  or `Repository`) and `Erase` of its `Key`. The owner commits an outbox
  `Entry` and hands it over with `Make`; the client returns `Progress`
  and, when the entry settles, an `Outcome`.

### 2.1 The commit rule

A decision is one call of `step`, `fire` or `resume`. The owner commits
the client's `Save`, `Progress`, `Erase` and `Outcome` of a decision
before it releases the `Request::Call`s the same decision made.

- **Why:** a write's attempt is in the `Progress` of the decision that
  sends it. After a crash that attempt says the write may have landed,
  and restart looks for it (section 8). A decision not committed sent
  nothing: the client rebuilds from the rows that did commit.
- **Output room:** one decision pushes at most `max_out(l)` requests,
  `max(4, 2 * resources + 2 * repositories + 3)`.

### 2.2 Limits and the worst case

The owner chooses the limits and checks them against `worst_case` before
construction: `pending` and `calls` (at most `pending`), `op_bytes` and
`answer_bytes`; `rate`, `reserve` (at most `rate`) and `window`;
`read_attempts`, `backoff` and `backoff_max` (at least `backoff`);
`resources`, `repositories`, `rows` and `inbox`; `poll`, `poll_max` (at
least `poll`), `hinted`, `slow` and `settle`; `entries`,
`write_attempts`, `lifetime` and `clock_margin`; `facts`.

`worst_case(l)` is none when a limit is 0, except `op_bytes`,
`answer_bytes`, `reserve`, `backoff_max`, `poll_max` and `facts`, or a
pair above is out of order. Else it sums every table and queue at its
limit: operations at `op_bytes`, held and again in flight, the entries,
the working set at saturation, the deadlines and the diagnostics.

### 2.3 Admission and answer checks

- **The measure** is the vocabulary's (forge.md, section 3.7), which
  `skein-forge`'s `measure::op` and `measure::answer` give. The owner
  bounds what it stores of the client's by the same widths, with
  `skein-forge-client`'s `measure::stored(&Stored)` and
  `measure::effect(&Effect)`: a comment, a review or an echo a live row
  keeps counts 16, a label 16 and its length, a reviewer 8, and every
  string its length.
- **An operation is admitted** when its bytes are at most
  `op_bytes`, its pages start at 1, a file's path is not empty and a
  `max_bytes` is from 1 to `answer_bytes`; else it is `TooLarge`, with no
  call. `Busy` is the client's own refusal, never a dialect's.
- **An answer is checked** against its call before anyone takes it, by
  forge.md, section 3.7's table. Past `answer_bytes` or `rows` it is
  `TooLarge` for a read and `InvalidAnswer` for a write. Any other
  mismatch is `InvalidAnswer`.

## 3. The working set

The client holds the resources the owner names, and nothing else.

- **A resource** is a `Repository`, `Branch`, `Pull` or `Issue` of a
  repository. A `Watch` adds whether it **participates**, delivering
  people's comments and reviews; only a pull request or issue may. A
  `Repository` watch uses its listing only.
- **Rows,** saved as they change (section 2.1): a repository's `mark` (the
  forge's time of its last complete listing) and `clock` (the newest
  forge time seen); per resource, its `Watch`, scan cursors, the stamps
  (id, revision) of comments and reviews delivered (at most `inbox`
  each), cached values, pushed head and echo ledger (section 7.6).
- **`Event::Keep { owner, watches }`** names the complete set the owner
  needs. `Kept` answers once, and anything but `Ok` changes nothing:
  - `TooLarge`: over `resources` watches, or a branch name empty or over
    `op_bytes`;
  - `Busy`: before restore, after a failed one (section 8), or when the
    set plus the leavers still busy passes `resources` or `repositories`;
  - `Refused`: participation on a repository or branch, or a duplicate.
- **Leaving.** A resource the owner stops naming is erased and gets no
  further call; a call already out keeps its one terminal, its answer
  dropped, and counts against capacity until then. A closed item is not
  dropped by the client: the owner stops naming it.
- **Scan positions** advance in the decision that delivers their news, so
  a restart between pages repeats no row. A scan that would pass `inbox`
  stamps or pages fails `TooLarge` and delivers nothing from that page.
  `cached(resource)` returns the latest values read; a decision that
  depends on one reads it afresh (section 5).

## 4. Keeping up

- **A listing per repository** reads what changed since its mark: by the
  forge's time, inclusive, least recently updated first, paged from 1.
  Its cost follows the rate of change, not the items held. It repeats
  after `poll`, or `hinted` once a hint named the repository.
  - **No mark:** the first listing takes the forge's time as the mark and
    reads no history.
  - **Paging.** A full page ending at a later time moves `since` there
    and restarts at page 1. A full page of one time goes to the next.
  - **The mark** is the newest time seen. If an item changed at or after
    the listing began, it is instead the time of the first full page of
    one time, when there was one. The next listing replays what may have
    moved, and the stamps drop repeats.
  - **A listing that fails** keeps its place and goes again after
    `backoff`. It reports nothing.
- **Per resource.** An item a listing reports changed, and a resource a
  hint affects, is read after `hinted`, as `Keep`. A pull request then
  rests `poll`, doubling after each quiet read to `poll_max` (capped at
  `slow`), and starts again at `poll` on a change or hint. That finds what
  moves no updated time: CI, a base. Issues and branches rest at `slow`.
  The read that ends a `slow` rest, or retries a failure, is `Slow`. A
  participating item's `Slow` read rescans its comments from 0 and rereads
  its reviews, finding an edit that moved no time; a hint naming an item
  asks for the same.
- **Hints.** `Hint { repository, change, key }` names an item, a branch,
  a branch moved (with head and actor) or a commit. It affects the held
  resources it can have moved: the item, the branch, the pull requests
  with that branch or commit as head or base, the repository's listing.
  Nothing it says is stored or delivered. A branch name or key over
  `op_bytes` is refused with a fact and read for nothing.
  - **Echoes are the owner's.** The owner drops the hints its own writes
    cause, by key and by pusher. The client reads on every hint it holds a
    resource for, and uses a hint's pusher (its actor) for drift alone.
  - **No pace depends on a hint:** one lost, refused or never sent costs
    latency, never liveness.
- **Drift.** The owner says which branches a writer holds (`Writer`) and
  which commit its run pushed (`Pushed`). The client reports
  `Drift { resource, expected, observed }` when a branch's tip differs
  from the pushed head with no writer holding the branch, moves backwards
  under a writer (a `Fresh` comparison finds the old tip not contained in
  the new), is moved by a hint's actor who is not a configured writer, or
  is gone (`observed` none). A forward move by a writer is its own and
  reports no news. Releasing a branch (`taken` false) reads it afresh.
- **News.** `Changed { resource, result }` carries the item with its new
  comments (a page at a time, `more`), the pull request, the new reviews,
  or the tip. A comment or review is delivered once for each id and
  revision, and again when its revision moves; one the owner wrote is not
  (section 7.6). A read that fails reports `Changed` with its error and
  is read again after `backoff`.
- **Settling.** Forgejo answers a pull request not mergeable while it
  checks, and orders a job's end and its commit status nowhere (forge.md,
  section 9). Both settle within `settle`, which the client checks only
  to be above 0. It runs no window for it: the rereads within `settle`
  are the owner's fresh reads (section 5).

## 5. Fresh reads

Before a decision depends on a fact, the owner asks for it afresh with
`Event::Read { owner, repository, read }`: any `Read` of the vocabulary
(forge.md, section 3).

- **One terminal:** exactly one `Request::Read`, an answer or an error.
- **Refused at the entrance,** with no API call: `TooLarge` by section
  2.3; `Busy` when the call slab is full or when the foreground share is
  spent and a keeping read is due (section 6).
- **Retries.** `Unavailable` and `Timeout` are tried again, `read_attempts`
  tries in all, after `backoff` doubling to `backoff_max` plus a draw
  below that delay, capped at `backoff_max`. A rate refusal queues the
  read again and spends no try.
- **Settling** (section 4) is the owner's: it rereads a pull request not
  yet mergeable, or a commit status, with fresh reads within `settle`.
  The client keeps no window and treats each read as any other.

## 6. The request budget

Every call has a class. They go in this order, and in arrival order
within one: `Fresh` (a fact for a decision; a write's clock, checks and
finds), `Write` (an effect ready to be made), `Keep` (a read of changed
live resources) and `Slow` (the periodic backstop).

- **Charging.** A window starts with its first call and lasts `window`;
  at `rate` spent, nothing is sent until it ends. A call is charged 1 when
  sent and settled by the cost its answer reports: 0 refunds the 1 (in the
  same window), `n` above 1 debits `n - 1` more.
- **The reserve.** While a `Keep` or `Slow` call waits, `Fresh` and
  `Write` take at most `rate - reserve` of a window; with none waiting,
  all of it. Once that share is spent and a keeping read is due, a new
  fresh read is refused `Busy`, and `resume` sends the keeping read.
- **A rate refusal** (`RateLimited { after }`) holds all four classes
  until its reset; overlapping resets keep the later. The call is queued
  again under a new token, spending no retry, unless its resource has
  left.
- **In flight:** at most `calls`, one `Request::Call` per `resume`.

## 7. Effects: keys, write lanes and recovery

An effect is a `Write` of the vocabulary with its `Condition` (none for
most), committed by the owner as an `Entry`. The client makes it once
and says what happened.

### 7.1 Keys

`effect_key(namespace, identity, max)` gives the bytes of version `1`,
the namespace's length as two bytes big-endian, the namespace, and the
identity. It is none for an empty or over-long (65535 bytes) namespace,
an empty identity, or a total over `max`. A creation carries its key in
a marker (forge.md, section 6).

- **The namespace** is the owner's, stable across restarts and releases;
  two services on one forge have two. **The identity** is the owner's
  nonempty bytes: the same effect has the same, different effects
  different ones. The client never inspects it.
- **Belief.** A key read from the forge is the owner's when it has
  version 1, the configured namespace and, as author, a configured writer
  of that forge. A party copying its bytes gains no standing.
- **`Config { namespace, writers }`** holds `Writer { forge, author }`
  entries: one to `repositories` of them, at most one per forge, no
  author 0, the namespace of 1 to 65535 bytes, and both within
  `op_bytes`. `configured` refuses an invalid one (`TooLarge`). With none,
  the client refuses every effect `Forbidden` and believes no key.

### 7.2 Entries and lanes

- **Admission.** `Make { entry }` is refused with `Outcome::Failed`:
  `Busy` before restore ends or with `entries` held; `Forbidden` where
  the forge has no configured writer; `TooLarge` over `op_bytes`, with an
  attempt but no start, an empty key, or a condition its write does not
  take (a merge takes a base, an update a head and a base, a review a
  head, the rest none). A new entry's number is above every held one.
- **A lane** is a repository and an item number for a write that names an
  item (comment, review, edit, reviewers, close, reopen, merge, update),
  else the repository alone.
- **Commit order, one at a time.** An entry runs when no earlier entry of
  its lane remains. An uncertain entry holds its lane until found or
  settled, so a later write cannot overtake one that may still land.
- **Withdrawal.** `Withdraw { entry }` works only for an entry with no
  attempt. A call already out ends it `Withdrawn` at its answer.

### 7.3 Recovery classes

A class says what the forge lets the client do after a lost answer. It is
the forge's guarantee, not the owner's choice, and a client's check never
changes it, because a late copy can pass the check. **Keyed:** the forge
refuses a second creation with the same identity (no Forgejo write is).
**Conditional:** it checks the state the write names. **Idempotent:**
repeating gives the same state. **Unrecoverable:** none of these.

`recovery(&Write)` gives the class. A lost write is found again by:

| Write | Class | Found by |
|---|---|---|
| open a pull request | Conditional | the open pull of its branches |
| merge | Conditional | its pull merged at its exact head |
| update from base | Unrecoverable | its head containing the base |
| create an issue, comment, review | Unrecoverable | its key since start |
| create a branch | Unrecoverable | the branch at that commit |
| delete a branch | Idempotent | the branch gone |
| edit, reviewers, close, reopen, status | Idempotent | not looked for |

A branch can be deleted by another hand and nothing then shows it was
made; Forgejo's update checks neither the old head nor an operation id.
Of several pulls for its branches, the open one is found, else the
oldest. A row settles a find only if it is original (not revised,
provenance known) and carries the entry's key, believed (section 7.1).

### 7.4 One write's way

1. **Clock.** A new entry reads the forge's time (an empty listing, a
   `Fresh` read) as its start time.
2. **Check.** Only a write carrying a `Condition` (a merge, an update or
   a review) reads its pull request afresh; any other goes to step 3. A
   closed pull request fails `Closed`; another head or base fails `Stale`.
3. **Make.** In the decision that sends the call the client emits
   `Progress` with the entry's `start` (a `Position`, fixed once) and its
   `attempt`: the send, its monotonic `deadline` and its absolute
   `expires`, both `lifetime + clock_margin` after it. The owner commits
   it first (section 2.1).
4. **Answer.** A typed answer ends in `Outcome::Made { made, found }`.
   A review, merge and update are proved by a read after the answer: the
   review by its key, the merge by its pull request merged at the entry's
   head and base, the update by a comparison showing the head holds the
   base. A merge not so proved is `Raced { Merged, Stale }`; an update
   not so proved is not found (section 7.5).
5. **A lost answer.** `Timeout` or `InvalidAnswer` on a write may have
   reached the forge: the entry counts a failure, emits `Progress` and
   `Outcome::Uncertain`, and is looked for (section 7.5). `Exists` after
   an attempt is looked for too; before one it fails `Exists`.
6. **A typed refusal** (conflict, protected branch, stale head, `Closed`
   and the like) is final, never retried blind. `Unavailable` did not
   reach the forge: it is tried again after `backoff` doubling to
   `backoff_max` plus jitter, up to `write_attempts` failures.
7. **Missing.** Deleting a branch already gone is made (`Set`). A find
   that meets `Missing` is a find not found; a set that meets it fails.

### 7.5 Finding again

`lifetime` is the longest a sent write may still arrive. A find does not
restart the saved deadline.

- **Found:** `Outcome::Made { found: true }`.
- **Not found before the deadline:** the entry waits for it, then looks
  once more. A find that fails is tried again after `backoff`, with no
  ceiling while the entry is uncertain.
- **Not found after it:** an unrecoverable write is `Outcome::Held`. The
  owner holds its task for a person; the client never sends it again by
  itself. `Remake`, a person's choice, hands the entry back with that rule
  lifted. Any other write, or one remade, fails `Timeout` after
  `write_attempts` failures; else it is checked and made under a new
  attempt.
- **A set** (edit, reviewers, close, reopen, status) is not looked for. It
  waits out the first attempt's lifetime and is written again, so a late
  copy cannot land after the second.
- **Raced.** `Outcome::Raced { made, why }`: a known effect landed and its
  intended state cannot be confirmed, as when another hand closed the
  pull request, or a merge's pull request shows another base, head or
  merge commit.

### 7.6 Echoes

A comment or review the owner wrote comes back through the next scan, and
is not news. `Made::Commented` and `Made::Reviewed` add the new id to the
item's ledger (at most `rows`) until the scan passes it. A row is an echo
too when a configured writer wrote it and its key is an outbox entry's or
has version 1 and the owner's namespace. A keyed write waits for room in
the ledger, and its landing brings the item's next read forward to
`hinted`.

## 8. Restart

After a restart the owner gives the client its rows, then has it read
before it writes.

1. **Restore.** The namespace is given first (section 2).
   `Restore { record }` for each stored row, then `Restored { clock }`.
   A row that breaks a limit or is inconsistent (a
   bad page or stamps, a cache or echoes over bounds, a live row without
   its repository's, or the reverse) makes `restoration_failed()` true.
   The owner reconciles its store with its limits; meanwhile `Keep` is
   `Busy`.
2. **The recovery clock** (`clock`): `Monotonic` while the saved origin
   survives, else `Wall`, when a find's remaining lifetime is the saved
   `expires` less the wall clock.
3. **Prepare.** The owner tells `Writer` for the branches its writers
   hold, sends `PauseOutbox`, and hands every stored entry not held for a
   person to `Make`. An entry with an attempt starts at its find.
4. **Read afresh.** `ReadAfresh` reads each repository's listing from its
   stored mark, inclusive, and each live resource, all as `Fresh`, so what
   changed while it was down is read once. `ReadAfreshDone` ends it.
5. **Settle.** `SettleOutbox` lets the entries run. Each with an attempt
   is found, or waits for its saved deadline (section 7.5); `OutboxDone`
   comes when each has been looked for once.

## 9. What it never knows

- **What an effect is for,** or which effects exist: the owner chooses
  the writes and their identities.
- **Policy:** gates, merges, landings, grants and subscribers.
- **The owner's outbox.** The client holds execution copies of the
  entries it is handed, and tasks only as numbers it echoes back.
- **What people see,** credentials, connections and webhook secrets.

## 10. Verification

- **Step tests,** in the crate, hold each rule above: the budget's
  order, reserve, refunds and rate refusal; the listing's ties and
  overlap; drift against the owner's pushes; the start and attempt saved
  before the first write; original deadlines across a restart and a
  changed clock; copied, revised and unknown rows never settling; golden
  bytes for keys; every table full at once against `worst_case`.
- **The client world** (testing-strategy.md, section 2.2) runs the client
  alone under a durable scripted parent that commits a decision whole
  before it releases any call, over skein's fake forge domain
  (fake-forge.md, section 2) through small total translations. A seeded
  loop injects faults, late copies of lost writes, restarts and rate
  resets. Its stories: creations survive faults, late landings and
  restart; an uncertain write is found without a second; a created
  branch deleted before its find is held after its deadline; a
  conditional merge lands once; a late set reaches its final state; a
  branch name taken by another writer is reported, not overwritten;
  every fresh read finishes once; no comment or review is delivered twice.
- **Its referee** reads no client state. It checks that every write
  released was planned by the parent and had its start and attempt saved
  first, that a keyed creation happens once on the fake, that news is
  delivered once for each id and revision, and that a seed replays, the
  diagnostics changing no decision.
- **The protocol world** joins the client, the component and the fake
  forge's protocol layer (forge-connection.md, section 10). The owner's
  connector worlds and cost ceilings are the owner's.

## 11. Open questions

- **A keyed class in use.** No Forgejo write is `Keyed`. A dialect with a
  deduplicating key would move a creation there; GitHub's design decides
  (forge.md, section 10). Default: every Forgejo write keeps the class
  section 7.3 gives it, and `Keyed` stays unused.
- **Where `recovery` lives.** Forgejo's classes sit in the client. A
  second dialect may carry its own (forge.md, section 4), the client then
  taking the class as a value. Default: the classes stay in the client
  until a second dialect is built.

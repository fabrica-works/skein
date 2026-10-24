# The landing flow

Provisional, 2026-10-24, proposed with the user and not built yet. How
changes reach `main` across skein, smith, jig, hearth and temper:
- one set of branches per milestone;
- a landing queue that is the only writer to `main`;
- a keeper that runs the queue.

It has two goals. It gets the most out of the shared build cache
([build-cache.md](build-cache.md)), and it leaves owners nothing to
coordinate with each other but `main`. Today's `heavy` and `land` are
[build-infrastructure.md](build-infrastructure.md); missions and
milestones are `docs/foundation/owners.md`.

## 1. In one page

- **The invariant.** Every repository's `main` pins the current `main`
  of each of its upstreams. Only a landing moves a `main`, and a landing
  keeps the invariant: it moves every repository it touches, and repins
  every repository downstream of them, all at once (section 2).
- **A set of branches per milestone.** A milestone is built on a set of
  branches, one in each repository it reaches, and the set keeps the
  same invariant inside it (section 3).
- **One landing per milestone.** At a milestone's end the owner rebases
  its set on the current mains, makes it green and asks the queue to
  land it. Tasks inside a milestone merge into the set's branches, not
  into `main`.
- **The queue is `main`'s only writer** (section 4).
  - If `main` moved since the set was rebased, the queue rebases it
    again and runs the gate of every repository the set reaches.
  - Then it lands the whole set, or sends it back to its owner.
  - A set sent back and returned goes to the front of the queue.
- **A keeper runs the queue** (section 5). It is a Codex session that
  orders the owners' requests, runs the queue's tool and talks to
  owners. Directors and sub-directors take no part in merging: they get
  news of what landed, and escalations when merging goes wrong (the
  user, 10-24).
- **Why per milestone** (section 6):
  - **The cache:** a change is compiled once for the fleet, and two
    versions of the same crates coexist for at most a milestone.
  - **Coordination:** owners neither wait on nor relay for each other.
  - **QA:** the lab takes each milestone as soon as it lands, while the
    next one is being built (the user).
- **What goes away:**
  - repin slots, and the director's calls on repin and landing order;
  - landing hashes relayed between owners;
  - code passed from one owner to another;
  - failures routed by hand after they reach `main`.

## 2. The invariant

The repositories depend on each other in one direction: skein, then
smith, then jig, then temper; hearth depends on skein alone. Each names
its upstreams by git URL with no revision, and its `Cargo.lock` pins a
commit of each.

- **On `main`,** each lock pins every upstream's current `main` head.
  jig is then tested against the same skein that temper builds it
  with. Cargo builds one revision per git source, so temper's lock
  decides that revision.
- **A landing keeps it.** A landing that changes skein also repins
  smith, hearth, jig and temper to the new skein, and lands every one
  of them, after their gates pass. A temper-only landing touches
  temper alone.
- **It is affordable because of the patch.** Before it, every pin move
  rebuilt everything downstream with no cache hits (build-cache.md 3).
  Now a pin move compiles only what actually changed upstream and the
  crates built on it, and restores the rest.

## 3. Working in a set

- **The set.** A milestone's set has a branch in every repository the
  milestone edits. It also has a pin-only branch in every repository
  downstream of the most upstream one it edits.
  - A jig milestone has jig and temper.
  - A skein milestone has all five repositories.
  - Branches are named `<mission>/m<k>`, such as `accounts-move/m4`.
- **Why pin-only branches.** The invariant will make the landing repin
  them anyway, so their breakage is the milestone's to fix, and the
  sooner it shows the better.
  - Example: skein `55f46fc6` added a field to `Shutdown`, and the
    smith pinned at `d9a67aca` stopped compiling.
  - In a set, that break shows at the owner's first repin, and the
    smith fix travels in the same set as the skein change.
- **The base and the tuple.** The set records its base: the `main` of
  each repository it was cut from or last rebased on.
  - Inside the set, each branch pins the set's own branch of an upstream
    where there is one, and the base `main` otherwise.
  - One tuple of commits therefore holds across the whole set, just as
    one holds across the mains.
- **One pin mechanism.** Pins inside a set are `Cargo.lock` commits,
  like `main`'s, moved with `scripts/dev/repin`, never with `[patch]`
  overrides to local paths.
  - The kache patch normalizes Cargo's git checkouts, not arbitrary
    paths. A build from a path would key differently, and the queue
    could not reuse the owner's builds. This follows from the patch's
    design; it was not measured.
- **Tasks.** Implementers branch from the set's branch in their
  repository and merge back into it, after their focused tests through
  `heavy`. An upstream task that downstream work needs merges first,
  and the owner then repins the downstream branches of its own set. No
  one outside the set is involved.
- **Rebasing during the milestone** is the owner's choice, when it needs
  something that has just landed. It follows the same steps as at the
  end.
- **At the milestone's end:**
  1. Rebase every branch of the set on its repository's current `main`.
     That gives a new base. The owner resolves conflicts.
     - A `Cargo.lock` conflict is never merged by hand: take `main`'s
       lock and repin to the set's tuple.
  2. Run the gate of every repository in the set through `heavy
     --gate`. These builds publish to the shared store.
  3. Push the set's branches and ask the queue to land it.

## 4. The queue

The queue is the only writer to every repository's `main`. It takes one
set at a time, in the keeper's order.

1. **Unchanged base.** If each repository's `main` is still the set's
   base, and the set carries a valid gate receipt for each candidate
   tree, the set lands without a new gate.
   - A receipt already names the tree it proved (`Gate-Tree`), and the
     tree includes the lock, and with it the tuple.
2. **Moved base.** Otherwise the queue rebases each branch on its
   current `main`, repins the set and runs every gate, upstream first.
   - A conflict sends the set back, unless it is mechanical: a
     `Cargo.lock` conflict, or a generated file that has a generator.
   - With the cache, only the set's own changes and the crates built on
     them compile here (section 6.1).
3. **Green.** The queue pushes the mains upstream first: skein, then
   smith and hearth, then jig, then temper.
   - Each push is a fast-forward and is journaled, so a crash between
     pushes resumes where it stopped.
   - No push can be refused, because nothing else writes to `main`.
   - The landing goes into the landings feed.
4. **Red.** The set goes back to its owner with the failure: the
   repository, the test or build step, the log and the receipt.
   - A failure that passes on one rerun is a flake. The set lands, and
     the flake becomes a finding.
5. **Returned sets go first.** A set sent back and returned is next in
   line, after the landing that is running.
6. **Docs sets.** Markdown-only changes (`land --docs` today) go through
   the queue too, with no gate, so the queue stays `main`'s only writer.
   They take seconds.
7. **After landing.** The slow tiers that run after a landing still do.
   A failure goes to the set's owner through the keeper.

Gates run on whichever build machine is free, through `heavy --gate`,
and publish to the store. The queue takes over from today's landing
turn (build-infrastructure.md 8). Speculative runs come later, when the
queue backs up: testing set B on top of set A while A's gates run, and
batching several sets.

## 5. The keeper

The keeper is a Codex session (GPT Sol, medium or high effort), the
user's choice on 10-24. Who does what is still the user's to settle.
The split below is a proposal.

- **The tool** (`queue`, deterministic and journaled) does the
  mechanics:
  - rebases, repins and gates;
  - checks receipts, pushes and resumes;
  - writes the landings feed.

  It is tested in fixtures, like `heavy` and `land`.
- **The keeper** brings the judgement:
  - it reads requests and orders them: first come, first served, with
    returned sets first and docs sets batched;
  - it runs the tool;
  - it classifies failures (conflict, build, test, flake,
    infrastructure);
  - it reruns a suspected flake once;
  - it resolves mechanical conflicts only, and sends a set back with
    its evidence;
  - it tells each owner when its set lands or comes back.

  Keeping the mechanics in a tool keeps the keeper cheap and the
  mechanics testable.
- **Owners** reach the keeper only through requests and returns. A
  request names the set, its branches and heads, its base and its
  receipts.
- **Directors and sub-directors** read the landings feed. They hear
  from the keeper only in these cases:
  - an infrastructure fault (a machine, the store, GitHub);
  - a set sent back a third time;
  - two sets that keep breaking each other;
  - an owner that doesn't answer.

  They don't order landings, and they don't relay them.

## 6. Why this shape

### 6.1 For the cache

- **Pin moves are cheap now.** With the patch, a repin compiles what
  changed upstream and the crates built on it, and restores the rest.
  The trial's pin move reached 5 changed units and 17 built on them out
  of about 700 (build-cache.md 6).
  - Before the patch, a pin move rebuilt everything downstream. That
    cost is what made repins something to batch and schedule.
  - The invariant, which repins on every upstream landing, is only
    affordable because of the patch.
- **Each tree is built once for the fleet.**
  - The owner's gates at the milestone's end publish their builds.
  - If `main` hasn't moved, the queue's build of the same tree is all
    hits, and the receipt saves even the tests.
  - Once a set lands, every other owner's next rebase restores the
    queue's builds of the new `main` instead of compiling them.
- **Divergence stays short.** While a set is unlanded, every crate
  downstream of its upstream changes exists in two versions: the set's
  and `main`'s.
  - Each version is built, and rebuilt at each rebase, by its own users,
    and neither can reuse the other.
  - Landing per milestone bounds that window by a milestone, which is
    capped at 4 hours. Landing per mission would stretch it to the
    mission, about 16 hours.
- **The cost of a change follows its ripple** (build-cache.md 2). A
  change deep in skein is the most expensive kind. Landed, its ripple is
  built once by the queue and then shared; held on a set, it is built by
  that set's machines alone. The keeper never needs to know this; it
  follows from landing early.

### 6.2 For coordination

On 10-23 most of the director's traffic was ordering and relaying:
- about 49 log entries on repin slots and landing order since the
  sub-director pilot began;
- 156 messages from the director, and 301 from one sub-director.

Their kinds were landing hashes, code passed between owners, failures
routed after they reached `main`, and interface facts. Their causes:
- repins were done by whichever owner needed a newer upstream;
- order mattered, and only the director could decide it;
- failures surfaced after landing, on someone else's work.

In this flow:
- **An owner's only interface to other owners is `main`.**
  - It never waits on another owner's landing: it rebases on whatever
    `main` holds at its milestone's end.
  - It repins for no one.
  - It carries no one else's code: an upstream break lands with its
    downstream fix, in one set.
- **Failures are caught before `main`,** and go back to the set that
  caused them.
- **Conflicts belong to the later set,** which resolves them at its
  rebase or when it comes back.
- **The only order is the queue's,** and the keeper sets it.
- **What's left for the directors:** dispatching work, policy, machines
  and escalations.

Some things stay shared, and the queue can't fix them, which is why
grouping missions by coupling still matters:
- version counters, frozen until the first release (codec.md 5);
- shared test-time budgets;
- hot files. The temper memory figures are being split into one file
  per term.

### 6.3 For QA and pipelining

Each milestone works on its own (owners.md 3.1). As soon as it lands,
the QA lab can take it from `main`, while its owner starts the next
milestone on the new mains. Landing per mission would leave the lab idle
for the length of a mission, then flood it with four milestones at once.

### 6.4 Why not per task, or per mission

- **Per task**, today's roughly 400 landings a day. Under the invariant
  that came to about 38 hours a day of serial queue work (the 10-23
  estimate, with warm caches): more than one queue can do.
- **Per mission:**
  - conflicts between missions surface at the end;
  - a set sent back carries up to four milestones of work;
  - an upstream change stays hidden from other missions for most of a
    day;
  - QA is idle, then flooded;
  - divergence lasts longest (6.1).
- **Per milestone:** about 8 to 12 hours a day of queue work, by the
  same estimate, before the cache's savings and before reused receipts.
  It fits, with room to spare.

## 7. Costs and risks

- **Throughput.** The queue is serial, and a skein set runs five gates.
  The levers, in order:
  1. reused receipts;
  2. speculative runs;
  3. batching docs sets;
  4. tests selected by the crates a set changed (nextest's `rdeps()`).

  Measure before adding any of them (section 9).
- **Flakes are everyone's problem.** One flaky temper test sends a
  skein set back. Hence the single rerun, and the finding it files.
- **Wide sets.** A milestone that reaches all five repositories runs
  five gates. Plans should keep sets narrow; the footprint in the
  owner's notes shows when they can't.
- **Later visibility.** Other missions see a milestone's work only when
  it lands, at most a milestone later.
- **One point of failure.** While the queue is down nothing lands, but
  owners keep working on their sets.

## 8. What changes when it goes live

- **owners.md 3.1:** a milestone ends when its set has landed through the
  queue.
- **owners.md 4.4:** tasks merge into the set's branches. The owner
  requests the milestone's landing, and the queue lands it.
- **owners.md 4.5:**
  - Repins happen inside the set, by its owner, with `scripts/dev/repin`.
  - "Breaking upstream" waits for no one's word: the break and its
    downstream fix land together.
  - The footprint stays, to plan narrow sets and to group missions, but
    no longer orders landings.
- **director.md:**
  - The ordering of landings and repins, and the relaying of hashes,
    leave the director and the sub-directors.
  - The landings feed replaces landing news.
  - The keeper's escalations come to the director.

## 9. Building it

1. **The shared store on every build machine** (build-cache.md 7),
   under way.
2. **The queue's tool and the keeper,** as a build-infra mission. The
   first version:
   - serial, with sets across repositories;
   - reused receipts, automatic rebases, and mechanical lock and
     generated-file resolution;
   - returned sets first, and docs sets;
   - journaled pushes and the landings feed.

   Fixtures cover it as they cover `heavy`.
3. **`scripts/dev/repin` learns sets:** one command moves every branch of
   a set to its tuple.
4. **A trial** with two or three missions, then everyone. owners.md and
   director.md change then.
5. **Later:** speculative runs, tests selected by affected crates, and
   reuse of test results for byte-identical test binaries.

## 10. Open

- **Who does what** among the keeper, the tool and the owners (the
  user: to be settled). Section 5 is a proposal.
- **Where the queue runs,** and how owners on other machines send their
  requests. Options include git refs plus a request record over ssh,
  as the landing turn does today.
- **The queue's throughput,** measured: replay 10-23's milestones
  through it with the cache.
- **Whether owners' receipts are trusted** to land an unchanged base
  without a new gate (gate-policy.md).
- **The thresholds for escalation:** how many times a set may be sent
  back, and how long an owner may stay silent.

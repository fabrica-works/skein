# The landing flow

Provisional, 2026-10-24, proposed with the user. Live in part since
10-24: `scripts/dev/land-set` is its starting primitive, and agents do
the rest by hand until it is built (section 9.1). How changes reach
`main` across skein, smith, jig, hearth and temper:
- one set of branches per milestone;
- a landing queue that is the only writer to `main`;
- a tool that runs the queue, and a keeper that watches it.

It has two goals. It gets the most out of the shared build cache
([build-cache.md](build-cache.md)), and it leaves owners nothing to
coordinate with each other but `main`. Today's `heavy` and `land` are
[build-infrastructure.md](../design/build-infrastructure.md); missions
and milestones are [owners.md](owners.md).

## 1. In one page

- **Where it stands (10-24).** Only `land-set` is built: it lands a set
  under one lock, upstream first. Agents do by hand what the rest of
  this flow will do, as section 9.1 lists, and the tools come over time.
- **The invariant.** Every repository's `main` pins the current `main`
  of each of its upstreams. Only a landing moves a `main`, and a landing
  keeps the invariant: it moves every repository it touches, and repins
  every repository downstream of them, all at once (section 2).
- **A set of branches per milestone.** A milestone is built on a set of
  branches, one in each repository it reaches, and the set keeps the
  same invariant inside it (section 3).
- **One landing per milestone.** At a milestone's end the set is
  rebased on the current mains and made green, and the owner asks the
  queue to land it. Tasks inside a milestone merge into the set's
  branches, not into `main`.
- **The next milestone doesn't wait for the landing** (the user,
  10-24). It starts as soon as the set is in the queue, on top of it.
  - If the landing fails, the set is repaired beside the new
    milestone, as a QA pass runs today.
  - Once the earlier milestone has landed, the new one rebases on
    `main` when its owner chooses (section 3).
- **Rebases, repairs and QA fixes are implementer tasks** (the user,
  10-24). Codex (GPT Sol, high effort) does them, and escalates to the
  milestone's Claude owner or sub-owner as any implementer does. The
  owner keeps the judgement: when to rebase, which findings to fix, and
  the check of each result (section 3).
- **The queue is `main`'s only writer** (section 4).
  - If `main` moved since the set was rebased, the queue rebases it
    again and runs the gate of every repository the set reaches.
  - Then it lands the whole set, or sends it back to its owner.
  - Sets that need no gate go first, then returned sets, then the rest
    as they came.
- **A tool runs the queue, and a keeper watches it** (section 5).
  - The tool does the mechanics, the order's fixed rules included.
  - The keeper is a Codex session, woken only for what needs judgement.
    It reads failures and writes the owner a note, overrides the order
    when it must, and escalates.
  - Directors and sub-directors take no part in merging: they get news
    of what landed, and escalations when merging goes wrong (the user,
    10-24).
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
- **Rebases, repairs and QA fixes are tasks too** (the user, 10-24).
  Each goes to a Codex implementer, GPT Sol at high effort, briefed and
  launched like any other task (owners.md 4.2, 4.3):
  - every rebase of the set, with its conflicts and the gates after it:
    during the milestone, at its end, and a stacked milestone's onto
    `main` (below);
  - the repair of a set the queue sent back (section 4);
  - the fixes for the findings of the owner's QA pass that triage lets
    in (owners.md 4.6).

  None of them needs taste once the design is settled, so they go where
  other work without taste goes (owners.md 1). The milestone's owner,
  or its sub-owner, keeps the judgement: when to rebase, which findings
  to fix, the task's brief, the check of each diff (owners.md 4.4) and
  the request to the queue.
  - **Escalation** follows the implementers' rules (owners.md 4.3, 5):
    on a doubt the implementer stops, writes the question in its
    owner's `inbox/` and ends, and it never improvises across a
    boundary. A conflict that needs a call on whose intent wins is
    such a doubt.
  - Where the work gets hard, Opus advises the owner, as for any task
    (owners.md 4.2).
- **Rebasing during the milestone** is the owner's choice, when it needs
  something that has just landed. It follows the same steps as at the
  end.
- **At the milestone's end:**
  1. Rebase every branch of the set on its repository's current `main`.
     That gives a new base.
     - A `Cargo.lock` conflict is never merged by hand: take `main`'s
       lock and repin to the set's tuple.
  2. Run the gate of every repository in the set through `heavy
     --gate`. These builds publish to the shared store.
  3. Push the set's branches and ask the queue to land it.
  4. Start the next milestone at once (below).

  Steps 1 and 2 are one implementer task (above). The owner checks its
  result, then takes steps 3 and 4.
- **The next milestone stacks on the queued set.** It doesn't wait for
  the landing, just as it doesn't wait for a QA pass (owners.md 3.1).
  - **Where it starts.** Its branches start from the queued set's heads,
    since `main` doesn't hold the earlier milestone yet. Those heads are
    recorded as its fork point. A repository the earlier set didn't
    reach starts from `main`.
  - **Its builds start warm,** because the queued set's gates published
    every unit of that tuple.
  - **Repairs.** The ended milestone's sub-owner stays on for its
    landing, as it stays on for QA. If the queue sends the set back, the
    sub-owner briefs an implementer with the failure, and the repair
    loop runs beside the new milestone: repair, rebase and gates by the
    implementer, then the sub-owner's check and a new request. A
    returned set goes first (section 4).
  - **No early queueing.** The new milestone can't enter the queue
    before the earlier one has landed, since its base would hold
    unlanded work.
  - **Rebasing on `main`.** Once the earlier milestone has landed, the
    new one rebases when its owner chooses. An implementer moves only
    the new milestone's commits onto `main` (`git rebase --onto main
    <fork point>` in each repository).
    - The earlier milestone may have changed on its way in, rebased by
      the queue or repaired. If it landed as queued, the tuple is
      unchanged and the rebase rebuilds nothing.
    - The latest moment is the new milestone's own end, which rebases
      anyway.

## 4. The queue

The queue is the only writer to every repository's `main`. Its tool,
`queue`, runs on hetzner3 (section 5) and takes one set at a time.

1. **The request.** The owner pushes the set's branches and runs `queue
   request` with a set file: the set's name, each repository's branch,
   head and base, and its gate receipts. The tool refuses at once, with
   the reason, a request with any of these:
   - a head that isn't pushed;
   - a base that isn't on its repository's `main`, which also refuses a
     stacked milestone queued before the earlier one has landed;
   - a missing pin-only branch, or a lock that doesn't pin the set's
     tuple;
   - a receipt that doesn't check out (step 3).
2. **The order.** It follows fixed rules, which the keeper may override
   (section 5):
   1. sets that need no gate: docs sets, and sets whose base is still
      current and whose receipts check out. Each takes seconds, and
      landing them early saves their gates;
   2. returned sets;
   3. the rest, first come, first served.
3. **Unchanged base.** If each repository's `main` is still the set's
   base, and each candidate tree has a receipt that checks out, the set
   lands without a new gate.
   - The tool reads heavy's receipt on the machine that ran the gate.
     It checks it as `land` does (gate-policy.md 3.1): the tree, the
     policy, the outcome and the log's digest.
   - The tree includes the lock, and with it the tuple.
   - Receipts are checked, never taken on an owner's word.
4. **Moved base.** Otherwise the tool rebases each branch on its
   current `main`, repins the set and runs every gate.
   - It pushes the rebased branches under new names, and never forces a
     push.
   - It resolves only mechanical conflicts: a `Cargo.lock` conflict
     (take `main`'s lock, repin to the tuple), or a generated file that
     has a generator. Any other conflict sends the set back, for the
     owner's implementer to resolve (section 3).
   - Gates run through `heavy --gate` on a free build machine, upstream
     first. A failing skein gate stops the set before four more run, and
     each downstream gate restores its upstream's builds from the store.
   - With the cache, only the set's own changes and the crates built on
     them compile here (section 6.1).
5. **Green.** The tool pushes the mains upstream first: skein, then
   smith and hearth, then jig, then temper.
   - Each push is a fast-forward and is journaled, so a crash between
     pushes resumes where it stopped.
   - No push can be refused, because nothing else writes to `main`.
   - The gate evidence goes in a git note on each landed commit, not in
     a trailer. Amending skein's head after temper's lock pinned it
     would leave temper pinning a commit that `main` never holds.
   - The landing goes into the landings feed.
6. **Red.** The tool reruns the tests that failed, alone, once.
   - If they pass, it was a flake. The set lands, and the keeper files
     the flake as a finding.
   - If not, the set is parked and the next one starts. The keeper
     reads the failure and returns the set with its note (section 5).
   - A fault of the infrastructure is not a red: heavy refusing the
     run, or a machine or GitHub out of reach. The tool retries it, and
     wakes the keeper if it lasts.
7. **Docs sets.** Markdown-only changes go through the queue too, with
   no gate, so the queue stays `main`'s only writer.
8. **After landing.** The slow tiers that run after a landing still do.
   A failure goes to the set's owner with the keeper's note.

The queue replaces today's advisory landing turn
(build-infrastructure.md 8). Speculative runs come later, when the
queue backs up: testing set B on top of set A while A's gates run, and
batching several sets.

## 5. Who does what

Mechanics go to the tool, and judging a failure to the keeper. All code
goes to the owners' implementers, and decisions about the work stay
with owners and directors (the user, 10-24). The keeper never edits
code, and never decides whether a set lands: the gates do.

- **The tool,** `queue`, is a hand-rolled script like `land` and
  `repin`. It is deterministic and journaled, and runs as a user
  systemd service on hetzner3.
  - hetzner3 is already the landing turn's coordinator and hosts the
    store. Clients reach it over the landing turn's ssh
    (build-infrastructure.md 8).
  - It does section 4's mechanics: checking requests and receipts, the
    order, rebases, repins, mechanical conflicts, gates, the rerun,
    pushes, the journal and the feed.
  - It is tested in fixtures, like `heavy` and `land`.
- **The keeper** is a Codex session (GPT Sol, high effort). The tool
  starts it fresh for each event that needs judgement, and it is never
  left running. The events:
  - a red result that survives its rerun;
  - an infrastructure fault the tool can't retry past;
  - a note from an owner;
  - an owner's silence.

  Such events should be few, and a wrong reading costs an owner a
  repair round, hence high effort. Its memory is the queue's journal
  and a notes file. It acts only through the tool's commands: return a
  set with a note, hold or move a set, file a finding, escalate. The
  queue doesn't wait for it.
  - **The return note** names the repository, the step and the test,
    where the log is, the likely cause and the commits behind it. It
    says whether the fault looks like the set's, `main`'s, a flake's or
    the infrastructure's.
  - **Overrides of the order** are rare, and each goes in the feed with
    its reason: two sets that keep breaking each other, or an owner's
    note asking to wait.
- **Owners** reach the queue through three commands, and need no
  messages:
  - `queue request <set file>` (section 4);
  - `queue wait <id>`, on the owner's detached route. It exits when the
    set lands, comes back with the note's path, or is escalated, and its
    exit wakes the owner, on any machine. A sub-owner that stays on for
    the landing then runs `queue wait --after <id>` for the
    after-landing tiers;
  - `queue note <id> "..."`, which wakes the keeper: a large repair
    under way, or a failure the owner reads as the infrastructure's.

  A returned set goes to the sub-owner of the milestone that made it,
  who has an implementer repair it (section 3) while the mission's next
  milestone goes on.
- **`land` stays, as a front end.** A one-repository landing becomes a
  one-branch request, and the tool adds the downstream pin-only branches
  the invariant needs. `land --docs` becomes a docs request. Habits and
  scripts keep working.
- **Directors and sub-directors** read the landings feed. The keeper
  writes to the director's inbox only for:
  - a set sent back a third time;
  - a returned set with no new request or note from its owner after 2
    hours;
  - two sets that keep breaking each other;
  - an infrastructure fault that lasts 15 minutes: a machine, or
    GitHub. The store going down isn't one, since kache then compiles
    locally;
  - a flake, as a finding for the director to assign.

  They don't order landings, and they don't relay them.
- **Building and running it.** The build-infra mission, with a Codex
  owner, builds the tool and writes the keeper's brief. The director
  runs the service.

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
- **A stacked milestone starts warm.** The next milestone builds on the
  queued set's tuple, which that set's gates have already published. If
  that set lands unchanged, the rebase onto `main` leaves the tuple as
  it was and rebuilds nothing.
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
- **The only order is the queue's,** set by fixed rules and, rarely,
  the keeper.
- **What's left for the directors:** dispatching work, policy, machines
  and escalations.

Some things stay shared, and the queue can't fix them, which is why
grouping missions by coupling still matters:
- version counters, frozen until the first release (codec.md 5);
- shared test-time budgets;
- hot files. The temper memory figures are being split into one file
  per term.

### 6.3 For QA and pipelining

Each milestone works on its own (owners.md 3.1), and three things now
run side by side:
- **The next milestone,** which starts when the set is queued, not when
  it lands, so time in the queue never idles an owner.
- **The owner's QA pass, its fixes and any repair loop,** all on the
  ended milestone: implementer tasks under its sub-owner.
- **The QA lab,** which takes the milestone from `main` as soon as it
  lands.

Landing per mission would leave the lab idle for the length of a
mission, then flood it with four milestones at once.

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
- **Building on a set that fails.** If the earlier milestone needs a
  large repair, the next one has built on code that is about to change,
  and its rebase absorbs the change. It is the mission's own code, so
  no one else is involved.
- **Conflicts resolved by Codex.** A rebase's resolution may keep the
  code compiling but lose one side's intent. The gates catch most of
  it; the owner's check of the diff and the implementer's escalation on
  a doubt catch the rest.
- **One point of failure.** While the queue is down nothing lands, but
  owners keep working on their sets.

## 8. What changes when it goes live

- **owners.md 3.1:**
  - A milestone hands over when its set is queued, and the next
    milestone starts then.
  - It ends when the set has landed. Its 4-hour cap runs to the
    request. The repair loop after it doesn't count against the cap
    (the user, 10-24); the keeper's escalations bound it.
  - Its sub-owner stays on for the landing as well as for QA, and
    briefs and checks the implementers that repair the set and fix the
    pass's findings.
- **owners.md 4.2:** rebases, repairs and QA fixes are implementer
  tasks at high effort, which the launcher doesn't lower.
- **owners.md 4.4:** tasks merge into the set's branches. The owner
  checks the end-of-milestone rebase as it checks any task's diff,
  requests the milestone's landing with `queue request`, and waits on
  it with `queue wait`. The queue lands it.
- **owners.md 4.5:**
  - Repins happen inside the set, by its owner, with `scripts/dev/repin`.
  - "Breaking upstream" waits for no one's word: the break and its
    downstream fix land together.
  - The footprint stays, to plan narrow sets and to group missions, but
    no longer orders landings.
- **owners.md 4.6:** a QA pass may start on the queued set. Its fixes
  are implementer tasks, and land as their own small set once the
  milestone has landed.
- **director.md:**
  - The ordering of landings and repins, and the relaying of hashes,
    leave the director and the sub-directors.
  - The landings feed replaces landing news.
  - The keeper's escalations come to the director.
  - The director runs the queue's service.
- **gate-policy.md 3 and build-infrastructure.md 8:** gate evidence
  moves from trailers to git notes, and the queue replaces the advisory
  landing turn.

## 9. Building it

1. **The shared store on every build machine** (build-cache.md 7),
   under way.
2. **The queue's tool and the keeper,** as a build-infra mission
   (section 5). The first version:
   - serial, with sets across repositories;
   - request and receipt checks, the order's rules, automatic rebases,
     and mechanical lock and generated-file resolution;
   - the rerun, journaled pushes, git-note evidence and the landings
     feed;
   - `queue wait` and `queue note`, and `land` as a front end;
   - the keeper's brief.

   Fixtures cover it as they cover `heavy`.
3. **`scripts/dev/repin` learns sets:** one command moves every branch of
   a set to its tuple. It also learns a stacked set's rebase: onto
   `main` from the recorded fork point, in each repository.
4. **A trial** with two or three missions, then everyone. owners.md and
   director.md change then.
5. **Later:** speculative runs, tests selected by affected crates, and
   reuse of test results for byte-identical test binaries.

### 9.1 Where it stands

Live as soon as possible, perfected over time (the user, 10-24). The
starting primitive is `scripts/dev/land-set`
(build-infrastructure.md 10), run from a skein checkout like `repin`:
- It takes one set lock on hetzner3, through the landing turn's holder.
  Sets land one at a time.
- In each repository of the set, upstream first, it rebases the branch
  on `main`, keeping `main`'s side of a `Cargo.lock` conflict, and
  repins it to what the run has just landed upstream. Then it lands it
  with the ordinary `land`.
- It refuses a set without its pin-only repositories, and makes their
  branches itself when the set file says `pin-only`.
- Each landing goes into a feed on hetzner3.

Until the rest is built, agents do by hand what the tools don't:
- **Sets and repins.** The owner keeps the set's branches. Repins inside
  the set are `heavy cargo update --precise SHA -p <package>`, since
  `repin` doesn't take a branch with commits yet.
- **The milestone's end** follows section 3. One implementer task
  rebases the set and runs every repository's gate. Then the owner runs
  `land-set` on its detached route, and starts the next milestone on the
  set's branches without waiting for it.
- **Stacking.** Once the earlier set has landed, an implementer moves
  the new milestone with `git rebase --onto main <fork point>` in each
  repository, then repins.
- **A failure.** `land-set` lands one repository at a time, so a failure
  leaves the repositories before it landed. Its summary says which. The
  owner reads the failure, which is the keeper's job later. Its
  implementer repairs the rest, and the owner runs `land-set` again: the
  repositories already landed have nothing left and are skipped.
- **Order and escalation.** The set lock is the only order. An owner
  brings to its director what the keeper would escalate (section 5).
- **Landing news.** Owners report their landings as today, until the
  directors read the feed.
- **What `land` still does.** A docs change still lands with `land
  --docs`. Code that moves skein, smith or jig goes through `land-set`,
  with its pin-only repositories; a lone temper or hearth change may use
  either.

Not built yet:
- the queue's service and order;
- gating the whole set before any push;
- reused receipts and git-note evidence (land's trailers stay);
- the rerun of failed tests, and the keeper;
- `queue wait` and `queue note`, and `land` as a front end;
- `repin` for sets and stacked rebases.

The mains don't yet all pin each other's current `main` (section 2).
`land-set` moves only the pins of what it lands, so the invariant grows
set by set.

## 10. Open

- **The queue's throughput,** measured: replay 10-23's milestones
  through it with the cache.
- **Where the queue's gates run.** A gate pauses the other builds on
  its machine (build-infrastructure.md 1). The queue's gates could
  keep to one or two machines, or take whichever is free.

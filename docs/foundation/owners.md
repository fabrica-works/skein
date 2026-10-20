# Owners

Experiment, 2026-10-19. How a draft reaches `main` through an owner:
one session that designs the draft and implements it, instead of
development.md's designer, planner, supervisor and coordinator. Runs
started before 2026-10-19 finish under development.md. Designs and plans
landed by then stay, and those not yet run may be carried out by an
owner. If the experiment fails, this file goes and development.md
stands.

**What an owner reads:** its brief, this file, and section 1, "In one
page", of [programming-model.md](programming-model.md) and
[testing-strategy.md](testing-strategy.md), the rules for the code's
shape and its tests. That page is enough to start. A later section is
read when the work touches it (a new protocol, a world's checks, a
memory bound), and [notes.md](notes.md), the reasons and open questions
behind the rules, when a rule's reason matters. The repository's
`AGENTS.md` loads on its own.
The rest of this directory (development.md, implementer.md,
supervisor.md) belongs to the runs started before 2026-10-19: an owner
needs none of it.

## 1. In one page

- **The input is a draft** that the user wrote and reviewed: the goal,
  the constraints, the expectations. A landed design or plan not yet run
  can stand in for it.
- **The director** knows every draft and the
  direction. It starts one owner per draft, gives the final call on what
  owners escalate, and asks the user only what it can't reasonably
  decide.
- **An owner** (Claude Code, Opus 5.5) designs its draft, or its part
  of one, and then implements it. A large scope is split into parts,
  each given to a sub-owner, a session like itself (section 3). Its
  effort is its parent's choice (section 2).
- **The flow:** design, report, a brief check by the parent, its green
  light, implementation with the QA passes the owner chooses, landings,
  a closing note.
- **The compass and the guardrails** are the intended design and
  architecture, and the tests and QA. The bar for tests is
  testing-strategy.md and the project's testing documents (temper's
  `docs/design/domain/testing.md`). Momentum and tokens count more than
  upfront perfection: QA (section 4.6) catches the rest, and what is wrong is
  corrected as it shows.
- **Rules and guidelines.** programming-model.md, testing-strategy.md
  and the project's testing documents are rules: they shape the code
  and its tests, and a departure needs its reason, stated in the design
  or the commit. This file is guidelines: good practice for the owner
  to apply with judgement, case by case, as the work goes, partly,
  later, or not at all where it doesn't pay. None of it is upfront work
  to complete before starting, as the old designer's and planner's was.
  What the tools enforce holds: `land` and its gate, `heavy`, the disk
  floor.

## 2. The owner's session

- **Started by its parent** in `~/src/rust` of the machine its work runs
  on, named `o/<name>`:
  `claude --bg -n o/<name> --model claude-opus-5-5 --effort <effort>
  --autocompact 150000 --dangerously-skip-permissions --settings
  '{"autoMemoryEnabled": false}' "Read <brief> and do what it says."`
  Its notes are its memory: Claude Code's auto-memory stays off, since
  in `~/src/rust` it would share the director's.
- **Its effort** is the parent's choice by the part's difficulty: xhigh,
  high or medium. The parent may raise it after the first report, when
  the output falls short.
- **An advisor.** An owner below xhigh may start an advisor, a session
  at xhigh under the same discipline (notes, compaction), and consult
  it on what it judges delicate. Sparingly: tokens count.
- **Its brief** names the input (draft sections, designs, a plan, a
  README to grow), its parent and how to reach it, its machine, its notes
  directory, and anything the parent knows that bears on it: other
  owners' footprints, budgets, dates.
- **Its notes** are in `~/src/rust/tmp/owners/<name>/`:
  - `notes.md`: the state, the decisions taken with their reasons, the
    open questions, and the footprint (section 4.5). Kept short and
    true: corrected, pruned, never only appended.
  - `ledger.md`: its own and its children's session ids, its
    implementers' sessions and Codex threads, each landing's commit, and
    the start and end times. The director computes the cost from it.
  - `inbox/`: its children's reports and questions.
- **After a compaction or a restart,** it reads its brief and its notes
  first.
- **Its context is the scarce thing.** It reads code through sub-agents
  (Sonnet, short reports) and the code graph. It ends its turn while
  implementers work, and is woken by their completion: it never polls.
  Between designing and implementing, it may write its notes and go on
  in a fresh session rather than carry the design conversation.

## 3. Designing

- **The owner is its part's designer.** It writes the design documents
  in `docs/design/` and lands them with `land --docs` (4.4), from a
  worktree on a branch. The house style:
  - a title, then a dated status line ("Provisional, 2026-10-09");
  - "1. In one page" first, with bold-led bullets, then numbered
    sections, cited as "io.md, 6.3";
  - never renumber: code cites sections by number, so new material goes
    in an existing section, a new subsection, or a new last section;
  - contracts, not history: no spikes, findings or benchmark numbers;
  - rules in the domain documents, numbers in the documents the
    repository names for them;
  - what no decision settles stays under "Open questions".
- **It settles what the draft leaves open.** It escalates to its parent
  only what changes the draft's goal, constraints or expectations, or
  would be hard to reverse: an interface between repositories, a stored
  format, a budget (testing-strategy.md, 8: raising a budget is the
  user's decision). Each question goes with its options and a
  recommendation.
- **Reviews are its own choice:** none, or a cheap sub-agent's where the
  risk warrants one.
- **Splitting.** When the scope is too large for one context, the owner
  splits it into parts with their dependencies, like a jig task tree.
  For each, it writes the input (the draft's sections, or the aggregate
  design's README that the parts grow), and starts a sub-owner with a
  brief: together for parts that stand alone, in turn for parts that
  depend on another. The sub-owner designs its part. Split only for a
  part that needs a design of its own: implementers, not sub-owners,
  make implementation parallel. A parent keeps its children on its own
  machine, where SendMessage reaches them.
- **The report.** When its design has landed, the owner writes to its
  parent, in at most 15 lines: what is designed and where, the commits,
  where it departs from the draft, what is open, and how it means to
  implement.
- **The parent's check** is brief: the report and the design's summary,
  against the draft's direction, not a review. It answers with a green
  light, or names the obvious departures to amend.

## 4. Implementing

### 4.1 The shape

After the green light, the owner implements directly what is small.
Otherwise it breaks the work into tasks, all upfront, as it goes, or
both, and prepares the next tasks while implementers work.

What the old planners learned serves its tasks, as guidelines (section
1), applied as the work goes and where they pay:
- **Settle first** what the tasks rest on: the scope, the APIs that may
  break and who repairs them, credentials and live services, the
  interface sketches.
- **Small tasks,** each landed on its own and leaving `main` coherent:
  nothing half-built in sight, no dead code. Transitional code names
  the task that removes it.
- **Upstream first:** a task that needs an upstream change waits for
  it, and repins in a commit of its own (4.5). Dependencies are named
  by capability, never by a task's number alone.
- **Open behaviour stays open** until the owner, as its designer,
  decides it. An implementer never does.
- **Each task proves itself** at the highest layer within its reach,
  and layers open as early as they can: the first joint, process or
  real-loop story as soon as the code can run it.
- **Test cost holds from the first task.** A new world costs at most
  about 1 s focused and 5 s fuzzy. A suite's first breach of its budget
  is profiled and its tests made cheaper; the budget is not raised. A
  bound over a sum of several tasks' parts is computed by the first
  task that can, and kept current. A suite that several tasks add to
  ends merged: their copies of captures, polls and helpers become one.

The one that pays most often: **the meeting point comes first.** Where two parts
meet (a client and its server, a protocol's two ends, a domain and its
store, a service and its host), one thin story runs them together, the
real thing on both sides, as soon as the far side can answer, and each
task grows it. Neither side runs ahead of the other. Where their
vocabularies differ, the translation is exhaustive from the first
types, so that drift is a build error the day it happens. A fake's
scenarios also run against the real side. A parent whose sub-owners
build the two sides orders their parts so that they meet early.

### 4.2 The implementer

The owner picks the model and effort. The default is gpt-6.1-sol at
high, through `codex exec` in the implementer's own worktree. Sonnet 5.5
or Opus 5.5 go through `claude -p` or a sub-agent: when Codex is out, or
when a task needs judgement more than typing.

### 4.3 The task

An implementer other than Opus 5.5 at xhigh reads its task literally.
Its task file is whole, and holds:
- the goal, in two or three lines;
- what to read first: the design's sections and the files, never a
  whole design, and of the rules, the sections the task touches beyond
  their one-page summaries;
- sketches of what matters: the public types and signatures with their
  names, what is internal and what crosses a boundary (a crate, a
  repository, the wire), and each test's sketch in the world the testing
  documents name;
- the files it may touch;
- done when: the tests that pass, and the gate;
- on a doubt: stop, write the question in the owner's `inbox/`, and
  end. Never improvise across a boundary.

### 4.4 Landing

The owner checks each diff before it lands, lightly and cheaply, and
not as a code review:
- it keeps to the task's sketches: the names, the signatures, the
  boundaries;
- its tests ran and pass, and that proof is in the implementer's report;
- its tests follow the testing documents and the stories and test
  sketches the task gave, in the right world.

A sub-agent may check a large one. Then the owner lands it, or tells
the implementer to, with `land`, from a clean worktree on its branch:
it rebases on `origin/main`, builds and lints, runs the repository's
gate (`scripts/check.sh`) through `heavy --gate`, records the evidence
in the tip commit's trailers, and pushes. `land --docs` lands Markdown
alone, without the gate. Its exit status 3 is a rebase stopped on
conflicts, left in progress; 4 a failed build, lint or gate. The owner
updates the repository's `docs/design/STATUS.md` rows its work changes.

### 4.5 Sharing the machines

- **Worktrees** under `tmp/owners/<name>/`, one per implementer, never
  a primary checkout. A worktree's `target/` is cleared when done with.
  Worktrees of different repositories may share a target directory,
  named by its real path in `CARGO_TARGET_DIR`, never through a
  `target` link: a test that refuses a path reached through a link
  fails on its scratch directory then. Two worktrees of one repository
  may not share one. Cargo names a workspace member's artifacts by a
  hash that leaves out the worktree's path and judges them fresh by
  their sources' mtimes, so one worktree takes another's older build
  as current: a false failure, or a false pass.
- **`heavy`** is the only way to build and test, for everyone: `heavy
  cargo nextest run -p <crate>`. It queues on pueue in two lanes:
  builds, two at a time, and gates, alone on a quiet machine.
- **Every machine's gate.** A repository's `main` must pass its gate
  on every machine that builds it, x1 the slowest. A test that fits
  its time budget only on a fast machine breaks other owners' and
  runs' landings: keep a new test well inside its budget (half of it
  is a good mark), and make a slow one cheaper rather than raise the
  budget.
- **After landing.** A tier too slow for a repository's gate runs
  after each of its code landings instead: when the landed tree has
  `scripts/after-landing.sh`, `land` starts `after-landing` in the
  background, which runs it on the landed commit through `heavy
  --gate`, in a worktree kept for the repository under
  `tmp/after-landing/`. A failure adds one line to
  `tmp/director/inbox/after-landing.md`, naming the repository, the
  commit, its branch and subject, and the log; the director sends it to
  the owner whose landing broke the tier.
- **The disk floor.** Below the machine's floor, 40 GB unless
  `tmp/machine.env` says otherwise, no new implementer starts, and
  finished targets are cleared. A target grows to about 21 GB. `heavy`
  refuses to queue below it, naming the floor, what is free and the
  ten largest targets, worktrees and caches. It also collects the
  build cache (`kache gc`) after each task and every two minutes of a
  gate, since the cache holds its limit only when collected.
- **Repins.** A repository's `Cargo.lock` pins its upstreams' git
  revisions. A repin moves a pin to the upstream's `origin/main` (`heavy
  cargo update -p <one of its packages>`; jig moves smith and skein
  together), runs `heavy cargo check --workspace --all-targets`, repairs
  what breaks, and lands with the gate. Its repairs: the pins, what an
  upstream trait now requires, names that moved, and the tests an
  upstream change of behaviour breaks, in at most about 200 lines and
  no change of design. A conflict in `Cargo.lock` is never merged by
  hand: take `origin/main`'s and update again.
- **Breaking upstream.** An upstream change that breaks a downstream
  build, and a repin of a repository others also repin, wait for the
  director's word: it orders them across owners and runs.
- **The footprint.** An owner's notes list the repositories, crates and
  protocol versions it changes, and the repins it needs. The director
  orders owners whose footprints overlap, and one repin at a time per
  repository.

### 4.6 QA passes

An owner or a sub-owner may give its part QA passes while it
implements, or decide that none is warranted. It decides only when: when
to start its tester, resume it after more has landed, stop it, and when
it is happy with what the passes have done.

- **The tester** is a session of the owner's choice of model and
  effort, with its notes beside the owner's, in `qa/`. It reads the
  design only: the sections its part covers, the testing documents (of
  testing-strategy.md, its one page and the sections its world needs),
  and the public surface of the world or harness it writes in. It never
  reads the code under test or its tests.
- **The tester decides which tests to write,** not the owner: the owner
  names the part and the design, never the cases.
- **Its tests are adversarial,** aimed at what the design promises: the
  edges, the orders, the failures, the limits, and the seams between
  parts. Each goes in the highest layer that can express it: the real
  loop or the process tier in temper, a kit's highest world.
- **On the existing infrastructure.** They use the worlds, harnesses
  and fakes that exist; a pass builds no test infrastructure of its own.
  When a test needs a fake or a hook that is missing, the tester says
  so, and the owner decides whether its layer grows it, as ordinary work
  under that layer's own tests.
- **A suite apart.** Their test binaries are named `qa_<area>`. The
  repository's nextest default and fuzzy profiles exclude
  `binary(/^qa_/)`, and a `qa` profile selects them: `cargo nextest run
  --workspace --profile qa`. The gate builds them but does not run them,
  and no test-time ceiling counts them. The first owner to add one to a
  repository adds the profiles.
- **Each failure is the owner's to judge.** A bug is fixed, and its
  regression test goes in the default suite, in the lowest world that
  owns the behaviour. A test that misreads the design is corrected or
  dropped, and a design that let it is made clearer. A gap the owner
  accepts stays, as `#[ignore = "<why>"]`, listed in its notes. When
  the owner ends a pass, the `qa` profile passes.
- **Its tests land** like any other (section 4.4).

Continuous QA, agents using a running build (temper's draft
`docs/design/draft/qa.md`), is another thing, and still a draft.

## 5. Escalation

- An implementer asks its owner, a sub-owner its parent, an owner the
  director, and the director the user.
- On one machine, by SendMessage. To the director on another machine:
  the report or question in that machine's
  `~/src/rust/tmp/director/inbox/<name>.md`, which the director watches,
  and the turn ends. The director answers by message.
- The director asks the user only what has long-term consequences that
  would be hard to change, or what the user's rules keep theirs.

## 6. The closing note

When its work has landed, the owner writes `closing.md` in its notes
directory, in at most half a page:
- what landed, with its commits;
- the wall time from its start to its last landing, and its ledger's
  sessions;
- for a world it added to, the test time at its start and at its end;
- its QA passes: how many, what they found, and the tests left ignored;
- what went well and what didn't, in this process.

It reports to its parent, which folds the note into its own. The
director compares the notes with the old process's runs to judge the
experiment.

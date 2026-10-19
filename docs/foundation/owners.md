# Owners

Experiment, 2026-10-19. How a draft reaches `main` through an owner:
one session that designs the draft and implements it, instead of
development.md's designer, planner, supervisor and coordinator. Runs
started before 2026-10-19 finish under development.md. Designs and plans
landed by then stay, and those not yet run may be carried out by an
owner. If the experiment fails, this file goes and development.md
stands. Read it with [the testing strategy](testing-strategy.md) and
the repository's own `AGENTS.md`.

## 1. In one page

- **The input is a draft** that the user wrote and reviewed: the goal,
  the constraints, the expectations. A landed design or plan not yet run
  can stand in for it.
- **The director** (development.md, 2.7) knows every draft and the
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

## 2. The owner's session

- **Started by its parent** in `~/src/rust` of the machine its work runs
  on, named `o/<name>`:
  `claude --bg -n o/<name> --model claude-opus-5-5 --effort <effort>
  --autocompact 150000 --dangerously-skip-permissions "Read <brief> and
  do what it says."`
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
  in `docs/design/` (development.md, 4.3, the house style) and lands
  them with `land --docs` (4.4), from a worktree on a branch.
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
  whole design;
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
the implementer to, with `land` (development.md, section 9). It updates the repository's
`docs/design/STATUS.md` rows its work changes.

### 4.5 Sharing the machines

- The kit stays: worktrees under `tmp/owners/<name>/`, `heavy`'s queue
  and its gate (development.md, 11.1), the disk floor and the cap on
  sessions (11.4), and downstream repins (5.7).
- **The footprint.** An owner's notes list the repositories, crates and
  protocol versions it changes, and the repins it needs. The director
  orders owners whose footprints overlap, and one repin at a time per
  repository.

### 4.6 QA passes

An owner or a sub-owner may run QA passes on its part while it
implements: none, one or several, at any point. It decides when each
starts and when it is done.

- **The tester** is a session of the owner's choice of model and
  effort. It reads the design only: the sections its part covers, the
  testing documents, and the public surface of the world or harness it
  writes in. It never reads the code under test or its tests.
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
director compares the notes with the runs' metrics (development.md,
15.2) to judge the experiment.

# Development process

Provisional, 2026-10-09, revised 2026-10-18. How work goes from an idea
to `main` in skein and in every project built on it (smith, jig, hearth,
temper): who designs, who plans, who builds, who supervises, where each
of them writes, and how they talk to each other. Read it before starting
a design, a plan or a run.

**How to read this.** Section 1 is the whole process in one page. Section
2 names the roles and section 3 says where everything lives. Sections 4 to
10 follow the work: designing, planning, starting a run, coordinating,
implementing, landing and escalating. Section 11 is each machine's build
infrastructure, section 12 several machines at once, and section 13 the
questions still open. Section 14 is the supervisor's hourly check, and
section 15 the retrospective that ends a run. Two roles read a card of
their own instead of this document: the implementer's card, section 8,
is `implementer.md`, and the supervisor's is `supervisor.md`, both beside
this file. Section 4.5 binds everything written for the coordinator and
the implementers.

## 1. In one page

- **Six roles, two agent programs, and an optional director.**
  - The designer and the planner (Claude Code, Opus 5.5 at xhigh effort,
    interactive with the user) turn an idea into design documents and a
    plan. The planner works in a fresh session, from the landed design.
  - The coordinator (Codex, gpt-6.1-sol at high effort) runs the plan with
    implementers (Codex sub-agents, same model and effort), as many at once
    as the plan and the machine allow.
  - The supervisor (Claude Code, Opus 5.5 at xhigh) watches the run, and is
    the coordinator's only counterpart.
  - The retrospective's author (Claude Code, Opus 5.5 at xhigh), a fresh
    session after the run, writes the run's retrospective.
  - A director, if the user appoints one (Claude Code), acts for the user
    across machines while they are away (section 2.7).
- **The design is the contract.** It lives in each repository's
  `docs/design/`, and only the designer edits it. Drafts live in
  `docs/design/draft/`. `docs/design/STATUS.md` says how far the code is
  from the design, and which plans take it there.
- **A plan breaks a design into sessions of small increments.**
  - It lives in `docs/development/plans/<plan>/` of the most downstream
    repository it changes, with a code-organization document.
  - A session is one implementer's work, and an increment lands on its
    own.
  - Each increment gives its shape (paths, names and signatures), its test
    shape, the things it must not do, and its proof: a smoke check, on top
    of its tests, that it works for real, connected to what is around it.
  - Parts that meet are connected from the start, so that neither is
    built against its own idea of the other.
  - It ends by repinning the repositories downstream of what it changed,
    and repairing what that breaks (section 5.7).
- **Plans are written for a literal reader** (section 4.5). The executor
  follows the words as written, so rules are marked, numbers replace
  adjectives, and every request names its cheapest way to comply.
- **A run is a plan executing on one machine.** Its state lives outside
  the repositories, in `~/src/rust/tmp/<repository>/<plan>/`: the ledger,
  the log, the escalation files and the session worktrees. A machine runs
  at most one plan; several machines run different plans at once.
- **`plan start` launches a run, and every role runs in the background:**
  the supervisor as a Claude Code session, the coordinator as a thread on
  Codex's daemon. `plan status` shows them, `plan attach` opens one, and
  `plan resume` starts them again after a restart.
- **Every increment lands through `land`:** rebase on `origin/main`, run
  the gate, record its evidence in the commit, push. The push is the
  merge, and its rejection says that main moved. The local `main` follows.
- **Heavy commands queue, whoever runs them.** They go through `heavy`
  into the machine's pueue queue: a gate lane that runs alone, and a build
  lane that runs a few at once. kache caches the crates that did not
  change.
- **Escalation goes through files.**
  - An implementer stops on a real gap and tells the coordinator, and the
    coordinator writes `blocked.md`.
  - The supervisor answers in `unblock.md`. Where the design must change,
    it asks the designer first, through `question.md`.
  - Only decisions with long-term consequences that are hard to change
    reach the user, in a push notification, never through a tool that
    waits for the answer (section 10.4).
- **The supervisor checks the run once an hour** (section 14), through a
  sub-agent. It uses numbers from `plan pulse`, reads what changed, and
  samples one test and one diff. It steers through `steer.md`, and adjusts
  the plan when that helps the run. Before a session that did not start
  at the claim begins, it checks the session file against `main`.
- **Contexts stay small, and nothing is written twice.** A long context
  is carried by every request, and checks thin as it grows.
  - The supervisor and the designer compact their contexts in place, at
    about 180,000 tokens, and re-read their files after each compaction
    (section 10.5). A role hands over only when it finds itself confused,
    or when its watch says so, not at a count of compactions (section 13).
  - A designer idle for an hour is woken fresh, from a brief.
  - An implementer hands over between increments past 100,000 tokens,
    and a session idle for an hour goes on in a fresh thread, from its
    checkpoint.
  - Implementers log their own reports, so that the coordinator does not
    type them again.
  - A ruling is written once, and `plan ruling` lands it.
  - An idle coordinator ends its turn, and the doorbell starts the next.
- **A retrospective ends every run** (section 15). It lands as
  `retrospective.md` beside the plan, with the changes it proposes to this
  document. The user, or the director, decides each before the next run
  starts: `plan check` refuses a plan while a proposal of the last
  retrospective is open.
- **Everything else is decided where it arises,** and reported:
  implementation choices by the implementer, the run's rulings by the
  supervisor, design gaps by the designer.

## 2. Roles

| Role | Runs as | Decides | Writes |
|---|---|---|---|
| Designer | Claude Code, Opus 5.5, xhigh | the design, within the user's delegation | `docs/design/**` in any repository, `STATUS.md` included; during a run, the session files that mention a point it changed, the plan's `later.md` rows for what it defers, and the run's `answer.md` |
| Planner | the same, in a fresh session of its own (section 2.2) | how the design is cut into sessions and increments | `docs/development/plans/<plan>/` |
| Supervisor | Claude Code, Opus 5.5, xhigh, its sub-agents on Sonnet; started with the run | the run's rulings within the design: the plan's scope, order and increments, steps not in the plan, process, infrastructure | the run's `unblock.md`, `steer.md`, `question.md` and `supervision.md`; the plan, during its run; a plan's state in `STATUS.md` |
| Coordinator | Codex `-p coordinator` (gpt-6.1-sol, high) | what starts when, within the plan's dependencies and the machine | the run's directory |
| Implementer | Codex sub-agent, same model and effort | implementation choices within the design and the plan | code and tests in its session's worktree |
| Retrospective's author | Claude Code, Opus 5.5, xhigh; a fresh session after the run | nothing: it proposes | `docs/development/plans/<plan>/retrospective.md` |
| Director (optional) | Claude Code, one long-running session the user appoints | for the user, with the user's authority (section 2.7) | `~/src/rust/tmp/director/` |

### 2.1 The designer

- **Talks with the user.** It reads the design, `STATUS.md`, skein's
  foundation documents and the code, and turns an idea into a draft or
  into contract text.
- **Is the only role that edits `docs/design/`,** in any repository,
  including `draft/` and `STATUS.md`. Two exceptions: the supervisor
  changes a plan's state in `STATUS.md` (section 3.3), and an implementer
  lands a vocabulary addition's rows, as the designer gave them, with
  their code (section 4.6).
- **Answers a run's design questions** (section 10.4). They reach it from
  the supervisor, in the run's `question.md`. It amends the design, and
  the session files that mention the point, with Markdown-only landings.
- **Decides within the user's delegation.** What stays with the user is in
  section 10.4.
- **Writes a design in a session of its own,** as the planner writes a
  plan (section 2.2). A bounded task of the designer's, with one report,
  such as deciding a retrospective's proposals, may run as a fresh
  sub-agent of the session the user works in; its landing names both
  sessions in `Designer-Session:` trailers (section 4.4).
- **Waits for its sub-agents' notifications,** and never polls their
  files or sleeps: a sub-agent in the background notifies when it ends,
  and each request in between re-reads the whole context. In
  client-composition, the designer's 600-second sleep outlived its
  cache, and re-wrote 419K.
- **Checks its context after each sub-agent's report** with `plan context`
  (section 3.6), which prints its context and compactions.
- **Compacts in place, and hands over only when it finds itself
  confused.** It starts with `claude --autocompact 200000` (the
  workspace's `AGENTS.md`; `plan` starts and resumes a run's designer so,
  section 10.4), and its context compacts at about 180K. After each
  compaction it re-reads this section and the files its work depends on
  (the design worktree, its drafts, the notes in its scratch directory)
  before acting. If it then finds itself confused, it writes what it
  holds to its files (the design worktree, a handover note in its scratch
  directory) and hands over to a fresh session that starts from them.
  During a run, its answer says so, and the supervisor wakes a fresh
  designer for the next question, from a brief (section 10.4). A
  designer woken for a question reads the design sections and files
  `question.md` names, and earlier answers in `decisions.md`'s design
  rulings: never `log.md`, `supervision.md`, `decisions.md` or this
  document whole. In temper's client-api run, designers read
  `supervision.md` 123 times for earlier answers, and late questions
  needed up to 217K of reading.

### 2.2 The planner

- **Turns a landed design into a plan** (section 5). It is the designer's
  model and effort, in a fresh session started from the landed design: a
  designer's session carries the whole design conversation into every
  request (test-budgets was planned in one that reached 709K tokens). The
  plan's landing commit names the session in a `Planner-Session:`
  trailer, so that the metrics count it (section 15.2).
- **Runs as a session of its own:** started by the user, or with `claude
  --bg` in the workspace; never as a sub-agent of another session, which
  can neither end its turn and be woken, nor hand over. In smith's parity
  plan, a planner run as a sub-agent reached 572K tokens, with 136 of its
  193 requests past 300K, and waited in polling loops.
- **Splits a design's steps across plans first** (section 5.1): from the
  step list and its longest chain, before it drafts any session file, and
  then drafts only the first plan's sessions. In temper's tools plan,
  sixteen Opus drafters ran from 22:29 and the split came at 01:46: about
  $62 of drafts became the next plan's `findings/next/`, to be checked
  again against the `main` the run left.
- **Does not change the design while planning.** A gap the plan exposes is
  closed by the designer, under the designer's rules, before the plan
  relies on it. The planner traces the design, and maps what its output
  prints to the upstream types that carry it (section 5.4), before it
  drafts the session files, and has every gap the trace or the map finds
  answered first. In temper's forge-setup plan, the drafts came first:
  $81 of them raised 45 questions, and their revisions and fixes cost $69
  more.
- **Writes for the executor** (section 4.5). The coordinator and the
  implementers follow the plan's words as written.
- **Runs what it prescribes** before the plan lands (section 5.4), through
  `heavy`, like every role (section 11.1).
- **Checks its context after each sub-agent's report** with `plan context`
  (section 3.6), as the designer does.
- **Waits, compacts and hands over as the designer does** (section 2.1):
  on its sub-agents' notifications, never polling or sleeping; started
  with `claude --autocompact 200000`, it re-reads section 5 and the
  plan's files after each compaction, and, if it finds itself confused,
  hands over to a fresh session that starts from the plan's files. Each
  successor's session gets its own `Planner-Session:` trailer. In
  client-composition, the planner ran to 617K in one context, and spent
  $16.96 at 465K to 567K while its reviewers ran, about 57 of those
  requests polls.

### 2.3 The coordinator

- **Runs one plan on one machine** (section 7): sets up the run, starts a
  session as soon as its dependencies have landed, records every report,
  keeps the ledger, watches the machine and escalates.
- **Never designs, decides, reviews or writes code.**
  - It never edits a design or a plan, never answers a design question,
    and never fixes anything itself.
  - Its Codex thread takes every coordinator profile setting except
    `model` as dotted config keys; `PLAN_CODEX_EFFORT` and
    `PLAN_CODEX_THREADS` override them. The profile disables the code
    graph server.
  - It checks reports with git only, and never queries the code graph,
    whatever other instructions say: it reads no code. In protocol, a
    machine's `~/.codex/AGENTS.md` had it query the graph before 80 of its
    82 spawns, 10.7% of its input.
  - It creates no session, step, file-ownership rule or protocol of its
    own. Work that is not in the plan, an unblock or a steer is not done:
    the coordinator escalates the gap instead (section 7.3).

### 2.4 The implementer

- **Implements one session at a time,** increment by increment, in its own
  worktree, as its card says (section 8, `implementer.md`). It lands its
  own increments and resolves its own rebase conflicts. It logs its own
  reports with `plan log`, and sends the coordinator a pointer.
- **Hands over between increments** when its context grows past 100,000
  tokens: a fresh implementer goes on from its checkpoint (section 7.3).
- **Escalates to the coordinator,** and only on real gaps (section 8.7).

### 2.5 The supervisor

- **Is the coordinator's only counterpart.** It reads `blocked.md`, and
  writes `unblock.md` and `steer.md` (section 10). The coordinator reaches
  the designer and the user only through it.
- **Rules on what is not design** (section 10.4):
  - plan defects, the order of steps and the scope of the plan;
  - steps not in the plan;
  - process, infrastructure and credentials.

  It records each ruling in the plan's `decisions.md`.
- **Adjusts the plan when that helps the run.** For example, it:
  - splits an increment that turned out large;
  - reorders steps whose dependencies were wrong;
  - tightens an increment's shape, test shape, proof or "Don't" after a
    check;
  - writes the short session file of a step not in the plan.

  Each adjustment is recorded in `decisions.md` and lands with `plan
  ruling`, which writes the ruling once (section 10.3), and reaches the
  coordinator as a steer or an unblock that names the commit. It never
  changes what the design asks: a change that would is a design question,
  for the designer.
- **Checks each session file against `main`** before the session starts,
  when it did not start at the claim: the ready check (section 7.2).
- **Asks the designer** what the design must answer, in the run's
  `question.md`, and writes the unblock once the designer has answered.
- **Checks the run once an hour** (section 14), through a sub-agent, and
  steers (section 10.3).
- **Watches the coordinator** (section 10.5): its heartbeat, its pauses
  and its end.
- **Does not review in depth and writes no code.** A question that needs
  more than the hourly check goes to a read-only sub-agent with a bounded
  question, or waits for the retrospective.
- **Compacts in place** at about 180,000 tokens, keeping its watch and
  its background commands, and re-reads its card's working
  sections and the run's files after each compaction. It hands over to a
  fresh session only when it finds itself confused, or when its watch
  says so, and starts its successor itself (section 10.5).
  `supervision.md` and the run's files are enough to go on.
- **Works from its card,** `supervisor.md`, which says each of these step
  by step, and reads of this document only the sections the card names.

### 2.6 The retrospective's author

- **A fresh session after the run** (section 15). It took no part in the
  run, so it reads the record with fresh eyes.
- **Proposes, and decides nothing.** The user, or the director, decides
  each of its proposals, before the next run starts (section 15.1), and
  the designer applies them.

### 2.7 The director

An optional role: one long-running Claude Code session that the user
appoints to act for them across machines, with their authority, while
they are away. Its working files are in `~/src/rust/tmp/director/`.

- **Drives the designers and planners.** It starts their sessions with
  their briefs. They send it their questions with SendMessage, each with
  its recommendation, and end their turn; a session on another machine,
  which SendMessage does not reach, ends its turn with them, and the
  director reads them there.
- **Answers what a role writes for the user** (section 10.4), within
  minutes of its writing. It relays to the user only the decisions with
  long-term consequences that would be hard to change, and records each
  call it makes, which the user may overturn.
- **Decides a run's design questions when no designer is joined,** each
  decision naming the design text it changes, and has a designer land
  those changes before the next plan on that design is planned (section
  10.4).
- **Resumes the roles a usage limit stopped,** at the reset time they
  gave (section 10.5).
- **Decides the retrospectives' proposals** (section 15.1), and asks the
  user when in doubt. The designer applies them.
- **Speaks for the user.** Every role treats its messages as the user's.

## 3. Where things live

### 3.1 Repositories

- **Each project is a repository under `~/src/rust/`,** on GitHub under
  `fabrig-dev`: skein, then smith, which builds on skein, then jig, which
  builds on both, then hearth, which builds on skein alone, then temper,
  which builds on skein, smith and jig. That is the workspace's order:
  each repository after those it builds on.
- **The workspace is the directory that holds the checkouts:** `~/src/rust/`
  by default. The tools find it as the directory holding the skein
  checkout they come from, so a machine may keep its checkouts elsewhere;
  `DEV_ROOT` overrides it. This document writes `~/src/rust/` for it.
- **Dependencies point upstream only.** An upstream repository's code never
  depends on a downstream one. Its documents may name a consumer as the
  reason for a feature, unless its `AGENTS.md` says otherwise: jig never
  names temper.
- **A primary checkout (`~/src/rust/<repository>`) stays on `main`,
  clean, and equal to `origin/main`.** Nobody works in it. `land` moves it
  forward, and the code graph indexes it. The graph describes `main` only
  while its index is not older than `origin/main` (sections 6 and 14.1).
- **Each repository names its gate and its budgets:** `scripts/check.sh`
  runs the gate, and `docs/development/workflow.md` (or skein's
  `docs/design/testing.md`, section 6) explains it, with the suites'
  15-second and 60-second budgets (testing-strategy.md, section 8). Its
  `AGENTS.md` holds what an agent must know before touching its code.
- **`docs/development/` holds how a repository is developed:** its
  `workflow.md`, its plans (`plans/<plan>/`, section 3.4) and, where it
  keeps one, its `tech-debt.md`. skein's holds its plans, from its first;
  its workflow stays in `docs/design/testing.md` for now.

### 3.2 The design

- **`docs/design/` holds the contract:** what the code must do, cited by
  the code as `<file>.md, section N`.
- **`docs/design/draft/` holds drafts:** ideas worked out but not yet
  contract. A plan never implements a draft. When a draft becomes design,
  its content moves into the design documents and the draft is deleted,
  or cut down to what is still open.

### 3.3 The status file

Each repository's `docs/design/STATUS.md` says how far its code is from
its design, and which plans take it there:

```
# Status

Updated <date>. How far <repository>'s code is from its design, and which
plans take it there. The designer keeps this file (skein's
development.md, section 3.3).

## 1. Plans

| Plan | State | Takes the design to | Where |
|---|---|---|---|

## 2. The design

| Document | Built | Left |
|---|---|---|

## 3. Not planned

- **<item>** (`<document>.md`, section N): what it is, and what it waits for.

## 4. Drafts

| Draft | About | Next |
|---|---|---|
```

- **A plan's row** names it by its directory, `<plan>`. Its state is
  `drafting`, `ready` (landed, not started), `active on <machine> since
  <date>` or `done <date>`. A plan that changes several repositories has a
  row in each; an upstream repository's row names the plan with its
  repository ("smith's reliability"), and jig's never names temper.
- **Built** is `yes`, `partly` or `no`. **Left** names what is not built,
  by section, and the plan that builds it (": <plan>" after its items),
  or "not planned" (naming the `later.md` that defers it, if one does).
- **The designer keeps the file.** It changes it in the same landing as the
  design or the plan that changes what it says.
- **When a plan ends, the designer clears its items** from section 2, in
  the landing that records the decisions on its retrospective (section
  15.1): what it built leaves Left, and what it left becomes "not
  planned", with its item in section 3. `plan check` warns of an item
  section 2 still leaves to a plan that section 1 marks done. jig's
  `STATUS.md` kept jig-protocol's items for five days after its end, and
  called the store, the channel and the services unbuilt.
- **A plan's state is the planner's until `ready`** (section 5.4), **then
  the supervisor's,** who changes it when its run starts and ends
  (sections 6 and 7.6). The supervisor touches no other line of the
  file.

### 3.4 Plans

A plan lives in `docs/development/plans/<plan>/` of the most downstream
repository it changes, its repins aside (section 5.7). The reason for a
plan, and the proof that it is done, live with its consumer, and an
upstream repository's history stays its own. A plan that changes only skein lives in skein; one that changes
skein and smith lives in smith.

| File | Holds |
|---|---|
| `README.md` | inputs, the session table and its dependencies, the rules particular to this plan, the pulse's settings, "Done when" |
| `code-organization.md` | crates, modules, names, public API, test layout and fixtures, the owner of each shared interface, the plan's "Don't" list and size limits |
| `<repository>/NN-<name>.md` | one session: its increments, with their shapes, test shapes and proofs, and its interface sketches |
| `decisions.md` | the decisions the plan rests on, the supervisor's rulings during the run, and those left to the user |
| `later.md` | what the design names but the plan does not do, and when it should be done |
| `findings/` | evidence the plan cites, if any (section 4.1) |
| `retrospective.md` | after the run: what happened, what should change, and the user's decision on each proposal (section 15) |
| `metrics.tsv` | after the run: its numbers, as `plan metrics` computes them (section 15.2) |

The shapes are in section 5.2. A plan is committed and pushed before its
run starts. It stays in place after its run, with its retrospective:
later planners read both. A plan that lived elsewhere before this layout
stays where it is, and its `STATUS.md` row says where.

### 3.5 A run's directory

A run lives in `~/src/rust/tmp/<repository>/<plan>/`, where
`<repository>` is the plan's own. `~/src/rust/tmp/active-plan` names the
machine's run. Nothing here is versioned.

| File | Written by | Holds |
|---|---|---|
| `ledger.tsv` | the coordinator, through `plan set` | one row per increment: `step`, `repo`, `session`, `state`, `slot`, `thread`, `branch`, `commit`, `updated`, `title`, `note` (section 7.3) |
| `status.md` | `plan status` | the ledger, rendered, checked against `origin/main` |
| `log.md` | the implementers, through `plan log`; the coordinator | every report, every escalation and its answer; appended only |
| `reports/<repository>-<NN>-<time>.md` | `plan log` | each report, as its implementer wrote it, for the files that name it (section 8.6) |
| `context.md` | the coordinator | what a restarted coordinator needs; rewritten, never appended |
| `thread`, `threads` | `plan start`, `plan resume`, the coordinator | the coordinator's Codex thread's id, for the doorbell; every thread it has had, with when, for the metrics. Each starts with the coordinator profile's config settings |
| `claude.tsv` | `plan start`, `join`, `wake`, `handover`, `retro`, and each `plan` command that finds a role's job on a session id the file lacks | one line per Claude Code session that works on the run, one for each id a role's job takes (a `/resume` gives it a new one): its role, its session id and when, for `plan attach`, `plan wake`, `plan handover` and the metrics; a role's last line is its current session |
| `heartbeat` | the coordinator | touched at every wake-up |
| `alerts.md` | the supervisor | each alert to the user, one line with its UTC time; `plan status` prints its last ten lines first (section 10.5) |
| `idle.md` | `plan idle`; removed by `plan ring` or the watch’s queue wake | since when, why, and optional third line `tasks: <ids>` naming the queue tasks the coordinator waits on (section 7.4) |
| `awaits.tsv` | `plan await`; the watch | the landings a role waits for, one line per step: the role, the plan and the step; the watch removes a role's lines once all its steps are on `origin/main` (section 7.4) |
| `.watch` | `plan watch`; cleared by `plan handover supervisor` and a supervisor started by `plan resume` | the escalations and events this supervisor session has seen |
| `handed-over` | `plan handover` | the supervisor sessions that handed over, to which `plan watch` shows nothing more |
| `blocked.md` | the coordinator | open escalations (section 10.2) |
| `unblock.draft.md`, `unblock.md` | the supervisor; `plan ruling` | the answer, written once as a draft, then landed and moved into place (section 10.3) |
| `docs/` | `plan docs` | the supervisor's worktree of the plan's repository, for its plan edits; removed by `plan release` |
| `kit/` | `plan claim`; `plan resume` | the run's kit: a worktree of skein at the commit the run was claimed at (section 3.6), moved to skein's `origin/main` when a paused run resumes (section 7.6); removed by `plan release` |
| `steer.md` | the supervisor | an instruction nobody asked for (section 10.3) |
| `question.md`, `answer.md` | the supervisor; the designer | a design question, and its answer (section 10.4) |
| `supervision.md` | the supervisor; `plan handover` | one entry per hourly check (section 14), each compaction and each handover (section 10.5), and each design question with its answer; appended only |
| `.pulse` | `plan pulse` | where the last check stopped |
| `paused.md`, `done.md` | the coordinator | a pause's checkpoint; the run's end, from its opening `Recorded <UTC time>.` line (section 7.6) |
| `sessions/<repository>-<NN>.md` | the implementer | its session's checkpoint (section 8.6) |
| `logs/` | `heavy`; the implementers | each heavy command's output, and `heavy.tsv`, the queue's statistics (section 11.2); without an active run, `~/src/rust/tmp/logs/`. Each session's scripts and long outputs, in `logs/<repository>-<NN>/` |
| `downstream/<repository>/` | `plan downstream` | fresh worktrees of downstream mains, removed on every exit |
| `wt/<repository>-<NN>/` | `plan worktree` | the session's worktree, with its own `target/`; beside them, the supervisor's `wt/<repository>-tier/` for the process tier (section 14.1), its ready checks' `wt/<repository>-<NN>r/` (section 10.4) and its "Done when" worktrees (supervisor.md, section 8), all removed at the run's end (section 7.6) |

The escalation files are written whole and renamed into place, so the
other side never reads half a file. A writer that finds its file still
there adds its new headings to it.

Nothing in the run's directory is removed before the run's retrospective
has landed (section 15), except:
- the worktrees (section 7.6), and the supervisor's `docs/` and the
  run's `kit/`, which `plan release` removes;
- sessions' `target/` directories (section 11.4);
- escalation files, once they have been appended to `log.md` or
  `supervision.md`.

### 3.6 The kit

skein's `scripts/dev/` holds the tools every repository uses, and
`scripts/dev/workspace/` the workspace's own files:
`~/src/rust/AGENTS.md`, its `CLAUDE.md`, its Claude Code settings
(`.claude/settings.json`, from `claude-settings.json`: a hook that tells
each session of the workspace, once compacted, to re-read its role's card
and its files before acting), and the coordinator's Codex profile. `plan
check --fix` links the tools into `~/.local/bin`, so they are on every
agent's `PATH`, and the files into place. The tools read
`DEV_ROOT` (default `~/src/rust`), `DEV_TMP` (`$DEV_ROOT/tmp`) and
`DEV_REPOS`: the repositories `plan check` checks, by default the plan's
and those upstream of it, or all of them without a plan.

The machine keeps its settings in `$DEV_TMP/machine.env`: lines
`KEY=integer`, for `PLAN_MIN_MEM_GB`, `PLAN_MIN_DISK_GB`,
`HEAVY_BUILD_SLOTS` and `HEAVY_BUILD_JOBS` only. The environment wins
over the file, and the file over the defaults (6 GB, 40 GB, 2 tasks and
4 cargo jobs). The file is parsed, never sourced; `plan check` warns of
invalid lines and prints each minimum and lane width with its source.

`plan` holds the command dispatch, settings, ledger and worktree commands.
It sources `plan-check.sh` for the checks, `plan-watch.sh` for the watch,
`plan-roles.sh` for the roles and their prompts, and `numbers.sh` for the
run's numbers and pulse, and `metrics.sh` for its metrics. These files
are part of the same kit.

**A run takes its kit from its claim,** as it takes its process (section
4.4). `plan claim`, which `plan start` runs, adds `kit/` to the run's
directory: a worktree of the workspace's skein at its checkout's commit,
which `plan check` found equal to `origin/main`. While the run is the
machine's, `plan`, `land` and `heavy`, wherever they are run from, hand
over to `kit/`'s copies: every role, and `plan resume`, runs the claim's
kit, and `plan check` still links the tools to the copy on `PATH`. So a
kit change lands as soon as it is decided, and reaches the runs claimed
after it. A run that must take it sooner does so by its supervisor's
ruling, which moves `kit/` to the commit that landed it (`git -C
<run>/kit checkout --detach <commit>`). A fix for a defect the run has
met is such a change: the supervisor's next hourly check moves `kit/` to
it, once `<run>/kit/scripts/dev/test-plan` passes there, and keeps the
kit only for a reason it writes in the ruling. In temper's client-api
run, five defects met by hand (the ref lock, the coordinator's
compaction limit, loading before a ring, pueued's restart, the
65-minute alarm) had their fixes on `main` 2 to 6 hours before the
workaround. A run claimed without `kit/`,
before skein e6fd9fb, runs the copy on `PATH`, main's, to its end. A
paused run is the exception: its `plan resume` moves `kit/` to skein's
`origin/main`, and the run resumes with that kit and its process
(section 7.6).

**A run's roles read the process in `kit/`,** never in the skein
checkout: development.md, their card (`supervisor.md` or `implementer.md`)
and `testing-strategy.md`, at `<run>/kit/docs/foundation/`. They read
them there at their start and after each compaction, restart or
handover, and so do their sub-agents. The roles are the supervisor, the
coordinator, the implementers and a designer answering a run's question.
The prompts `plan` gives them, and those they give their sub-agents,
name the kit's copies. The retrospective's author starts after `plan
release` has removed `kit/`, and reads the checkout's, as does a run
claimed without `kit/`.

**A plan that changes the kit names the steps its own run takes,** in
its README's Kit steps (section 5.2), and the supervisor moves `kit/` to
each by ruling once it lands (supervisor.md, section 3). A ruling or a
steer names only commands and sections that the run's `kit/` has, which
may be behind `main` (section 10.3). kit-batch named none: its claim's
`land` had no landing turn, and 30 of its 61 gates passed and were thrown
away; ruling 105 named `plan landed`, which its kit lacked, and made a
block of its own.

| Tool | Does |
|---|---|
| `heavy [--gate] [--tail N] command…` | queues a heavy command, waits, prints the tail of its log, exits with its status (section 11); `--gate` refuses a tree with uncommitted changes; internal `heavy --task TOKEN -- command…` runs the task and writes its row |
| `land [--docs] [--local] [--gate COMMAND] [--tries N]` | lands the current branch (section 9); `LAND_LOCK` names the lock for `--local`, and `LAND_PREBUILD` the build lane's command before the gate |
| `plan check [--fix] [PLAN_DIR]` | checks the machine and the repositories (the code graph's index of each against its `origin/main` among them), and, given a plan, the plan (its base run, its design answers and the decisions on its "For the user" items, section 5.4), the user's decisions on the last retrospective's proposals, the items its repository's `STATUS.md` still leaves to done plans (a warning, section 3.3), the queue and its repository's gate (sections 6 and 12); `--fix` fast-forwards a clean primary checkout that is only behind `origin/main`, and keeps the code graph's `.codebase-memory/` out of each checkout's `git status` |
| `plan start PLAN_DIR` | checks the machine, claims the run, starts the supervisor and the coordinator, and rings the doorbell once (section 6) |
| `plan attach ROLE` | opens a role's session in this terminal: `coordinator`, `supervisor`, `designer`, `retrospective` |
| `plan resume [CAUSE]` | after a restart, a crash or a pause, starts again what is not running: pueued, Codex's daemon, the supervisor, the coordinator's turn; records its cause in log.md itself; a paused run first moves `kit/` to skein's `origin/main`, adds its Process line to `supervision.md` and resumes with that kit (section 7.6) |
| `plan join ROLE` | records the Claude Code session it runs in as the run's ROLE, usually the designer |
| `plan wake ROLE MESSAGE` | wakes a Claude Code role in the background with MESSAGE: resumes its session, or starts a fresh designer from a brief once its session has been idle `PLAN_DESIGNER_IDLE` (3,600 s); never wakes a session at work (section 10.4) |
| `plan unblocked` | logs blocked.md and unblock.md whole, removes the answered block headings and deletes unblock.md (section 10.3) |
| `plan steered` | logs and deletes steer.md (section 10.3) |
| `plan handover REPOSITORY NN CONTEXT` | appends the session handover line to log.md (section 7.3) |
| `plan handover supervisor` | from the supervisor's session, starts its successor, and records it (section 10.5) |
| `plan retro [PLAN_DIR]` | starts the run's retrospective, by default the last run's, released or not (section 15) |
| `plan claim PLAN_DIR`, `plan release` | start and end the machine's run, adding and removing its `kit/`; `plan start` claims |
| `plan worktree [--remove] REPOSITORY NAME` | a session's worktree, at `origin/main` |
| `plan landed STEP COMMIT` | fetches and checks a landing against origin/main, its Step trailer and its worktree, then sets the ledger only when all pass (section 7.3) |
| `plan set STEP KEY=VALUE…` | adds or changes a ledger row |
| `plan log REPOSITORY NN [FILE]` | logs an implementer's report, from FILE or its standard input, in `log.md` and `reports/`, and prints the report's file (section 8.6) |
| `plan idle REASON [--tasks ID…]` | the coordinator, with nothing running, ends its turn; `idle.md` says why, and the next ring starts its next turn (section 7.4) |
| `plan await ROLE [PLAN/]STEP…` | records in `awaits.tsv` the landings ROLE, `coordinator` or `supervisor`, waits for, by default of this run's plan; once all their Plan and Step trailers are on `origin/main`, the watch tells the supervisor, or starts the coordinator's turn (section 7.4) |
| `plan docs` | prints the supervisor's worktree of the plan's repository, made at `origin/main`, and moved there whenever it is clean; refuses while its branch has commits `origin/main` lacks, a ruling that did not land, rather than drop them (section 10.3) |
| `plan ruling [--text FILE] TITLE SOURCE` | lands a ruling, written once: the Decision of `unblock.draft.md`, or FILE's text, as the next ruling of `decisions.md`, with the plan edits in the supervisor's worktree; then, for an unblock, names the commit, moves the draft to `unblock.md` and rings (section 10.3) |
| `plan ready STEP [NOTE]` | sets a step the supervisor has checked to `ready`, and rings (section 7.2) |
| `plan status [PLAN_DIR]` | renders `status.md` of the plan's run, by default the last run's, released or not |
| `plan watch` | waits until the supervisor has something to do, and prints its context and its compactions (section 10.5) |
| `plan pulse` | prints the run's numbers since the last check (section 14) |
| `plan context [SESSION]` | prints a Claude Code session's context and compactions; defaults to `CLAUDE_CODE_SESSION_ID` (sections 2.1 and 2.2) |
| `plan contexts` | prints each Codex thread's context, the input of its last request, and marks the sessions past `PLAN_CONTEXT_LIMIT` (100,000 tokens) (section 7.3) |
| `plan downstream` | type-checks each repository downstream of what the plan landed against those repositories' mains, through the build lane in fresh `downstream/` worktrees, removes them on every exit, and lands nothing (sections 5.7 and 14.1) |
| `plan ring MESSAGE` | loads the coordinator's thread if needed, rings its doorbell, and starts the turn of an idle one (section 7.4) |
| `plan metrics [--out FILE] [PLAN_DIR]` | prints the run's metrics, by default the last run's, or writes them to FILE; with `--history`, prints every plan's metrics side by side (section 15.2) |
| `test-plan [--live]` | tests `plan` on a workspace of its own, and fails when that process layer runs past its budget, 40 s, by more than its 5 s margin for the lanes' noise; `--live` also starts real roles, on cheap models. It compiles nothing: it runs in the build lane (`heavy test-plan`) |

The roles run in the background:
- **Claude Code roles** are background sessions (`claude --bg`), started in
  the workspace, which Claude Code must trust: run `claude` there once,
  and accept. `plan start`, `resume`, `wake` and `handover` start and
  resume the supervisor and the designer with `--autocompact` at
  `PLAN_CLAUDE_AUTOCOMPACT` (200,000 tokens), and they compact a little
  below it (section 10.5); the retrospective, short-lived, without. `plan
  attach` on a session that is gone resumes it in the terminal, without.
- **The coordinator** is a thread on Codex's app-server daemon, reached
  through `codex app-server proxy` (`scripts/dev/codex.sh`). It goes on
  without a client attached.
- **`PLAN_CLAUDE_MODEL`, `PLAN_CLAUDE_EFFORT`** (claude-opus-5-5, xhigh)
  set the Claude Code roles' model and effort. The coordinator's come from
  `workspace/coordinator.config.toml`, overridden by `PLAN_CODEX_MODEL`,
  `PLAN_CODEX_EFFORT` and `PLAN_CODEX_THREADS`. Its sandbox is
  `PLAN_CODEX_SANDBOX` (danger-full-access), since no one is there to
  approve.

`HEAVY_BUILD_JOBS`, `HEAVY_BUILD_SLOTS` and `HEAVY_TAIL` tune the queue;
`PLAN_MIN_DISK_GB`, `PLAN_MIN_MEM_GB`, `PLAN_QUIET`, `PLAN_POLL`,
`PLAN_CLAUDE_DIR` (where the Claude Code roles start), the roles' limits
above tune `plan`. `PLAN_DRY_RUN=1` prints what `plan` would start or
ring, and starts and rings nothing.

## 4. Designing

### 4.1 How a design starts

- **In conversation.** The user brings an idea, a report or a spike. The
  designer reads what the idea touches: the design documents, `STATUS.md`,
  skein's foundation documents and the code, through the code graph.
- **Its result is a draft or contract text.** A draft lands in
  `docs/design/draft/`. Contract text lands in the design documents, with
  `STATUS.md` updated in the same landing.
- **A large change goes through evidence and decisions first:**
  - **Spikes** are branches that try a change quickly (`spike/*`,
    `bench/*`). They are evidence and are never merged.
  - **Findings** are written by read-only sub-agents, one per theme, each
    entry with its symptom, root cause, the spike's change, an assessment
    (keep, rework, redo, drop), the clean fix and its tests. When a plan
    follows, they go in its `findings/`.
  - **Decisions** are the questions the findings raise, each with the
    designer's recommendation. The user answers those that are theirs
    (section 10.4); the designer decides the rest. They go in the plan's
    `decisions.md`.

### 4.2 Writing in parallel

When a change spans many documents, the designer writes a brief and has
sub-agents write and review in parallel.

- **The brief gives:**
  - the inputs, in the order they bind;
  - the house style (section 4.3);
  - a shared vocabulary: the names every writer uses for the concepts
    that cross documents;
  - an assignment table, one writer per set of documents, with no
    document in two sets;
  - the shape of the final report: files changed, sections whose scope
    changed, what other documents must say for the writer's own to hold,
    and what is left open.
- **Writers then reviewers.** Writers edit only their own documents, in
  the design worktree, and never commit. Reviewers then own disjoint sets
  of documents. They check every cross-document item the writers
  reported, and fix their own side. The designer pins the names that
  settle a conflict, and applies what is left.
- **Sub-agents report in their final message.** The designer keeps the
  reports in its session's scratch directory.
- **A design worktree** is a branch `design/<topic>` checked out in
  `~/src/rust/tmp/design/<repository>-<topic>/`.

### 4.3 House style

- **A title, then a dated status line:** "Provisional, 2026-10-09", or
  "revised 2026-10-09".
- **"1. In one page" first,** with bold-led bullets, then numbered
  sections. Cross-references read "run.md, section 5.3", or skein's
  "programming-model.md, section 5.2".
- **Never renumber.** Code cites sections by number. New material goes into
  an existing section or a new subsection; a new top-level section goes
  after the last one.
- **Contracts, not history.** A design never cites a spike, a finding or a
  benchmark number. It justifies by principle and by other design
  documents.
- **Policy above, numbers below.** Domain documents state rules. Numbers
  belong in the documents a repository names for them (smith:
  `protocol/limits.md` derives limits, `shell.md` holds presets).
- **Keep what is right.** Change what a decision changes. Leave open what
  no decision settles, in the document's "Open questions".
- **The world sections** name the stories and invariants that check the
  contract, within the suites' budgets.

### 4.4 Landing a design

A design lands as a Markdown-only branch, with `land --docs` (section
9.4), and with `STATUS.md` updated in the same landing. Its commit body
says what changed and why. Its trailers name each Claude Code session that
wrote it, the designer's, its successors' and its sub-agents', one
`Designer-Session: <session id>` line each, as a plan's landing names its
planner (section 5.4), so that a plan's whole cost, its design included,
can be counted. `plan metrics` reads those trailers on the commits the
plan's Inputs name, and on no other (sections 5.2 and 15.2).

**A change during another run.** A run keeps the process and the kit of
its claim: development.md, its cards and `scripts/dev` at the skein
commit of its `kit/` (section 3.6). A change to them that lands while a
plan's row in a `STATUS.md` reads `active on` says in its commit body,
for each such run, whether that run takes it ("<plan>'s run takes this")
or keeps its process. The run's supervisor passes on, as a steer, each
change its run takes, once its hourly check finds it (section 14.1, step
2), and moves the run's `kit/` to a kit change it takes; the others wait
for the run's end, or for its resume after a pause, which takes them all
(section 7.6). In smith's parity run, five adoptions landed on
other machines and none reached it; the run found three of their rules
again, hours later, by its own steers and rulings.

### 4.5 Writing for the executor

The coordinator and the implementers follow the words they are given as
written. Where the words leave a gap, they fill it with the broadest
reading. These rules bind the designer, the planner and the supervisor in
everything those agents read: plans, prompts, unblocks and steers.

- **Mark rules and guides.**
  - A rule says "must" or "never", and something checks it.
  - A guide says "aim for", and says what happens when it is missed.
  - An allocation meant as a guide and worded as a rule makes sessions
    stop over a millisecond, or bend their code to fit it.
- **Use numbers, not adjectives:** "at most 40 lines", not "short".
- **Give every instruction a scope and an end:** who, which steps, from
  when, and until when.
- **Name the cheapest way to comply.**
  - When asking for evidence, name the command and what counts, and say
    that nothing else is wanted, and how often it runs (usually once).
  - Without that, a request for evidence grows into manifests, observers,
    repeated runs and polling scripts.
- **Say the check, not the virtue.** Words that ask for an unbounded
  effort are left out of instructions: prove, verify, ensure, confirm;
  every, all, whole, complete, exact. Say which check to run instead.
- **Say what not to do, and what stays unchanged.** An instruction that
  adds a rule names the rule it replaces.
- **Say what takes the place of what goes.** An instruction that removes
  words says what replaces them: nothing, or what the code checks, never
  another qualifier. "None of the Pulse words" alone moved "actual" to
  "observed"; "do not put another word in their place" held.
- **When there are options, name the default.**
- **Quote the text an answer relies on,** from the design or the plan; do
  not paraphrase it. Check an answer against the code before sending it.
- **Answer a bound with its first measure,** or with "measure it first".
  An answer about a bound (a worst case, a margin, a limit's sufficiency)
  gives the command that measured it and its output (section 10.4), or
  names the increment that measures it (section 5.3). A figure argued
  without a probe is neither. In render, the answer to "git's
  listing decoder alone holds more than 256 MiB" ran no probe, and the
  1 GiB bound was summed 9.5 hours into the run.
- **Check what matters, as well as saying it.** A rule that must hold for
  the rest of a plan becomes:
  - a lint;
  - a test;
  - a check in the gate;
  - or a line of `plan pulse` (section 14).

  Rules that are only written down drift.
- **A check fails when it reads nothing.** A command that reads a value
  out of a file and checks it fails when it reads none: an empty
  revision, count or line is a failure, never a pass. It matches what
  names the thing (a repository's name, `/skein.git#`), not what may be
  renamed (its organization). In temper's tools run, a "Done when" item
  read an empty revision for hearth and checked skein's index instead
  (section 5.2).
- **Repeat what matters where it applies.** Rules buried in a long README
  are missed. A session file repeats the two or three that bind its
  increments.
- **Name the rest; do not restate it.** What the reader can open, the
  design, the code, the findings, is named by section and symbol. A
  restatement is read as well as its source, and drifts from it.
- **Read every time from the clock.** A time written into a run's files,
  a heading, a ledger note or a `Reverses:` line, is read from `date -u`
  when it is written, never estimated.

### 4.6 Vocabulary additions

skein's `ui.md`, section 4.3, "The light path": an addition to skein-ui's
vocabulary that maps one to one onto an HTML or ARIA semantic lands as
one small skein change, without a design round. The variant, its rows in
`ui.md`, `ui-text.md` and `ui-web.md`, the fake person's name and their
golden rows land in one commit, with `land` and skein's gate. No draft,
no review, and no `STATUS.md` change: the design and its code land
together.

- **The designer decides that an addition takes the light path,** in a
  run or outside one: it is a design gap like any other (section 10.4).
  Its answer names the variant, what it maps to on the web and on the
  criterion's second platform (`ui.md`, 4.1), and the text of each row.
  An addition that does not map one to one is a change to the design
  (section 4.1).
- **During a run,** the need goes to the designer as any design
  question (section 10.4), from an implementer's block or a ready check.
  The designer lands nothing: `answer.md` gives the rows. The supervisor
  adds a step `skein-<NN>.s<k>`, whose short session file quotes them,
  and the step that uses the variant waits for its landing (section
  9.5).
- **Outside a run,** the designer decides it for the design or the plan
  that needs it (the planner asks, section 2.2), and the plan makes it an
  increment in skein, before those that use it, whose session file quotes
  the rows.
- **Its implementer writes the rows word for word,** and no other design
  text (section 2.1; implementer.md, 8.2).

## 5. Planning

### 5.1 What to settle first

These defaults hold unless the user says otherwise. The planner asks the
user only what they and the design leave open, and asks before writing.

- **Parallel.** Sessions run as many at once as dependencies and the
  machine allow. The plan keeps true dependencies only, at the
  granularity of an increment where it can.
- **Formats are pre-release.** Any format may change. Within a plan, each
  format's version changes once, at the first landing that changes it.
  Readers refuse other versions, and nothing is translated.
- **APIs may break** where the design asks. The plan repairs its
  consumers: it ends by repinning each repository downstream of what it
  changed (section 5.7).
- **Scope.** The plan names the repositories it changes; the others are
  read only, but for its repins.
- **A plan's size.** A design whose longest chain passes about 20
  increments, or whose steps pass about 80, runs as consecutive plans.
  The planner splits it from the step list and its longest chain, before
  it drafts any session file, and drafts only the first plan's sessions;
  the rest goes in `later.md` as the next plan's scope, naming where it
  starts. Each later plan is written once the plan before it has landed,
  against the `main` it left. In temper's client-api run, 159 steps at
  0.38 hours a step took 302 rulings, 130 of them correcting Shapes that
  drifted with each landing; render's 168 steps, at 0.08 hours on a
  short chain, show that length alone is not the cause.
- **Credentials and live services.** The plan names each one it uses, how
  it is borrowed (read only, from the user's existing login), that no
  agent signs in, and the budget of calls or tokens each may spend.
- **Proofs.** Each increment proves itself at the highest layer within
  reach (section 5.5). The plan also says what the repository asks for
  beyond its gate (smith: benchmark probes, `docs/design/benchmarks.md`).
- **An aim that can fail by chance.** A plan whose aim is a measured
  verdict, a statistical comparison such as smith's parity
  (`benchmarks.md`, section 16), settles before the run how many closing
  runs it holds, and what it does when the last misses: it ends with the
  aim unmet, its "Done when" naming the criteria that failed, and a
  `later.md` row for their answer. Its baseline's miss goes to the
  designer at once (section 10.4). In smith's parity plan, a truly equal
  smith passed the whole verdict with a chance of 0.69, and a miss at the
  closing run was left "a question for the designer". The plan states,
  from its planning commands on the last committed baseline (section
  5.4), the resolution of each run that measures a change against its
  parent on the way: the smallest effect that run tells from none, and
  so which changes have a run of their own (section 5.5).
- **The executor.** The coordinator and the implementers read the plan as
  written (section 4.5).
- **Language.** Rust, for everything.
- **Code organization and interface sketches** are always part of the
  plan.

### 5.2 The plan's files

**`README.md`:**
- the title, and a dated line with each repository's start commit;
- **Inputs:** each design document by its full path in the repository
  (`docs/design/` or `docs/foundation/`), with the commits that wrote
  its design for this plan, `decisions.md`, `findings/`. The design's
  sessions are those commits' `Designer-Session:` trailers (section
  15.2): in smith's live-text run, the window of commits counted $424 of
  three sessions that designed other plans, and likely missed the
  design's own;
- **The sessions:** per repository, a table with columns File, Session,
  Needs at start, and Increments that need more. Then how many sessions
  can start at once, and the order in which to start them when slots are
  short: first the ones others wait on. Then the longest chain of
  increments that must land one after another, step by step (`temper
  01.1`, `temper 01.2`, …), with its hours at the
  last runs' `chain_step_min`, the median minutes from a step's
  thread's start to its landing (24 in temper's tools run, 26.6 in
  smith's live-text run), and their `chain_step_max`, the longest, as
  its upper bound: no run ends before it. `step_h` falls as steps run at
  once, and a chain step's own time does not: in temper's tools run, the
  README projected the chain at 1.9 to 2.9 hours from `step_h`, and it
  took 7.9 of the run's 8.3; in smith's live-text run, 0.5 to 0.7 hours,
  and it took 2.1, its steps spanning 25.7 to 27.1 minutes against the 5
  to 8 that `step_h` gave;
- **Rules of this plan:** only what this document, the repositories'
  `AGENTS.md` and their workflows do not say. For example: fences ("this
  increment lands after every other skein increment"), version owners,
  probes. A plan never restates this document, nor the rules of a
  test's shape (testing-strategy.md, section 10), which it cites by
  name. Every implementer thread reads them, so they stay under about
  300 lines through the run: a rule for one kind of session goes into
  those sessions' files, and a steer that refines a rule replaces its
  text (section 10.3). In temper's client-api run, the README grew from
  457 to 1,217 lines and its rules to 886; threads reached 96K by their
  30th request, and each 10K less in those requests saves about 5% of
  implementers' input;
- **Live services:** each one the plan uses, with its budget of calls or
  tokens;
- **One test of a suite that needs a built artifact:** for each suite
  whose tests need what the gate builds first (skein's browser suite,
  `SKEIN_UI_EXAMPLE_BUNDLE`), the build-lane command that builds it once,
  and the one that then runs a single test against it, each through
  `heavy` (section 5.5);
- **Process tier:** the command that runs the repository's process-tier
  stories, outside its gate, through `heavy`, in a checkout of
  `origin/main` (temper's `heavy --gate scripts/proof-one-process.sh`):
  the hourly check runs it whenever `main` has moved (section 14.1).
  Otherwise "none", with the reason;
- **Repin:** nothing, when the plan repins every repository downstream of
  it (section 5.7); otherwise "none", with the reason;
- **Kit steps:** in a plan that changes `scripts/dev`, each step whose
  landing its own run takes, with what it gives the run (a landing turn,
  a command); the supervisor moves `kit/` to each by ruling once it lands
  (section 3.6). Otherwise "none", with the reason;
- **Pulse:** what `plan pulse` looks for in this plan's diffs (section
  14), as lines of their own, in a code block:

  ```
  words: actual|exact|genuine|real
  types: ^(Owner|Legacy)[A-Z]|V2$
  file-lines: 800
  budgets: 15 60
  aims: hearth 8 30
  test-names: _and_
  ```

  `words` are the words the plan forbids in code, `types` the names that
  signal a parallel type, `test-names` the pattern no new test
  function's name matches (`_and_` when the line is absent: a test
  checks one behaviour, testing-strategy.md, section 10), which `land`
  refuses (section 9.1) and no Test shape names, `file-lines` the plan's
  file limit, `budgets`
  the focused and fuzzy suites' budgets in seconds, and `aims` the times
  a repository's focused and fuzzy suites stay within, in seconds: the
  base plus what the run may add (section 5.6), a line for each
  repository the README gives aims. When another run lands on the same
  `main`, the aim covers the packages the plan's sessions change, named
  after its two times (`aims: temper 8 30 temper-service temper-forge`),
  and is measured as their tests' summed time, not as the suite's wall
  time: the pulse, the hourly check and "Done when" sum the packages'
  times from the gate's log, which `heavy` keeps (section 11), the pulse
  from the repository's last land gate before each landing. In temper's forge-setup run, the default suite's
  aim missed by 2.06 s with another plan's 510 tests inside it;
- **Done when:** observable conditions, each with the command that checks
  it. Each aim the README states (fewer lines, one engine, plainer words)
  is an item with its command, or is named there as unchecked: an aim
  nothing counts is not met. An item reads a property of `origin/main`'s
  files. It never greps the history, counts a whole file, or names a
  test, a file or a member that a later session's Files may rename or
  move; a live item's command reads the summary its run committed. In
  temper's forge-setup run, five of seven items failed so, after every
  step had landed. A command that reads a value out of a file (a
  revision, a count, a version) fails when it reads none, and a pin
  check matches the repository's name in the lock (`/skein.git#`), not
  its organization (section 4.5). In temper's tools run, item 11 passed
  for hearth on an empty revision, reading skein's index, and the
  organization's rename would have passed it, item 12 and the plan's
  Pins rule for every repository. No command a plan gives writes inside
  the worktree it builds or measures: its logs go under the run's
  directory, since `heavy --gate` refuses a tree with changes. A loop of
  runs fails when any run fails (`|| exit 1` on each), not only when its
  last does. In smith's live-text run, item 8's logs dirtied the tree it
  measured, and smith-bench refused it; its focused runs stopped after
  about 15 of 2,388 tests, and the task exited 0 because the loop's last
  run passed. The supervisor runs each
  once, after `done.md` (section 7.6). A plan that proves with live probes
  counts the probes calibrated in their task files.

**`code-organization.md`:**
- crates, with their kinds and names, and what each holds;
- modules, naming, public API, where tests go;
- each crate's test fixtures: one module (`src/tests/fixtures.rs`, or the
  world's own), made by the first session that tests the crate, extended
  by the others, never redefined;
- one owner per shared interface: the session that defines it, which
  every other session uses by name;
- **the plan's "Don't" list:** what no increment does. It starts from
  this list, which a plan may extend:
  - no type beside an existing one for the same thing: change the type in
    place, and its users in the same increment;
  - no field or predicate that its counterpart across a boundary lacks;
  - no test that breaks a rule of a test's shape (testing-strategy.md,
    section 10);
  - no lint exception in tests;
  - the words the plan forbids in code;
- **size limits:** a file of at most 800 lines and a test function of at
  most 40, unless an increment says otherwise. An increment's size is
  counted in hand-written production lines (tests, generated code,
  goldens and transcripts aside), by the command the document gives,
  written out with the paths it counts; each landing's report gives that
  count by file, against the Size, whose rule (section 5.3) the document
  does not repeat. A move that any of several sessions may need, such as
  splitting a file that every session grows, is a step of its own, owned
  by one session, whose Needs is the file passing a count the document
  sets below the limit (650 of 800 lines, say): never left to whichever
  increment passes the limit. kit-batch left test-plan's split so, and
  three sessions made it: two add/add conflicts, a failed gate, an
  unrecorded landing and the blocks of rulings 105 and 106.

**A session file, `<repository>/NN-<name>.md`,** gives each increment at
most about 100 lines, and is read in one read: at most about 400 lines.
A session that needs more is split. It names the design, the code and
the findings by section, path and symbol, never by line number (in
render, 27 of the 48 corrections at ready checks and re-checks refreshed
line numbers), and restates none of them (section 4.5). In forgejo-17,
the session files held about 400 lines an increment, against 55 to 80 in
jig's, smith's and skein's plans: they brought no fewer defects, three of
them were too long for one Codex read, and their writers cost $42. It
holds:
- **Title and summary:** "Session NN: …", with one paragraph.
- **Read first:** the design sections, decisions and findings it builds
  on, and, by number, the sections of skein's `programming-model.md` and
  `testing-strategy.md` its increments rely on. The implementer reads
  those two documents by these sections only: in client-composition,
  each of the 13 threads read both whole, 14K to 23K tokens that every
  later request carried.
- **What the design asks:** its sections, by number, each with a line on
  what this session takes from it.
- **Choices this plan makes,** within the design's latitude.
- **Rules that bind it:** the two or three rules of the README and
  `code-organization.md` that matter most here, repeated (section 4.5).
- **Increments,** numbered N.1, N.2, …, each in this shape:

  ```
  ### N.k <title>
  Scope: <what it puts in place, with design sections>.
  Needs: <what it needs beyond the session's start, by capability>.
  Files: <crates and files, with shared hotspots named>.
  Shape: <module paths, type and function names, signatures; a type that
    crosses a boundary written field by field, naming what it mirrors>.
  Test shape: <the test file; the fixtures and helpers to reuse, by
    path and symbol; where its worlds' and fixtures' limits come from; one
    reference test written out for each new test file or new kind of
    test; the tests' names; their ceilings (section 5.6)>.
  Proof: <the layer's word, then the test or the script, what it shows,
    its bound (section 5.5)>.
  Don't: <what this increment must not do, beyond the plan's list>.
  Size: <at most N lines of production code>.
  Removes: <what goes, or nothing>.
  ```

  Every Size is at most about 300 lines, and an increment that needs more
  is split. Increments land at about twice their Size: 1.85 to 2.35 times
  in protocol, 2.4 in jig-protocol, 1.6 in reliability. In protocol, 11
  of the 13 increments planned at 600 lines, the old threshold, were
  split by their sessions, and six rulings were about sizes. In skein-ui,
  with no count given, six Sizes were above 300, and two increments
  landed at 2.2 times theirs without asking.
- **Interface sketches:** in Rust, the shape of the interfaces another
  crate or session depends on, or that are easy to get wrong.
  - Signatures only, with no bodies. The exception is an algorithm that
    is easy to get wrong, sketched in at most 15 lines.
  - One doc line per type and variant: what it is, who sends it, its
    terminal.
  - A module path header on each sketch, and a few lines of prose on who
    owns what and what ends what.
  - A sketch shows decisions. A type that crosses a boundary shows every
    field, since its counterpart must match it field for field; others
    may elide fields. Where a sketch and landed code disagree, the landed
    code wins, unless the design asks for the change.
  - A sketch compiles against the base's own crates, which it imports,
    with stand-ins only for what the run adds (section 5.4). In smith's
    live-text run, ruling 101's `Time` and ruling 103's `profile::derive`
    were facts of the base that the sketches' stand-ins hid, and the
    session file they corrected was corrected four times in all.
- **Questions to raise when reached:** only those whose answer depends on
  what the run records (a measured time, a probe's result, a recording).
  What the design leaves open is answered before the file is drafted
  (section 5.4).
- **Done when.**

**`decisions.md`:** numbered decisions, each with a short rationale, and
a closing section "For the user", listing the long-term choices the user
may veto, each an item of its own (a line starting `- ` or `<n>. `). An
item is answered when the user, or the director, records a decision on
it, "no veto" included: a line under the item, before the next one,
`Decision: <the decision>, by <the user or the director>, <date>.`,
landed with `land --docs` (section 5.4).

**`later.md`:** a table with columns Item, Design and When.

### 5.3 Increments

- **A session holds about four or five increments.** An implementer's
  context is near 105,000 tokens at its first change, from what it reads
  before it, and grows by about 50,000 with each increment (protocol's
  medians); past 100,000 it hands over (section 7.3), so a session's
  first thread lands about one increment, and each successor, which reads
  less (implementer.md, 8.1), one or two. An area that needs more is two
  sessions, the second needing the first.
- **A chain of about 20 increments at most.** The plan's longest chain
  runs one increment at a time, so at four or five a session it is about
  five sessions end to end. A step that would make it longer, or a plan
  past about 80 steps, goes to the next plan (section 5.1), never into a
  session of this one.
- **Small, and landed on its own.** Each passes the gate within budget and
  leaves `main` coherent: nothing half-built visible to users, no dead
  code. Prefer several small increments to one large one: parallel
  sessions rebase on each other.
- **An increment past its Size lands without asking,** if it keeps to its
  Files and to the rules for edits outside them (implementer.md, 8.2, and
  the plan's README); its report gives the count by file (8.6). An edit
  outside its Files that those rules do not cover stays a question,
  whatever the count. In render, ten Size blocks were each answered
  "land", and the rule went from twice the Size to four times and was
  passed anyway.
- **The increments that need nothing outside come first,** so the session
  can start at once.
- **An increment whose Needs land before its session's comes first in
  its session,** and the README's "Needs at start" for that session is
  that first increment's Needs: the session starts when they land, and
  its later increments wait within it for theirs. In client-composition,
  jig 08.2 needed only what had landed by 14:48, but came second in a
  session that waited for jig 07, and started at 16:07.
- **Dependencies are named by capability,** for example "skein 03's
  `TimedOut` phase", never by increment number alone: numbers move.
- **Upstream first** (section 9.5): an increment that needs an upstream
  change waits for it, and repins in a commit of its own, at its start.
- **Shared files are named,** so the coordinator sees likely conflicts.
- **Open behaviour stays open.** An increment does not decide what the
  design leaves open if the choice is observable: the designer answers it
  before the session file is drafted (section 5.4), and the increment
  follows the answer.
- **A new test world** costs at most about 1 second focused and 5 seconds
  fuzzy. Anything beyond that is a decision to raise, not one to take.
- **Each increment proves itself** at the highest layer within reach
  (section 5.5). A plan that reads a live suite's verdict places the
  increment that runs the suite's pipeline whole, at one attempt per
  task, before the first step that runs the suite in full (section 5.5).
- **The meeting point comes first.** Where two parts meet, across a wire
  or inside one process (a client and its server, a protocol's two ends,
  a domain and its store, a service and its host):
  - one thin end-to-end story runs as soon as the far side can answer, and
    each increment grows it;
  - neither side runs ahead by more than one session: a session that
    needs an earlier session of its own side also needs, in the README's
    "Needs at start", a story that runs that earlier session's work
    against the other side. A story counts when it runs the real other
    side, at the joint layer or above; a fake does not. Where the other
    side cannot answer yet, the plan builds the thinnest part of it that
    can first, or says in one line why it cannot;
  - where the two speak different vocabularies, the translation starts
    with the first types on both sides, exhaustive both ways, and every
    increment that adds a variant extends it, so that drift is a build
    error the day it happens;
  - a fake's scenarios also run against the real side, so that the fake
    cannot agree with one side where the other does not;
  - it costs some parallelism, and is worth it: it is what lets later
    increments prove themselves at the joint layer. jig's client was built
    against a scripted engine in its own vocabulary, and met the engine
    only near the end of its run; the drift came at once, as a burst of
    design questions and supplementary increments (jig's
    `docs/development/tech-debt.md`, section 9).
- **Test cost holds from the first increment.**
  - The plan cites the repository's rules for what tests may cost (jig:
    `docs/development/workflow.md`, section 2), and gives each increment
    its ceilings (section 5.6).
  - A suite's first breach of its budget is profiled, and the tests are
    made cheaper. A budget's rule is not changed to fit the tests.
- **A bound over a sum of several sessions' parts is computed early.**
  When a test or a "Done when" item bounds a sum over parts that several
  sessions build (a worst case over every layer's holders), the first
  increment that can compute the sum lands the computation, and its
  report gives the number. Each later increment that changes a part keeps
  the computation current and reports the number again. In render, ten
  sessions tested their own layer's worst case; the last increment of the
  last session summed them, 9.5 hours in, and nine supplements and four
  design answers followed.
- **Transitional code has an end.** An increment that lands a change
  piecemeal and keeps transitional code names the step that removes it:
  legacy paths, compatibility constructors, adapters that supply a value
  their source lacks. A plan that allows this ends with a consolidation
  session.
- **A suite that every session adds to ends merged.** A plan whose
  sessions all add to one test suite plans a last step, after the last
  step that edits the suite, that merges the copies the sessions made of
  its captures, polls and helpers, and trims its time, keeping every
  check (testing-strategy.md, section 10). In kit-batch, parallel
  sessions each added their own to test-plan, and a steer against copies
  was logged done and not followed; 01.s2, added by ruling 107, merged
  them and cut 20 s in 11 minutes.
- **A session may reorder its own increments** when an earlier one needs a
  later one, the later one needs nothing the earlier provides, and landing
  it first leaves nothing unguarded or half-built on `main`. It says so in
  its report and its commit body. Reordering never moves work across
  sessions and never changes an increment's scope.

### 5.4 Before the run

- **Each sub-agent gets one question.** The audits and reviews below run
  in read-only sub-agents, each started fresh, never as a fork, which
  would carry the planner's whole context. Each prompt names its one
  question, the files to read, and a report of at most 40 lines. In
  test-budgets, the planning's sub-agents read 283M input tokens, each
  growing to 280K to 400K; with its own session's, the planning read
  more than the run.
- **A code audit.** For the parts the plan touches, the planner checks the
  code against the design, so that the run does not discover that the
  code already differs from it. What it finds goes in `findings/`, and
  into the increments that fix it. A finding marked "not verified" is
  verified (a race reproduced or traced) before a Shape rests on it.
- **A consistency review.** A sub-agent reads the whole plan against the
  design and looks for:
  - dependency cycles;
  - fences that contradict each other;
  - shared interfaces without one owner;
  - increments that cannot land alone;
  - increments without a shape, a test shape or a proof;
  - a session in which a later increment's Needs land before its first
    increment's, or whose "Needs at start" in the README is not its first
    increment's (section 5.3);
  - proofs no higher than their increment's tests without a reason, and
    sides that run ahead of what they meet (sections 5.3 and 5.5);
  - a bound over a sum of several sessions' parts, with no increment that
    first computes the sum (section 5.3);
  - Proof lines that do not start with their layer's word (section 5.5);
  - a `code-organization.md` without the command that counts an
    increment's hand-written production lines, and each Size above about
    300 (section 5.2).

  The planner fixes what it finds.
- **A literal reading.** A second sub-agent reads the plan as the executor
  will (section 4.5), and lists:
  - each instruction that can be read two ways;
  - each rule that nothing checks;
  - each request for evidence without a command, a bound or a count of
    runs;
  - each "Done when" item, against the Files of the sessions that land
    after what it reads: one that greps the history, counts a whole file,
    or names a test, a file or a member those Files may rename or move,
    and each that passes when it reads nothing or names an organization
    (section 5.2);
  - each command that writes inside the worktree it builds or measures,
    and each loop of runs that fails only when its last run does
    (section 5.2);
  - each test-time aim that names no packages, in a plan whose repository
    another run lands on (section 5.2);
  - each test the plan shows, the reference tests first, against
    testing-strategy.md's section 10, rule by rule: one line for each
    rule a test breaks, citing the rule by name. In smith's live-text
    run, the reference tests had bare literals, three behaviours in one
    test and a name with "or", and steer point 2 and ruling 102 came
    after six steps had started.

  The planner fixes what it lists.
- **A design trace, before the session files are drafted.** A third
  sub-agent reads, for each planned session, the design sections it will
  cite against the increments the plan gives it, and lists each
  observable rule that no increment builds or tests and no line leaves
  to later (`later.md`): a key, an event, a dialog, the order of two
  answers. The designer answers each before the planner drafts the files
  that rely on it, and the planner drafts to the answer; a session's
  "Questions to raise when reached" keeps only what depends on the run's
  record (section 5.2). Once the files are drafted, the trace reads them
  once more. In skein-ui,
  four of the seven design questions were in such text: `ui-web.md`,
  section 6, gives `Control+K` or `Meta+K` by platform, and 09.3's Shape
  took no platform.
- **An upstream map, with the trace.** A sub-agent maps each thing the
  design's output prints (each field, cursor, bound and act) to the
  upstream type that carries it, by path and symbol on the upstream
  `origin/main`, in `findings/`. Each with no carrier (a field with no
  default and no `Option`, an act no tool performs, a recipient nothing
  keeps), or past the upstream bound, goes to the designer as the trace's
  gaps do. Each step's Size counts the types it covers: an encoder for
  each of 23 types is 23 encoders' lines. In temper's tools run, all
  eight design questions, J to Q, were such mismatches (50 rows past a
  ceiling of 32, two cursors for one, a read with no row count), five
  came up in temper 03 within an hour, and 03.2's Size of 300 covered 23
  encoders, one per type of jig's contract: 6,008 lines.
- **The plan's commands, run once.** At the plan's base, the planner runs,
  through `heavy` (section 11.1):
  - every command the plan prescribes: each Proof, each count, each "Done
    when" item, and `cargo doc` where a Proof asks for no warning;
  - each type sketch, compiled under clippy with warnings as errors, in a
    scratch worktree, as a module of the crate it extends, or a crate that
    depends on the base's crates: a stand-in defines only what the run
    adds, never what the base has (section 5.2);
  - each limit a sketch, a Test shape or a configuration gives, through
    the check that admits the shipped profile with it (temper: the
    start's checks of `configuration.md`, section 7), beside the
    profile's other numbers;
  - a search for the users of each thing a Shape changes (a type, a
    generator's output, a fixture, a literal, a count): its name, its
    copies by content and the values derived from it, in its repository
    and every repository downstream. They go in the increment's Files: in
    test-budgets, nine of the eleven plan defects were such users;
  - each premise that a bound holds unchanged ("memory needs no
    change"): the command that computes the bound's margin at the base,
    with its output in the plan's `findings/`, and the floor the plan
    names for the margin (section 5.6). A bound whose sum no command
    computes at the base, because its parts are not built, names instead
    the increment that first computes it (section 5.3).

  A command that fails, a count that is not what the plan says, or a
  sketch that does not compile is a plan defect, fixed before the plan
  lands. The fixes of the reviews above get the same run. Each command's
  output goes in the plan's `findings/`, with the command. `plan check`
  refuses a plan unless a `findings/*.md` file holds a line starting
  `$ ` inside a fenced block.

  What cannot run at the base, because it names code an earlier session
  lands, is checked when its session becomes ready: the supervisor's
  ready check (section 7.2) greps its names and compiles its sketches
  against `main` before the session starts.
- **A plan lands finished:** its design questions answered, and its
  commands run. `plan check` refuses a plan whose `decisions.md` has an
  item (a line starting `- ` or `<n>. `) in `## Design questions` with
  no `Answer:` line before the next item or section. When planning must
  be lighter, the writing is cut, never these runs. In forgejo-17, they were what the planner cut: the process
  tier, a "Done when" item, would have shown at the base the panic that
  cost a ruling, a design question and two supplements; uncompiled
  sketches cost three rulings of corrections and about 28 failed build
  rounds; and questions answered after the claim held 02.1 for 9.5
  minutes. The runs would have taken minutes.
- **The user decides before the claim.** `plan check` refuses, as it
  refuses an open design question, a plan whose `decisions.md` has an
  item in its "For the user" section (a heading ending `For the user`,
  numbered or not) with no `Decision:` line before the next item or
  section (section 5.2). The planner sends the items to the user, as
  section 10.4 says, when the plan lands, and the user, or the director,
  lands each decision. In kit-batch, the fence was one of eight such
  items: it idled the run 1 h 31 min, 40% of it, until the director
  pinned client-api's kit, a change of minutes; the other seven had no
  recorded answer.
- **The plan lands** with `land --docs`, and `STATUS.md` gets the plan's
  row, `ready`. The landing commit carries the trailer `Planner-Session:
  <the planner's $CLAUDE_CODE_SESSION_ID>`, one for each of the planner's
  sessions, in the paragraph of its other trailers; the metrics read them
  (section 15.2), and `land` refuses a plan's landing whose lines git does
  not read as trailers (section 9.1).

### 5.5 Each increment's proof

Most of an increment's testing sits at the layer that suits each
behaviour, mostly the lowest, so that development and the suites stay
fast (testing-strategy.md, section 1). The proof is something else: one
check, on top of those tests, that what the increment puts in place works
for real, connected to what is around it, not in theory. It is a smoke
test of the plumbing, not more coverage. The planner chooses it as part
of the increment, with judgement: within reason and where it applies,
balancing confidence against the run's pace and its tokens.

- **The layer.** The proof sits at the highest layer the code reached by
  then allows. From lowest to highest, with jig's as examples:

  | Layer | What runs | jig | Aim for |
  |---|---|---|---|
  | unit | a component's step tests | a domain's unit tests | milliseconds |
  | world | a component with fakes around it, under a referee | the client's world and its scripted engine | 0.5 s per test |
  | joint | the real components together, I/O simulated | the joint world | seconds |
  | process | the real binaries, on this machine | `ops` standalone, in-memory sockets or loopback | about a minute |
  | live | real outside services | a real LLM provider, a browser | at most 5 minutes |

- **Above its tests.** The proof runs the increment's work at least one
  layer above the increment's own tests, the way its caller reaches it: a
  domain's change in the joint world, a client's change against the real
  server, a binary's change by running the binary. Where nothing above
  exists yet (a codec, a type, a step machine that nothing calls), the
  Proof line says so, and names the increment that connects it (section
  5.3). In jig-protocol, 163 of the 180 landed proofs were at the unit or
  world layer.
- **The Proof line** of an increment (section 5.2) gives:
  - **the layer,** first, by its word in the table above: unit, world,
    joint, process or live, which `plan metrics` reads. A second layer
    follows a semicolon, with its own word (`joint, …; live, …`); each part
    that starts with a layer's word counts, so a semicolon in the prose
    names none, and the first part always does, so a line that starts
    with no layer's word shows. A layer is the first word up to its first
    hyphen (`process-tier` is process), save `one-process`, which is
    process; `browser` is live. A split increment is judged at its
    last part. When the layer is not the highest one that exists, or is
    no higher than the increment's tests, the line says why, in one line.
    In forgejo-17, Proofs that began "the real Forgejo" or "the fake
    service" left 9 of 11 uncounted;
  - **what runs:** a test by name, in the suite, which is preferred, or a
    script the implementer writes and keeps in the run's
    `logs/<repository>-<NN>/`, with its output. A script is for what the
    suite cannot hold: live services, real binaries, a browser, a one-off
    run of commands against a running server. A Proof that is the
    repository's gate is `land`'s own gate: the implementer runs no gate
    of its own for it, and runs a story that fails there alone in the
    build lane, with the README's command that builds the story's artifact
    once (section 5.2). In skein-ui, 33 gates run as proofs, 17 of them
    failing, took 2,340 seconds alone on the machine, and `land` gated
    each passing tree again;
  - **what it shows,** as something observable: "the person sees the task
    that needs them first", not "the inbox works";
  - **its bound:** wall time, and for a live service the number of calls
    or tokens.
- **It runs once, when the increment is done.**
  - A rebase does not run it again, unless the rebase's conflicts touched
    what it proves.
  - Nobody proves the proof: it gets no repeated runs, manifests or
    observers.
- **A failing proof is a bug.** The implementer fixes it before landing.
  If the fix lies outside its increment, it reports a question.
- **No Proof lands "unavailable".** A step whose proof's layer cannot run
  (a service, a tool or the machine missing) blocks as infrastructure
  (section 10.2), and lands with that layer once it runs, never with a
  lower layer in its place. A ruling that ignores a test names the step
  that restores it, and each hourly check runs the ignored test on
  `main` until that step has landed (section 14.1). In temper's
  client-api run, three Proofs said "unavailable"; the ignored landing
  story broke for 27 hours unseen, and fixing it took two added steps
  and 13 rulings.
- **It is diagnosed below its layer.** A proof that fails is reproduced
  where a run is cheap, in a world or a focused test in the build lane,
  and runs again only after a change aimed at a cause its log names. The
  same symptom a third time is a question, with the logs (implementer.md,
  8.8). In protocol, 12 steps ran their process and live proofs 99 times,
  3,711 seconds alone on the machine: 15.3 failed 37 of 38 runs, about
  124M tokens, and nine rulings each set one step's budget of runs, while
  a world's run took about 4 seconds.
- **Balance.** Size the proof to the risk.
  - Aim for a proof that costs at most about a tenth of the increment's
    work, and runs in minutes at most.
  - Where the highest layer would cost more, take the next one down, and
    say so.
  - One story, not a matrix: the cases belong in the tests (section 5.6).
  - Most increments prove in the suite. A live proof is for an increment
    that touches a live service, about once or twice a session.
- **A comparison proves only an effect it resolves.** Where a plan
  measures a change against its parent before it lands, on the way to a
  measured verdict (smith's `parity-change`, `benchmarks.md` 16.6), only
  a change whose expected effect its run can tell from none runs alone.
  Any other proves itself on the counts it targets, such as a task's
  responses, and lands; the plan's closing run measures such changes
  together. In smith's wall-clock run, four change runs took 9.3 hours
  of the gate lane, a third of the run, and $69.85: their RT intervals
  were 0.34 to 0.78 wide, against effects of 4% to 22%, and two stopped
  on noise, while the counts the steps targeted moved (responses on
  `shell-cwd`, 70.1 to 54.2).
- **A live proof ends at its task's first terminal failure,** not at its
  deadline, and its usage is counted whole: the step's commit carries
  `Completions:` and `Wall:` trailers that total every live run of the
  plan, the failed ones included (implementer.md, 8.4), which a live
  "Done when" item reads. In protocol, the first live run waited out 849
  of its 900 seconds after its task had failed, a process proof waited
  792 seconds behind it, and its numbers were left in the body's prose.
- **Generated code proves itself downstream.** An increment that changes
  what a generator emits for other repositories runs, once, before it
  lands: in a scratch worktree of each downstream repository with
  generated families, its generator patched to the increment's worktree
  (`cargo --config 'patch."<the upstream's URL>".<crate>.path="<worktree>/crates/<crate>"'`),
  the families regenerated, and that repository's clippy. The planner
  writes and runs that command at the plan's base (section 5.4). In
  test-budgets, two added steps on the longest chain fixed what only
  smith's and jig's clippy saw.
- **A script needed twice is kept.** When a later increment needs the same
  proof, it becomes a test in the suite, or a script under the
  repository's `scripts/`, and the plan names it.
- **Layers open as early as they can.** The plan schedules the first
  joint, process and live stories as soon as the code can run them. Once
  a layer exists, each later increment that it can prove proves there.
- **A live pipeline runs whole, small, before it runs in full.** When a
  plan's aim, or a step's proof, reads the verdict of a live suite
  through a pipeline (the suite, its summary, its checks, its verdict),
  the increment that completes the pipeline proves it live at one attempt
  per task, through each of those parts to the verdict, before any step
  runs the suite in full. A step before it that a stand-in proved, such
  as a recorded summary, says so in its Proof line, and names that
  increment. In smith's parity run, the pipeline first ran live at the
  baseline, 12 hours in: four runs, 117 attempts against 58, and three
  added steps, for faults that one attempt per task shows (a provider's
  answer with no usage, a setup deadline a cold build missed, a summary
  its check refused).

### 5.6 Each increment's tests

The implementer writes tests as the session file shows them, so the plan
shows them precisely.

- **The test shape** of an increment names:
  - the test file;
  - the fixtures and helpers to reuse, by path and symbol;
  - each test's name;
  - for each new test file, or each new kind of test, one reference test
    written out in full;
  - where each world's, fixture's and process configuration's limits
    come from.
- **No expected number depends on the checkout's path.** A test, world
  or proof whose number counts a path (a size, a count of bytes, a worst
  case) lays each path at its bound, so that the number is the same in
  every checkout (testing-strategy.md, section 10). In temper's
  forge-setup run, the owned example's minima moved with the scratch
  directory three times, and a "Done when" run in a longer path found it
  by chance.
- **Limits come from the shipped profile.** Every test, world and
  process-tier configuration takes its limits from the builder of the
  profile the product ships, never set by hand. A test that needs another
  value names it beside the profile's, with the reason. In protocol,
  three production faults (rulings 136, 140 and 153) met only the process
  tier, the first to run the shipped profile, and the shipped limits'
  module cited test fixtures as the source of 61 of its numbers.
- **Tests the plan shows take a test's shape** (testing-strategy.md,
  section 10), and reuse the crate's fixtures module, which grows with
  each session.
- **Ceilings are per test, not shares of a budget:** a unit test under 20
  ms, a focused world test under 0.5 s, a fuzzy test about 5 s, on the
  development machine, unoptimized. A test past its ceiling is made
  cheaper before it lands. It is not merged with others to fit.
- **A suite's time is aimed at for the whole run.** The README states what
  the run may add to each suite in all, which the hourly checks follow
  and "Done when" measures by the median of three gate runs on the gate
  machine, with its logs outside the worktree and a loop that fails on
  any run (section 5.2). A plan gives a session a share of that aim only
  where the share is larger than one gate's swing on the gate machine
  (about 1.3 s on carbon-forge): a smaller share fires on noise. In client-composition,
  shares of 0.2 s to 0.8 s were checked against single gates that swung
  by 1.3 s, and the whole run added 0.69 s.
- **The Proof bounds of one suite sum within what the run may add to
  it,** and a suite every session adds to ends with the step that merges
  it (section 5.3). In kit-batch, the bounds for test-plan summed to 45 s
  against the 25 s the plan allowed, which nothing checked, and the suite
  reached 57.7 s against its 40.
- **A bound the plan holds unchanged has a test.** Where the plan rests
  on a bound holding without change (a worst case within its declared
  limit), it names a floor for the margin (section 5.4), and the first
  increment that adds to what the bound covers adds a test that fails
  when the margin falls below that floor. In smith's parity plan,
  "Memory needs no change" was checked by nothing: the worst case passed
  the shipped limit eight times, six rulings followed, and 28,783 bytes
  of 2 GiB were left.
- **Six tests or so per increment are enough to show.** More adds little.
  Production bodies stay out of the plan (section 5.2): they go stale at
  once.

### 5.7 Repinning downstream

A landing upstream reaches no downstream repository until that repository
repins: each pins its upstream revisions in `Cargo.lock` (section 9.5).
Left alone, the distance grows unseen. On 2026-10-12, fifteen of smith's
hosts lacked a method skein had required for three days, and jig was 338
commits behind smith.

- **A plan repins what it may have broken.** A plan that lands in a
  repository others build on ends with one repin step for each repository
  after its own in the workspace's order (section 3.1: skein, smith,
  jig, hearth, temper) that pins one of them. The plan writes no session
  file for them: this section is theirs.
- **The step,** `<repository>-repin`, in a worktree of that repository at
  `origin/main`:
  1. moves each pin on a repository the plan changed, or that a repin
     before it moved, to that repository's `origin/main`: `heavy cargo
     update -p <one package of it>`, which moves its whole git source.
     jig moves smith and skein together: it resolves one revision of each,
     for itself and for smith's crates;
  2. runs `heavy cargo check --workspace --all-targets`, and repairs what
     breaks;
  3. lands with the repository's gate. Its `Proof:` line is the gate's.
- **What a repin may change:** the pins; items an upstream trait now
  requires; names and paths that moved; the downstream tests that an
  upstream change of behaviour breaks, adapted the way the upstream
  commit adapted its own (its body names the rule). At most about 200
  lines besides `Cargo.lock`, and no change of design. A break that needs
  more, or a decision, is a question: the step lands nothing, and the gap
  goes to the downstream repository's `STATUS.md`, under "Not planned",
  and to the plan's `later.md`.
- **Work left downstream.** What a plan leaves for a repository
  downstream of it, by its own decision, a ruling or an answer, goes the
  same way, whether or not the plan repins it: a row in the plan's
  `later.md`, and the item in that repository's `STATUS.md`, under "Not
  planned", in the designer's next landing (section 10.4). In skein-ui,
  three `later.md` rows for temper's next repin never reached temper's
  `STATUS.md`.
- **When it starts.** A closing repin starts once every step of the plan
  upstream of its repository has landed, dropped or split (a `split` row
  is done once its parts are), and the repin before it in the order. The
  supervisor may add one earlier, `<repository>-repin.s<k>` (section
  10.4), when `plan downstream` shows the repository broken (section
  14.1), most of all when sessions are about to change further the API
  that broke it. The closing repin still runs.
- **A step added after a closing repin has landed.** A ruling that adds a
  step to a repository also adds, for each downstream repository whose
  closing repin has landed, that repin again, `<repository>-repin.s<k>`,
  the first to start once the added step lands, the others in the order
  (section 10.3). Otherwise the landed repin leaves the pin on an earlier
  revision. In render, two skein steps added by rulings after smith-repin
  left smith on an older skein, and "Done when" item 9 failed.
- **A conflict in `Cargo.lock`** is never merged by hand: take
  `origin/main`'s, and run the step's `cargo update` again.
- **Names.** A repin of a repository that the plan's own repository never
  names (jig never names temper, section 3.1) appears only in the run's
  files: the ledger, the log and the step's report. The plan's own files,
  `decisions.md` with the rulings and answers appended to it and
  `later.md` included, name such a repository as the plan's README does
  ("the repository downstream of jig").
- **Another machine's run** in the downstream repository sees a repin as
  `main` moving (section 12).

## 6. Starting a run

1. **The plan is on `origin/main`,** with its row `ready` in `STATUS.md`.
2. **The machine is ready.** `plan check --fix <plan-dir>` passes. It
   checks:
   - the plan and its `STATUS.md` row, and that the plan is finished: a
     base run in `findings/`, its design questions answered, each of its
     "For the user" items decided (section 5.4), and its Inputs naming
     the commits that wrote its design (section 5.2);
   - that the user, or the director, has decided each proposal of the last
     retrospective: the one added last to any repository's `origin/main`
     has a row for each of its proposals in its section 10, and none of
     them open, nor any adopted still to apply (section 15.1). A
     retrospective's proposals left undecided recur: six of
     jig-protocol's did, in test-budgets;
   - the primary checkouts of the plan's repository and of those upstream
     of it: on `main`, clean, equal to `origin/main`, with an `origin` that
     takes a push (`git push --dry-run`);
   - the code graph: each of those repositories' index against its
     `origin/main`. An older index is a warning, not a failure: the
     supervisor re-indexes it (step 4);
   - the tools, pueued and the queue's lanes, memory and disk;
   - that Claude Code trusts the workspace, Codex's daemon, and the
     coordinator's Codex profile;
   - the queue and the gate, run once: one `heavy true`, then the plan's
     repository's `scripts/check.sh` through `heavy --gate` on `main`,
     unless `land`'s cache holds a pass for `main`'s tree. A machine that
     cannot run them would block every session at its first landing.

   `--fix` links the tools, starts pueued (as a user service where
   systemd has a user bus) and Codex's daemon, and creates and starts the
   lanes. It repairs pueued's unit without restarting a running daemon
   and warns if unattended upgrades may reboot the machine. It fixes no gate: a gate that fails on `main` goes to the user.

   It does not yet check whether the machine may reboot by itself (apt's
   `Unattended-Upgrade::Automatic-Reboot`): a reboot stops the run until
   someone runs `plan resume`, and the supervisor, on the same machine,
   cannot tell anyone. In protocol, an upgrade's reboot stopped the run
   for 98 minutes. Whether a machine reboots during a run is the user's
   choice.
3. **`plan start <plan-dir>`** does the rest, and returns:
   - it runs `plan check`, and claims the run, naming it in
     `~/src/rust/tmp/active-plan` and adding its kit (section 3.6);
   - it starts the supervisor, a background Claude Code session (Opus 5.5,
     xhigh) in the workspace, named `<plan> supervisor`, with "Supervise the
     run of `<plan-dir>` as `<run>/kit/docs/foundation/supervisor.md` says.";
   - it starts the coordinator, a thread on Codex's daemon named `<plan>
     coordinator`, with its profile's dotted config settings and the code
     graph server disabled, and "Coordinate the run of `<plan-dir>`, as
     `<run>/kit/docs/foundation/development.md`, section 7, says.";
   - it rings the doorbell once, which the coordinator logs (section 7.4).
     If the ring fails, steers wait for the coordinator's next wake-up.
4. **The supervisor, first** (`supervisor.md`, section 1):
   - marks the plan `active on <machine> since <date>` in each `STATUS.md`
     that has its row, with `land --docs`; the row is what keeps another
     machine from claiming the same plan;
   - compares each plan repository's graph index with its `origin/main`
     and re-indexes one that is older; until it has, steers the
     implementers away from the graph (section 14.1, step 2);
   - starts its watch, `plan watch`, in the background (section 10.5);
   - handles `hourly:` from the watch with its hourly check (section 14).
5. **The designer joins,** if the plan may raise design questions: in its
   Claude Code session, it runs `plan join designer`, so that the
   supervisor can wake it (section 10.4), then ends its turn. It does not
   watch the run: no watch, loop or cron job. `plan wake` brings it back
   with each question, or starts a fresh designer from a brief (section
   10.4). Watching costs its whole context at every wake (49M input tokens
   in tls-cleanup, with no question asked); an observer, if the user wants
   one, is a fresh session with a small context.
6. **The user looks in when they want:** `plan status` shows the steps and
   the roles; `plan attach supervisor` and `plan attach coordinator` open
   their sessions.

A session that takes over from another, supervisor or designer, records
itself with `plan join <role>`, unless `plan handover` or `plan wake`
started it, which record it themselves.

## 7. Coordinating

### 7.1 Setting up

1. Read this document's sections 3.5, 3.6, 7 and 9 to 12, `implementer.md`'s
   section 8.6 (what reports say), both in the run's `kit/docs/foundation/`
   (section 3.6), the plan's `README.md` and `code-organization.md`, and
   the head of each session file: its increments and dependencies.
2. Write your thread's id to the run's `thread` file:
   `printf '%s\n' "${CODEX_THREAD_ID:-unknown}" > <run>/thread`. If it
   writes `unknown`, say so in `log.md`.
3. Fill the ledger: one `plan set <step> repo=… session=… title=…
   state=…` per increment. A step is named `<repository>-<NN>.<item>`, for
   example `smith-05.3`. Add the plan's repins (section 5.7), unless its
   README says "Repin: none": one `<repository>-repin` step, `waiting`,
   for each repository after the plan's own in section 3.1's order whose
   `Cargo.lock` pins one of the plan's repositories.
4. Write `context.md`.

### 7.2 Starting sessions

- **Start every session whose start dependencies have landed,** unless
  memory or disk is short (section 11.4). The sessions that can start at
  the claim start at once: the planner checked their files at the plan's
  base (section 5.4).
- **The ready check.** Any other session waits for the supervisor to check
  its file against `main`. As soon as every session it needs at its start
  is running, or has landed what it needs, `plan set <its first step>
  state=check`, and go on with everything else: the supervisor checks it
  while they run. When it has checked it, it runs `plan ready <step>`,
  which sets the step `ready` and rings: "ready: <step>". Start it as soon
  as its needs have landed, then or later: a re-check never holds a start.
  The supervisor re-checks what `main` moved after its check, and sends
  what it finds as a steer. Repins (section 5.7), restarts, handovers and
  steps added by an unblock or a steer start without a check. In
  forgejo-17, re-checks held 02.1 for 9.5 minutes after its need landed,
  and 04.1 for 11, for a compile error and line numbers that an
  implementer's first build finds in seconds. In client-composition,
  checks started only after the landing held sessions 103 minutes in all,
  about 70 of them on the chain that set the run's length. In skein-ui,
  checks set once all but one need had landed still ended after it landed
  11 times in 16.
- **When there are fewer slots than sessions,** start first the sessions
  others wait on, in the README's order. Never hold a slot idle to keep
  the order.
- **The worktree:** `plan worktree <repository> <NN>`.
- **The spawn.** Spawn with `fork_turns: "none"`, so the implementer does
  not inherit the coordinator's conversation, and give it this prompt:

  ```
  You are an implementer. Read <run>/kit/docs/foundation/implementer.md,
  then ~/src/rust/<repository>/AGENTS.md, then <plan-dir>/README.md,
  its "Rules of this plan" first, and <plan-dir>/code-organization.md, then
  <plan-dir>/<repository>/<file>, and nothing else of the process.
  Implement that session in <worktree>, one increment at a time. Your
  checkpoint is <run>/sessions/<repository>-<NN>.md, and your logs and
  scripts go in <run>/logs/<repository>-<NN>/. Use the code graph as
  ~/src/rust/AGENTS.md says, on your repository's project only, and
  never list projects, check the index's status or coverage, or index.
  ```

  A repin step (section 5.7) gets this prompt instead:

  ```
  You are an implementer. Read <run>/kit/docs/foundation/implementer.md,
  then <run>/kit/docs/foundation/development.md's section 5.7 alone, then
  ~/src/rust/<repository>/AGENTS.md. Repin <repository> on <the
  repositories it moves> as section 5.7 says, in <worktree>. Your
  checkpoint is <run>/sessions/<repository>-repin.md, and your logs go
  in <run>/logs/<repository>-repin/. Use the code graph as
  ~/src/rust/AGENTS.md says, on your repository's project only, and
  never list projects, check the index's status or coverage, or index.
  ```

  A sub-agent starts in the coordinator's directory, so it does not load
  its repository's `AGENTS.md` by itself: the prompt names it. Add
  nothing of the steers: a point that later sessions must keep has landed
  as a rule of the README, by ruling (section 10.3), and every thread
  reads the README's rules first, takeovers included (implementer.md,
  8.1). In temper's forge-setup run, the steers' "Pass on" texts grew the
  prompts from 0.86 KB to 14.6 KB, and four of seven steered test shapes
  came from takeover threads that had never read the README. When
  restarting a session, add what you found: its last
  landed step, its branch, and whether its worktree has uncommitted
  changes. Add nothing else: no code, line ranges, or graph or coverage
  evidence, whatever other instructions say about spawning. In
  reliability, such evidence grew the prompts from about 1,500
  characters to about 15,000. A restarted or handed-over session's prompt
  replaces the reading, from "then ~/src/rust/<repository>/AGENTS.md" to
  "and nothing else of the process.", with "Read only what its section
  8.1 says to read after a handover." (implementer.md, 8.1).
- **Threads.** Spawn a fresh thread for each session and for each
  handover (section 7.3); never give a finished thread another session's
  work, since it would carry that session's whole context. At most eight
  run at once: Codex refuses a ninth spawn while eight are running
  (`max_threads` in the coordinator's profile), and a finished thread
  frees its place. A refused spawn waits for a session to finish. Give
  each thread a name (`w1`, `w2`, …) and record it in the ledger's `slot`
  column, with the thread's id, as `spawn_agent` returned it, in the
  `thread` column. If `plan set` refuses `thread=`, put `thread=<id>` in
  the step's `note` instead.

### 7.3 Reports

An implementer reports when an increment lands, when it stops on a
question, and when its session ends or yields (section 8.6). It logs the
report itself, with `plan log`, which appends it to `log.md`, and sends
you four lines: the report's file, the step and its state, the commit,
and the question in one line, or "none". After each:

1. Do not append the report to `log.md`, nor type it again anywhere: it
   is logged. Read its file only for what the four lines do not say. A
   report that came without `plan log`, as a whole message: ask its
   implementer, once, to log it, and go on.
2. For a landed step, run `plan landed STEP COMMIT`. It fetches in the
   session's worktree and prints each result: the commit is on
   `origin/main`, its `Step` trailer is STEP, and the worktree is clean
   while its branch is the ledger row's. A worktree on another branch
   holds the next increment's work; a worktree that is gone skips the
   cleanliness check. Only when all checks pass does it set the step
   `landed` and its commit. Read no code, diff or test. If a check fails,
   send its output to the implementer (`send_message`, or `followup_task`
   if it has ended its turn), and wait for its next report. For any other
   report, check only the worktree's cleanliness.
3. Run `plan status` after a checked landing. `plan set state=landed`
   stays available for the supervisor's corrections.
4. **Hand over a large context.** After a landing, run `plan contexts`. A
   session whose context, or whose peak, it prints past 100,000 tokens,
   flagged or not, hands over, between increments, never in one: a
   compaction shrinks the last request but not the peak, and `plan
   contexts` flags only past `PLAN_CONTEXT_LIMIT`. In smith's parity run,
   handovers past 200K came at up to 329K, and 06.2 started at 164K and
   cost 24.2M against 06.1's 8.6M; a steer to hand over past 150K brought
   them to 156K to 186K. In hearth's render, with increments of 500 to
   1,000 lines, sessions past 150K at a landing reached 182K in median and
   344K at most:
   - send it, with `send_message`: "Hand over: write your checkpoint as
     section 8.6 says, then end your turn with your report.";
   - when it has reported, run `plan handover REPOSITORY NN CONTEXT` to append its handover
     line to `log.md`, and start the session's next
     increment in a fresh thread, as for a restarted session (section
     7.2), with its checkpoint.

   A session that ended its turn, on a question or a yield, is checked
   before it is resumed, and its limit is 100,000 tokens: when its
   context, the first number `plan contexts` prints for it, is past 100K,
   flagged or not, its answer or its next step goes to a fresh thread
   instead of a `followup_task`. So does a session that has stood idle
   for over an hour since its last report, whatever its context, since
   its cache has gone. The fresh thread starts from its checkpoint, as
   after a handover. In protocol, w26 was resumed at 171K after 4 hours
   idle, and became the run's dearest thread, and w45 grew from 122K to
   252K in 14 minutes after 3 hours idle. In skein-ui, twelve turns
   resumed at 123K to 184K took 28% of the implementers' input.
5. Start what the plan now allows, and resume the sessions waiting for
   what landed.

A report's question goes into `blocked.md` (section 10.2), with the
report's file and the question's line, unless it is only a dependency not
yet landed: that is not a block, and the session is resumed when the
dependency lands.

**Steps not in the plan.**
- **You start a step only from the plan, an unblock or a steer.**
- A step that an unblock or a steer adds is named as the instruction
  names it, `<repository>-<NN>.s<k>`. Its ledger note reads
  `supplement: <the instruction's time>`. The instruction names its
  session file; the step goes to that session's implementer if it is
  running, and otherwise to a new implementer, started as section 7.2
  says, with that file.
- When a session splits an increment, the coordinator sets its row `split`
  (note: the parts) and adds `<NN>.<M>a`, `b`, ... with the note
  "part of <NN>.<M>".
- When a report shows work that no step covers, write it to `blocked.md`
  as a plan defect, and go on with everything else. Do not create a
  session, a step, a rule about who owns which files, or a protocol
  between sessions.

**A step's state** in the ledger is one of:
- `waiting`: something it needs has not landed;
- `check`: every session it needs at its start is running, or has landed
  what it needs, and its session file waits for the supervisor's ready
  check (section 7.2);
- `ready`: it can start;
- `running`: its session is on it, with the slot and the branch;
- `blocked`: on an escalation, named in the note;
- `landed`: with its commit;
- `dropped`: by an instruction, named in the note;
- `split`: replaced by its parts, named by letter, `<NN>.<M>a`, `b`, ...;
  counts in none.

### 7.4 Waiting

- **One wait.** Between events, call `wait_agent` with `timeout_ms:
  300000`. This rule takes precedence over Codex's guidance against
  waits longer than 60 seconds. The wait ends early on any implementer's
  message, and on the doorbell. Never wait less: the pulse counts each
  Codex role's waits that ran out sooner (section 14.2). In temper's
  forge-setup run, 331 waits of 60 s made 37% of the coordinator's
  requests.
- **At every wake-up,** run this one command, and act on what it shows:
  `touch <run>/heartbeat; ls <run>/unblock.md <run>/steer.md 2>/dev/null;
  free -g; df -h ~/src`.
  - If `unblock.md` is listed, handle it (section 10.3); then `steer.md`,
    if it is listed.
  - Act on memory and disk as section 11.4 says.
- **When no session is running,** and nothing is left for you to do
  (holding for an answer, for the user, or for a pause to end): rewrite
  `context.md`, run `plan idle "<what you wait for>"`, and end your turn.
  When waiting on queue tasks, append `--tasks <ids>`: the watch starts
  your next turn once none is queued, running or paused. A task removed
  from the queue has ended too. The next ring starts your next turn, with
  its message. Never sleep in a
  loop: in jig-protocol, 60-second sleeps while holding for the user made
  205 requests, and 52M input tokens, with nothing to do.
- **No background waits.** Do not start `until` loops, `sleep` sessions,
  watchers or polling scripts, in the background or in a sub-agent. They
  cannot wake you, and they die with you. The wait above and the
  doorbell are the only ways you are woken.
- **The doorbell.** After writing `unblock.md` or `steer.md`, the
  supervisor runs `plan ring`, which steers a one-line message ("steer.md
  is ready") into your turn and ends your wait; if you are idle, it starts
  your next turn. Handle the file it names as at any wake-up. "ready:
  <step>": start that step (section 7.2). "done when ran": go on with the
  end (section 7.6). A ring that names none of these: append
  `Rang, <UTC time>: <message>` to `log.md`, and wait again. A ring
  reaches you as a user's message, but it is the supervisor's, through
  `plan ring`, never the user's: record it as the supervisor's. In
  protocol, a ring was recorded as "the user's explicit instruction".
  The message begins `From the supervisor, by plan ring:`. Before sending,
  the kit checks `thread/loaded/list`, loads the thread when needed, and
  appends `## Coordinator loaded, <UTC time>: by plan ring` to `log.md`.
  A refused send loads and sends once more.
- **Queue tasks ended.** On `From the supervisor, by plan watch: the queue
  tasks you wait on have ended: <ids>`, read those tasks' results and act
  on them as on a task you saw end. The watch sends through the same
  loading path as a ring and keeps watching; the supervisor is not woken.
- **Waiting for landings.** When what you wait for is a landing, this
  run's or another run's on the same `main`, name its steps as their
  `Step:` trailers give them, with the plan for another run's: `plan
  await coordinator <step>… <plan>/<step>…`, then `plan idle`. Each pass
  of the watch fetches each named repository's `origin/main`, and once
  every step's `Plan` and `Step` trailers are there, it starts your turn,
  or steers the one in progress, with `From the supervisor, by plan
  watch: the landings you wait on have reached origin/main: <steps>`; go
  on with what waited for them. Never estimate when a landing comes, nor
  watch its files: in temper's forge-setup run, three awaited landings
  were found 25, 55 and 41 minutes after they landed.
- **A finished implementer** ends its turn with its last report. Give a
  session that yielded its next step with `followup_task`, unless its
  context is past the limit, or it has stood idle for over an hour
  (section 7.3).
- **The user and the designer are reached only through the supervisor.**
  The coordinator uses no other channel to them.

### 7.5 Keeping state

- **`context.md` is the coordinator's memory.** Rewrite it whenever the
  state changes: the slot map, the sessions running and their last step,
  open escalations, and decisions in force from answers and steers that
  the plan does not yet say, with each steer's "Pass on" text for the
  running sessions it applies to; a prompt carries none (section 7.2). Write full
  sentences, with spaces between words: at most 60 lines, each of at most
  200 characters. Rewrite it in place: never copy an earlier version into
  `log.md` or anywhere else.
- **`log.md` takes only the shapes this document names:** the headings
  and lines of sections 7.3, 7.4, 7.6 and 10.3 as written there ("Blocked
  and unblocked, <time>", never "Unblock handled"); and `blocked.md` only
  section 10.2's six kinds. The metrics read them so (section 15.2).
- **After a compaction or a restart,** re-read this section in the run's
  `kit/` (section 3.6), `context.md` and the ledger before acting.

### 7.6 Pausing, resuming, ending

- **Pause** at the user's or the supervisor's request: the supervisor
  asks for one, by steer, when no session is running and nothing can
  start for an hour (supervisor.md, section 6). Start nothing new.
  Tell each running session, with `send_message`: "Pause: land your
  increment, or commit it on your branch; report; end your turn." When
  each has reported, write `paused.md`: each session's worktree, branch,
  head and last landed step.
- **Resume after a restart or a pause.**
  1. A paused run first takes skein's `main`: `plan resume` moves `kit/`
     to skein's `origin/main`, appends `## Process, <UTC time>: skein
     <commit>` to `supervision.md`, prints `kit: skein <old>, moved to
     main's <new>`, and resumes with that kit, whose process is the
     run's from then on (sections 3.6 and 12). Nothing runs across a
     pause, so no session meets a change of kit mid-step. In temper's
     forge-setup run, a run idle 30 hours on its claim's kit met, once
     resumed, six faults that `main` had fixed.
     `plan resume [CAUSE]` appends the heading "Coordinator resumed,
     <UTC time>: <cause>" to `log.md` itself. It takes the given cause;
     else `reboot` when `uptime -s` is after the heartbeat's mtime;
     else `the user's request` when `paused.md` exists; else `unknown`.
     A missing heartbeat is no error. Read `context.md`, the ledger and
     the end of `log.md`.
     Append `paused.md`, if there is one, to `log.md`, and delete it.
  2. Write your thread's id to `thread` again: a restarted coordinator may
     have a new one.
  3. Check with git: each worktree's branch, whether it is clean, and
     whether its work has landed.
  4. Run `plan check`.
  5. Restart each session that was running, with what you found. A
     worktree with uncommitted changes belongs to a session that stopped
     mid-increment: its implementer inspects them, then finishes or
     discards them. The coordinator never discards them itself.

  `plan start` enables `plan-resume.service` in the user manager, after
  pueued, so a reboot runs `plan resume` automatically. `plan release`
  disables and removes it only if it names this run's `DEV_TMP`; a unit
  for another run stays, with a warning. Both units live under
  `${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/`.

  After a restart of the machine, the user may also run `plan resume reboot`;
  after a crash, the supervisor runs `plan resume crash` (section 10.5).
  It starts
  pueued and Codex's daemon if they are down, resumes the supervisor's
  session (which starts its watch again; it prints `hourly:` when a check
  is due), and
  starts a coordinator turn with "Resume the run of `<plan-dir>` as
  `<run>/kit/docs/foundation/development.md`, section 7.6, says." The
  coordinator starts no Claude session.
- **End** when every step has landed, dropped or split (a `split` row is
  done once its parts are).
  1. Write `done.md`, starting with `Recorded <UTC time>.`, so the
     metrics read the run's end before later checks append to it:
     - each item of the plan's "Done when", with the ledger steps that
       cover it; run nothing;
     - the decisions in force from unblocks and steers, as `context.md`
       lists them.

     The supervisor adds to each decision the design commit that took it
     in, or "not in the design yet". It then has a sub-agent run each
     "Done when" command once, in a path longer than any session's, and appends each command, with its
     output, to `done.md` (section 5.2). A command that fails gets a
     ruling (supervisor.md, section 3): a step that fixes it, or the miss,
     recorded.
  2. `plan idle "the Done when run"`, and release nothing until the
     supervisor rings "done when ran": every command has passed or has its
     ruling.
  3. Remove the worktrees.
  4. `plan release`. The run's directory stays.

  The supervisor marks the plan `done <date>` in `STATUS.md` when it rings,
  and watches until the release, which reaches it as any event does
  (supervisor.md, section 8); the retrospective follows (section 15). In
  hearth's render run, the release came 40 seconds before the commands
  began and `STATUS.md` said `done` before they ended; item 9 failed, and
  the run could no longer add a step.

## 8. Implementing: the implementer's card

The implementer's card is `implementer.md`, beside this document: what an
implementing agent reads instead of this one. Its sections keep their
numbers, so "section 8.5" names the same rule here and there:

- **8.1 Each increment:** what to read, and nothing else; the steps from
  branch to landing; the code graph.
- **8.2 Rules:** the design as the contract; implementation choices; the
  edits outside Files that need no question; test ceilings and table
  tests; evidence.
- **8.3 Heavy commands:** the lanes, a committed and built tree before the
  gate lane, and how to wait.
- **8.4 Commits:** the message and its trailers.
- **8.5 Landing:** `land`'s exit statuses, and what each asks.
- **8.6 Reports and the checkpoint:** a report logged with `plan log`, and
  the four lines sent to the coordinator.
- **8.7 When to stop:** a real gap, after looking for the rule that
  answers it.
- **8.8 The proof.**

## 9. Landing

### 9.1 What `land` does

From a clean worktree on the branch to land, `land`:

0. for a code landing, takes its repository's turn on this machine, a
   lock in the repository's git directory, from the rebase to the push.
   Another code landing waits, saying so, to keep main from moving under
   its gate. A Markdown-only landing takes no turn: a gate holds across
   it (section 9.2);
1. rebases the branch on `origin/main`, and refuses (exit 2):
   - a commit whose `Plan:`, `Step:` or `Proof:` line git does not read as
     a trailer, so that `plan status` and the metrics can read it;
   - a commit whose `Planner-Session:` or `Designer-Session:` line git
     does not read as a trailer, so that the metrics count the planning
     and the design (section 15.2). In render, the plan's nine lines stood
     in a paragraph apart from its trailers, and $221 of planning went
     uncounted;
   - code that adds a word the active plan's Pulse forbids, on a branch
     that carries that plan's steps;
   - a test function, added on such a branch, whose name the Pulse's
     `test-names:` pattern matches, `_and_` when it has none (section
     5.2). In temper's tools run, eight steer points, three README rules
     and a withdrawn mend on test shape left 19 new tests named with
     "and"; render's proposal 6, that a rule sent twice becomes a gate
     check, had not been built;
2. unless a gate already passed on this tree, or on one that differs from
   it only in Markdown files under `docs/` (section 9.2):
   - in a Cargo repository, builds the tree and runs its lints in the build
     lane, `LAND_PREBUILD`: by default `cargo fmt --check && cargo clippy
     --workspace --all-targets -- -D warnings && cargo nextest run
     --workspace --no-run`. A failure here is exit 4, and costs the gate
     lane nothing;
   - runs the repository's gate, `scripts/check.sh`, through `heavy
     --gate`. Its own build and lints then find nothing to redo, so the
     lane that runs alone holds the machine for the suites: in
     jig-protocol, a passing gate spent 65 of its 98 seconds alone on
     building and lints, and the build lane stood paused 76% of the run;
3. writes the evidence into the tip commit's trailers;
4. pushes the tip to `origin`'s `main`;
5. moves the local `main`, and the checkout that has it, to the tip.

A push only fast-forwards. If it is rejected, main moved from another
machine or through a Markdown landing, and `land` starts again from the
rebase. It gives up after five
tries. Its exit status:

| Status | Means |
|---|---|
| 0 | landed |
| 2 | refused before starting, a commit's trailers, a forbidden word or test name, or nothing to land |
| 3 | the rebase stopped on conflicts, left in progress |
| 4 | the build, its lints or the gate failed |
| 5 | the local `main` could not follow: with a push, the commit landed on `origin/main`; with `--local`, nothing landed |
| 6 | main kept moving, or its ref stayed locked: a lost ref-lock race is retried like a moved main, and exits 6 when the retries run out |
| 7 | fetching or pushing failed |
| 8 | another command failed |

### 9.2 Evidence

- **`Gate-Tree`** is the tree the gate ran on. **`Gate-Result`** gives the
  test count and time of each suite, in the order the gate runs them, and
  the time the gate ran alone.
- **Amending the message keeps the tree,** so the evidence holds for what
  lands.
- **A tree already gated is not gated again,** with the same gate and the
  same compiler. `land` keeps the results under `~/src/rust/tmp/gates/`.
- **A gate holds across Markdown under `docs/`.** No gate reads those
  files. When `main` moved since a gate passed, and the commits that
  moved it change only Markdown files under `docs/`, `land` keeps that
  gate: its `Gate-Tree` stays the tree the gate ran on. A supervisor's
  ruling, a design answer or a plan's change no longer costs every waiting
  branch a gate.
- **Anyone can check the evidence:** the landed commit's tree equals its
  `Gate-Tree`, or differs from it only in Markdown files under `docs/`.

### 9.3 Conflicts

A rebase that stops on conflicts is left in progress, for the branch's
author to resolve. `land` exits 3. An implementer resolves its own
conflicts (section 8.5), and escalates only when resolving them needs a
decision.

### 9.4 Markdown only

`land --docs` checks that the branch changes only Markdown files, and a
plan's `metrics.tsv`, and skips the gate. It is how designs, plans,
`STATUS.md` and retrospectives land. Rust doc comments are code, and are
gated.

### 9.5 Pins across repositories

- **Dependencies on other repositories of the workspace** are git
  dependencies on their GitHub URLs, pinned by `Cargo.lock`.
- **Pin only revisions on the upstream `origin/main`.** A downstream
  increment waits until the upstream change has landed.

### 9.6 The local main

- **The local `main` only follows `origin/main`.** `land` fast-forwards
  it, and the checkout that has it, after each push.
- **If the local `main` has commits that `origin` lacks,** `land` pushes
  but cannot follow, and exits 5. The coordinator writes it to
  `blocked.md` (kind: process). The unblock names who reconciles: rebase
  those commits on `origin/main`, then land them.
- **Never force-push, and never move `main` backwards,** without the
  user's approval.
- **A repository that does not push yet** lands with `land --local`: on
  the local `main`, under a lock, with no push.

## 10. Escalation

### 10.1 From the implementer

An implementer stops on a real gap (section 8.7), logs its report with
the question in it (`plan log`), and sends the coordinator the report's
four lines (section 8.6).

### 10.2 From the coordinator: `blocked.md`

The coordinator writes the escalation to `blocked.md` in the run's
directory, then keeps everything else going.

```
# Blocked

## <repository> <NN>.<item>: <topic>, <UTC time>

- Kind: design gap | plan defect | cross-session scope | process |
  infrastructure | credentials
- Since: <time>. Still running: <sessions>.
- Needed: <the decision, in one sentence>.
- Report: <the report's file, as its four lines name it>.
- Question: <the question's line, as sent>.
```

The supervisor reads the report from its file: nobody types it again.

- **One file.** A new escalation while one is open goes under a new
  heading in the same file.
- **The coordinator never answers** a design question itself, and never
  writes `unblock.md`.

### 10.3 From the supervisor: `unblock.md` and `steer.md`

```
# Unblock

Answer to the blocked report of <time> (<headings answered>).
Reverses: <the time of each earlier ruling or steer this one replaces any part of>, or nothing.
Amends: <the plan or design text this changes, by file and section>, or nothing.

## Decision

<The decision, quoting the design or plan text it rests on: the ruling's
whole text, written once; for a designer's answer, only its decision
part, at most 40 lines (section 10.4).>

## Checked

<Each premise the decision rests on, about the code, the run or the
machine: the command that checked it, and its output, at most three
lines each.>

## Landed

<Design and plan commits, with their hashes: plan ruling fills it.>

## Instructions

1. …
```

The headings answered are each written whole, as `blocked.md` has them
without `## `, separated by `; `.

- **The supervisor writes an unblock once,** as `unblock.draft.md`, and
  runs `plan ruling "<title>" "<kind>, <session>'s block of <time>"`. It
  appends the Decision to the plan's `decisions.md` as the next ruling,
  lands it with the plan edits in the supervisor's worktree (`plan docs`),
  fills in Landed, moves the draft to `unblock.md`, and rings. The ruling
  is typed nowhere else: in the second supervisor session of jig-protocol,
  its own writing, each ruling typed four or five times, was about 14% of
  its input. A
  plan change a steer rests on lands the same way, with `plan ruling
  --text <file>`, which rings nothing. `plan ruling` refuses while the
  worktree's branch has commits that `origin/main` lacks, a ruling that
  did not land: the supervisor lands it there, or drops it, and rules
  again (supervisor.md, section 3). In temper's forge-setup run, a kit
  that switched the branch back lost eight rulings' edits.
- **Every premise shows its check,** in `## Checked`. `Reverses:` names
  each earlier ruling or steer that this one replaces any part of, by
  its time, and `Amends:` the plan or design text it changes; the
  metrics count the first (`answers_reversed`, section 15.2). In temper's
  forge-setup run, `answers_reversed` counted 2, and the rulings' text
  replaced at least eleven. In reliability, 10 of the 11 reversed answers rested on a
  premise nobody checked, and 23 of 156 unblocks quoted a command. A
  command that a ruling or a steer tells a role to run is a premise
  checked against the run's `kit/`, not `main`: `grep -n '<command>'
  <run>/kit/scripts/dev/<tool>` (section 3.6).
- **A ruling that adds a step names, in its Instructions,** the step and
  its session file, and the repins it adds with it (section 5.7): each
  `<repository>-repin.s<k>`, with the step it starts after. `plan status`
  checks which closing repins have landed.
- **When the coordinator finds `unblock.md`:**
  1. it runs `plan unblocked`, which logs both files whole under
     "Blocked and unblocked, <time>", removes the block headings named
     by each `Answer to` line and deletes `unblock.md`; it prints an
     answer line that names no entry and removes none for that line;
  2. `blocked.md` remains with only the unanswered entries, or is deleted
     when none remains;
  3. it gives each session the answer concerns its Decision and
     Instructions, word for word (`send_message`; or `followup_task` if
     the session has ended its turn; or a fresh thread, from its
     checkpoint, if it ended its turn past 100,000 tokens or has stood
     idle for over an hour, as section 7.3, step 4, says), and `plan
     set`s its step back to `running`;
  4. it follows the instructions, which take precedence over the plan
     where they differ;
  5. it updates `context.md`.
- **`steer.md`** is an instruction nobody asked for: a plan amended, a
  pause, a rule changed, a course corrected after a check (section 14).
  Each point has this shape:

  ```
  # Steer

  <UTC time>, from the check of <UTC time>. Follow it before the plan
  where they differ.
  Reverses: <the time of each earlier ruling or steer this one replaces any part of>, or nothing.

  ## 1. <topic>

  - Seen: <the number, or the log line>.
  - Do: <the instruction, in the imperative>.
  - Stop: <what to stop doing>, or nothing.
  - Applies to: <the coordinator, or which sessions>; <from now, or from
    their next increment>.
  - Until: <an event, or the run's end>.
  - Unchanged: <what this does not change>.
  - Pass on: <text for implementers, given word for word>, or nothing.
  ```

  A point that later sessions must keep lands first as a rule of the
  README's "Rules of this plan", with `plan ruling --text`, and its "Pass
  on" names the rule: the sessions started later read it there
  (section 7.2). A point that refines a rule replaces that rule's text,
  never adds beside it, and a point for one kind of session goes into
  those sessions' files, so the rules stay under about 300 lines
  (section 5.2). In temper's client-api run, 15 steer points were
  refused because their text was not yet in the README.

- **When the coordinator finds `steer.md`:**
  1. it runs `plan steered`, which logs it under "Steered, <time>" and
     deletes it;
  2. it acts on each point;
  3. it logs one line per point: "Steer <its time>, point 1: done, <what
     it did>", or "…: cannot, <why>", which also goes into `blocked.md`
     (kind: process);
  4. it gives each point's "Pass on" text, word for word, with
     `send_message`, to the running sessions it applies to;
  5. it updates `context.md`. A point whose "Until" has come leaves it.
- **The supervisor follows up.** The next check verifies each point. A
  point not followed is sent again, with what was seen. A test-shape rule
  that a script can test, sent a second time, also becomes a check in the
  gate (section 14.4).

### 10.4 Who decides what

- **The supervisor decides what is not design:**
  - a plan defect, the order of steps, the plan's scope;
  - a step not in the plan, which it names `<repository>-<NN>.s<k>`, with
    a short session file (shape, test shape, proof and "Don't", in about
    15 lines). A step whose Scope is to find a cause gets a budget there,
    in runs and minutes, and its implementer asks only past it: in the
    reliability run, one such step raised 14 blocks in 105 minutes;
  - process, infrastructure and credentials, within what the user allows.

  It records each ruling in the plan's `decisions.md`, and lands each
  change to the plan before the unblock that relies on it: `plan ruling`
  does both (section 10.3). A ruling that defers work lands with its row
  in the plan's `later.md`. When the same kind of block comes back, it
  adds a standing rule to the plan's README; a rule that would serve every
  plan becomes a proposal for the retrospective (section 15).
- **The designer decides a design gap,** always. When the step can go on
  meanwhile, the supervisor's interim ruling lets it go on only with work
  that does not rest on the answer, and the question still goes to the
  designer at once: in test-budgets, a gap ruled a fixture defect left
  recovery's behaviour unchecked against the design. A block on what a
  term of the design means (what decides, what counts, whose failure a
  miss is) is a design gap; it is a plan defect only when the ruling
  quotes design text that answers the block's question in its own words,
  not text that only bears on it. In smith's parity run, ruling 112 read
  whose failure a calibration counts as a plan defect; the designer was
  asked 87 minutes later, and the area took eight rulings and three added
  steps. The supervisor writes the question to the run's `question.md`,
  in `blocked.md`'s shape, with its own reading of the design, and wakes
  the designer: `plan wake designer "question.md has a question: answer
  it as <run>/kit/docs/foundation/development.md, section 10.4,
  says."` This resumes the designer's own session in the background, with
  its context; a designer session the user has open gets a copy instead.
  - **The designer's context compacts in place:** `plan wake` resumes it
    with `--autocompact`, at about 180K (section 10.5). After a
    compaction, the designer re-reads sections 2.1, 4.4, 4.5 and this
    one, in the run's `kit/` (section 3.6), `question.md`, and the design
    sections and plan files it names, before going on with the answer.
  - **A fresh designer** starts instead, from a brief (`plan wake`'s
    prompt: the role, `question.md`, and the earlier answers that touch
    it, in `decisions.md`'s design rulings), when the last designer's session has been idle
    for over an hour, longer than its cache lives, or when its last answer
    said it was confused: then the supervisor wakes it with
    `PLAN_DESIGNER_IDLE=0 plan wake designer …`. In jig-protocol, each
    wake re-read a history that grew to 966K tokens: 75% to 80% of the
    designer's 514M input tokens.
  - **A designer at work is never woken again:** a second wake forks a
    copy without the first's answers. `plan wake` refuses it, and the
    supervisor sends the designer a message instead (in Claude Code,
    `SendMessage`) naming `question.md`.

  The designer:
  1. decides;
  2. amends the design with `land --docs`, then the session files that
     mention the point, and, for each thing the answer defers, adds its
     row to the plan's `later.md` in the same landing. In protocol, two
     of the designer's three deferrals left no row. Each `later.md` row
     that leaves work for a repository downstream of the plan, and is not
     yet in that repository's `STATUS.md`, goes there too, under "Not
     planned", with this answer's landings (section 5.7);
  3. writes `answer.md`, naming the commits: first, under `## Decision`,
     the session's decision in at most 40 lines, quoting the design text
     it rests on; then, under `## Reasoning`, the reasoning, which stays
     there and in the design commit's body.

  The supervisor then writes the unblock, whose Decision appends only
  `answer.md`'s decision part (`sed '/^## Reasoning/,$d' answer.md`)
  rather than typing it again, lands it with `plan ruling`, appends both
  files to `supervision.md` under "Question and answer, <time>", and
  deletes them. In skein-ui, ruling 131's Decision, a whole answer,
  became a 19.6K-character prompt.
- **A design question decided without the designer reaches the design
  before the next plan on it is planned.** When no designer is joined,
  and the director or the user decides it (section 2.7), the decision
  names the design text it changes: the document, its section, and what
  that text will say. The supervisor records it in `supervision.md` as
  it would an answer, and the designer lands the change with `land
  --docs`, at the latest before a planner starts on that design. In
  temper's tools run, no designer was joined, the director decided all
  eight questions as their defaults, and at the run's end five of them,
  M to Q, were in the code and not in `tools.md`, which the next plan's
  planner was reading.
- **A measured aim's first miss goes to the designer at once.** When the
  run that measures a plan's measured aim first (its baseline) misses a
  criterion (section 5.1), the supervisor asks the designer, in
  `question.md`, whether the steps left aim at it, and the run goes on
  meanwhile. The designer answers with a change that names the
  criterion's cause, or with the plan ending short of it. In smith's
  parity run, the baseline missed RT at 03:48, a ruling called it "the
  baseline's finding", no later step aimed at it, and the plan ended
  short on it.
- **What a ready check finds goes on before its session starts.** A suite
  the check sees fail, or a rule the design must settle, goes to the
  designer as a question, or becomes a supplement step
  (`<repository>-<NN>.s<k>`), before `plan ready`: never "a question to
  raise when reached". In skein-ui, a seed that jig 05's check saw fail
  at 17:05 was ruled such a question, and reached the designer at 17:28,
  once the session had met it. A check or re-check finds it by compiling
  the step's sketches and running only the tests of the crates its Files
  name and their dependents (`rdeps(…)`), never the whole workspace or
  its fuzzy and browser profiles, which the step's land gate runs, in the
  worktree `wt/<repository>-<NN>r` (supervisor.md, section 5). In
  temper's tools run, the supervisor's 193 scratch tasks, 17 of them
  whole-workspace runs, were 51% of the build lane's waiting, and a
  scratch worktree at a session's path blocked temper 02.1 (ruling 111).
- **Before an answer is sent,** its author checks it against the code
  (the code graph indexes `main`) and quotes the text it rests on.
  - An answer keeps the plan's relative checks ("62 less the removed"). A
    number it adds is counted on `main` when the answer is written, by a
    command whose output it quotes, with that command.
  - A premise about the machine ("no root", "no Chromium") is checked by a
    command before a ruling rests on it.
- **What goes to the user:**
  - decisions with long-term consequences that would be hard to change;
  - any language other than Rust;
  - what the user reserved.

  A question for the user goes into `decisions.md`, under "For the user",
  and reaches the user at once in a push notification, in one line with
  the choice and the default. While a director is appointed, it reads them
  within minutes, and every push to the user is optional (section 2.7).
  The run blocks on it only if no work can continue without it. The
  answer is a `Decision:` line under the item (section 5.2).
- **No role of a run asks the user through a tool that waits for the
  answer** (in Claude Code, AskUserQuestion): its session waits inside
  the tool, and sees no event until the user answers. It writes the
  question as above, pushes it, and goes on with its work. In smith's
  parity run, a question asked so held the supervisor from 12:37 to
  17:31, while the queue's daemon lay dead behind it: 5.0 hours of the
  line, with nothing waiting on the answer.
- **A ruling that drops part of the gate reaches the user at once,** in a
  push notification, with the command that would restore it (`sudo
  apt-get install chromium`). Landings without a suite are the exception,
  not the run's way of working.

### 10.5 The supervisor's watch

- **The watch runs on the run's machine.** The supervisor runs `plan
  watch` in the background (in Claude Code, a Bash command with
  `run_in_background`). It ends, saying why, when:
  - `blocked.md` has headings it has not shown, and no answer is pending;
  - `answer.md` appears;
  - steps wait for their ready check (section 7.2), each with its session
    file;
  - `started:`: a step whose ready check named commits has started; the
    watch fetches and prints each repository's changes since those commits.
    The supervisor greps the session file's names in the moved files and
    re-checks a moved file that its sketch or suite reads (supervisor.md 5);
  - the coordinator's heartbeat is older than ten minutes, unless the
    coordinator is idle (`idle.md`, section 7.4);
  - `hourly:`: a check is due; the supervisor runs supervisor.md, section 6;
  - `alert: pueued is down`: the supervisor handles it as supervisor.md,
    section 2's alert row says, then starts the watch again;
  - `landed: <steps>`: the steps the supervisor awaits (`plan await
    supervisor <step>… <plan>/<step>…`, section 7.4) have all reached
    `origin/main`; for an awaiting coordinator the watch starts its turn
    itself, and goes on;
  - the run pauses or ends.

  Each event prints its handling material, indented and limited to 150
  lines, with a source path for the rest: new blocks with their ledger row,
  increment and report path; answers and pause/end files in full up to the
  limit; check rows with their session path; quiet with the coordinator's
  state and the last 20 log lines. It never prints a whole session file.
  Then it prints the supervisor's context, the input of its last request,
  and its compactions.
- **After each event,** the supervisor handles it, then runs the watch
  again.
- **A ready check never holds the watch.** Its sub-agent runs in the
  background, and the watch runs again at once. The sub-agent's end wakes
  the supervisor beside the watch, and the landing the check waits for
  comes as the watch's `landed:` (supervisor.md, sections 5 and 6). In client-composition, checks ran with the
  watch off, one after another: jig 06's, in `check` from 14:15, was
  shown at 14:22, after jig 10's.
- **A quiet coordinator.** When the heartbeat is ten minutes old, the
  supervisor rings the doorbell. If the ring finds no turn in progress,
  (the provider can end a turn), the supervisor runs `plan resume crash`
  itself, once, and records it in `supervision.md`. The ring loads a thread
  the daemon no longer holds and records the load in `log.md`. If the heartbeat is still old ten
  minutes later, the supervisor alerts the user (in Claude Code, a push
  notification). The alert gives the restart command, `plan resume`.
- **Every alert to the user** also goes, in one line with its UTC time, to
  the run's `alerts.md`; `plan status` prints its last ten lines first.
  The watch records `pueued is down` once per outage, when
  `pueue status --json` fails; a successful response ends the outage.
- **A usage limit.** A usage notice changes nothing: the role goes on,
  and writes no handover. A role stops only when a request is refused.
  If it can still write, as when the refusal is a sub-agent's, its last
  line in `supervision.md`, or its last message for a designer or a
  planner, is `Stopped, <UTC time>: usage limit, resets <the time the
  refusal gives>`. The user, or the director (section 2.7), resumes it
  then: `plan resume` for the supervisor. In forgejo-17, the supervisor
  took a notice for a stop: it wrote a handover, and told the user that
  its watch and cron job would die, and neither did. The refusal came
  half an hour later, with no line saying when it would end.
- **The designer has no watch.** After `plan join designer` it ends its
  turn; `plan wake` brings it back with each question (section 6, step 5).
- **The supervisor's context compacts in place** (section 13). `plan`
  starts and resumes it with `--autocompact` at
  `PLAN_CLAUDE_AUTOCOMPACT` (200,000 tokens), and Claude Code compacts it
  a little below that, at about 180K. A compacted session keeps its id,
  its cron jobs and its background commands: its watch and the waits
  beside it go on (tested 2026-10-17, on a cron job and a background
  loop); a ready check's sub-agent should too, untested. After
  each compaction, which the workspace's hook announces (section 3.6),
  and before acting, the supervisor (`supervisor.md`, section 1):
  1. re-reads its card's working sections, sections 2 to 9, and of this
     document only what a case needs, as at its start;
  2. re-reads the run's state: `plan status`, `supervision.md` from its
     last `## Check` entry, the escalation files there are, and the
     ledger's steps in `check` and `ready`;
  3. checks that its watch and its waits run, and starts what is missing;
  4. appends `## Compacted, <UTC time>: session <id>, compaction <N>` to
     `supervision.md`, N counted from its session's earlier entries, with
     what it found running and what it started.
- **The supervisor hands over only when it finds itself confused:** after
  the re-read, it cannot say what is open, which ruling landed last, or
  which background commands are its own. Or when its watch says so,
  which it does only in a run claimed before skein 71f6c38, whose kit
  does not compact the supervisor, once its context is past its limit.
  The word to hand over that a later run's kit prints at a count of
  compactions is ignored (section 13).
  It hands over to a fresh session, and starts it itself (`supervisor.md`,
  section 7):
  1. it finishes the event in hand;
  2. it appends a `## Handover, <time>` entry to `supervision.md`, of at
     most 30 lines: what is open, the sub-agents running and where each
     report will be, the steers it follows up, the rulings in force that
     the plan does not yet say, the next check's focus;
  3. it stops its watch and each ready check's wait, and leaves the
     sub-agents running;
  4. it runs `plan handover supervisor`, which starts the successor and
     records it in `claude.tsv`;
  5. it ends its turn. A watch it left running exits with "handed over:",
     and shows it nothing.

  Nobody else starts the successor: in jig-protocol, a supervisor that
  left it to the user ran nine more hours past 500K tokens, and its
  answers read less code as it grew (half under 200K, none above 800K).
  The successor reads its card, the handover entry and the open escalation
  files, reads each sub-agent's report when it ends, and starts again each
  ready check's wait. Its first watch prints every open event; each later
  watch shows only events this session has not seen.

### 10.6 Stopping the line

**Stop the line** for one repository, or for everything, when:
- a gate cannot be brought back within its budget;
- a design contradiction blocks more than one session;
- an upstream change breaks a downstream gate beyond its session's scope;
- the queue or its daemon fails, or a lane stays paused with nothing
  holding it (`plan check` shows it, `--fix` starts it);
- memory or disk runs short (section 11.4);
- credentials fail, which pauses every session using them;
- reconciling `main` would need a force-push.

**To stop the line:**
1. start nothing new in its scope;
2. let running increments land or stop at a clean point;
3. write `blocked.md`;
4. wait for the answer.

The supervisor may also stop the line, with a steer, when a check finds
the run heading away from its plan's "Done when".

## 11. Build infrastructure

### 11.1 The queue

- **pueued runs as a systemd user service,** with `StartLimitIntervalSec=0`,
  `Restart=always` and `RestartSec=5`, so it keeps trying after failures.
  It is never started from an agent's command:
  a daemon started by an agent dies with it. Lingering
  (`loginctl enable-linger`) keeps it running when the user logs out.
  Where systemd has no user bus (some containers), `plan check --fix` and
  `plan resume` start it on its own.
- **Two lanes:**
  - **gate** (parallelism 1) runs gates, whole suites, measurements,
    probes and benchmarks. A gate task pauses every other pueue group,
    waits for their running tasks, runs alone, then resumes them, so
    suite times are measured on a quiet machine. It takes only a
    committed tree: `heavy --gate` refuses a git worktree with changes
    before it queues;
  - **build** (parallelism 2) runs everything else, each task with
    `HEAVY_BUILD_JOBS` (4) cargo jobs, `land`'s build and lints before
    its gate included (section 9.1).
- **A gate task killed outright** (`pueue kill`) cannot resume what it
  paused. `plan check --fix` starts a paused lane again.
- **`heavy` is the only way in, for every role.** The planner's and the
  designer's builds and measurements, the supervisor's and the
  retrospective's checks, and the implementers' work all queue: a
  measurement taken beside the queue measures the queue's load too.
  Nobody changes a lane's parallelism, except as section 11.4 says.
- **What pueue does not hold** still loads the machine: the code graph
  indexing a checkout, another user's work. A gate task records it as it
  starts alone (section 11.2).

### 11.2 Logs

`heavy --task` runs inside the queued task and writes its log and row,
even if the waiting client is killed. It writes each command's whole output to the run's `logs/` and
prints only its tail and the log's path. It also appends a line to
`logs/heavy.tsv`: the task's id, its lane, when it ended, the seconds it
waited and ran, its exit status, the memory available as it ended (GB),
its directory, its log, its command and its label (`HEAVY_LABEL`: `land
<repository>` for `land`'s gates, which the metrics count apart from the
gate lane's other tasks). The client waits, reads the task's row, prints the
tail and removes the task from pueue, which keeps the daemon's state small.
If a task dies before writing its row, the client recovers it from pueue's
record, even when it cannot read pueue's answer.

A gate task's log starts with the machine's load average and its three
busiest processes as the command starts alone, so that a slow gate can
be told from a loaded machine.

### 11.3 The cache

- **kache is the global `rustc-wrapper`.** Its keys do not depend on
  paths, so a fresh worktree reuses every crate that did not change.
- **It gains most when worktrees are new,** and little on the crates a
  session keeps editing.
- **Mind its store.** Watch its size against its cap (`kache report`).
- **`cargo clippy --fix` fails on its read-only outputs.** Fix lints by
  hand, or run with `RUSTC_WRAPPER=` set empty.

### 11.4 Resources

- **Each session's `target/` takes up to about 21 GB:** 12 to 15 GB in
  temper's protocol run; 8 to 9 GB fresh, and 17 to 21 GB after a few
  increments, in smith's parity run.
- **A session's `target/` goes after each of its landings.** Its
  implementer removes it once the step has landed, before its report
  (implementer.md, 8.1), and the coordinator removes a session's, with
  `cargo clean` in its worktree, when it yields, hands over or ends: at
  once, or, while a heavy task the session queued is still queued or
  running (`pueue status` shows each task's path), once the last of them
  ends. kache refills it from its store (section 11.3). In smith's
  parity run, eight sessions' `target/` filled 29 GB in about five
  minutes, between two checks, and then the disk; after this rule
  (ruling 108), no disk event followed. In smith's wall-clock run, a
  baseline running from a yielded session's worktree died 33 seconds
  after the coordinator removed its `target/`, with the disk above 101
  GB.
- **The coordinator checks** available memory (`free -g`) and disk (`df -h
  ~/src`) before each start, and at every wake-up (section 7.4). It logs
  their figures in `log.md`, its `Resources` line, at most every ten
  minutes, sooner only when a bound below is crossed.
- **Below the memory and disk floors `plan check` prints,** it holds new
  starts: the machine file's `PLAN_MIN_MEM_GB` and `PLAN_MIN_DISK_GB`,
  6 and 40 GB unless it says otherwise.
  **Below 60 GB of disk,** at most six sessions run at once.
- **Below 3 GB of memory,** it narrows the build lane to one task (`pueue
  parallel 1 --group build`), and widens it again above the memory floor. It says so in
  `log.md` each time.
- **Below 1.5 GB of memory or 15 GB of disk,** it stops the line.
- **To free disk,** it may remove the `target/` of a session that is not
  running, once no heavy task the session queued is queued or running
  (above). Before it holds starts or blocks on disk, it frees what it
  can, and logs each: first the `target/` of each session that has
  finished; then, below 80 GB, `kache gc`, at most every ten minutes,
  once more if it ends with "No such file or directory", with its `Store:` line
  logged beside the check. It holds or blocks only if disk stays below
  the bound. kache evicts late, past its cap by 11 to 17 GiB between its
  passes, and on smith's parity machine not at all: its store reached
  135 GiB against a 74.5 GiB cap, twice. In protocol, disk blocked five
  times before freeing first (rulings 118 and 128), and never after; in
  parity, no disk event followed `kache gc` below 80 GB (ruling 111).
- **On btrfs, unallocated space is a bound beside disk and memory.**
  `df` counts the room left inside chunks already allocated, so metadata
  can run out with much of the disk free. At each disk check, on a btrfs
  filesystem that holds `~/src` or the caches, the coordinator reads its
  unallocated space (`sudo -n btrfs filesystem usage -b <mount>`, its
  `Device unallocated` line). Below 10 GiB, it runs `sudo -n btrfs
  balance start -dusage=75,limit=10 <mount>`, up to three times, logging
  each, and blocks as infrastructure if it stays below. pueued is
  restarted only through `plan resume`, which keeps its cgroup. In
  temper's client-api run, `/srv` had 1 MiB unallocated with 98 GiB
  free: mold's links died with bus errors, pueued and Codex's daemon
  died, four implementer threads were lost, and pueued, restarted by
  hand, lost the process tier's cgroup for three hours (rulings 309, 331
  and 334). The next kit batch moves this check into `plan check`, the
  pulse and `plan resume`.

### 11.5 Budgets

The suites' budgets are wall time on the reference machine: 8 threads,
15 GB of memory, through the gate lane. Gate times on another machine name
that machine.

## 12. Several machines

- **One run per machine.** `~/src/rust/tmp/active-plan` names it.
  Several machines run different plans at once; a plan's `STATUS.md` row
  keeps a second machine from claiming it (section 6).
- **The supervisor of a run, and its designer, work on the run's
  machine,** where its files are.
- **A new machine** clones the repositories into `~/src/rust/` and runs
  `skein/scripts/dev/plan check --fix`. It links the tools and the
  workspace's files, sets up the queue, and names what is still missing.
- **Shared:** the GitHub repositories, and with them the designs, plans and
  `STATUS.md`. **Per machine:** runs, the queue, the cache, worktrees and
  credentials.
- **Progress is visible from any machine.** A plan's `STATUS.md` row says
  where it runs, and every landed step's `Plan` and `Step` trailers are on
  `origin/main`.
- **Plans running at once avoid each other's hotspots.** When two must
  share one, the second plan's README names the first.
- **Another machine landing on the same repository** is, to `land`, main
  moving: rebase, gate again, push. To a run with a process tier, it is
  what the hourly check runs that tier for (section 14.1). A story that
  turned red goes, with its range of commits and their `Plan` and `Step`
  trailers, to the run that landed them: the supervisor writes it for the
  user (section 10.4), and the user, or the director, brings it to that
  run's supervisor, who handles it as a finding of its own check (section
  14.4).
- **A kit change from another machine** reaches a run only by its
  supervisor's ruling (section 3.6), which its next hourly check makes
  for a fix of a defect the run has met, or at its resume after a pause,
  which takes skein's `main` as the run's kit and process (section 7.6):
  otherwise each run keeps its claim's kit. This holds for the runs
  claimed after skein e6fd9fb, through their resumes after a restart or
  a crash; a run claimed before it runs main's kit to its end, resumed
  or not. A pause's resume moves only a kit that has that move, skein
  d450346 or later: an older kit keeps its claim's until its
  supervisor's ruling moves it.

## 13. Open questions

- **The build lane's width.** Two tasks of four jobs each is untested
  under a real run's load. The next run measures memory and the queue
  (`logs/heavy.tsv`) and settles it.
- **Gating again when main moves** is most of the queue's time: a branch
  is gated again each time another lands first. A gate now holds across
  Markdown under `docs/` (section 9.2), which removes the regates after
  rulings, and `land` builds in the build lane first (section 9.1), which
  shortens each gate. A code landing holds its repository's turn on its
  machine from rebase to push, so that another code landing there cannot
  move main under its gate. Landings from another machine still can. The
  metric `gates_per_step` (section 15.2) shows what remains.
- **The build lane's pass before the gate** was replayed against
  jig-protocol's queue, not run: build waits fell from 45.7 hours to
  about 6.8 in the replay. The first run checks, in a gate's log, that
  the gate's own clippy and build find nothing to redo.
- **An idle coordinator's next turn** comes from `plan ring` starting it
  (`turn/start`), as `plan resume` does: untested while sessions have
  ended their turns and wait on a `followup_task`.
- **Compaction in place** (the director's experiment, for the user,
  2026-10-17; decided by render's retrospective). The supervisor and the
  designer compact at about 180K and re-read their files after each
  compaction (sections 2.1, 10.4 and 10.5); the experiment also had them
  hand over after their second compaction. In render, compaction was
  cheap, about 7 requests and $0.4 to restore, and the count was costly:
  its handovers stopped eight sub-agents ($12.5, then relaunched) and
  preceded both late hourly checks. The one confused session followed an
  id change, not compactions. So compaction in place stays and the count
  is dropped: a role hands over only when it finds itself confused, or
  when its watch says so (section 10.5). The `supervisor` metric (section
  15.2) is to give each session's restore cost and count its sub-agents,
  in place of the bootstrap share, so that later runs show whether
  restoring stays cheap: render's proposal 11, for the next kit batch.
  The designer's idle limit (an hour) stays.
- **`wait_agent`'s longest timeout.** 300 seconds works, and the doorbell
  ends such a wait within a second (tested 2026-10-11). Codex's
  `multi_agent_v2.max_wait_timeout_ms` bounds it; its default is unknown.
  `codex queue --thread` is no doorbell: it delivers at the end of a
  turn, and the coordinator's turn lasts until it restarts.
- **`$CODEX_THREAD_ID`** names the thread in Codex's commands' environment,
  as far as the binary shows. The first run confirms it.
- **Attaching to a working coordinator.** `plan attach coordinator` runs
  `codex resume --remote unix:// <thread>`, which needs a terminal, and is
  untested on a thread whose turn is running.
- **A coordinator after a reboot.** `plan resume` loads its thread again
  with `thread/resume` only when reading it fails. After protocol's
  reboot the thread read as `notLoaded`, no turn could start, and the
  supervisor loaded it by hand (ruling 115). In smith's parity run, the
  daemon unloaded the thread at each idle, `plan ring` and `plan resume`
  read "thread not found" as a thread with no turn, and the supervisors
  loaded it by hand ten times. Loading an unloaded thread, naming the
  cause, resuming by itself at boot, and ringing an idle coordinator when
  the queue tasks it waits on end wait for the kit (client-composition's
  proposal 4, protocol's 8, parity's 5).
- **Credentials under load.** Does a run's work that borrows the Codex
  login (benchmarks, probes) put the coordinator's own session at risk?
  A `CODEX_HOME` of their own for such work is a candidate answer.
- **Pushing from Codex sub-agents** is untested: their sandbox and network
  settings must allow `land` to push.

## 14. The supervisor's hourly check

### 14.1 The check

Once an hour, the supervisor checks whether the run heads for its plan's
"Done when", at a healthy pace, without scope creep or waste, and with
tests that stay cheap and readable. It is a light check of about ten
minutes. Problems are cheaper to fix the hour they appear. The hour
counts from the last check, across handovers and restarts: the next
check is due 65 minutes after the last `## Check, <UTC time>` heading
in `supervision.md`, or its first `## Process, <UTC time>: skein <commit>`
if there is no check. The last Process line names the run's kit and
process: its claim's, or the one a resume after a pause took (section
7.6). The watch prints `hourly:` once for that heading
when it is over 65 minutes old (`supervisor.md`, section 6).

**A sub-agent runs steps 1 to 4** (`supervisor.md`, section 6): a fresh
one on Sonnet (`supervisor.md`, section 9), never a fork, which returns
the entry of step 5 and any steer's points, and changes no file itself.
What it reads stays in its own context: in jig-protocol's second
supervisor session, the checks run in the supervisor's own context cost
about a fifth of it, and each added 7K to 23K tokens that every later
request carried. The supervisor appends the entry, and sends the steer.
Its prompt names the sections and line ranges it reads: no sub-agent
reads `log.md`, `supervision.md`, `decisions.md` or this document whole.
In temper's client-api run, the supervisor's sub-agents re-read the 2 MB
`log.md` 53 times, `supervision.md` 54 times, this document 56 times and
the README 71 times: $338 of the supervisor's $604.

**A quiet hour needs no sub-agent.** While no session is running and the
run waits only on queue tasks (a live suite), the supervisor runs `plan
pulse`, step 1's process tier when it is due, and step 2's listing of
process changes and graph comparison itself, reads them unfiltered, and appends an entry of at most five
lines: what runs, its progress, the machine, the changes found. In
smith's parity run, the eight checks while one suite ran cost about $21,
and led to nothing. In smith's wall-clock run, the quiet checks from
18:48 to 02:17 skipped the listing, and a change that named the run
reached its coordinator ten hours after it landed.

1. **Run `plan pulse`** (section 14.2), and read its output unfiltered:
   no `grep`, `head` or `tail`. It prints the numbers since the last
   check, and moves its mark. Once the plan has landed in a repository others build on, also
   run `plan downstream`, in the background: a few minutes of `cargo
   check` in the build lane. Read its lines before the entry, one per
   repository, `builds` or `fails`. Its fresh worktrees live under the
   run's `downstream/`, outside sessions' `wt/`, and are removed on every
   exit. The entry gives each result, or says that it
   is still running. A repository it shows broken may get an early repin
   (section 5.7). In a plan whose README names a process tier (section
   5.2), while another run lands on the same repository's `main`, also
   run that tier, in the background, whenever `main` has moved since its
   last run, quiet hours included: in `<run>/wt/<repository>-tier`, a
   detached worktree (`git -C ~/src/rust/<repository> worktree add
   --detach <run>/wt/<repository>-tier origin/main` the first time), moved to
   `origin/main` after a fetch before each run. The entry gives the
   commit it ran at, and each story that turned red since the last run
   with the range of commits between the two; a red story whose range
   holds only another run's commits goes to that run (section 12). In
   temper's forge-setup run, the landing story was red on `main` for at
   least 4.5 hours before a ready check saw it, and 40 gate-lane runs
   bisected the cause, while 29 quiet checks ran nothing. Each test that
   a ruling ignored runs at every check too, run alone or not, on
   `origin/main` in that worktree, until the step the ruling names to
   restore it has landed (section 5.5): in temper's client-api run, the
   ignored landing story broke for 27 hours unseen, and another run's
   director found it.
2. **Read** the log's headings since the last check, the first five
   lines of each report, and the coordinator's acknowledgement of the
   last steer. Then list the process changes since the run's kit:
   `git -C ~/src/rust/skein fetch -q origin && git -C ~/src/rust/skein
   log --format='%h %s%n%b' <the run's kit commit, from
   supervision.md's last Process line>..origin/main -- docs/foundation
   scripts/dev`. Each
   commit whose body says the run takes it, and that no steer has passed
   on, is a steer point (section 4.4), sent with this check's steer: a
   change reaches the run's coordinator at the first check after its
   landing. Each commit that fixes a defect the run has met, the
   supervisor takes into `kit/` itself, once `test-plan` passes there
   (section 3.6). Then compare each plan repository's graph index with its
   `origin/main`, as `plan check` does (section 6). The supervisor
   re-indexes one that is older. Until it has, a ruling adds to the
   README's rules, and a steer point passes on to the running
   implementers: "Do not use the code graph for <repository>; search with
   `rg`." The ruling of the steer that lifts it removes the rule.
3. **Sample two files, about 300 lines in all:**
   - the largest test added since the last check, read against the test
     checklist (section 14.3);
   - one production diff, read against the plan's "Don't" lists and its
     increment's Shape.
4. **Judge,** one line each:
   - **direction:** does what landed move toward "Done when"?
   - **pace:** is the planned count moving? Has a session gone two hours
     without a landing?
   - **scope:** is every step not in the plan named by an instruction?
     Has the coordinator invented anything (section 2.3)?
   - **proof:** did what landed prove itself at the layer its Proof line
     named? Has a higher layer come within reach, so that later Proof
     lines should rise? Has one side of a meeting point run a session
     ahead of the other (section 5.3)?
   - **waste:** report lengths, evidence beyond the gate and the proof,
     gates per step, polling, idle slots, tokens per step;
   - **tests:** the checklist;
   - **taste:** the pulse's code signals, and the sampled diff.
5. **The entry,** for `supervision.md`, of at most 15 lines: the pulse's
   headline numbers, one line per judgement, the rulings since the last
   check, and the steers to send. When something needs action, the steer's
   points, in section 10.3's shape: the supervisor writes `steer.md` from
   them, then rings the doorbell. Each finding of the checklist (section
   14.3), or of the pulse's code signals, that no steer has named yet
   gets a point of its own (section 14.4).

### 14.2 What `plan pulse` prints

Since the last check:

- **progress:** steps landed, planned and not, against the plan's rest;
  the planned count over the last three hours; each running session's
  time since its last landing;
- **the record:** `log.md`'s headings and Kind values outside the shapes
  of sections 7.3, 7.5, 10.2 and 10.3, and `context.md`'s lines past 200
  characters, so the supervisor can keep the record readable by the metrics;
- **the channel:** open blocks and their age; steps waiting for their
  ready check, and for how long; an idle coordinator, since when; steers
  not acknowledged;
- **proofs:** each landed code step's `Proof:` line (docs landings left out), its layer against the one
  its increment named, and the steps with none;
- **waste:** the log's growth; the reports' median and longest length;
  `land`'s gates per landed step, and the gate lane's other tasks; queue
  wait against run time (`logs/heavy.tsv` and finished tasks pueue still
  holds, queued since the claim, excluding `heavy --task` rows already
  logged); the number of finished tasks no client removed; files and scripts written
  under `logs/`;
- **tests:** each suite's time from the `Gate-Result` trailers, against
  its budget and its aim, and each landing whose focused or fuzzy time is
  past the aim (in hearth's render run, ten were past 8 s before the
  steer), the time of the packages an aims line names summed from the
  gate's log (section 5.2); the tests past four fifths of a ceiling, and the SLOW ones;
  lines added in tests and in code, docs landings left out;
- **code signals in the diff:** code files the diff touches, tests
  included, over the plan's size limit, documents and data judged by no
  length; new
  lint exceptions such as `too_many_lines`; the words the plan forbids;
  types named like a parallel type (the plan's "Pulse" patterns); test
  helpers defined in more than three files;
- **rising words:** the eight words that rose most in the files the plan
  changed, from its base to `main`, each by ten or more and by half or
  more. A forbidden word moves to a synonym the list does not name
  ("actual" became "observed", 9 to 119); the check reads this line;
- **cost so far:** the time and token metrics of section 15.2, for the
  run so far, and what Claude Code's tokens cost at list prices, by role
  and by kind, since the design;
- **short waits:** the coordinator's and the sessions' waits so far that
  came back sooner than section 7.4's 300 s with nothing ended: a
  `wait_agent` that timed out, or a `wait`, a command's start or a poll
  that found the command still running, timed by when it returned,
  whatever it asked for (`write_stdin` returns at 31 s);
- **pins:** how many commits each repository's pins are behind the
  upstream mains, as of the last fetch (`plan metrics` fetches; the pulse
  does not);
- **contexts:** each session's context, and those past the limit
  (section 7.3); the supervisor's and the designer's;
- **the machine:** the heartbeat's age, memory, disk and the rung of
  section 11.4's disk ladder it is on.

The numbers come from the script, not from reading: a log read by eye is
easily misread.

### 14.3 The test checklist

- **Cost:**
  - the suites' trend, and any test past its ceiling;
  - fixtures built again for every case;
  - formatting or text search on the passing path (traces kept typed,
    formatted only on failure);
  - limits sized for production instead of for the test;
  - more seeds than the assertions need.
- **Shape:** testing-strategy.md's section 10, rule by rule. A finding
  names the rule it breaks, and a steer's point cites it by section and
  name.
- **Weight:**
  - a test function within the plan's limit, and a file within its own;
  - no lint exception in tests.
- **Awkwardness is a signal.** A test that needs a large setup to say
  something small points at the interface under test. It calls for a
  steer, or a change to the plan's Shape, not for a bigger helper.

### 14.4 When a check finds a problem

- **The first time,** a steer gives the corrected shape, with a concrete
  example: a test rewritten, a report's lines, the command that is the
  evidence.
- **The second time,** the point becomes a standing rule in the plan's
  README, or a line of the plan's "Don't" list, and a steer says so. A
  test-shape rule (testing-strategy.md, section 10) that a script can test
  also becomes a check in the repository's gate, added as a step (section
  10.4), so that a third steer is not needed. In render, test shape was
  about 19 of 45 steer points, and one rule was sent six times.
- **Every finding gets its steer, or a reason.** The check proposes a
  steer point for each finding not yet steered, and the supervisor sends
  it, or writes in the entry, in one line, why not. In smith's parity
  run, one behaviour per test was steered at its second and third
  sightings, bare numbers at their sixth, and helpers defined again (a
  `run` in 19 files) never.
- **A rule that would serve every plan** becomes a proposal for the
  retrospective (section 15).
- **The run heading away from "Done when"** calls for a steer that stops
  the line in its scope (section 10.6), and for the plan to be adjusted
  (section 2.5) before it starts again.

### 14.5 What the check is not

No code review, no running of tests but section 14.1's process tier, no
reading of whole reports. A
question that needs more goes to a read-only sub-agent with a bounded
question and its own budget, or waits for the retrospective.

## 15. The retrospective

### 15.1 Writing it

- **When:** after the last step lands, `done.md` is written and the
  supervisor has marked the plan done; before anything in the run's
  directory is removed. The supervisor may also call for one in the
  middle of a run, when a check finds the run off course.
- **Who:** a fresh Claude Code session, Opus 5.5 at xhigh, which took no
  part in the run. The supervisor starts it with `plan retro`, or the user
  does. It uses read-only sub-agents, one per source.
- **It reads:**
  - the plan, with `decisions.md`;
  - the run's ledger, `log.md`, `supervision.md` and `done.md`, and the
    escalations with their answers;
  - the git history, with its `Plan`, `Step`, `Proof` and `Gate-Result`
    trailers;
  - `logs/heavy.tsv`;
  - the Codex traces of the coordinator and of each session (from the
    run's `threads` file, and the threads each spawned), and the Claude
    Code transcripts of the supervisor and the designer (from
    `claude.tsv`) and of the planner (the plan's `Planner-Session:`
    trailer), and the design's sessions (the `Designer-Session:` trailers
    of the commits the Inputs name), with their sub-agents;
  - the earlier plans' metrics (`plan metrics --history`) and their
    retrospectives;
  - the skein commit its run's process came from (`supervision.md`'s
    `## Process` line), and the changes to development.md and its cards
    that landed during the run.
- **It runs `plan metrics --out <its branch's plan directory>/metrics.tsv
  <plan-dir>`** (section 15.2).
- **It writes `retrospective.md`** beside the plan, and lands it with
  `metrics.tsv`, with `land --docs`. Its shape:
  1. **In one page.**
  2. **Outcome:** "Done when", item by item.
  3. **Numbers:** the metrics, beside the earlier runs', with what explains
     the largest differences.
  4. **What worked.**
  5. **What did not,** each item with its cause, when it was first
     visible, and when it was acted on.
  6. **Decisions taken during the run,** and whether each reached the
     design.
  7. **The proposals adopted after the last retrospective:** each with the
     metric it was to move, and whether it moved; and the skein commit
     the run kept, with each process change that landed during the run
     and whether the run took it.
  8. **Proposals:** each a one-sentence rule, its evidence, the document
     it changes (this one, a repository's `workflow.md` or `AGENTS.md`,
     the plan's shape), and the metric it should move, and which way.
  9. **Debts left in the code,** for the repository's `tech-debt.md`.
  10. **Decisions:** a table with one row per proposal, `| <N> | open |
      |`, for the user. Each row's decision becomes `adopted`, `adopted in
      part: <what>`, `rejected: <why>` or `deferred: <until when>`, and its
      last column where it was applied.
- **The user, or the director, decides each proposal before the next run
  starts,** and the designer applies what is adopted and records each
  decision in section 10, and in the same landing clears the plan's items
  from its repositories' `STATUS.md` (section 3.3). `plan check` refuses
  a plan while the last retrospective lacks section 10 or has a proposal
  still open (section 6). In test-budgets, six of jig-protocol's
  seventeen undecided proposals recurred.
- **No run is claimed while the last retrospective has an adopted
  proposal still to apply,** its last column empty or naming no commit
  on `main`, unless the plan's README says how this run applies it,
  naming it as `<plan>'s proposal <N>`; `plan check` refuses it. In
  smith's live-text run, none of client-api's nine proposals adopted at
  01:43 was applied at the 09:07 claim, and its implementers spent 27.4%
  of their input waiting, which client-api's proposal 3 aimed at.

### 15.2 Metrics

The same metrics, defined the same way, for every run, so that changes to
this process can be judged by what they move. `plan metrics` computes
them from the run's files, the git history and the agents' traces; nobody
counts them by hand.

| Metric | Is |
|---|---|
| `wall_h` | hours from the claim to `done.md`'s `Recorded` line (section 7.6), or else to `done.md`'s last change |
| `running_h` | `wall_h`, less pauses and the time the heartbeat was older than ten minutes: like `wall_h`, it ends at `done.md`'s `Recorded` line |
| `steps_planned`, `steps_added`, `steps_dropped` | steps landed from the plan (a split's lettered parts included), steps added during the run (every `.s<k>` and a lettered part whose parent is not `split`), steps dropped; a `split` row counts in none; `-repin` is planned |
| `step_h` | `running_h` per landed step |
| `chain_step_min` | the median minutes from a landed step's start to its last commit on `main`: the start of its thread in the ledger, a UUIDv7's time (the last thread, for a step handed over). A chain's step takes about this whatever runs beside it, so the next README projects its chain at it (section 5.2) |
| `chain_step_max` | the longest of those spans: the chain's upper bound |
| `chain_step_max` | the longest of those spans, in minutes; the next kit batch computes it |
| `escalations` | blocks, by kind (section 10.2), each counted once: one listed again in later sections, while it stays open, counts at its last listing. A Kind line counts as the kind of section 10.2's set it starts with, whatever follows; one that starts with none is named apart, up to its first full stop or semicolon |
| `escalation_wait_min` | the median time from a block to its answer, in minutes |
| `blocked_h` | hours in which at least one block was open |
| `answers_reversed` | unblocks and steers whose `Reverses:` line names an earlier ruling or steer |
| `steers` | steers sent, and points the coordinator refused |
| `restarts` | `log.md`'s "Coordinator resumed" headings, by the whole cause after the time, or, when that is `unknown`, by the entry's first line up to its first full stop; its "Coordinator loaded" and `supervision.md`'s "Loaded" headings count as cause `loaded` |
| `tokens_coordinator` | input tokens, with the part read from the cache, and output tokens of the coordinator's threads, from the last `token_usage_record`'s `thread_token_usage`, else the last `total_token_usage` |
| `tokens_implementers` | the same usage totals, for every thread the coordinator spawned, at any depth |
| `tokens_claude` | the same, for the Claude Code sessions in `claude.tsv` and the planner's (the plan's `Planner-Session:` trailer), and the design's sessions (the `Designer-Session:` trailers of the commits the README's Inputs name, and of no other), their sub-agents and sub-agents of sub-agents included, each message counted once across every session, since a resumed or forked session copies the messages before it; the input also in three parts: before the claim (the design and the planning), from the claim to `done.md` (the run), and after (the retrospective) |
| `cost_claude` | what those tokens cost at list prices for each message's model (Opus 5.5's when absent or unknown, unknown models named), in all, by role, and by kind: cache reads, cache writes, fresh input, output; each session whose transcript is not on this machine is named, counted as $0. At Opus's and Sonnet's rates, a cache read costs a fortieth of a 1-hour cache write and a hundredth of an output token, so token counts alone hide where a long context's cost goes |
| `tokens_per_step` | the coordinator's, the implementers' and the run's part of the Claude Code sessions', per landed step |
| `supervisor` | the supervisor's sessions in `claude.tsv`: how many, their hours (each from its first request to its last), their weighted tokens per hour (fresh input 1, cache reads 0.1, cache writes 1.25, output 5; sub-agents left out, each message counted once), the share of those in the 40 requests after each session's start or compaction (the bootstrap), and their compactions (section 10.5); the next kit batch replaces the share with each session's restore cost (the requests and dollars from its start, and from each compaction, to its next `plan watch`) and its sub-agents' count and cost (section 13) |
| `coordinator_requests` | the coordinator's model requests, from its usage records (distinct `response_id` values, else `total_token_usage` lines), and how many followed a wait that ran out |
| `compactions` | the coordinator's context compactions |
| `gates_per_step` | `land`'s gates per landed step, by their label in `logs/heavy.tsv` (section 11.2) |
| `gate_lane_other` | the gate lane's other tasks: measurements, probes, live runs, `plan check`'s gate, including finished tasks pueue still holds since the claim |
| `queue_wait_ratio` | time waiting in the queue against time running (`logs/heavy.tsv` and finished tasks pueue still holds since the claim); `heavy --task` is counted only from its logged row |
| `suites` | for each repository, each suite's test count and time at its first and its last landing |
| `proofs` | landed code steps, each by its last gated commit, by each semicolon part's proof layer, its first word up to the first hyphen (`process-tier` is process; `one-process` is process, and `browser` live, as section 5.5's table has it), a later part only when that word is a layer's (a semicolon in the prose names none), the first part's words that name no layer named apart, and those below their increment's highest layer, a split judged only at its last part; docs landings count in none; whose heading writes the session with or without its zero (`### 06.1`, `### 6.1`) |
| `report_lines` | the reports' median and 90th percentile length |
| `lines` | lines added and removed by gated code landings (docs landings left out), in four kinds: production code, tests, generated code (under a `generated/` directory), and data (any file neither Rust nor the shell kit). The shell kit is a `.sh` file or one under `scripts/` with no extension, and its tests are named `test-*` |
| `contexts` | the median and 90th percentile of the largest context each session's thread reached, and the handovers (`log.md`'s `Handover` lines) |
| `pin_lag` | how many commits each repository's `origin/main:Cargo.lock` pins are behind the upstream mains, fetched best effort by `plan metrics`; the pulse reads the last fetch without fetching |

- **`metrics.tsv`** holds one line per metric: its name, its value and its
  unit. `plan metrics --history` reads the `metrics.tsv` of every plan in
  the workspace's repositories, and prints them side by side.
- **A definition changes only with this document.** The retrospective
  names the metrics that are not comparable with earlier runs, and why.
- **The loop.** Each proposal names the metric it should move. The next
  retrospective says whether it moved. A plan may set itself a target
  from the history, as a guide (section 4.5): for example, tokens per
  step.

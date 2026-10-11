# Development process

Provisional, 2026-10-09, revised 2026-10-11. How work goes from an idea
to `main` in skein and in every project built on it (smith, temper, jig):
who designs, who plans, who builds, who supervises, where each of them
writes, and how they talk to each other. Read it before starting a design,
a plan or a run.

**How to read this.** Section 1 is the whole process in one page. Section
2 names the roles and section 3 says where everything lives. Sections 4 to
10 follow the work: designing, planning, starting a run, coordinating,
implementing, landing and escalating. Section 11 is each machine's build
infrastructure, section 12 several machines at once, and section 13 the
questions still open. Section 14 is the supervisor's hourly check, and
section 15 the retrospective that ends a run. Section 8, the implementer's
card, stands on its own: it is what an implementing agent reads. Section
4.5 binds everything written for the coordinator and the implementers.

## 1. In one page

- **Six roles, two agent programs.**
  - The designer and the planner (Claude Code, Opus 5.5 at xhigh effort,
    interactive with the user) turn an idea into design documents and a
    plan.
  - The coordinator (Codex, gpt-6.1-sol at high effort) runs the plan with
    implementers (Codex sub-agents, same model and effort), as many at once
    as the plan and the machine allow.
  - The supervisor (Claude Code, Opus 5.5 at xhigh) watches the run, and is
    the coordinator's only counterpart.
  - The retrospective's author (Claude Code, Opus 5.5 at xhigh), a fresh
    session after the run, writes the run's retrospective.
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
    shape, the things it must not do, and its proof: the check that it
    works for real, at the highest layer the code reached by then allows.
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
- **Heavy commands queue.** They go through `heavy` into the machine's
  pueue queue: a gate lane that runs alone, and a build lane that runs a
  few at once. kache caches the crates that did not change.
- **Escalation goes through files.**
  - An implementer stops on a real gap and tells the coordinator, and the
    coordinator writes `blocked.md`.
  - The supervisor answers in `unblock.md`. Where the design must change,
    it asks the designer first, through `question.md`.
  - Only decisions with long-term consequences that are hard to change
    reach the user.
- **The supervisor checks the run once an hour** (section 14). It uses
  numbers from `plan pulse`, reads what changed, and samples one test and
  one diff. It steers through `steer.md`, and adjusts the plan when that
  helps the run.
- **A retrospective ends every run** (section 15). It lands as
  `retrospective.md` beside the plan, with the changes it proposes to this
  document.
- **Everything else is decided where it arises,** and reported:
  implementation choices by the implementer, the run's rulings by the
  supervisor, design gaps by the designer.

## 2. Roles

| Role | Runs as | Decides | Writes |
|---|---|---|---|
| Designer | Claude Code, Opus 5.5, xhigh | the design, within the user's delegation | `docs/design/**` in any repository, `STATUS.md` included; during a run, the session files that mention a point it changed, and the run's `answer.md` |
| Planner | the same, in the designer's session or a fresh one | how the design is cut into sessions and increments | `docs/development/plans/<plan>/` |
| Supervisor | Claude Code, Opus 5.5, xhigh; started with the run | the run's rulings within the design: the plan's scope, order and increments, steps not in the plan, process, infrastructure | the run's `unblock.md`, `steer.md`, `question.md` and `supervision.md`; the plan, during its run; a plan's state in `STATUS.md` |
| Coordinator | Codex `-p coordinator` (gpt-6.1-sol, high) | what starts when, within the plan's dependencies and the machine | the run's directory |
| Implementer | Codex sub-agent, same model and effort | implementation choices within the design and the plan | code and tests in its session's worktree |
| Retrospective's author | Claude Code, Opus 5.5, xhigh; a fresh session after the run | nothing: it proposes | `docs/development/plans/<plan>/retrospective.md` |

### 2.1 The designer

- **Talks with the user.** It reads the design, `STATUS.md`, skein's
  foundation documents and the code, and turns an idea into a draft or
  into contract text.
- **Is the only role that edits `docs/design/`,** in any repository,
  including `draft/` and `STATUS.md`. One exception: the supervisor
  changes a plan's state in `STATUS.md` (section 3.3).
- **Answers a run's design questions** (section 10.4). They reach it from
  the supervisor, in the run's `question.md`. It amends the design, and
  the session files that mention the point, with Markdown-only landings.
- **Decides within the user's delegation.** What stays with the user is in
  section 10.4.

### 2.2 The planner

- **Turns a landed design into a plan** (section 5). It is the designer's
  model and effort, often the designer's own session.
- **Does not change the design while planning.** A gap the plan exposes is
  closed by the designer, under the designer's rules, before the plan
  relies on it.
- **Writes for the executor** (section 4.5). The coordinator and the
  implementers follow the plan's words as written.

### 2.3 The coordinator

- **Runs one plan on one machine** (section 7): sets up the run, starts a
  session as soon as its dependencies have landed, records every report,
  keeps the ledger, watches the machine and escalates.
- **Never designs, decides, reviews or writes code.**
  - It never edits a design or a plan, never answers a design question,
    and never fixes anything itself.
  - It checks reports with git only.
  - It creates no session, step, file-ownership rule or protocol of its
    own. Work that is not in the plan, an unblock or a steer is not done:
    the coordinator escalates the gap instead (section 7.3).

### 2.4 The implementer

- **Implements one session at a time,** increment by increment, in its own
  worktree (section 8). It lands its own increments and resolves its own
  rebase conflicts.
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

  Each adjustment lands with `land --docs`, is recorded in
  `decisions.md`, and reaches the coordinator as a steer that names the
  commit. It never changes what the design asks: a change that would is a
  design question, for the designer.
- **Asks the designer** what the design must answer, in the run's
  `question.md`, and writes the unblock once the designer has answered.
- **Checks the run once an hour** (section 14), and steers (section 10.3).
- **Watches the coordinator** (section 10.5): its heartbeat, its pauses
  and its end.
- **Does not review in depth and writes no code.** A question that needs
  more than the hourly check goes to a read-only sub-agent with a bounded
  question, or waits for the retrospective.
- **Hands over** to a fresh session once past half its context.
  `supervision.md` and the run's files are enough to go on.

### 2.6 The retrospective's author

- **A fresh session after the run** (section 15). It took no part in the
  run, so it reads the record with fresh eyes.
- **Proposes, and decides nothing.** The user chooses which of its
  proposals to adopt, and the designer applies them.

## 3. Where things live

### 3.1 Repositories

- **Each project is a repository under `~/src/rust/`,** on GitHub under
  `forgetic`: skein, then smith, which builds on skein, then jig, which
  builds on both, then temper, which builds on all three.
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
  forward, and the code graph indexes it, so the graph always describes
  `main`.
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
  by section, and the plan that builds it, or "not planned".
- **The designer keeps the file.** It changes it in the same landing as the
  design or the plan that changes what it says.
- **A plan's state is the planner's until `ready`** (section 5.4), **then
  the supervisor's,** who changes it when its run starts and ends
  (sections 6 and 7.6). The supervisor touches no other line of the
  file.

### 3.4 Plans

A plan lives in `docs/development/plans/<plan>/` of the most downstream
repository it changes. The reason for a plan, and the proof that it is
done, live with its consumer, and an upstream repository's history stays
its own. A plan that changes only skein lives in skein; one that changes
skein and smith lives in smith.

| File | Holds |
|---|---|
| `README.md` | inputs, the session table and its dependencies, the rules particular to this plan, the pulse's settings, "Done when" |
| `code-organization.md` | crates, modules, names, public API, test layout and fixtures, the owner of each shared interface, the plan's "Don't" list and size limits |
| `<repository>/NN-<name>.md` | one session: its increments, with their shapes, test shapes and proofs, and its interface sketches |
| `decisions.md` | the decisions the plan rests on, the supervisor's rulings during the run, and those left to the user |
| `later.md` | what the design names but the plan does not do, and when it should be done |
| `findings/` | evidence the plan cites, if any (section 4.1) |
| `retrospective.md` | after the run: what happened, and what should change (section 15) |
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
| `log.md` | the coordinator | every report as received, every escalation and its answer; appended only |
| `context.md` | the coordinator | what a restarted coordinator needs; rewritten, never appended |
| `thread`, `threads` | `plan start`, `plan resume`, the coordinator | the coordinator's Codex thread's id, for the doorbell; every thread it has had, with when, for the metrics |
| `claude.tsv` | `plan start`, `join`, `wake`, `retro` | one line per Claude Code session that works on the run: its role, its session id and when, for `plan attach`, `plan wake` and the metrics |
| `heartbeat` | the coordinator | touched at every wake-up |
| `.watch` | `plan watch` | the escalations and events the supervisor, or the designer, has seen |
| `blocked.md` | the coordinator | open escalations (section 10.2) |
| `unblock.md` | the supervisor | the answer (section 10.3) |
| `steer.md` | the supervisor | an instruction nobody asked for (section 10.3) |
| `question.md`, `answer.md` | the supervisor; the designer | a design question, and its answer (section 10.4) |
| `supervision.md` | the supervisor | one entry per hourly check (section 14); appended only |
| `.pulse` | `plan pulse` | where the last check stopped |
| `paused.md`, `done.md` | the coordinator | a pause's checkpoint; the run's end |
| `sessions/<repository>-<NN>.md` | the implementer | its session's checkpoint (section 8.6) |
| `logs/` | `heavy`; the implementers | each heavy command's output, and `heavy.tsv`, the queue's statistics (section 11.2); without an active run, `~/src/rust/tmp/logs/`. Each session's scripts and long outputs, in `logs/<repository>-<NN>/` |
| `wt/<repository>-<NN>/` | `plan worktree` | the session's worktree, with its own `target/` |

The escalation files are written whole and renamed into place, so the
other side never reads half a file. A writer that finds its file still
there adds its new headings to it.

Nothing in the run's directory is removed before the run's retrospective
has landed (section 15), except:
- the worktrees (section 7.6);
- sessions' `target/` directories (section 11.4);
- escalation files, once they have been appended to `log.md` or
  `supervision.md`.

### 3.6 The kit

skein's `scripts/dev/` holds the tools every repository uses, and
`scripts/dev/workspace/` the workspace's own files:
`~/src/rust/AGENTS.md`, its `CLAUDE.md`, and the coordinator's Codex
profile. `plan check --fix` links the tools into `~/.local/bin`, so they
are on every agent's `PATH`, and the files into place. The tools read
`DEV_ROOT` (default `~/src/rust`), `DEV_TMP` (`$DEV_ROOT/tmp`) and
`DEV_REPOS` (`skein smith temper`).

| Tool | Does |
|---|---|
| `heavy [--gate] [--tail N] command…` | queues a heavy command, waits, prints the tail of its log, exits with its status (section 11) |
| `land [--docs] [--local] [--gate COMMAND] [--tries N]` | lands the current branch (section 9); `LAND_LOCK` names the lock for `--local` |
| `plan check [--fix] [PLAN_DIR]` | checks the machine and the repositories, and the plan when given one (sections 6 and 12) |
| `plan start PLAN_DIR` | checks the machine, claims the run, starts the supervisor and the coordinator, and rings the doorbell once (section 6) |
| `plan attach ROLE` | opens a role's session in this terminal: `coordinator`, `supervisor`, `designer`, `retrospective` |
| `plan resume` | after a restart, starts again what is not running: pueued, Codex's daemon, the supervisor, the coordinator's turn (section 7.6) |
| `plan join ROLE` | records the Claude Code session it runs in as the run's ROLE, usually the designer |
| `plan wake ROLE MESSAGE` | resumes a Claude Code role's session in the background with MESSAGE (section 10.4) |
| `plan retro [PLAN_DIR]` | starts the run's retrospective (section 15) |
| `plan claim PLAN_DIR`, `plan release` | start and end the machine's run; `plan start` claims |
| `plan worktree [--remove] REPOSITORY NAME` | a session's worktree, at `origin/main` |
| `plan set STEP KEY=VALUE…` | adds or changes a ledger row |
| `plan status` | renders `status.md` |
| `plan watch [--designer]` | waits until the supervisor has something to do, or with `--designer`, the designer (section 10.5) |
| `plan pulse` | prints the run's numbers since the last check (section 14) |
| `plan ring MESSAGE` | rings the coordinator's doorbell (section 7.4) |
| `plan metrics [--out FILE] [PLAN_DIR]` | prints the run's metrics, or writes them to FILE; with `--history`, prints every plan's metrics side by side (section 15.2) |
| `test-plan [--live]` | tests `plan` on a workspace of its own; `--live` also starts real roles, on cheap models |

The roles run in the background:
- **Claude Code roles** are background sessions (`claude --bg`), started in
  the workspace, which Claude Code must trust: run `claude` there once,
  and accept.
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
`PLAN_MIN_DISK_GB`, `PLAN_MIN_MEM_GB`, `PLAN_QUIET`, `PLAN_POLL` and
`PLAN_CLAUDE_DIR` (where the Claude Code roles start) tune `plan`.

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
says what changed and why.

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
- **When there are options, name the default.**
- **Quote the text an answer relies on,** from the design or the plan; do
  not paraphrase it. Check an answer against the code before sending it.
- **Check what matters, as well as saying it.** A rule that must hold for
  the rest of a plan becomes:
  - a lint;
  - a test;
  - a check in the gate;
  - or a line of `plan pulse` (section 14).

  Rules that are only written down drift.
- **Repeat what matters where it applies.** Rules buried in a long README
  are missed. A session file repeats the two or three that bind its
  increments.

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
- **APIs may break** where the design asks. Consumers in repositories the
  plan does not change are fixed separately, in a plan of their own.
- **Scope.** The plan names the repositories it changes; the others are
  read only.
- **Credentials and live services.** The plan names each one it uses, how
  it is borrowed (read only, from the user's existing login), that no
  agent signs in, and the budget of calls or tokens each may spend.
- **Proofs.** Each increment proves itself at the highest layer within
  reach (section 5.5). The plan also says what the repository asks for
  beyond its gate (smith: benchmark probes, `docs/design/benchmarks.md`).
- **The executor.** The coordinator and the implementers read the plan as
  written (section 4.5).
- **Language.** Rust, for everything.
- **Code organization and interface sketches** are always part of the
  plan.

### 5.2 The plan's files

**`README.md`:**
- the title, and a dated line with each repository's start commit;
- **Inputs:** the design documents, `decisions.md`, `findings/`;
- **The sessions:** per repository, a table with columns File, Session,
  Needs at start, and Increments that need more. Then how many sessions
  can start at once, and the order in which to start them when slots are
  short: first the ones others wait on;
- **Rules of this plan:** only what this document, the repositories'
  `AGENTS.md` and their workflows do not say. For example: fences ("this
  increment lands after every other skein increment"), version owners,
  probes. A plan never restates this document;
- **Live services:** each one the plan uses, with its budget of calls or
  tokens;
- **Pulse:** what `plan pulse` looks for in this plan's diffs (section
  14), as lines of their own, in a code block:

  ```
  words: actual|exact|genuine|real
  types: ^(Owner|Legacy)[A-Z]|V2$
  file-lines: 800
  budgets: 15 60
  ```

  `words` are the words the plan forbids in code, `types` the names that
  signal a parallel type, `file-lines` the plan's file limit, and
  `budgets` the focused and fuzzy suites' budgets in seconds;
- **Done when:** observable conditions.

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
  - no test that checks several behaviours under one name;
  - no lint exception in tests;
  - no expected value computed by the code under test;
  - the words the plan forbids in code;
- **size limits:** a file of at most 800 lines and a test function of at
  most 40, unless an increment says otherwise.

**A session file, `<repository>/NN-<name>.md`:**
- **Title and summary:** "Session NN: …", with one paragraph.
- **Read first:** the design sections, decisions and findings it builds
  on.
- **What the design asks:** a short restatement, with section references.
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
    file:line; one reference test written out for each new test file or
    new kind of test; the tests' names; their ceilings (section 5.6)>.
  Proof: <the layer, the test or the script, what it shows, its bound
    (section 5.5)>.
  Don't: <what this increment must not do, beyond the plan's list>.
  Size: <at most N lines of production code>.
  Removes: <what goes, or nothing>.
  ```

  An increment above about 600 lines of production code is split.
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
- **Questions to raise when reached:** what the design leaves open that
  changes observable behaviour.
- **Done when.**

**`decisions.md`:** numbered decisions, each with a short rationale, and
a closing section "For the user", listing the long-term choices the user
may veto.

**`later.md`:** a table with columns Item, Design and When.

### 5.3 Increments

- **Small, and landed on its own.** Each passes the gate within budget and
  leaves `main` coherent: nothing half-built visible to users, no dead
  code. Prefer several small increments to one large one: parallel
  sessions rebase on each other.
- **The increments that need nothing outside come first,** so the session
  can start at once.
- **Dependencies are named by capability,** for example "skein 03's
  `TimedOut` phase", never by increment number alone: numbers move.
- **Upstream first** (section 9.5): an increment that needs an upstream
  change waits for it, and repins in a commit of its own, at its start.
- **Shared files are named,** so the coordinator sees likely conflicts.
- **Open behaviour stays open.** An increment does not decide what the
  design leaves open if the choice is observable; it names the question.
- **A new test world** costs at most about 1 second focused and 5 seconds
  fuzzy. Anything beyond that is a decision to raise, not one to take.
- **Each increment proves itself** at the highest layer within reach
  (section 5.5).
- **The meeting point comes first.** Where two sides meet through a
  translation or a contract (a client and its engine, a protocol's two
  ends):
  - the translation starts with the first types on both sides, exhaustive
    both ways, and every increment that adds a variant extends it, so that
    drift is a build error the day it happens;
  - one thin end-to-end story runs as soon as the far side can answer, and
    each increment grows it;
  - a fake's scenarios also run against the real side, so that the fake
    cannot agree with one side where the other does not;
  - it costs some parallelism, and is worth it: it is what lets later
    increments prove themselves at the joint layer.
- **Test cost holds from the first increment.**
  - The plan cites the repository's rules for what tests may cost (jig:
    `docs/development/workflow.md`, section 2), and gives each increment
    its ceilings (section 5.6).
  - A suite's first breach of its budget is profiled, and the tests are
    made cheaper. A budget's rule is not changed to fit the tests.
- **Transitional code has an end.** An increment that lands a change
  piecemeal and keeps transitional code names the step that removes it:
  legacy paths, compatibility constructors, adapters that supply a value
  their source lacks. A plan that allows this ends with a consolidation
  session.
- **A session may reorder its own increments** when an earlier one needs a
  later one, the later one needs nothing the earlier provides, and landing
  it first leaves nothing unguarded or half-built on `main`. It says so in
  its report and its commit body. Reordering never moves work across
  sessions and never changes an increment's scope.

### 5.4 Before the run

- **A code audit.** For the parts the plan touches, the planner checks the
  code against the design, so that the run does not discover that the
  code already differs from it. What it finds goes in `findings/`, and
  into the increments that fix it.
- **A consistency review.** A sub-agent reads the whole plan against the
  design and looks for:
  - dependency cycles;
  - fences that contradict each other;
  - shared interfaces without one owner;
  - increments that cannot land alone;
  - increments without a shape, a test shape or a proof.

  The planner fixes what it finds.
- **A literal reading.** A second sub-agent reads the plan as the executor
  will (section 4.5), and lists:
  - each instruction that can be read two ways;
  - each rule that nothing checks;
  - each request for evidence without a command, a bound or a count of
    runs.

  The planner fixes what it lists.
- **The plan lands** with `land --docs`, and `STATUS.md` gets the plan's
  row, `ready`.

### 5.5 Each increment's proof

Each increment shows that what it puts in place works for real, not in
theory, with the most convincing check its moment allows. The planner
chooses the proof as part of the increment, balancing confidence against
the run's pace and its tokens.

- **The layer.** The proof sits at the highest layer the code reached by
  then allows. From lowest to highest, with jig's as examples:

  | Layer | What runs | jig | Aim for |
  |---|---|---|---|
  | unit | a component's step tests | a domain's unit tests | milliseconds |
  | world | a component with fakes around it, under a referee | the client's world and its scripted engine | 0.5 s per test |
  | joint | the real components together, I/O simulated | the joint world | seconds |
  | process | the real binaries, on this machine | `ops` standalone, in-memory sockets or loopback | about a minute |
  | live | real outside services | a real LLM provider, a browser | at most 5 minutes |

- **The Proof line** of an increment (section 5.2) gives:
  - **the layer,** and, when it is not the highest one that exists, why
    not, in one line;
  - **what runs:** a test by name, in the suite, which is preferred, or a
    script the implementer writes and keeps in the run's
    `logs/<repository>-<NN>/`. A
    script is for what the suite cannot hold: live services, real
    binaries, a browser;
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
- **Balance.** Size the proof to the risk.
  - Aim for a proof that costs at most about a tenth of the increment's
    work, and runs in minutes at most.
  - Where the highest layer would cost more, take the next one down, and
    say so.
  - Most increments prove in the suite. A live proof is for an increment
    that touches a live service, about once or twice a session.
- **A script needed twice is kept.** When a later increment needs the same
  proof, it becomes a test in the suite, or a script under the
  repository's `scripts/`, and the plan names it.
- **Layers open as early as they can.** The plan schedules the first
  joint, process and live stories as soon as the code can run them. Once
  a layer exists, each later increment that it can prove proves there.

### 5.6 Each increment's tests

The implementer writes tests as the session file shows them, so the plan
shows them precisely.

- **The test shape** of an increment names:
  - the test file;
  - the fixtures and helpers to reuse, by `file:line`;
  - each test's name;
  - for each new test file, or each new kind of test, one reference test
    written out in full.
- **Tests the plan shows:**
  - check one behaviour each;
  - write expected values as literals, never computed by the code under
    test;
  - name their fixtures (the parties, tasks and connectors a story uses),
    never bare numbers, unless the number is the point;
  - have names of about eight words that state the behaviour;
  - reuse the crate's fixtures module, which grows with each session.
- **Ceilings are per test, not shares of a budget:** a unit test under 20
  ms, a focused world test under 0.5 s, a fuzzy test about 5 s, on the
  development machine, unoptimized. A test past its ceiling is made
  cheaper before it lands. It is not merged with others to fit.
- **Six tests or so per increment are enough to show.** More adds little.
  Production bodies stay out of the plan (section 5.2): they go stale at
  once.

## 6. Starting a run

1. **The plan is on `origin/main`,** with its row `ready` in `STATUS.md`.
2. **The machine is ready.** `plan check --fix <plan-dir>` passes. It
   checks:
   - the plan and its `STATUS.md` row;
   - the primary checkouts: on `main`, clean, equal to `origin/main`;
   - the tools, pueued and the queue's lanes, memory and disk;
   - that Claude Code trusts the workspace, Codex's daemon, and the
     coordinator's Codex profile.

   `--fix` links the tools, starts pueued (as a user service where
   systemd has a user bus) and Codex's daemon, and creates and starts the
   lanes.
3. **`plan start <plan-dir>`** does the rest, and returns:
   - it runs `plan check`, and claims the run, naming it in
     `~/src/rust/tmp/active-plan`;
   - it starts the supervisor, a background Claude Code session (Opus 5.5,
     xhigh) in the workspace, named `<plan> supervisor`, with "Supervise the
     run of `<plan-dir>` as skein's docs/foundation/development.md,
     sections 2.5, 3.3, 4.5, 6, 7.6, 10 and 14, say.";
   - it starts the coordinator, a thread on Codex's daemon named `<plan>
     coordinator`, with "Coordinate the run of `<plan-dir>`, as skein's
     docs/foundation/development.md, section 7, says.";
   - it rings the doorbell once, which the coordinator logs (section 7.4).
     If the ring fails, steers wait for the coordinator's next wake-up.
4. **The supervisor, first:**
   - marks the plan `active on <machine> since <date>` in each `STATUS.md`
     that has its row, with `land --docs`; the row is what keeps another
     machine from claiming the same plan;
   - starts its watch, `plan watch`, in the background (section 10.5);
   - starts its hourly check (section 14): in Claude Code, a loop or a
     cron job, which lasts only as long as the session.
5. **The designer joins,** if the plan may raise design questions: in its
   Claude Code session, it runs `plan join designer`, so that the
   supervisor can wake it (section 10.4).
6. **The user looks in when they want:** `plan status` shows the steps and
   the roles; `plan attach supervisor` and `plan attach coordinator` open
   their sessions.

A session that takes over from another, supervisor or designer, records
itself with `plan join <role>`.

## 7. Coordinating

### 7.1 Setting up

1. Read this document's sections 3.5, 3.6 and 7 to 12, the plan's
   `README.md` and `code-organization.md`, and the head of each session
   file: its increments and dependencies.
2. Write your thread's id to the run's `thread` file:
   `printf '%s\n' "${CODEX_THREAD_ID:-unknown}" > <run>/thread`. If it
   writes `unknown`, say so in `log.md`.
3. Fill the ledger: one `plan set <step> repo=… session=… title=…
   state=…` per increment. A step is named `<repository>-<NN>.<item>`, for
   example `smith-05.3`.
4. Write `context.md`.

### 7.2 Starting sessions

- **Start every session whose start dependencies have landed,** unless
  memory or disk is short (section 11.4).
- **When there are fewer slots than sessions,** start first the sessions
  others wait on, in the README's order. Never hold a slot idle to keep
  the order.
- **The worktree:** `plan worktree <repository> <NN>`.
- **The spawn.** Spawn with `fork_turns: "none"`, so the implementer does
  not inherit the coordinator's conversation, and give it this prompt:

  ```
  You are an implementer. Read ~/src/rust/skein/docs/foundation/
  development.md, section 8, then ~/src/rust/<repository>/AGENTS.md,
  then <plan-dir>/README.md and <plan-dir>/code-organization.md, then
  <plan-dir>/<repository>/<file>. Implement that session in <worktree>,
  one increment at a time. Your checkpoint is
  <run>/sessions/<repository>-<NN>.md, and your logs and scripts go in
  <run>/logs/<repository>-<NN>/.
  ```

  A sub-agent starts in the coordinator's directory, so it does not load
  its repository's `AGENTS.md` by itself: the prompt names it. Add to the
  prompt, word for word, the "Pass on" text of each steer in force
  (`context.md`). When restarting a session, add what you found: its last
  landed step, its branch, and whether its worktree has uncommitted
  changes.
- **Slots.** Codex counts finished sub-agent threads against
  `max_threads`. Give each thread a slot name (`w1` to `w8`) and record it
  in the ledger's `slot` column, with the thread's id, as `spawn_agent`
  returned it, in the `thread` column. If `plan set` refuses `thread=`,
  put `thread=<id>` in the step's `note` instead. When a spawn is refused, reuse a
  finished slot with `followup_task`: say that its previous session is
  over, and give the new session's prompt.

### 7.3 Reports

An implementer reports when an increment lands, when it stops on a
question, and when its session ends or yields (section 8.6). After each
report:

1. Append it to `log.md`, as received, under the heading `## <repository>
   <NN>, <UTC time>`, for example `## jig 22, 2026-10-11T10:05:00Z`.
2. Check it with git only, in the session's worktree. For a landed step:
   - `git merge-base --is-ancestor <commit> origin/main` succeeds;
   - `git log -1 --format='%(trailers:key=Step,valueonly)' <commit>`
     prints the step;
   - `git status --porcelain` prints nothing.

   For any other report, only the last. Read no code, diff or test. If a
   check fails, send its output to the implementer (`send_message`, or
   `followup_task` if it has ended its turn), and wait for its next
   report.
3. `plan set` the step's new state and commit, then `plan status`.
4. Start what the plan now allows, and resume the sessions waiting for
   what landed.

A report's question goes into `blocked.md` (section 10.2), unless it is
only a dependency not yet landed: that is not a block, and the session is
resumed when the dependency lands.

**Steps not in the plan.**
- **You start a step only from the plan, an unblock or a steer.**
- A step that an unblock or a steer adds is named as the instruction
  names it, `<repository>-<NN>.s<k>`. Its ledger note reads
  `supplement: <the instruction's time>`. The instruction names its
  session file; the step goes to that session's implementer if it is
  running, and otherwise to a new implementer, started as section 7.2
  says, with that file.
- When a report shows work that no step covers, write it to `blocked.md`
  as a plan defect, and go on with everything else. Do not create a
  session, a step, a rule about who owns which files, or a protocol
  between sessions.

**A step's state** in the ledger is one of:
- `waiting`: something it needs has not landed;
- `ready`: it can start;
- `running`: its session is on it, with the slot and the branch;
- `blocked`: on an escalation, named in the note;
- `landed`: with its commit;
- `dropped`: by an instruction, named in the note.

### 7.4 Waiting

- **One wait.** Between events, call `wait_agent` with `timeout_ms:
  300000`. This rule takes precedence over Codex's guidance against
  waits longer than 60 seconds. The wait ends early on any implementer's
  message, and on the doorbell.
- **At every wake-up,** run this one command, and act on what it shows:
  `touch <run>/heartbeat; ls <run>/unblock.md <run>/steer.md 2>/dev/null;
  free -g; df -h ~/src`.
  - If `unblock.md` is listed, handle it (section 10.3); then `steer.md`,
    if it is listed.
  - Act on memory and disk as section 11.4 says.
- **When no session is running,** `wait_agent` may return at once. Wait
  with `sleep 60` instead, and run the wake-up command after each.
- **No background waits.** Do not start `until` loops, `sleep` sessions,
  watchers or polling scripts, in the background or in a sub-agent. They
  cannot wake you, and they die with you. The wait above and the
  doorbell are the only ways you are woken.
- **The doorbell.** After writing `unblock.md` or `steer.md`, the
  supervisor runs `plan ring`, which steers a one-line message ("steer.md
  is ready") into your turn and ends your wait. Handle the file it names
  as at any wake-up. A ring that names no file: append `Rang, <UTC time>:
  <message>` to `log.md`, and wait again.
- **A finished implementer** ends its turn with its last report. Give a
  session that yielded its next step with `followup_task`.
- **The user and the designer are reached only through the supervisor.**
  The coordinator uses no other channel to them.

### 7.5 Keeping state

- **`context.md` is the coordinator's memory.** Rewrite it whenever the
  state changes: the slot map, the sessions running and their last step,
  open escalations, and decisions in force from answers and steers that
  the plan does not yet say, with each steer's "Pass on" text. Write full
  sentences, at most 60 lines.
- **After a compaction or a restart,** re-read this section, `context.md`
  and the ledger before acting.

### 7.6 Pausing, resuming, ending

- **Pause** at the user's or the supervisor's request. Start nothing new.
  Tell each running session, with `send_message`: "Pause: land your
  increment, or commit it on your branch; report; end your turn." When
  each has reported, write `paused.md`: each session's worktree, branch,
  head and last landed step.
- **Resume after a restart.**
  1. Append to `log.md` a heading "Coordinator resumed, <UTC time>:
     <cause>", the cause one of: reboot, crash, the user's request,
     unknown. Read `context.md`, the ledger and the end of `log.md`.
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

  After a restart of the machine, the user runs `plan resume`. It starts
  pueued and Codex's daemon if they are down, resumes the supervisor's
  session (which starts its watch and its hourly check again), and
  starts a coordinator turn with "Resume the run of `<plan-dir>` as skein's
  docs/foundation/development.md, section 7.6, says." The coordinator
  starts no Claude session.
- **End** when every step has landed or been dropped.
  1. Write `done.md`:
     - each item of the plan's "Done when", with the ledger steps that
       cover it; run nothing;
     - the decisions in force from unblocks and steers, as `context.md`
       lists them.

     The supervisor adds to each decision the design commit that took it
     in, or "not in the design yet".
  2. Remove the worktrees.
  3. `plan release`. The run's directory stays.

  The supervisor then marks the plan `done <date>` in `STATUS.md`, and the
  retrospective follows (section 15).

## 8. Implementing: the implementer's card

### 8.1 Each increment

1. **Read the increment** in the session file, and the design sections its
   "Read first" names. Read only those, not the whole design.
2. **Branch from `origin/main`** in your worktree:
   `git fetch origin && git switch -c <plan>/<repository>-<NN>-<item>
   origin/main`.
3. **Implement it with its tests,** following the repository's `AGENTS.md`
   and skein's `programming-model.md` and `testing-strategy.md`:
   - the increment's Shape gives your paths, names and signatures;
   - its Test shape gives your test file, the fixtures to reuse, and the
     reference test to write the others like.
4. **Run its proof once,** as its Proof line says (section 8.8).
5. **Land it** with `land` (section 8.5), then report (section 8.6).

The code graph indexes the primary checkouts, which are on `main`: use it
for structure, and check each name it gives with `rg` in your worktree
before relying on it.

### 8.2 Rules

- **The design is the contract.** Never edit `docs/design/`, the plan or
  the run's files other than your checkpoint and your logs directory. Where the code seems to
  need a different design, stop (section 8.7).
- **Implementation choices are yours:** names, module placement, values
  within the presets' ranges. Report them; do not ask.
- **Work only in your worktree.** Never touch a primary checkout or another
  session's worktree.
- **No `git stash`:** the stash is shared by every worktree of a
  repository. Commit work in progress on your branch instead.
- **Upstream first** (section 9.5).
- **Removing a mechanism removes all of it,** in this repository and in
  this increment: code, limits, accounting, facts, tests and docs.
- **Each test lands with the code it covers.**
- **The plan's "Don't" lists bind you:** the plan's own, in
  `code-organization.md`, and the increment's.
- **Departing from the Shape** is an implementation choice. Make it when
  the code needs it, and say how and why in the commit body. The
  exception is a type that crosses a boundary: changing its fields is a
  question (section 8.7).
- **Test ceilings** (the Test shape's, or `code-organization.md`'s) are
  read once, from the times nextest prints in your last focused run. A
  test above twice its ceiling is made cheaper before it lands. Never
  merge tests to fit a budget.
- **Evidence is the gate, the proof, and what the repository's workflow
  or the plan asks beyond the gate; nothing else.** Write no manifests of
  inputs, no observers, no polling scripts, and no repeated measurements.
  Logs and scripts go in `<run>/logs/<repository>-<NN>/`.

### 8.3 Heavy commands

`heavy`, `land` and `plan` are on your `PATH`, from skein's
`scripts/dev/`.

- **`heavy <command>`** runs what compiles, and the tests of the crates you
  touch: `cargo check`, `clippy`, `build`, `doc`, `nextest -p …`. It uses
  the build lane.
- **`heavy --gate <command>`** runs, alone on the machine, whole suites,
  measurements (with the profiles and flags your repository's workflow
  documents, such as `--profile measure -j 1`), probes and benchmarks.
  `land` runs the gate itself.
- **Light commands run directly:** git, `cargo fmt`, reading, `rg`.
- **Whole suites run only in `land`'s gate,** or for a measurement or a
  probe that the Proof line or the repository's workflow names. Do not
  run the gate before `land`.
- **Never set `CARGO_BUILD_JOBS` or `NEXTEST_TEST_THREADS`,** nor pass
  `-j` beyond what the workflow documents: the lanes set the width.
- **Never nest them:** `heavy land`, or `heavy` inside a heavy command,
  waits for itself.
- **Read a command's log from the path `heavy` prints.** Never dump
  `pueue status`. If your tool call times out, the task runs on: `heavy`
  printed its id first, and `pueue wait <id>` and `pueue log <id>` reach
  it.

### 8.4 Commits

```
<Subject, imperative, at most 72 characters>

<What changed and why. The design section it follows. Where it departs
from a sketch, how and why. The evidence the repository asks for beyond
the gate.>

Proof: <layer>, <test name or script path>, <result in one line>
Plan: <plan>
Step: <repository>-<NN>.<item>
```

`land` adds the trailers `Gate-Tree` and `Gate-Result`. A commit that
landed without its body is never rewritten; the body goes in the report.

### 8.5 Landing

Run `land` from your worktree (section 9). By its exit status:

- **2, refused:** fix what it names. "Nothing to land" after a 5 means the
  commit had landed.
- **3, conflicts:** resolve them against the design, never by dropping
  another session's change. Then `git rebase --continue`, and `land` again.
  If resolving needs a decision, stop (section 8.7).
- **4, the gate failed:** fix it, and `land` again. If it fails in code
  your increment did not touch, stop on a question (section 8.7).
- **6, main kept moving:** wait 60 seconds, and `land` again. After three
  such tries, report it and stop.
- **5, 7 or 8:** report it, and stop.

### 8.6 Reports and the checkpoint

**When to message the coordinator:** when an increment lands, when you stop
on a question, and when the session ends or yields. Send nothing else; in
particular, no progress messages: every message wakes the coordinator.
After a landing, send the report and go on. When the session is done, or
nothing is left that does not wait (it yields), end your turn with the
report as your final answer.

A report is at most 40 lines. Name the files that hold anything longer
(`<run>/logs/<repository>-<NN>/…`) by path; do not paste them. Use this shape, in full
sentences:

```
Session: <repository> <NN>
Step: <step>, <landed | question | blocked | done | yielded>
Commit: <full hash> on origin/main, parent <full hash>
Gate: <the Gate-Result trailer>
Proof: <the commit's Proof line>
Evidence: <what the repository asks beyond the gate, or "none">
Next: <the next step, or what it waits for, by capability>
Questions: <none, or each question with the design text quoted, the
options, your proposal, and what you continue with meanwhile>
Agreements: <none, or what you agreed with another session>
Worktree: <clean, or what is in it>
```

**The checkpoint.** After each report, overwrite your checkpoint,
`sessions/<repository>-<NN>.md`, with the same facts and with what is in
progress. A restarted implementer continues from it.

### 8.7 When to stop

- **Stop only for a real gap:** the design, `decisions.md` and the session
  file are silent or contradict each other, and the choice changes
  behaviour that something outside the code can observe. Quote what they
  say.
- **A dependency not landed yet is not a block.** Do the increments that
  do not need it, then report what you wait for.
- **While a question is open,** go on with whatever does not depend on it.
- **A fix outside your increment** is a question too: a failing proof, or
  a gate failing in code you did not touch.

### 8.8 The proof

- **Run the increment's proof once,** when the increment is done and its
  focused tests pass: the test, or the script, its Proof line names.
- **A script** goes in `<run>/logs/<repository>-<NN>/`. Aim for at most
  about 100 lines. It stays within the Proof line's bound: wall time, and for a
  live service its calls or tokens.
- **Record it** in the commit's `Proof:` line (section 8.4) and in the
  report.
- **A rebase does not run it again,** unless the rebase's conflicts
  touched what it proves.
- **If it fails,** it found a bug: fix it before landing. If the fix lies
  outside your increment, stop on a question (section 8.7).
- **If the Proof line cannot be followed,** say why in the report, run the
  same check one layer down (section 5.5) within the same bound, and
  record that one. Do not stop for this.

## 9. Landing

### 9.1 What `land` does

From a clean worktree on the branch to land, `land`:

1. rebases the branch on `origin/main`;
2. runs the repository's gate, `scripts/check.sh`, through `heavy --gate`;
3. writes the evidence into the tip commit's trailers;
4. pushes the tip to `origin`'s `main`;
5. moves the local `main`, and the checkout that has it, to the tip.

A push only fast-forwards, so it is the lock. If it is rejected, main
moved, and `land` starts again from the rebase. It gives up after five
tries. Its exit status:

| Status | Means |
|---|---|
| 0 | landed |
| 2 | refused before starting, or nothing to land |
| 3 | the rebase stopped on conflicts, left in progress |
| 4 | the gate failed |
| 5 | the local `main` could not follow: with a push, the commit landed on `origin/main`; with `--local`, nothing landed |
| 6 | main kept moving |
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
- **Anyone can check the evidence:** the landed commit's tree equals its
  `Gate-Tree`.

### 9.3 Conflicts

A rebase that stops on conflicts is left in progress, for the branch's
author to resolve. `land` exits 3. An implementer resolves its own
conflicts (section 8.5), and escalates only when resolving them needs a
decision.

### 9.4 Markdown only

`land --docs` checks that the branch changes only Markdown files, and
skips the gate. It is how designs, plans and `STATUS.md` land. Rust doc
comments are code, and are gated.

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

An implementer stops on a real gap (section 8.7) and reports it to the
coordinator, in a Codex message, with the question in the report's
shape (section 8.6).

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

<The report, as received.>
```

- **One file.** A new escalation while one is open goes under a new
  heading in the same file.
- **The coordinator never answers** a design question itself, and never
  writes `unblock.md`.

### 10.3 From the supervisor: `unblock.md` and `steer.md`

```
# Unblock

Answer to the blocked report of <time> (<headings answered>).
Reverses: <the time of an earlier answer this one reverses>, or nothing.

## Decision

<The decision, quoting the design or plan text it rests on.>

## Landed

<Design and plan commits, with their hashes.>

## Instructions

1. …
```

- **When the coordinator finds `unblock.md`:**
  1. it appends both files to `log.md`, under "Blocked and unblocked,
     <time>";
  2. it deletes `unblock.md`, and removes from `blocked.md` the headings
     the answer names, deleting the file when none is left;
  3. it gives each session the answer concerns its Decision and
     Instructions, word for word (`send_message`, or `followup_task` if
     the session has ended its turn), and `plan set`s its step back to
     `running`;
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

- **When the coordinator finds `steer.md`:**
  1. it appends it to `log.md`, under "Steered, <time>", and deletes it;
  2. it acts on each point;
  3. it logs one line per point: "Steer <its time>, point 1: done, <what
     it did>", or "…: cannot, <why>", which also goes into `blocked.md`
     (kind: process);
  4. it gives each point's "Pass on" text, word for word, with
     `send_message`, to the running sessions it applies to, and adds it to
     the prompt of each session it starts while the point is in force;
  5. it updates `context.md`. A point whose "Until" has come leaves it.
- **The supervisor follows up.** The next check verifies each point. A
  point not followed is sent again, with what was seen.

### 10.4 Who decides what

- **The supervisor decides what is not design:**
  - a plan defect, the order of steps, the plan's scope;
  - a step not in the plan, which it names `<repository>-<NN>.s<k>`, with
    a short session file (shape, test shape, proof and "Don't", in about
    15 lines);
  - process, infrastructure and credentials, within what the user allows.

  It records each ruling in the plan's `decisions.md`, and lands each
  change to the plan with `land --docs` before the unblock that relies on
  it. When the same kind of block comes back, it adds a standing rule to
  the plan's README; a rule that would serve every plan becomes a proposal
  for the retrospective (section 15).
- **The designer decides a design gap.** The supervisor writes the
  question to the run's `question.md`, in `blocked.md`'s shape, with its
  own reading of the design, and wakes the designer: `plan wake designer
  "question.md has a question: answer it as development.md, section 10.4,
  says."` This resumes the designer's own session in the background, with
  its context; a designer session the user has open gets a copy instead.
  The designer:
  1. decides;
  2. amends the design with `land --docs`, then the session files that
     mention the point;
  3. writes `answer.md`, naming the commits.

  The supervisor then writes the unblock, appends both files to
  `supervision.md`, and deletes them.
- **Before an answer is sent,** its author checks it against the code
  (the code graph indexes `main`) and quotes the text it rests on.
- **What goes to the user:**
  - decisions with long-term consequences that would be hard to change;
  - any language other than Rust;
  - what the user reserved.

  A question for the user goes into `decisions.md`, under "For the user".
  The run blocks on it only if no work can continue without it.

### 10.5 The supervisor's watch

- **The watch runs on the run's machine.** The supervisor runs `plan
  watch` in the background (in Claude Code, a Bash command with
  `run_in_background`). It ends, saying why, when:
  - `blocked.md` has headings it has not shown, and no answer is pending;
  - `answer.md` appears;
  - the coordinator's heartbeat is older than ten minutes;
  - the run pauses or ends.
- **After each event,** the supervisor handles it, then runs the watch
  again.
- **A quiet coordinator.** When the heartbeat is ten minutes old, the
  supervisor rings the doorbell. If it is still old ten minutes later,
  the supervisor alerts the user (in Claude Code, a push notification).
  The alert gives the restart command, `plan resume`.
- **The designer's watch,** `plan watch --designer`, ends when
  `question.md` has headings it has not shown. A designer the user keeps
  open runs it; one that waits in the background needs none, since `plan
  wake` resumes it.
- **When the supervisor's context grows past about half its window,** a
  fresh supervisor session takes over. It reads `blocked.md` and the end
  of `supervision.md` first: the watch does not show again what it has
  shown. This document, the plan, `decisions.md`, `log.md` and
  `supervision.md` are enough to go on.

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

- **pueued runs as a systemd user service,** never from an agent's command:
  a daemon started by an agent dies with it. Lingering
  (`loginctl enable-linger`) keeps it running when the user logs out.
  Where systemd has no user bus (some containers), `plan check --fix` and
  `plan resume` start it on its own.
- **Two lanes:**
  - **gate** (parallelism 1) runs gates, whole suites, measurements,
    probes and benchmarks. A gate task pauses every other pueue group,
    waits for their running tasks, runs alone, then resumes them, so
    suite times are measured on a quiet machine;
  - **build** (parallelism 2) runs everything else, each task with
    `HEAVY_BUILD_JOBS` (4) cargo jobs.
- **A gate task killed outright** (`pueue kill`) cannot resume what it
  paused. `plan check --fix` starts a paused lane again.
- **`heavy` is the only way in.** Nobody changes a lane's parallelism,
  except as section 11.4 says.

### 11.2 Logs

`heavy` writes each command's whole output to the run's `logs/` and
prints only its tail and the log's path. It also appends a line to
`logs/heavy.tsv`: the task's id, its lane, when it ended, the seconds it
waited and ran, its exit status, the memory available as it ended (GB),
its directory, its log and its command. Then it removes the task from
pueue, which keeps the daemon's state small.

### 11.3 The cache

- **kache is the global `rustc-wrapper`.** Its keys do not depend on
  paths, so a fresh worktree reuses every crate that did not change.
- **It gains most when worktrees are new,** and little on the crates a
  session keeps editing.
- **Mind its store.** Watch its size against its cap (`kache report`).
- **`cargo clippy --fix` fails on its read-only outputs.** Fix lints by
  hand, or run with `RUSTC_WRAPPER=` set empty.

### 11.4 Resources

- **Each session's `target/` takes 1 to 8 GB.**
- **The coordinator checks** available memory (`free -g`) and disk (`df -h
  ~/src`) before each start, and at every wake-up (section 7.4).
- **Below 6 GB of memory or 40 GB of disk,** it holds new starts.
- **Below 3 GB of memory,** it narrows the build lane to one task (`pueue
  parallel 1 --group build`), and widens it again above 6 GB. It says so in
  `log.md` each time.
- **Below 1.5 GB of memory or 15 GB of disk,** it stops the line.
- **To free disk,** it may remove the `target/` of a session that is not
  running.

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
  moving: rebase, gate again, push.

## 13. Open questions

- **The build lane's width.** Two tasks of four jobs each is untested
  under a real run's load. The next run measures memory and the queue
  (`logs/heavy.tsv`) and settles it.
- **Gating again when main moves** is most of the queue's time: a branch
  is gated again each time another lands first. A single lander that
  rebases and gates several branches in turn could cut it. The metric
  `gates_per_step` (section 15.2) shows what it would save.
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
  with `thread/resume` before starting a turn. That path is untested
  after a real reboot.
- **Closing finished Codex threads,** instead of reusing slots, if the
  runtime allows it.
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
minutes. Problems are cheaper to fix the hour they appear.

1. **Run `plan pulse`** (section 14.2). It prints the numbers since the
   last check, and moves its mark.
2. **Read** the log's headings since the last check, the first five
   lines of each report, and the coordinator's acknowledgement of the
   last steer.
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
     lines should rise?
   - **waste:** report lengths, evidence beyond the gate and the proof,
     gates per step, polling, idle slots, tokens per step;
   - **tests:** the checklist;
   - **taste:** the pulse's code signals, and the sampled diff.
5. **Write** an entry in `supervision.md`, of at most 15 lines: the
   pulse's headline numbers, one line per judgement, and the steers sent.
   When something needs action, write `steer.md` (section 10.3), then
   ring the doorbell.

### 14.2 What `plan pulse` prints

Since the last check:

- **progress:** steps landed, planned and not, against the plan's rest;
  the planned count over the last three hours; each running session's
  time since its last landing;
- **the channel:** open blocks and their age; steers not acknowledged;
- **proofs:** each landed step's `Proof:` line, its layer against the one
  its increment named, and the steps with none;
- **waste:** the log's growth; the reports' median and longest length;
  gates per landed step; queue wait against run time (`logs/heavy.tsv`);
  files and scripts written under `logs/`;
- **tests:** each suite's time from the `Gate-Result` trailers, against
  its budget; the tests past four fifths of a ceiling, and the SLOW ones;
  lines added in tests and in code;
- **code signals in the diff:** new files over the plan's size limit; new
  lint exceptions such as `too_many_lines`; the words the plan forbids;
  types named like a parallel type (the plan's "Pulse" patterns); test
  helpers defined in more than three files;
- **cost so far:** the time and token metrics of section 15.2, for the
  run so far;
- **the machine:** the heartbeat's age, memory and disk.

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
- **Shape:**
  - one behaviour per test;
  - expected values written as literals;
  - named fixtures, not bare numbers;
  - helpers reused from the crate's fixtures, not defined again;
  - names that state the behaviour in about eight words.
- **Weight:**
  - a test function within the plan's limit, and a file within its own;
  - no test that checks several behaviours under one name;
  - no lint exception in tests.
- **Awkwardness is a signal.** A test that needs a large setup to say
  something small points at the interface under test. It calls for a
  steer, or a change to the plan's Shape, not for a bigger helper.

### 14.4 When a check finds a problem

- **The first time,** a steer gives the corrected shape, with a concrete
  example: a test rewritten, a report's lines, the command that is the
  evidence.
- **The second time,** the point becomes a standing rule in the plan's
  README, or a line of the plan's "Don't" list, and a steer says so.
- **A rule that would serve every plan** becomes a proposal for the
  retrospective (section 15).
- **The run heading away from "Done when"** calls for a steer that stops
  the line in its scope (section 10.6), and for the plan to be adjusted
  (section 2.5) before it starts again.

### 14.5 What the check is not

No code review, no running of tests, no reading of whole reports. A
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
    `claude.tsv`);
  - the earlier plans' metrics (`plan metrics --history`) and their
    retrospectives.
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
     metric it was to move, and whether it moved.
  8. **Proposals:** each a one-sentence rule, its evidence, the document
     it changes (this one, a repository's `workflow.md` or `AGENTS.md`,
     the plan's shape), and the metric it should move, and which way.
  9. **Debts left in the code,** for the repository's `tech-debt.md`.
- **The user decides** which proposals to adopt; the designer applies
  them.

### 15.2 Metrics

The same metrics, defined the same way, for every run, so that changes to
this process can be judged by what they move. `plan metrics` computes
them from the run's files, the git history and the agents' traces; nobody
counts them by hand.

| Metric | Is |
|---|---|
| `wall_h` | hours from the claim to `done.md` |
| `running_h` | `wall_h`, less pauses and the time the heartbeat was older than ten minutes |
| `steps_planned`, `steps_added`, `steps_dropped` | steps landed from the plan, steps added during the run, steps dropped |
| `step_h` | `running_h` per landed step |
| `escalations` | blocks, by kind (section 10.2) |
| `escalation_wait_min` | the median time from a block to its answer, in minutes |
| `blocked_h` | hours in which at least one block was open |
| `answers_reversed` | unblocks whose `Reverses:` line names an earlier answer |
| `steers` | steers sent, and points the coordinator refused |
| `restarts` | `log.md`'s "Coordinator resumed" headings, by cause |
| `tokens_coordinator` | input tokens, with the part read from the cache, and output tokens of the coordinator's threads |
| `tokens_implementers` | the same, for every thread the coordinator spawned, at any depth |
| `tokens_claude` | the same, for the Claude Code sessions in `claude.tsv`, their sub-agents included |
| `tokens_per_step` | all of the above, per landed step |
| `coordinator_requests` | the coordinator's model requests, and those that only saw a wait end |
| `compactions` | the coordinator's context compactions |
| `gates_per_step` | gates run per landed step |
| `queue_wait_ratio` | time waiting in the queue against time running (`logs/heavy.tsv`) |
| `suites` | each suite's test count and time at the first and the last landing |
| `proofs` | landed steps by proof layer, and those below their increment's layer |
| `report_lines` | the reports' median and 90th percentile length |
| `lines` | lines added and removed, in code and in tests |

- **`metrics.tsv`** holds one line per metric: its name, its value and its
  unit. `plan metrics --history` reads the `metrics.tsv` of every plan in
  the workspace's repositories, and prints them side by side.
- **A definition changes only with this document.** The retrospective
  names the metrics that are not comparable with earlier runs, and why.
- **The loop.** Each proposal names the metric it should move. The next
  retrospective says whether it moved. A plan may set itself a target
  from the history, as a guide (section 4.5): for example, tokens per
  step.

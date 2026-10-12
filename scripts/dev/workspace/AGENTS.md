# Agent guidance for ~/src/rust

This directory holds the Rust projects, each a git repository with its own
`AGENTS.md`, and the machine's working files in `tmp/`. Read a
repository's `AGENTS.md` before touching its code.

This file is skein's `scripts/dev/workspace/AGENTS.md`, linked into
`~/src/rust/`: change it there, and land it in skein.

**The code graph.** Use the codebase-memory-mcp tools to inspect the
code: they index each repository's primary checkout, `main`.

## The projects

| Repository | What it is | Builds on | Its gate |
|---|---|---|---|
| `skein` | the io and protocol kit, and the foundation documents | — | `scripts/check.sh` |
| `smith` | the agent kit | skein | `docs/development/workflow.md`, section 1 |
| `jig` | the kit for agentic applications | skein, smith | `scripts/check.sh` |
| `temper` | the application | skein, smith, jig | `docs/development/workflow.md`, section 1 |

Dependencies point upstream only: skein names neither smith nor temper,
smith never names temper, and jig never names temper.

This directory is the workspace. On a machine that keeps its checkouts
elsewhere (`~/src/` rather than `~/src/rust/`), read that directory where
this file says `~/src/rust`; the tools find it by themselves.

## The process

skein's `docs/foundation/development.md` says how work goes from an idea
to `main`: the roles (designer, planner, supervisor, coordinator,
implementer, and the retrospective's author), where designs, plans and
runs live, how increments land, how a run is supervised and escalates,
and how it ends with a retrospective and its metrics. Its tools are in
`skein/scripts/dev/`: `heavy`, `land` and `plan`.

- **The design** of each project is in `<repository>/docs/design/`, with
  drafts in `docs/design/draft/`. Only the designer edits it.
- **How far each project is from its design** is in
  `<repository>/docs/design/STATUS.md`.
- **Plans** are in `docs/development/plans/<plan>/` of the most
  downstream repository they change, with their retrospective and
  metrics once run.
- **A run's state** is in `tmp/<repository>/<plan>/`, and
  `tmp/active-plan` names this machine's run. `plan status` shows it.

## Starting things

`heavy`, `land` and `plan` are skein's `scripts/dev/` tools, linked into
`~/.local/bin`.

**A new machine.** Clone skein, smith, jig and temper from
`git@github.com:forgetic/` into `~/src/rust/`, then run
`skein/scripts/dev/plan check --fix`. It links the tools, this file, its
`CLAUDE.md` and the coordinator's Codex profile, sets up the queue, and
names what is still missing (pueue, kache, jq, cargo-nextest, the git
settings below).

**A design.** Start Claude (Opus 5.5, effort xhigh) in `~/src/rust`, and
say:

> Design: <the idea>. Work as the designer of skein's
> docs/foundation/development.md, sections 2.1 and 4.

**A plan.** In the designer's session or a fresh one:

> Plan <the design documents, or the STATUS.md rows> as development.md,
> section 5, says, in <repository>/docs/development/plans/<name>/.

**A run.** Run these from `~/src/rust`:

1. `plan check --fix <repository>/docs/development/plans/<name>`, until
   it passes.
2. `plan start <repository>/docs/development/plans/<name>`. It claims the
   run, starts the supervisor (a background Claude session) and the
   coordinator (a thread on Codex's daemon), and returns.
3. If the plan may raise design questions, in the designer's Claude
   session: `plan join designer`, then let it end its turn; it does not
   watch the run. The supervisor wakes it with each question.

**While it runs:** `plan status`; `plan attach supervisor`, `plan attach
coordinator`. After a restart of the machine: `plan resume`.

**After a run.** The supervisor starts the retrospective (`plan retro`),
a fresh Claude session that writes it beside the plan.

**Where things stand:**
- a run: `cat tmp/<repository>/<plan>/status.md`, or `plan status`;
- a project: `<repository>/docs/design/STATUS.md`.

## Until the reliability plan ends

The reliability plan, started before development.md, runs on
`debian-16gb-hel1-1` from `~/src/rust/plans/reliability-plan/`, under its
own rules: its own `heavy` wrapper and queue group, its merge lock, and
landings on that machine's local `main` of skein and smith, without a
push. That `main` is pushed to GitHub from time to time. Until the plan
ends:

- **Every machine may land in skein,** designs, plans and the kit
  included, and runs on other machines may change it (the user's
  decision, 2026-10-12: the reliability plan touches it little).
  `debian-16gb-hel1-1` merges GitHub's `main` when it next pushes, and
  resolves what conflicts.
- **Other machines do not land in smith,** designs and plans included: a
  landing there would split GitHub's `main` from that machine's. temper
  and jig are free. The exception is smith's move to GitHub's skein
  (2026-10-12, from `carbon-forge`, the user's decision): smith names
  skein by its GitHub URL, at GitHub's `main`, and its hosts state their
  policy deadlines. `debian-16gb-hel1-1` merges it when it next pushes.
- **On `debian-16gb-hel1-1`:**
  - no other run starts: `plan check` fails on purpose, since smith's
    primary checkout stays on `bench/smith-codex-2026-10-08`, as the
    plan's sessions expect;
  - in skein and smith, land with `land --local` under the plan's merge
    lock, `LAND_LOCK=~/src/rust/<repository>/.git/reliability-merge.lock`,
    then push `main` under the same lock: `flock <lock> git -C
    ~/src/rust/<repository> push origin main refs/notes/commits`;
  - a gate-lane task pauses the plan's queue group too while it runs: keep
    them few.
- **smith and temper have no `scripts/check.sh` yet.** A code change there
  gives `land` its workflow's section 1 as the gate: `land --gate '<the
  four commands, joined with &&>'`.
- **jig's and temper's Cargo git URLs still name `git.ekanayaka.io`;**
  smith's name GitHub (2026-10-12). Cargo treats two URLs of one
  repository as two sources, so a repository moves its URLs when it takes
  a smith or jig that names GitHub: jig when it takes smith's latest
  `main`, whose host protocol, charter and transcript changed under jig's
  smith-facing crates; temper after jig. Until then every machine
  rewrites the old URLs to GitHub, and cargo fetches with git:
  - `git config --global url.git@github.com:forgetic/.insteadOf
    https://git.ekanayaka.io/ai/`
  - `[net] git-fetch-with-cli = true` in `~/.cargo/config.toml`.

When the plan ends:
- jig's and temper's Cargo git URLs move to GitHub, if they have not
  already;
- smith's primary checkout returns to `main`, and every landing pushes;
- smith and temper get `scripts/check.sh`;
- the push rules in the repositories' `AGENTS.md` become "land pushes";
- the plan moves to `smith/docs/development/plans/reliability/`.

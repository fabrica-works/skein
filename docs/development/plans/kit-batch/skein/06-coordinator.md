# Session 06: the coordinator's fixed lines, its thread's settings, `plan context`

2026-10-17. What the coordinator types by hand, the kit types. In
protocol, 34 commands set a step `landed` in the command that checked it,
before the checks' output was read, and the coordinator wrote its
headings by hand, a reboot as "the user's request" (protocol 6, 8). The
profile's `model_auto_compact_token_limit` never reached the thread: the
coordinator compacted at 349K to 395K (protocol 4; findings/inventory.md,
3). It had the code graph's server it never used (protocol 5). The
planner and designer have no command for their own context (skein-ui 9).
And `test-plan --live` does not check the 300,000 ms cell the
implementer's card prescribes (protocol 9).

## Read first

- skein `docs/foundation/development.md`, sections 2.1, 2.2, 3.6, 7.3
  (steps 2 to 4), 7.6 (its resume), 10.3 (when the coordinator finds
  `unblock.md`, `steer.md`), 15.2 (`restarts`); implementer.md 8.3.
- `decisions.md`, 7, 8, 16, 18; `findings/inventory.md`, section 3.
- `code-organization.md`, sections 2 (log.md's headings), 3, 4.
- skein `testing-strategy.md`, section 10.

## What the design asks

- **7.3:** a step is `landed` only once its three checks have passed.
- **7.3, 7.6, 10.3:** the log's headings, each in one shape, which
  `plan metrics` reads (15.2).
- **7.6:** a resume's cause is known: reboot, crash, the user's request,
  unknown.
- **2.3, 7:** the coordinator runs with its profile's settings, and no
  graph server.
- **2.1, 2.2, 10.5:** each Claude role knows its context and when to hand
  over.

## Choices this plan makes

- `plan landed`, `plan unblocked`, `plan steered`, `plan handover
  REPOSITORY NN CONTEXT` and `plan resume [CAUSE]` are decisions.md 7's;
  the cause rule is 8's. `unblocked`, `steered` and `resume` write their
  headings through `log_entry` (04.1); `plan handover REPOSITORY NN
  CONTEXT` appends 7.3 step 4's plain line, `Handover, <time>: …`, which
  `plan metrics` counts (`^Handover, `, numbers.sh:753); `plan landed`
  writes none.
- `start_coordinator` passes the profile's keys but `model` as dotted
  `config` keys, as `-c` takes them; the environment's `PLAN_CODEX_*`
  still win (decisions.md, 18).
- `plan context` reads what `supervisor_context` reads, for any session
  (decisions.md, 16).

## Rules that bind it

- **The gate** (README, Rules).
- **No real daemon in the process layer** (code-organization.md, 4):
  06.3 and 06.4 test against the fake Codex of 04.1.

## Increments

### 06.1 `plan landed STEP COMMIT`
Scope: protocol 6 (the checks); decisions.md 7; 7.3 steps 2 and 3.
Needs: skein 01.2 (`set_step` in `plan` after the move).
Files: `scripts/dev/plan` (beside `set_step`; its usage, the header
  comment from line 3, and the `case $cmd` dispatch, plan:534 at
  817b15c); `scripts/dev/test-plan`; development.md 3.6, 7.3.
Shape: from the step's ledger row (repository, session, branch), in
  `$run/wt/<repository>-<session>` after a fetch: `merge-base
  --is-ancestor COMMIT origin/main`; COMMIT's `Step` trailer is STEP;
  when the worktree's branch is the row's, `status --porcelain` is empty.
  Each prints `ok` or `FAIL` and its reason. All passed: `set_step STEP
  state=landed commit=COMMIT` and `landed STEP COMMIT`; else exit 1, the
  ledger as it was. A worktree that is gone skips the third check, saying
  so. 7.3's steps 2 and 3 become one command; `plan set state=landed`
  stays for the supervisor's corrections.
Test shape: test-plan, a block after "decisions made" (:312 after
  01.1), on a copy of the ledger put back at its end. The fixture's alpha
  has no origin: the block gives it one, `$t/alpha.git` (`git clone -q
  --bare "$repo"`, `git -C "$repo" remote add origin`), and removes the
  remote at its end; its commits are made in `$run/wt/alpha-01`, a
  worktree on branch b4, so that alpha's main, which the later checks
  read, is unchanged. A row `alpha-01.4` (repo alpha, session 01,
  running, branch b4). The refusals first, each exiting 1 with the ledger
  as it was: "landed refuses a commit off main" (not pushed); "landed
  refuses another step's commit" (`Step: alpha-01.3`, pushed to origin's
  main); "landed refuses a dirty worktree" (a file changed in it). Then
  "landed sets a checked step" (`Step: alpha-01.4`, pushed: state
  `landed`, its commit set).
Proof: process, those checks, under 3 s.
Don't: read the step's diff or tests; set any state but `landed`; leave
  the origin or the worktree for the checks after the block.
Size: about 40 lines.
Removes: 34 premature `landed` states' cause.

### 06.2 `plan context [SESSION]`
Scope: skein-ui 9; decisions.md 16; 2.1, 2.2.
Needs: skein 01.2 (`plan-roles.sh`; `supervisor_context` in `plan-watch.sh`).
Files: `scripts/dev/plan` (usage, dispatch); `scripts/dev/plan-roles.sh`;
  `scripts/dev/plan-watch.sh`
  (`supervisor_context`, which calls it); `scripts/dev/test-plan`;
  development.md 2.1, 2.2, 3.6.
Shape: `plan context [SESSION]`, SESSION by default
  `$CLAUDE_CODE_SESSION_ID`: `context: <N>K (compactions: C)`, and at
  `PLAN_COMPACTIONS` `: hand over now`. `supervisor_context` prints it
  for the supervisor's session, with its words as today. 2.1 and 2.2: the
  planner and the designer run it after each sub-agent's report.
  usage gains its line (01.2's usage prints the header to its first
  `# shellcheck` line, so no range changes).
Test shape: "context of a session" on the fixture's `$session` (2
  compactions): `hand over now`; with none given and
  `CLAUDE_CODE_SESSION_ID` empty: exit 2 (`die`), `name a session`.
Proof: process, that check, under 1 s.
Don't: change `plan contexts`.
Size: about 20 lines.
Removes: nothing.

### 06.3 The coordinator's headings: unblocked, steered, handover, resume
Scope: protocol 6 (headings), protocol 8 (the cause); decisions.md 7, 8;
  7.3 step 4, 7.6, 10.3.
Needs: skein 04.2 (04.1's `log_entry`, `coordinator_turn` and fake Codex;
  04.2's `.watch` line in `handover` and `resume`).
Files: `scripts/dev/plan` (the usage and the dispatch: `unblocked`,
  `steered`, `handover` with three arguments); `scripts/dev/plan-roles.sh`
  (`handover`, `resume`); `scripts/dev/test-plan`; development.md 3.6, 7.3, 7.6, 10.3;
  `coordinator_resume_prompt`'s text.
Shape: `plan unblocked` reads each of unblock.md's `Answer to the
  blocked report of <time> (<headings answered>).` lines (its first line
  is `# Unblock`; 10.3; `plan ruling` appends a second draft to an
  unblock.md still there, plan:436–438 at 817b15c), logs `Blocked and
  unblocked, <time>` with blocked.md and unblock.md, whole, as 10.3's
  step 1 says (one body file, the two concatenated into a temporary
  file: `log_entry HEADING [BODY_FILE]` takes one); removes from
  blocked.md each `## ` entry whose heading text occurs in one of those
  lines (a substring
  match, no split: a heading may hold commas, so no delimiter is safe),
  and the file when none is left; deletes unblock.md. A line that
  matches no entry is printed and removes none. 10.3 words the
  parentheses so: the headings answered, each written whole, as
  blocked.md has it without `## `, separated by "; ". `plan steered`
  logs `Steered, <time>` with steer.md, and deletes it. `plan handover
  REPOSITORY NN CONTEXT` appends the plain line `Handover, <time>:
  REPOSITORY NN at CONTEXT` to log.md, not through `log_entry`;
  `handover supervisor` is as before. `plan resume [CAUSE]` takes CAUSE,
  else `reboot` when the heartbeat exists and `uptime -s` is after its
  mtime, else `the user's request` when paused.md is there, else
  `unknown`; a missing heartbeat is no error. It logs `Coordinator
  resumed, <time>: <cause>` itself; reaches the thread through
  `coordinator_turn resume "" "<the resume prompt>"`, so it loads as a
  ring does; its own `thread/read` test and comment (plan-roles.sh:245–257
  at 817b15c), which took a read's success for a loaded thread, go. The resume
  prompt no longer asks the coordinator to write the heading. Under
  `PLAN_CODEX_FAKE`, `resume` neither checks nor starts pueued or the
  real daemon.
Test shape: "unblocked files the answer" (blocked.md with three
  entries, two named in the `Answer to` line, separated by "; ", one of
  them with a comma in its heading: log.md has both files under the
  heading, blocked.md keeps the third and only the third); "steered
  files steer.md"; "handover logs a session's line"; "resume names a
  reboot" (heartbeat `touch -d` before `uptime -s`) and "resume names
  the user's request" (paused.md), each with `PLAN_DRY_RUN=1` (no
  supervisor is started) and the fake: its `turn/start` text and
  log.md's heading; "resume takes a given cause" (`PLAN_DRY_RUN=1
  "$plan" resume crash`, the reboot's heartbeat still in place: log.md
  has `## Coordinator resumed, <time>: crash`); "resume with no
  heartbeat" (`unknown`, exit 0). `PLAN_UPTIME` is not added: the
  heartbeat is set before the boot.
Proof: process, those checks, under 3 s; live, one `test-plan --live` run
  (about $0.05 and 90 s): the live block's resume (its `expect resume`,
  test-plan:471 at 817b15c) gains a check that log.md has `## Coordinator resumed, <time>:
  unknown`.
Don't: parse unblock.md beyond its `Answer to` lines; split such a line
  on any delimiter.
Size: about 70 lines.
Removes: the coordinator's hand-written headings (7.3, 7.6, 10.3).

### 06.4 The coordinator's thread takes the profile
Scope: protocol 4, protocol 5, parity 14 (never a sub-agent);
  decisions.md 18; findings/inventory.md 3.
Needs: 06.3 (`resume` through `coordinator_turn`, which the test runs
  under the fake).
Files: `scripts/dev/plan-roles.sh` (`start_coordinator`, and `profile_config`
  beside it; `profile_value` and the settings it reads, plan:68–74 at 817b15c, stay);
  `scripts/dev/workspace/coordinator.config.toml`;
  `scripts/dev/workspace/AGENTS.md`; `scripts/dev/test-plan`;
  development.md 2.3, 3.5 (the `thread`, `threads` row), 6.
Shape: `profile_config` prints the profile as one JSON object of dotted
  keys (`[agents] max_threads = 8` → `"agents.max_threads": 8`), each
  value TOML's own (quoted strings, numbers, booleans), `model` left out;
  `start_coordinator` merges `PLAN_CODEX_EFFORT` and the threads over it
  and sends it as `config`. The profile gains
  `[mcp_servers.codebase-memory-mcp]` `enabled = false`. The workspace
  AGENTS.md says that implementers call none of `list_projects`,
  `index_status`, `check_index_coverage`, `index_repository`, and that a
  design or plan session is started by the user, never as a sub-agent.
Test shape: `PLAN_DRY_RUN=1 PLAN_CODEX_FAKE=$t/codex-fake plan resume`
  with `$run/thread` moved aside, and it and `$run/threads` (which
  `start_coordinator` appends to, plan-roles.sh:138 at 817b15c, and
  numbers.sh reads) put back after (it starts the
  coordinator; the dry run starts no Claude session): the fake's `started` params have
  `model_auto_compact_token_limit` 200000 and
  `mcp_servers.codebase-memory-mcp.enabled` false ("the thread takes the
  profile").
Proof: process, that check, under 2 s; live, one `test-plan --live` run
  (about $0.05 and 90 s): its start's daemon accepts those keys, and the
  thread's rollout lists no codebase-memory tool.
Don't: send `model` twice; read the profile with anything but awk and jq.
Size: about 30 lines.
Removes: `config`'s two hand-written keys.

### 06.5 The live layer checks the 300,000 ms cell
Scope: protocol 9; implementer.md 8.3.
Needs: 06.4 (the live block's start with the profile's keys, which this
  check follows).
Files: `scripts/dev/test-plan` (the live block).
Shape: implementer.md 8.3 needs no change; the check proves its sentence
  "On Codex 0.162, that is `wait` on the command's cell with
  `yield_time_ms: 300000`, which returns as soon as the command ends".
  The live coordinator's prompt runs `sleep 70` in one cell and waits on
  it with `wait`, `yield_time_ms: 300000`; the check reads the rollout:
  one `wait` for that cell, with that yield, whose output came when the
  command ended (no second `wait`), and the turn took at least 70 s. The
  rollout `find` (test-plan:469 at 817b15c) gets `2>/dev/null`: it
  printed `find: …/archived_sessions: No such file or directory` where
  Codex had archived nothing (findings/base-runs.md, 4).
Test shape: the live block's "the cell waits 300 s" after "the
  coordinator heard the doorbell".
Proof: live, one `test-plan --live` run, about $0.06 and 110 s: the check
  passes on this machine's Codex, its version in the report.
Don't: lengthen the live block by more than 40 s.
Size: about 10 lines.
Removes: an unchecked rule of the implementer's card.

## Interface sketches

```bash
# plan: the step's three checks of development.md 7.3, then landed.
plan landed STEP COMMIT
# plan-roles.sh: the profile, as Codex's dotted config keys.
profile_config   # {"model_reasoning_effort": "high", "agents.max_threads": 8, …}
plan context [SESSION]
```

## Questions to raise when reached

None.

## Done when

06.1 to 06.5 have landed; the coordinator of the next `plan start`
compacts at about 200K and lists no graph tool.

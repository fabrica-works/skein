# Session 04: one way to the coordinator, and a watch that prints what each event needs

2026-10-17. The supervisor's side of the run. Codex's daemon unloads an
idle coordinator's thread; a ring then exits 3, and in smith's parity run
the supervisors loaded it by hand ten times (client-composition 4, parity
5). A coordinator waiting on queue tasks has no waker that outlives its
turn: about an hour of late wakes in parity. A new supervisor's first
watch shows only events not yet shown, so it misses what its predecessor
left open (client-composition 5). On client-api a supervisor event cost
about 20 tool calls at about 190K of context: the watch said what
happened, not what handling it needs (the director's candidate 7). And
the hourly check is a cron job the session must keep (parity 11), while
a pueued that died went unnoticed for five hours (parity 8).

## Read first

- skein `docs/foundation/development.md`, sections 3.5 (`idle.md`,
  `alerts.md`), 3.6 (`plan ring`, `plan idle`, `plan watch`), 7.4, 7.6,
  10.5; `docs/foundation/supervisor.md`, sections 1, 2, 5, 6, 7.
- `decisions.md`, 9, 14, 15, 22; `findings/base-runs.md`, section 5.
- `code-organization.md`, sections 2 (the watch's words, log.md's
  headings), 3, 4.
- skein `testing-strategy.md`, section 4 (a fake's scenarios against the
  real side) and 10.

## What the design asks

- **7.4, 10.5:** a ring reaches the coordinator whatever the daemon
  holds; an idle coordinator waiting on queue tasks is rung when they end.
- **10.5; supervisor.md 2:** the watch ends when there is something to
  do, and prints it: every open event, to a supervisor that has not seen
  them.
- **supervisor.md 5, 6:** the ready check's re-check and the hourly check
  are started by the watch, not by background loops or cron jobs.
- **11.1, 10.5:** the queue down is an alert, in `alerts.md`.

## Choices this plan makes

- One function, `coordinator_turn`, for every send to the coordinator,
  loading the thread first when `thread/loaded/list` lacks it
  (decisions.md, 9); each load is a log.md entry.
- A fake Codex daemon, `scripts/dev/test-codex`, behind `PLAN_CODEX_FAKE`,
  for the process layer (decisions.md, 22). The live layer checks that the
  real daemon's `thread/loaded/list` answers as the fake's does.
- The watch prints each event's material itself (candidate 7): the block's
  text, the step's ledger row and increment, what moved since its check.
  It prints paths, not whole session files.
- `hourly:` is a watch event (decisions.md, 14); `alert: pueued is down`
  once an outage (15). The `pueue` the watch reads in test-plan is a stub.

## Rules that bind it

- **The gate** (README, Rules).
- **No real daemon in the process layer** (code-organization.md, 4, 5):
  `PLAN_CODEX_FAKE` in every test that sends to a thread; the watch's
  `pueue` is `$t/stub/pueue`.
- **The kit and its documents change together:** supervisor.md loses
  each step the kit now does.

## Increments

### 04.1 The fake Codex, `coordinator_turn` and the ring's loading
Scope: client-composition 4, parity 5 (the loading); protocol 8 (the
  ring's prefix); decisions.md 9, 22.
Needs: skein 01.2 (`ring` and `coordinator_state` in `plan-roles.sh`).
Files: `scripts/dev/codex.sh`; new `scripts/dev/test-codex`;
  `scripts/dev/plan-roles.sh` (`ring`, `coordinator_state`); `scripts/dev/plan`
  (`log_entry`, beside `log_report`); `scripts/dev/numbers.sh`
  (`record_check`'s allowed headings, :432); `scripts/dev/test-plan`;
  development.md 3.6, 7.4 (where the heading is written; 7.5's bullet on
  the shapes log.md takes, :1576–1579 at 817b15c, names 7.4 already),
  10.5 ("A quiet coordinator"); supervisor.md 2 (the `quiet:` row, "A
  thread not loaded").
Shape: with `PLAN_CODEX_FAKE=<dir>` set, codex.sh's `codex_open` succeeds
  unless `<dir>/down` exists, `codex_close` does nothing, and `codex_call
  METHOD JSON` runs `test-codex <dir> METHOD JSON`. test-codex appends
  `METHOD JSON` to `<dir>/calls`, and answers from `<dir>/loaded` (thread
  ids) and `<dir>/turns/<thread>` (its last turn's id and status):
  `thread/loaded/list` → `{data: [ids]}`; `thread/resume` adds the id;
  `thread/read` answers from `turns/` loaded or not, as the real daemon
  does from its rollout (client-composition's retrospective, 5; resume's
  comment in plan-roles.sh, :245–246 at 817b15c, says otherwise, and
  goes in 06.3); `turn/start` and `turn/steer` fail `thread not
  found` for a thread not loaded, and once for each line of
  `<dir>/refuse` naming the method; `thread/start` writes a new id to
  `loaded` and its params to `<dir>/started`; any other method answers
  `{}`. `log_entry HEADING
  [BODY_FILE]` appends `## HEADING` and the body to log.md under
  `log_report`'s lock. `coordinator_turn COMMAND STEER START` (plan-roles.sh):
  asks `thread/loaded/list`; loads a thread not in it with
  `thread/resume`, and logs `Coordinator loaded, <time>: by plan
  COMMAND`; reads the last turn; steers STEER into a turn in progress, or
  starts one with START, or with START empty returns 3; a send refused
  loads and sends once more. It prints `steered`, `started` or `working`
  (STEER empty and a turn in progress). `ring` calls it with STEER `From
  the supervisor, by plan ring: <message>` and START the wake prompt when
  `idle.md` is there, else empty, as today; it keeps today's lines,
  `rang the coordinator: <message>` and `woke the idle coordinator:
  <message>`, which test-plan's live block and supervisor.md 2 read. The
  heading `## Coordinator loaded, <time>: by <command>` is written in
  7.4, which 7.5's list of log.md's shapes already names (`Coordinator
  resumed` is in 7.6, :1591 at 817b15c), and joins `record_check`'s
  allowed headings (beside `Coordinator resumed`, numbers.sh:432), so
  that the pulse does not flag each load.
Test shape: test-plan, a fake state in `$t/codex-fake` with the run's
  thread: "ring loads an unloaded thread" (not in `loaded`, `idle.md`
  present: calls `thread/loaded/list`, `thread/resume`, `turn/start` in
  that order; log.md's `## Coordinator loaded, … by plan ring`); "ring
  steers a turn in progress" (its text begins `From the supervisor, by
  plan ring:`); "a refused steer loads and sends again" (`refuse` holds
  `turn/steer`: two `turn/steer` calls, one load); "ring with no turn
  says resume" (exit 3, as today). The pulse world (test-plan, :236–244
  at 817b15c) gains `## Coordinator loaded, 2026-10-11T12:07:00Z: by plan
  ring` in its log.md, after the `Unblock handled` printf (:243), and its
  record check (the Record expectation, :301) stays silent on it:
  `headings no shape names: 1`, still the unblock one.
Proof: process, those checks, under 3 s; live, one `test-plan --live` run
  (about $0.05 and 90 s): after its start, the real daemon's
  `thread/loaded/list` holds the run's thread, in the fake's answer shape.
  A test-plan run from a worktree with uncommitted changes fails at its
  `heavy_run --gate true` check (:203 at 817b15c): the gate lane refuses
  a dirty tree, so commit before running it.
Don't: test against the machine's daemon in the process layer; unload a
  thread anywhere.
Size: about 95 lines (test-codex aside).
Removes: supervisor.md 2's "A thread not loaded" and its command; ring's
  exit 3 for a thread not loaded.

### 04.2 A new supervisor's first watch prints every open event
Scope: client-composition 5; 10.5.
Needs: skein 01.2 (`watch_once` in `plan-watch.sh`; `handover` and
  `resume` in `plan-roles.sh`).
Files: `scripts/dev/plan-watch.sh` (`watch_once`); `scripts/dev/plan-roles.sh`
  (`handover`, `resume`, where they start a supervisor); `scripts/dev/test-plan`;
  supervisor.md 1 ("A successor"), 5 ("A check left open");
  development.md 3.5 (the `.watch` row, which names `plan handover`),
  10.5 (its last bullet).
Shape: `watch_once` gathers every open event not yet in `.watch` (done,
  paused, blocked, answer, check) and prints them all in one call, in that
  order; `quiet:` only when none is. A new supervisor session (`handover
  supervisor`, and `resume` when it starts one) empties `.watch`.
  `handover` writes `handed-over` first (its `printf` to it,
  plan-roles.sh:317 at 817b15c), then empties `.watch`: the old session's
  watch, still running, stops at its next poll once it is named there
  (`watch`'s poll loop, plan-watch.sh:28 at 817b15c); emptied first, a
  poll between could refill `.watch` with events the successor then
  misses.
Test shape: "the first watch prints every open event": blocked.md,
  answer.md and a step in `check` together, `.watch` empty: one watch
  prints `blocked:`, `answer:` and `check:`; the next, `timeout 2 "$plan"
  watch`, prints nothing and exits 124 (a watch with no event never
  returns; the `watch` helper, test-plan:340 at 817b15c, has a fixed 10 s
  for events and passes no `"$@"`, so a quiet watch calls `timeout`
  itself, as "watch idle", :362, does with 3). Then `handover
  supervisor` under `PLAN_DRY_RUN=1` empties `.watch`.
Proof: process, those checks, under 2 s.
Don't: show an event twice to one session.
Size: about 25 lines.
Removes: events lost at a handover.

### 04.3 `plan idle --tasks`: the watch rings when the queue tasks end
Scope: parity 5 (its waits); 7.4.
Needs: 04.1 (`coordinator_turn`, the fake Codex).
Files: `scripts/dev/plan-watch.sh` (`idle`, `watch_once`);
  `scripts/dev/plan` (its usage header, :15 at 817b15c, reads `plan idle
  REASON`: it needs `[--tasks ID…]`); `scripts/dev/test-plan`;
  development.md 3.5 (`idle.md`), 3.6, 7.4 (the idle bullet, :1541–1543
  at 817b15c, tells the coordinator to run `plan idle "<what you wait
  for>"`: it must name `--tasks`; the doorbell bullet, :1551–1560 at
  817b15c, handles a ring by its prefix: it gains a bullet: on `From the
  supervisor, by plan watch: the queue tasks you wait on have ended:
  <ids>`, read those tasks' results and act on them as on a task you saw
  end).
Shape: `plan idle REASON [--tasks ID…]` writes the ids as idle.md's third
  line, `tasks: <ids>`. `watch_once`, when idle.md has tasks and `pueue
  status --json` shows none of them queued, running or paused (each
  task's `.status | keys[0]`; a task gone from the list has ended), rings
  through `coordinator_turn watch` with STEER `From the supervisor, by plan
  watch: the queue tasks you wait on have ended: <ids>` (decisions.md 9's
  prefix, which 7.4's new bullet matches, so it is not logged as `Rang`)
  and START the same message in `coordinator_wake_prompt`, the wake prompt
  `ring` uses,
  removes idle.md as `ring` does, and goes on watching: the supervisor is
  not woken.
Test shape: `$t/stub/pueue` (this increment makes it): `status --json`
  prints `$t/pueue.json`, else `{"tasks":{}}`; exits 1 when `$t/pueue.down`
  exists, or for any other subcommand. The `watch` helper (test-plan:340
  at 817b15c; it passes no `"$@"`) puts `$t/stub` first on PATH. "the
  watch rings when the tasks end": `plan idle "the gate" --tasks 12` with
  task 12 Running: no turn/start in the fake's calls after `timeout 2
  "$plan" watch` (exit 124); Done: one, whose text begins `From the
  supervisor, by plan watch: the queue tasks you wait on have ended: 12`,
  idle.md gone, and that watch, under `timeout 2` too, prints no event.
Proof: process, that check, under 3 s.
Don't: wake the supervisor for it; ring twice for one idle.md.
Size: about 30 lines.
Removes: the coordinator's wakers that die with its turn.

### 04.4 Each event prints what handling it needs (candidate 7)
Scope: the director's candidate 7; 10.5; supervisor.md 2, 5.
Needs: 04.2 (`watch_once` gathering every open event).
Files: `scripts/dev/plan-watch.sh`; `scripts/dev/test-plan`;
  supervisor.md 2 (the table), 5 (step 4); development.md 10.5 (the
  list of events that end the watch, from :2001 at 817b15c, gains
  `started:`: a step
  whose ready check named commits has started; the supervisor reads the
  `git diff --stat` the watch printed per repository, greps the session
  file's names in the files that moved, and re-checks only a moved file
  that a sketch or a suite of the check reads, as supervisor.md 5, step 4,
  says).
Shape: under each event, indented two spaces: `blocked:` each new
  entry's text, and for the step its heading names, its ledger row, its
  session file and the `### <NN.M>` section of it, and its last report's
  path (`reports/`); `answer:` answer.md's text; `check:` each step's
  ledger row and session file; `started:` (new) a step gone from `ready` to
  `running` whose ready note names `checked <repository> <commit>`: per
  repository, `git diff --stat <commit> origin/main` after a fetch;
  `quiet:` `coordinator_state` and log.md's last 20 lines; `paused:` and
  `done:` their files' text. An event's lines stop at 150, with `… (N
  more lines: <file>)`.
Test shape: the watch helper and checks of test-plan (:340–355 at
  817b15c) gain each event's material: "blocked prints the increment"
  (an entry `## alpha 01.2: a question`: the `### 01.2` section of the
  demo's 01-first.md, whose increments are `### 1.1` and `### 01.2`, and
  alpha-01.2's ledger row); "answer prints its text"; "started prints
  what moved" (a ready note `checked alpha <commit>`, a commit after it
  on origin: `started: alpha-01.2` and the moved file).
Proof: process, those checks, under 3 s.
Don't: print a whole session file.
Size: about 70 lines.
Removes: supervisor.md 5 step 4's background wait loop; the reads each
  event cost by hand.

### 04.5 `hourly:` and `alert: pueued is down`
Scope: parity 11, parity 8 (the alert); decisions.md 14, 15.
Needs: 04.3 (`$t/stub/pueue`, the watch helper's `PATH`).
Files: `scripts/dev/plan-watch.sh` (`watch_once`, `handed_over`'s
  line, :110 at 817b15c); `scripts/dev/plan-roles.sh`
  (`supervisor_resume_prompt`, `handover`'s comment and last line,
  :306–320 at 817b15c); `scripts/dev/test-plan`;
  supervisor.md 1 (step 3, and "After a compaction", :66 and :84; the
  steps after 3 renumber), 2, 6 (its opening, :298 at 817b15c: the check
  appends its `## Check, <time>` heading to supervision.md first, before
  its sub-agent, a quiet hour's too, and step 2 appends the entry under
  it), 7 (:355 at 817b15c), 8 (:393 at 817b15c); development.md 2.5
  (:247), 6 (the hourly check's bullet, :1323–1325 at 817b15c), 10.5
  (:2063 checks the job, :2084 deletes it, at 817b15c): every line `grep
  -n cron` finds that schedules, checks or deletes the job (10.5's :2045
  and :2053–2054 are the 2026-10-17 evidence, and stay); 10.5's
  alerts.md rule (:2035 at 817b15c); development.md 7.6 (:1615 at
  817b15c: "which starts its watch and its hourly check again" says that
  the watch, restarted, prints `hourly:` when a check is due); 14.1
  (:2325–2327 at 817b15c: the next check is due 65 minutes after the last
  `## Check` heading in supervision.md, and the watch prints `hourly:`
  then, in place of "an hour after"); 10.5's list of the events that end the
  watch (from :2001 at 817b15c), which gains `hourly:` (a check is due:
  the supervisor runs supervisor.md 6's) and `alert:` (a daemon is down:
  the supervisor handles it as supervisor.md 2's "An alert to the user"
  and `alert:` row say, then starts the watch again).
Shape: `hourly:` when supervision.md's newest `## Check, <time>` heading,
  or its first `## Process, <time>: skein <commit>` when it has none (its
  time's trailing colon stripped), is over 65 minutes old (3,900 s,
  fixed: no setting); once for that heading. supervisor.md's check
  appends its `## Check, <time>` heading first, before its sub-agent, as
  the cron job was scheduled first (decisions.md 14), so a check that
  stops halfway leaves the next one due, 65 minutes after that heading.
  `alert: pueued is down` when `pueue status --json` fails (the stub
  answers only that), once an outage, appending `<UTC time> pueued is
  down` to alerts.md; the outage ends when it answers. supervisor.md 2
  gains both rows (hourly: section 6; alert: `systemctl --user start
  pueued`, then a push notification if it stays down); 1 loses step 3,
  6 its cron scheduling.
Test shape: "hourly after 65 minutes" (a bare `## Check, ` heading, as a
  check that stopped halfway leaves it, 66 minutes old: `hourly:`; the
  next watch, under `timeout 2`, none); "hourly from the first process
  entry"; "the queue down alerts once" (`$t/pueue.down`: one `alert:` and
  one alerts.md line over two watches; removed, then again: a second).
Proof: process, those checks, under 3 s.
Don't: start pueued from the watch; keep a step that schedules, checks
  or deletes a cron job (evidence that names one, as 10.5's test of
  2026-10-17, stays; so does 6's "no watch, loop or cron job" for the
  designer).
Size: about 40 lines.
Removes: the cron job of supervisor.md 1, 6 and 7.

## Interface sketches

```bash
# plan-roles.sh: every send to the coordinator. Loads the thread when the
# daemon does not hold it, and logs the load; steers STEER into a turn in
# progress, else starts one with START; START empty and no turn: returns 3.
coordinator_turn COMMAND STEER START   # prints steered|started|working
# plan: one log.md entry, under log_report's lock.
log_entry HEADING [BODY_FILE]
# test-codex: the fake daemon, one call.
test-codex STATE_DIR METHOD JSON
```

## Questions to raise when reached

None.

## Done when

04.1 to 04.5 have landed; supervisor.md has no cron job and no hand
load; `test-plan --live` shows the real daemon's loaded list holds the
run's thread.

# Retrospective: kit-batch

2026-10-18. The run of skein's `kit-batch` plan on debian-16gb-hel1-1.
It was claimed at 2026-10-17T22:16:04Z. A fence held it until 23:46,
and its first sessions started at 23:47:12. The coordinator released it
at 01:59:49 on 10-18. The supervisor ran the "Done when" commands from
02:00:48 to 02:04:50, and marked the plan done at 02:05:10 (`b11ce40`).
The plan put into the kit every change that six retrospectives had
adopted and that was still waiting (its README, Inputs). This
retrospective was written by a fresh session that took no part in the
run (development.md, section 15).

Ten read-only sub-agents, on Sonnet, one per source, read:
- the plan, `decisions.md` (rulings 101 to 108), `later.md`,
  `findings/`, the session files and the plan's git history;
- the ledger, `log.md`, the 33 reports, `sessions/`, `context.md` and
  the run's small files;
- `supervision.md`, `done.md`, and the supervisor's transcript with its
  ten sub-agents;
- the coordinator's Codex rollout;
- the eleven implementer rollouts, in two groups;
- skein's history from `6926c2d` to `b11ce40`, with its trailers, and
  the process documents at `6926c2d` and at `main`;
- the kit at `c611973`, for its debts;
- `logs/heavy.tsv`, the 134 task logs and pueue's record;
- the wall-clock and forgejo-17 retrospectives, sections 1 and 5 of the
  eight before them, and the metrics history.

This session read:
- development.md's sections 3.5, 3.6, 7.2, 7.4, 7.6 and 15, and
  supervisor.md's section 5 at `6926c2d`;
- the plan's README, `later.md`, and the "For the user" section of
  `decisions.md`;
- `done.md`, `status.md`, `supervision.md`, `context.md` and
  `claude.tsv`;
- the kit's ring and load (`plan-roles.sh`), and the metrics' code for
  `lines`, `proofs` and the sessions they count;
- the supervisor's transcript and its sub-agents, priced by model;
- the wall-clock retrospective's shape.

Not read: the planner's session (`76d15852`, the plan's
`Planner-Session:`) and the design's (`00d6ad9d`, the
`Designer-Session:` of `b30cc95`). Neither transcript is on this
machine.

Times are UTC. A time without a date is on the day last named: 10-17
until midnight, then 10-18. Costs are at the list prices of each
message's model, as `plan metrics` prices them since 05.2. The earlier
runs' costs priced every token at Opus's rates (section 3). This
document proposes; the user, or the director, decides, and the designer
applies what is adopted.

**Which process.**
- The run's process and kit were skein `6926c2d`, the plan's own
  landing, from the claim to the release. `kit/` never moved (ruling
  106). `b30cc95`, wall-clock's text decisions, is in it.
- Every change to the process that landed during the run was the
  run's own: 24 of its 29 steps changed development.md, six changed
  supervisor.md and two changed implementer.md. None reached the run,
  as development.md 3.6 intends: a kit change reaches the runs claimed
  after it. No other process change landed. hearth's render landed two
  Rust commits in skein, `05aa3c4` and `0b08745`.
- So the run worked without what its own first steps landed:
  - `land`'s landing turn (01.1, 23:49);
  - `plan landed` (06.1, 00:22);
  - the ring that loads the coordinator (04.1, 01:03);
  - done.md's `Recorded` line (01.1).

  Section 5, item 1, counts what that cost.
- Section 7 judges what was in force.

## 1. In one page

- **Done: 29 steps in 2 h 13 min of work, after 1 h 31 min held by a
  fence.**
  - 25 planned steps and four added ones landed; none was dropped.
  - "Done when" 2 to 9 pass on `main`.
  - Item 1 fails as written: `plan status` needs a claimed run, and
    the coordinator releases the run before the supervisor checks
    (section 2).
- **The cheapest run on record per step, and the fastest.**
  - `step_h` 0.08 and `tokens_per_step` 5.38M are the lowest of eleven
    runs. The next lowest are 0.09 (test-budgets) and 7.78M (skein-ui).
  - The steps were small, well-specified shell steps, with five
    sessions at once.
  - The coordinator recorded a landing a median 13.5 s after its
    report.
  - Three blocks were answered in a median 1.2 minutes.
- **Half the gate lane was thrown away.**
  - There were 61 gates for 28 gated landings. 30 passed and were
    discarded because `main` moved before their push. They took 50% of
    the gate lane's running time and 73% of its waiting.
  - 04.1, at the head of the longest chain, took 13 gates and 43 minutes
    to land.
  - The cause: the claim kit's `land` has no landing turn. 01.1 landed
    one at 23:49, but the run kept `6926c2d`'s kit.
  - A flock, sent by steer at 01:29, made it one gate a landing
    (proposal 1).
- **test-plan's time rose from 13 s at the base to 57.7 s, against the
  plan's 40 s.** test-plan is part of every step's gate.
  - Each step added checks, at about 1.1 s for each `plan` call.
  - The plan's Proof bounds summed to 45 s against the 25 s it allowed
    them.
  - A steer at 00:28 asked every report for its times, and they rose
    another 20 s.
  - An added step, 01.s2, found the causes in 3.5 minutes and cut the
    time to 37.2 s in 11 minutes, keeping every check (proposals 2 and
    4).
- **The fence took 1 h 31 min of the run's 3 h 44 min.**
  - The plan held every session while temper's client-api was active.
  - The director lifted it at 23:45, once client-api's kit was pinned
    (temper's ruling 295, at 23:43): a change of minutes that could have
    been made before the claim.
  - The fence was one of the plan's eight "For the user" items. Seven
    of the others still have no answer (proposal 3).
- **test-plan reached outside its world.**
  - From 01.1's gate at 23:49 until 01.s1 landed at 00:06, each run
    relinked the machine's `~/.local/bin/{heavy,land,plan,test-plan}`
    into scratch worktrees.
  - Each run also left its world's daemons behind: 11 Codex daemon
    pairs, 1.8 GB.
  - The supervisor's ready checks found this. It relinked the tools by
    hand at 00:01:54, killed the daemons at 00:27, and ruled 01.s1,
    which fixed both.
- **The ready checks paid their way, but held sessions 03 to 06 for 24
  minutes after 01.2 landed.**
  - Four Sonnet checks, costing $8.13, made 31 corrections. Eight would
    have failed or misled a session.
  - They also found the relink above.
  - 06's check, the fastest, was corrected by 00:03 and then waited 11
    minutes for one combined ruling (proposal 6).
- **Cheap supervision.**
  - One session, one compaction, no handover.
  - Hourly checks ran 60 to 62 minutes apart, from a chain of one-time
    cron jobs.
  - It cost $22.76, of which its Sonnet sub-agents were $14.40.
  - The planner's and the designer's costs are missing: their
    transcripts are not on this machine (proposal 8).
- **Nine proposals** (section 8):
  - a kit plan's run takes the kit fixes it needs;
  - test-plan fails past its budget;
  - "For the user" is answered before the start;
  - a suite that every session grows gets a planned trim;
  - a move several sessions may need becomes one step;
  - one ruling per ready check;
  - `plan status` on a released run;
  - the metrics' misses;
  - the graph project's name, by rule and not one machine's.

## 2. Outcome

The plan's "Done when", as the supervisor's sub-agent ran it at 02:00:48
to 02:04:36 on `origin/main` `c611973`, each command once (done.md):

| Item | Result | Evidence |
|---|---|---|
| 1. Every step landed or dropped: `plan status` lists none waiting, ready, running or blocked | **Fails as written; holds** | `plan status` printed "plan: no active run on this machine" and exited 2. The coordinator released the run at 01:59:49, as development.md 7.6 orders, before the supervisor runs its checks. `status.md` (01:59:07) says "29 of 29 steps landed; 0 running, 0 blocked, 0 ready, 0 to check, 0 waiting". Proposal 7. |
| 2. The plan's commits list 01.1 to 06.5 | Pass | 29 steps: the 25 planned, with 01.s1, 01.s2, 04.s1 and 05.s1. |
| 3. The process layer passes on main | Pass | `test-plan: passed` in 38 s, against 13 s at the base and 57.7 s at its peak. |
| 4. The live layer passes on main, once | Pass | 146 s. It was the tenth live run, the one ruling 108 kept for this item. |
| 5. shellcheck finds no warning | Pass | |
| 6. No kit file is over 800 lines | Pass | The largest is numbers.sh, at 734 lines (section 9). |
| 7. The four new commands are in 3.6's table | Pass | `plan landed`, `context`, `unblocked` and `steered`. |
| 8. The cron job is gone | Pass | 04.5 moved the hourly check into the watch's `hourly:` event. |
| 9. Each model is priced; the context limit is 150,000 | Pass | 05.2 and 05.5. |
| 10. The aims are these items | — | |

The plan's aim holds: `findings/inventory.md` maps each adopted change to
one of the 25 increments, and all of them landed. What the plan left out
is in `later.md`'s eight rows. Section 9 adds the debts.

Every test-plan run, Done when 3's and 4's included, ends with pueue's
"Failed to cleanup pid during shutdown" for its world's daemon: harmless,
in every gate's tail since 02.3 (section 9).

## 3. Numbers

`plan metrics` ran without error, from the kit at `main`, which holds
this run's own 05.x changes, and wrote `metrics.tsv`. The comparisons:
- wall-clock, the last retrospective's run: smith, 20 steps, 29.1 h,
  most of it live suites;
- forgejo-17: temper, 11 steps, 2.4 h;
- test-budgets: jig, 39 steps, 3.7 h, the nearest in size and length.

| Metric | Printed | Notes | wall-clock | forgejo-17 | test-budgets |
|---|---|---|---|---|---|
| `wall_h`, `running_h` | 3.8, 2.2 | 3.73 h to the release. done.md has no `Recorded` line (the claim kit lacked it), so `wall_h` ends at the supervisor's append at 02:04:50. The fence took 1.52 h of it; the work, 23:47 to 01:59, took 2.21 h. | 29.1, 5.6 | 2.4, 2.0 | 3.7, 3.7 |
| `steps_planned`, `added`, `dropped` | 25, 4, 0 | 01.s1 (ruling 102); 05.s1 and 04.s1 (105); 01.s2 (107). | 20, 0, 1 | 9, 2, 0 | 36, 3, 0 |
| `step_h` | 0.08 | 4.6 minutes a landing during the work. The lowest of eleven runs. | 0.28 | 0.18 | 0.09 |
| `escalations` | plan defect 2, process 1 | 0.10 a step. 05.s1's unnamed step (105), `plan landed` missing from the claim kit (106), and 06.5's Size and live runs (108). | 12 | 3 | 24 |
| `escalation_wait_min` | 1.2 | 28 to 113 s from the watch's print to the landed ruling. | 2.3 | 10.5 | 1.8 |
| `blocked_h` | 0.1 | 4.4 minutes in all. Only 06.5's block held a session. | 1.0 | 0.6 | 0.6 |
| `answers_reversed` | 0 | | 0 | 0 | 3 |
| `steers` | 4 sent, 0 points refused | 7 points. One was logged "done" but not followed (section 5, item 6). | 10, 1 | 1, 0 | 4, 0 |
| `restarts` | loaded 1 | The hand load at 23:46:59 (supervision.md's "Loaded"). | none printed; 3 hand loads | | |
| `tokens_coordinator` | 22.4M in (98% cached), 51K out | 0.77M a step. No tokens in its four idle spells (1 h 46 min). | 82.6M; 4.13M a step | 16.6M; 1.51M | 71.8M; 1.84M |
| `tokens_implementers` | 68.0M in (98%), 228K out | 2.35M a step. | 143.4M; 7.17M | 51.1M; 4.65M | 201.9M; 5.18M |
| `tokens_claude` | 182.8M in, 1.60M out: 0 before the claim, 64.6M in the run, 118.2M after | 2.23M a step in the run. "0 before the claim" is a miss: the planner's and the design's transcripts are not on this machine. "After" is this retrospective's, up to its last draft. | 511.6M: 96.2M, 243.2M, 172.2M | 439.4M: 195.6M, 141.8M, 102.0M | 129.4M: 0, 62.7M, 66.7M |
| `cost_claude` | $54: supervisor $23, retrospective $31 | The supervisor's own Opus thread cost $8.36 and its Sonnet sub-agents $14.40. At the earlier runs' all-Opus rates the supervisor's cost is $37.16. The retrospective's thread cost $7.08 and its ten sub-agents $23.86. No planner or designer is counted. | $233 (supervisor $95) | $174 (supervisor $37) | not printed |
| `tokens_per_step` | 5.38M | The lowest of eleven runs. | 23.6M | 19.1M | 8.6M |
| `coordinator_requests` | 188, 3 after a wait ran out | 6.5 a step. The three timeouts cost 7 requests and 1.28M input. | 461, 26 | 162, 9 | 398, 9 |
| `compactions` | 0 | The coordinator peaked at 194K of its 475K window. | 1 | 0 | 1 |
| `gates_per_step` | 2.1 | 61 gates for 28 gated landings: 28 landed, 30 discarded after `main` moved, 3 failed. After the flock at 01:29: 1.0. | 1.2 | 1.4 | 3.1 |
| `gate_lane_other` | 1 | 02.4's traced baseline, sent as a gate. | 30 (37) | 14 | |
| `queue_wait_ratio` | 0.83 | The gate lane 1.21, the build lane 0.32. After the flock: 0.00. | 0.97 (0.17) | 0.29 | 2.13 |
| `suites` | gate 23 s first, 46 s last | The gate (test-plan and shellcheck) peaked at 67 s, at 06.5. test-plan was 13 s at the base, 57.7 s at its peak and 37.2 s at the end. | | | |
| `proofs` | process 27, live 8, first 1; 0 below plan | 28 gated steps. "first" is a fault: 04.1's Proof has a semicolon inside a parenthesis ("101 s; first run found …"). | world 3, process 5, live 7 | process 2 | |
| `report_lines` | 11 median, 11 p90 | 33 reports; the longest, 01.2's, is 16 lines. | 11, 18 | 12, 15 | 11, 14 |
| `lines` | data +3418 −1904 | **Not comparable:** the shell kit counts as data. Corrected: shell kit +2,046 −1,313; test-plan files +1,080 −367; Markdown +292 −224. | | | |
| `contexts` | 113K median, 160K p90; 3 handovers | The p90 is the lowest of the history. Handovers came at 160K (05), 167K (04) and 156K (06); each session crossed 150K inside an increment. | 101K, 199K; 7 | 113K, 182K; 2 | 142K, 209K; 3 |
| `supervisor` | 1 session, 3.8 h, 0.7M weighted tokens an hour, 36% in the bootstrap, 1 compaction | Compacted at 166.8K, at 00:18. 175 requests over 19 turns, 22 minutes of which were active. | not printed | | |
| `pin_lag` | smith: skein 43; jig: skein 69, smith 193; temper: skein 64, smith 193, jig 13 | The plan repins nothing (its Repin: none). | | | |

**Not comparable, and why:**
- **`cost_claude`, `tokens_claude`.**
  - 05.2 prices each message at its model's rates. The earlier columns
    price Sonnet's tokens at Opus's: at those rates this run's
    supervisor would be $37.16, as forgejo-17's was.
  - This run counts no planning or design. The planner's session
    `76d15852` and the design's `00d6ad9d` are not on this machine, and
    the metric drops a session it cannot find without a word (proposal
    8). wall-clock counted $44 of planning and forgejo-17 $78.
- **`lines`.** 01.1 counts any file not Rust as data. A shell kit shows
  code +0 and tests +0. The earlier runs counted non-test paths as code.
- **`proofs`.** 05.3 counts each semicolon part, so 27 + 8 + 1 exceeds
  the 28 gated steps. "first" is 04.1's prose.
- **`wall_h`.** It ends at done.md's last change, the supervisor's
  append, about five minutes after the release. Main's kit reads the
  `Recorded` line, which this run's kit did not write.
- **`restarts`.** 05.4 counts a hand load as `loaded`. wall-clock's three
  hand loads were not counted.
- **`suites`.** These are test-plan's and shellcheck's seconds, not Rust
  test counts.
- **`queue_wait_ratio`, `gate_lane_other`.** These come from `heavy.tsv`
  only. `heavy` removes each pueue task, so 02.4's reading of pueue adds
  nothing here. They compare with the earlier runs' printed values, not
  with wall-clock's corrected 0.17.
- **`escalation_wait_min`, `blocked_h`.** 01.1 skips a "Since" written
  after its answer. That fixes the negative values of reliability and
  protocol, not this run's.

**What explains the largest differences:**
- **Tokens and hours per step.**
  - The steps were shell and Markdown: about 1,000 production lines in
    25 increments, against about 1,144 planned, with no compile cycle.
  - Sessions began from small contexts: most made their first edit
    within a minute.
  - The coordinator spent nothing while idle.
  - The run had no design question and no planner in its count.
- **Gates per step.** The claim kit's `land` takes no turn. With five
  sessions landing every few minutes and gates of 40 to 67 s, a gate
  that passed often found `main` moved. 04.1 ran 13 gates and 06.2 nine.
- **Queue waits.**
  - From 00:17 to 01:01 the gate lane was never idle for more than 10 s.
    From 00:30 to 01:00 it ran 1,799 of 1,800 seconds.
  - 986 of the build lane's 1,020 s of waiting were behind a gate,
    which pauses the build lane while it runs.

## 4. What worked

1. **The pace.**
   - 29 landings in 2 h 13 min, five sessions at once from 00:15 to
     00:46.
   - Each step reached `main` in a median 4.9 minutes from its
     commit. The two exceptions were 04.1 (47 minutes) and 06.2 (34),
     section 5, item 1.
   - 05's chain of five landed between 00:33 and 01:07.
   - Every increment landed within twice its Size, 06.5 once ruling 108
     corrected the count: about 1,000 production lines against about
     1,144 planned.
2. **The coordinator.**
   - 188 requests, no compaction, nothing spent in its four idle spells.
   - It verified each landing with git (`merge-base --is-ancestor`, the
     Step trailer, a clean worktree) and recorded it a median 13.5 s
     after the report.
   - It took each ring in 3 to 8 s and logged all seven steer points
     done within 9 to 39 s.
   - It handed three sessions over between increments at the card's
     150K, and resumed a yielded session 4 to 6 s after its dependency
     landed.
   - No session waited for a slot.
3. **Handovers from checkpoints.**
   - w9 made its first edit 70 s after its start, at 58K, and landed
     04.5 in 5 minutes.
   - w10 landed 06.5 in 2.2 minutes at 40K, from the checkpoint, the
     session file and the "Don't" list.
4. **The blocks.**
   - Three blocks, answered 28 to 113 s after the watch printed them.
     None was reversed, and none needed the designer.
   - Rulings 105 and 106 together took about four minutes, from 05.s1's
     report at 01:15:42 to its recorded landing at 01:19:46.
5. **The ready checks.**
   - Four checks, from 23:48 to 00:06, with three sub-agents applying
     their corrections.
   - They made 31 corrections. Eight would have failed or misled a
     session:
     - 03.4's `plan downstream` aborted by `pipefail` before it printed;
     - 03.3's boot unit with a relative `ExecStart`;
     - 05.1's `metrics.sh` failing the gate's shellcheck;
     - 05.2's `jsonl` unable to take `--arg`;
     - 05.4's fixture without `response_id`;
     - 06.3 reading one "Answer to" line of several;
     - 06.4 restoring `thread` but not `threads`;
     - 04.3's Files without `plan idle --tasks`.
   - Their worlds also showed the relink (section 5, item 4), ten
     minutes after it began.
6. **The flock.** Ruling 107's steer at 01:29:48 put every code landing
   under `flock` on the run's `land.lock`. Before it, 52 gates ran for
   19 landings, from 00:17 to 01:30. After it, 4 gates for 4 landings,
   with 1 s of waiting. It is what `main`'s `land` does now.
7. **01.s2, the trim.**
   - A traced run with `BASH_XTRACEFD=9 PS4='+ $EPOCHREALTIME '`,
     rolled up by file and line, found the causes in 3.5 minutes:
     - 23 `pueue status --json` calls;
     - readiness polls;
     - 316 `sed|head` forks in `profile_value`;
     - the real `claude agents` called 10 times;
     - 113 `plan` calls at about 1.1 s each.
   - It cut test-plan from 57.7 s to 37.2 and 37.8 s, kept every check
     (146 to 147 `expect` lines), and merged the copied captures into
     helpers.
   - It took 11 of its 45 minutes and three traced runs, exactly its
     budget.
8. **The supervisor.**
   - One session of 3 h 50 min. It compacted once, at 166.8K, without
     losing its watch, its next check's cron job or a running
     sub-agent.
   - The hourly checks ran at 23:17, 00:17 and 01:18 (wall-clock's
     proposal 3).
   - All ten sub-agents ran on Sonnet (forgejo-17's 5).
   - It verified the director's word against temper's ruling 295 in git
     before lifting the fence.
   - It cost $22.76, or $37.16 at the earlier runs' all-Opus rates:
     $9.8 an hour, against wall-clock's $3.3 and forgejo-17's $15.4.
9. **Proofs.** Each of the 28 gated Proofs starts with its layer's word
   (forgejo-17's 8), and none falls below its plan's layer. The live
   layer passed on `main` at the end.
10. **Practices worth keeping.**
    - Byte-for-byte proofs of a pure move (01.2, 05.1, 05.s1).
    - Overlapping the baseline run with reading.
    - Reusing the previous increment's after-time as the next one's
      before-time.
    - Edit scripts kept under `logs/<session>/`.

## 5. What did not

1. **The run kept a kit without its own fixes.**
   - **Gates thrown away.** 30 of the 61 gates passed and were thrown
     away because `main` moved between their rebase and their push:
     2,096 s of the gate lane's running and 3,695 s of its waiting.
     - 04.1 ran 13 gates over 42.6 minutes, 57% of w4's thread. Its
       `land` gave up after five losses at 00:41, and w4 slept and ran
       it again.
     - 06.2 ran nine gates over 33.5 minutes.
     - hearth's render landing at 00:28 likely cost two more.
   - **Inference:** 04.1 heads the longest chain (04.1, 04.2, 06.3,
     06.4, 06.5, then 01.s2). It landed at 01:03 rather than about
     00:25, so the run ended about 40 minutes later than it would have.
   - **The other missing fixes.**
     - Ruling 105 told the coordinator to record 05.s1 with `plan
       landed`, which the claim kit lacks. That made a block of its own
       at 01:19:02 (ruling 106).
     - The ring of 23:46:52 failed with "thread not found", and the
       supervisor loaded the coordinator's thread by hand.
     - done.md has no `Recorded` line, so `wall_h` ends at the
       supervisor's append.
   - **Cause.**
     - The run keeps its claim's kit (3.6), and its kit was the one the
       plan was fixing.
     - 3.6 lets the supervisor move `kit/` to a commit by ruling. The
       plan named no such commit, and the supervisor chose a flock
       instead.
     - Rulings are checked against `main`, whose documents had moved
       ahead of `kit/`.
   - **First visible:** at the plan, which expected rebases on its
     shared files, with five sessions landing at once through the claim
     kit's `land`. The waits showed in the queue from 00:20 (three gates
     queued). The
     00:17 check counted one gate a landing; the 01:18 check counted 50
     for 17.
   - **Acted on:** at 01:29:48, by steer (ruling 107).
   - Proposal 1.
2. **test-plan's time.**
   - test-plan runs in every gate. It was 15 to 17 s up to 02.2 and 28
     to 34 s after 02.3. It crossed 40 s at 05.3 (00:53), stood at 54
     to 57 s from 06.3, and peaked at 57.7 s before 01.s2.
   - The gate went from 23 s to 67 s, and 23 gates ran 50 s or more.
   - **Cause.**
     - Each increment added checks, and most of them added `plan` calls
       of about 1.1 s each.
     - 02.3 alone added 9 to 13 s (a private pueued, `heavy sleep 1`, a
       poll, `pueue wait`). It came before any time rule, and nobody
       timed it.
     - The session files' Proof bounds summed to 45 s, against the 25 s
       the plan allowed above the base. The Pulse's budgets were
       development.md's Rust example, `15 60`.
     - Nothing failed past a bound.
   - **First visible:** at the 00:17 check, whose pulse showed test-plan
     at 227% of the 15 s budget.
   - **Acted on:**
     - Ruling 104 and a steer at 00:28 asked each report for its times
       and for checks cut to their bounds. They were followed in form:
       02.4 cut 02.3's checks from 10.0 to 5.2 s. They were not
       followed in effect: the suite rose 20 s more over 18 landings.
     - Ruling 107 at 01:29 added 01.s2, which landed at 01:58.
   - Proposals 2 and 4.
3. **The fence.**
   - The coordinator idled from 22:16:51 to 23:46:56, 40% of the run.
     It spent no tokens.
   - The supervisor's 23:17 check found the fence at 1, started a
     five-minute poll, and asked nobody.
   - **Cause.**
     - Decision 3 held every session while temper's client-api was
       active. client-api was claimed before `e6fd9fb`, so it ran the
       checkout's kit, which this run's landings would change.
     - The fence's command read client-api's `STATUS.md` row. It still
       printed 1 when the fence was lifted.
   - **What cleared it:** pinning client-api's kit at skein `841f487`
     (temper's ruling 295, at 23:43). The director relayed it at 23:45,
     and the coordinator was at work 79 s later.
   - **Inference:** asked before the claim, the director could have
     done the same then. The fence was a "For the user" item.
   - **First visible:** at the plan (decision 3, "For the user").
   - **Acted on:** at 23:45, by the director.
   - Proposal 3.
4. **test-plan reached outside its world.**
   - From 01.1's first gate at 23:49:38 until 01.s1 landed at 00:06:10,
     about 15 runs of test-plan each did three things:
     - they ran `plan check --fix` against the machine's `$HOME`, which
       relinked `~/.local/bin/{heavy,land,plan,test-plan}` into their
       scratch worktrees and reached the user bus;
     - they left their world's pueued and Codex daemons running: 11
       Codex pairs, 1.8 GB;
     - some failed on a race in starting their private pueued
       (test-plan:36).
   - **Cause.**
     - 01.1's own Test shape ran `plan check --fix` in test-plan's
       world, which used the real `$HOME`.
     - code-organization.md forbade a check that reads the machine, but
       nothing enforced it.
     - `findings/base-runs.md` never ran the new check.
   - **First visible:** at 23:59:24, in 06's ready check.
   - **Acted on:**
     - The supervisor relinked the tools at 00:01:54.
     - It landed ruling 102 at 00:02.
     - 01.s1 landed at 00:06: its own `HOME`, no user bus, a polled
       pueued.
     - The supervisor killed the daemons, 22 processes, and removed 11
       world directories at 00:27, with no rule to cite.
   - **Inference:** while a run is claimed, the relinked tools still
     hand over to `kit/` (3.6), so the roles kept the claim's kit. No
     harm was found.
   - No proposal: 01.s1 is in the kit, and no daemon leaked after it.
5. **The ready checks held sessions 03 to 06.**
   - 01.2 landed at 23:51. Sessions 03 to 06 started at 00:15:08 to
     00:15:23, 24 minutes later.
   - **Cause.**
     - The checks took 11 to 18 minutes each, at 70 to 95 Sonnet
       requests, up to 166K of context.
     - The session files cited line numbers in files that 01.2 split.
       Restating them by content was most of ruling 103's +274 −172.
     - Three Apply sub-agents edited the files. Apply 05 took 11 minutes
       over one file.
     - The supervisor landed one ruling for all four sessions at
       00:14:42, so 06, corrected by 00:03, and 04, corrected by 00:08,
       waited for 05.
     - Ruling 102's supplement came in between.
   - **Inference:** 04, the longest chain's head, could have started 6
     minutes sooner, and 06 eleven.
   - **First visible:** at 23:59:24, when 06's check returned.
   - **Acted on:** not during the run.
   - Proposal 6.
6. **A test-shape steer point not followed.**
   - The 00:28 steer's point 2 asked that a check assert one behaviour,
     a capture or a poll be a helper, and a pair print as `name=value`.
     It was logged done.
   - After it came three more `|| status=$?` captures (06.2, 04.1,
     04.3), a fourth poll loop (02.4) and a bare pair (04.1).
   - **Cause.** Parallel sessions each add to one file. A session cannot
     share a helper that another has not landed yet, and testing
     strategy's "A setup written once" reaches only what the session
     sees.
   - **First visible:** at the 01:18 check.
   - **Acted on:** a standing README rule and 01.s2 (ruling 107), which
     merged the copies in 11 minutes.
   - The same shape recurs in wall-clock (its section 5, item 7) and in
     parity (item 9).
   - Proposal 4.
7. **One move done three times.**
   - code-organization.md section 6 said the increment that takes
     test-plan past 800 lines first moves its live block out, but named
     no step and no owner.
   - test-plan grew from 409 lines after 01.1 to 798, then 823 at 05.5's
     rebase and 836 at 04.2.
   - The move was done three times:
     - by w8, as 05.s1;
     - by w4, as 04.s1, which kept only its watch half once 05.s1
       landed;
     - by w6's `split-live.py`, dropped at 06.3's rebase.
   - It cost two add/add conflicts in `test-plan-live.sh` (w4 at
     01:16:38, w6 at 01:21:05), a shellcheck failure (04.s1) and two
     blocks (rulings 105 and 106).
   - 04.s1 landed with no ledger row, named in no report's Next.
   - Ruling 105 then made the moved files count as production in the
     plan's Size command, so 06.3 reported 132 lines against 70. 06.5's
     38 test lines against a Size of 10 made the third block, ruling
     108. development.md 5.2 already counts production lines only: the
     plan's command was wrong.
   - **First visible:** at 01:07:41, in 05.5's Next.
   - **Acted on:** at 01:18, ruling 105.
   - Proposal 5.
8. **"Done when" 1 cannot pass after the release.**
   - `plan status` needs a claimed run. 7.6 has the coordinator release
     the run, and only then does the supervisor run the checks.
   - `findings/base-runs.md` rehearsed items 2, 6, 7 and 8 only.
   - reliability's item 11 was of the same kind.
   - **First visible:** at 02:04:50.
   - **Acted on:** an alert, and this retrospective.
   - Proposal 7.
9. **Live runs past their cap.**
   - The README allowed eight: one for each of five live Proofs, one
     for Done when 4, and two for fixes.
   - Nine ran by 06.5, and four failed:
     - 04.1's and 03.3's, on a `done.md` that a metrics fixture left in
       the demo run;
     - 06.4's, on a reader that missed `custom_tool_call_output`;
     - 06.5's, cut short by the start-up doorbell.
   - The live suite grew from 81 s at the base to 162 s. Nobody counted
     the runs until 06.5's implementer did.
   - Cost: 1,048 s of the build lane, and cents.
   - **Acted on:** ruling 108, 70 s after the block.
   - No proposal.
10. **The metrics' and the pulse's misses.**
    - `lines` counted the kit as data.
    - The pulse's "tests +0 −0" misses `test-plan*` (noted at 00:17 and
      01:18).
    - The pulse flagged `development.md` as "over 800": it is a
      2,655-line document.
    - `proofs` read "first" as a layer.
    - The planner's and the design's sessions were silently 0.
    - **First visible:** at the 00:17 check.
    - **Acted on:** noted for the retrospective.
    - Proposal 8.
11. **Small misses.**
    - **The graph project's name.** Five of the first six implementer
      threads called the graph with `srv-data-git-runner-src-skein`.
      skein's `AGENTS.md` gives that name "on this machine", and the
      workspace's `AGENTS.md` gives it as its example. It is another
      machine's checkout and not indexed here; each call failed and was
      retried with `home-free-src-rust-skein`. Proposal 9.
    - **Short poll windows.** w4 polled its long commands with 60 s
      windows: 131 of its 193 requests, each carrying 100K to 125K of
      cached context. w2 did the same, 32 of 88. implementer.md 8.3
      already says `yield_time_ms: 300000`, which w6, w9 and w11
      followed. No proposal.
    - **Re-found timing tricks.** Ruling 104 asked for times but gave no
      method. Three sessions each traced test-plan with `bash -x`,
      polluted a captured stderr, and found `BASH_XTRACEFD` themselves
      (about 320 s lost). The coordinator later passed the method on in
      its briefs.
    - **The coordinator.**
      - An unfiltered `pueue status --json` returned 12.2 MB at 00:46.
      - It skipped the `target/` check at four early yields, with no
        `target/` there.
      - It wrote a time that had not come into 05.s1's note, corrected
        within a minute.
      - It logged the doorbell test of `plan start` as a supervisor's
        ring. development.md says `plan start` rings it (3.6, 6), and
        7.4 says every ring is the supervisor's.
    - **The supervisor's context.** It passed 150K twice without a
      handover (00:06 to 00:18, and from 02:00). The card at `6926c2d`
      hands over at the second compaction, and there was one.

## 6. Decisions taken during the run

Eight rulings and four steers. done.md marks each "not in the design
yet". Here is where each stands on `main`:

| Ruling | Time | Decision | In the design? |
|---|---|---|---|
| 101, with a steer | 23:46 | The fence is lifted, since client-api's kit is pinned at skein `841f487`. Start 01 and 02. | Not a design matter. `later.md` row 5: a run claimed after `e6fd9fb` needs no fence. |
| 102, with a steer | 00:02 | test-plan keeps off the machine: its own `HOME`, no user bus, pueued polled. Supplement 01.s1; relink the tools by hand meanwhile. | In the kit (`7574d00`). development.md's 3.6 row says "a workspace of its own"; no text says what the world must not reach. |
| 103 | 00:14 | Sessions 03 to 06's Shapes against `main` at `817b15c`: 31 corrections. | Plan only. |
| 104, with a steer | 00:28 | Pulse budgets `40 60`. Each report gives test-plan's time before and after its step. A check over its Proof's bound is cut before landing. | Plan only. Proposal 2. |
| 105 | 01:18 | 05.s1 is an added step. A move the size limit requires is `<session>.s<N>`, named in the report's Next before it starts. | development.md 7.3 names added steps `.s<k>`, from an unblock or a steer. "Named in Next first" is plan only. Proposal 5. |
| 106 | 01:19 | Record 05.s1 with `plan set`, since the claim kit has no `plan landed`. | Moot on `main`: `plan landed` (`2c4db1d`). Proposal 1. |
| 107, with a steer | 01:29 | Supplement 01.s2. A standing README rule for test-plan's checks. Code landings under `flock` on `land.lock` (steer only). | The flock is moot on `main` (`land`'s turn, `9ffbcfd`). The checks rule: testing-strategy.md 10's "One behaviour per test" and "A setup written once" already say most of it; `name=value` pairs are plan only. Proposal 4. |
| 108 | 01:45 | The Size command leaves out `test-plan*`. Nine live runs stand, and none more but Done when 4's. | Size: development.md 5.2 already counts production only; the plan's command was wrong. The live cap is plan only. |

The supervisor also acted without a ruling: it relinked the machine's
tools (00:01:54), and killed 22 daemon processes of test-plan's worlds
and removed 11 world directories (00:27). No document asks for either.
Each was right at the time, and 01.s1 removed the need.

**"For the user", still open.** The planner decided eight long-term
choices, each of which the user may veto. Only the fence has an answer,
the director's, in ruling 101. The other seven have no recorded answer.
They are on `main` now, and every later plan follows them:
1. **The kit's gate** is test-plan and shellcheck, not
   `scripts/check.sh`, for a change to `scripts/dev/` and Markdown
   alone. skein's `AGENTS.md` still names check.sh, and `later.md` row 1
   waits on this answer.
2. **What `plan check` counts as a base run and an open question:** a
   `$ ` line in `findings/`, and a `## Design questions` item without
   `Answer:`.
3. **The machine file**, `~/src/rust/tmp/machine.env`, and its four keys.
4. **`hourly:`** replaces the cron job.
5. **`plan handover`'s two forms.**
6. **`experiments.md` was not read:** candidate 7 was planned from the
   brief.
7. **Process text written as the designer would** (decisions 12, 14,
   and 23 to 26).

Proposal 3 asks for these answers before a plan starts.

## 7. The proposals adopted after the last retrospective

The last retrospective is wall-clock's, decided on 10-17. Its text was
applied in skein `b30cc95` at 20:05, two hours before the claim, and was
in force at `6926c2d`. Its kit parts, proposals 7 and 8, were this
plan's: 02.3 and 02.4, and 05.2. The run did not have them, but this
retrospective's `plan metrics` did.

| Proposal | Decision | Meant to move | wall-clock | kit-batch | Moved |
|---|---|---|---|---|---|
| 1. Changes smaller than a run can resolve are measured together | adopted | a verdict plan's change-run hours | 9.3 h | no verdict plan | Not tested |
| 2. A session's `target/` goes only when it has no queued heavy task | adopted | live runs killed by a removed `target/` | 1 | 0. The kit builds nothing, and the coordinator's checks printed `no-target` | Not tested |
| 3. Hourly checks as a chain of one-time cron jobs | adopted | checks more than 65 minutes apart | 6 | 0. 23:17, 00:17, 01:18; the 01:17 job fired 1 m 44 s late, behind an open turn | Yes |
| 4. A quiet hour still runs 14.1 step 2; a change naming a run reaches it within the hour | adopted | hours from a change's landing to its take-up | 10 | Each check reported "process changes: none". No change named this run | Not tested |
| 5. Test-shape rules live in testing-strategy.md 10, cited, not restated | adopted | test-shape steer points | 11 of 18 | 2 of 7. One was not followed (section 5, item 6) | In part: proposal 4 |
| 6. `cargo clean` and `python3` in the card's commands | adopted | failed cells from the card's commands | 31 | none in the reports or the rollouts' failures | Yes |
| 7. `heavy` writes its row inside its task; the pulse reads pueue | adopted; kit in 02.3, 02.4 | comparability of the queue metrics | `heavy.tsv` lacked 7 suites | All 134 rows written. `heavy` removes each pueue task, so pueue adds nothing | In the kit, for the next run |
| 8. Each message priced at its model's rates; design sessions counted | adopted; kit in 05.2 | comparability of `cost_claude` | Sonnet priced as Opus | Priced by model: $8.36 Opus, $14.40 Sonnet. The design's session was not on this machine | Pricing yes; design not tested |

forgejo-17's proposals, decided on 10-16 and in `ce02728` and
`e6fd9fb`, were in force too. Four of them were tested:
- **3, a run takes its claim's kit:** it made the fence (section 5,
  item 3), and it kept the run off its own fixes (item 1).
- **4, a re-check never holds a start:** held. No re-check held one; the
  first checks did (item 5).
- **5, the supervisor's sub-agents on Sonnet:** held. All ten ran on
  Sonnet, with no usage stop.
- **8, a Proof starts with its layer's word:** held, 28 of 28.

**Changes to the process that landed during the run** (skein, 22:16 to
02:05):

| Commits | What | Did the run take it? |
|---|---|---|
| the 29 steps, `e28122c` to `c611973` | the kit and its documents, as the plan says | No. The run keeps its claim's kit (3.6), and no ruling moved `kit/`. In effect, the 01:29 steer took 01.1's landing turn as a `flock` |
| the rulings, `086c43b` to `a3e5bcb` | this plan's files | Yes: they are the plan |
| `05aa3c4`, `0b08745` | hearth's render: Rust in skein | Not process |

## 8. Proposals

Each gives a rule in one sentence, its evidence, the document it
changes, and the metric it should move. They are ordered by expected
effect.

1. **A plan that changes the kit names the steps its own run should
   take; the supervisor moves `kit/` to each by ruling once it lands;
   and a ruling or a steer names only commands that the run's `kit/`
   has.**
   - **Evidence:**
     - 30 of 61 gates passed and were thrown away, because the claim
       kit's `land` has no landing turn. That is 2,096 s of the gate
       lane's running and 3,695 s of its waiting. 04.1 ran 13 gates and
       06.2 nine.
     - 01.1 landed the turn at 23:49. After the flock steer at 01:29
       it was one gate a landing (section 5, item 1).
     - Ruling 105 named `plan landed`, which the run's kit lacks. That
       made a block of its own (ruling 106).
     - The ring that loads the coordinator (04.1) and done.md's
       `Recorded` line (01.1) were missing too.
     - 3.6 already allows moving `kit/` by ruling. A plan that names the
       commits turns that into a checked step, not a judgment made
       mid-run.
   - **Changes:** development.md 3.6 and 5.2 (the README of a kit plan);
     supervisor.md, the rulings (10.3's premises against `kit/`).
   - **Moves:** `gates_per_step` of a kit plan down, 2.1 toward 1.0;
     blocks of kind process over a missing kit command, from 1 to 0.
2. **test-plan fails when its process layer runs past its budget, with
   a margin for noise, and a plan's Proof bounds for test-plan sum
   within the time it allows them.**
   - **Evidence:**
     - test-plan went from 13 s at the base to 57.7 s, against 40 s,
       and the gate from 23 s to 67 s (section 5, item 2).
     - A steer asking for times in each report was followed in form
       while the suite rose 20 s.
     - The plan's bounds summed to 45 s against an allowance of 25 s,
       which nothing checked.
     - test-plan now runs at 37.2 to 37.8 s with no assertion on its
       time. The next kit plan starts 2.2 to 2.8 s below its budget.
   - **Changes:** the kit (`test-plan`, with development.md 3.6's row);
     development.md 5.6 (each increment's tests).
   - **Moves:** test-plan's peak against its budget, from 57.7 s
     against 40 s to within it; the gate's mean, from 49.6 s down.
3. **`plan check` refuses a plan whose "For the user" items have no
   answer, as it refuses an open design question.**
   - **Evidence:**
     - The fence was one of the eight items. It idled the run 1 h 31
       min, 40% of it, until the director pinned client-api's kit, a
       change of minutes (section 5, item 3).
     - The other seven have no recorded answer, though every later plan
       follows them. `later.md` row 1 waits on one, and skein's
       `AGENTS.md` still names a gate that the kit no longer uses
       (section 6).
   - **Changes:** development.md 5.4 and 6; the kit (`plan check`, by
     decision 6's shape).
   - **Moves:** hours from the claim to the first session, from 1.5 to
     0; "For the user" items open at the claim, from 7 to 0.
4. **A plan whose sessions all add to one test suite plans a last step
   that merges the suite's copies and trims its time, after the last
   step that edits it.**
   - **Evidence:**
     - Parallel sessions each added their own captures and polls to
       test-plan. A steer against copies was logged done and not
       followed (section 5, item 6).
     - 01.s2, added mid-run by ruling 107, found the causes in 3.5
       minutes, cut 20 s, merged the copies and kept every check, in 11
       minutes.
     - Planned, it costs one step on the chain's end, and no steers.
     - wall-clock's proposal 5 put the rules in testing-strategy.md 10.
       This run's session files cited them, and the copies came anyway,
       from parallel sessions.
   - **Changes:** development.md 5.3 (increments) and 5.6.
   - **Moves:** `steps_added` of this kind, from 1 to 0, moving into
     planned; test-shape steer points, from 2 to 0.
5. **A move that any of several sessions may need is a step of its
   own, owned by one session, which starts when its file passes a count
   set below the ceiling.**
   - **Evidence:**
     - code-organization.md section 6 left test-plan's live-block move
       to whichever increment passed 800 lines.
     - Three sessions made it: 05.s1, 04.s1, and w6's dropped script.
       That cost two add/add conflicts, a failed gate, an unrecorded
       landing, and the blocks of rulings 105 and 106 (section 5,
       item 7).
     - The plan's own figures (409 lines after 01.1, "grows with every
       session") showed it would be needed.
   - **Changes:** development.md 5.2 (code-organization.md's size
     limits).
   - **Moves:** steps added for a size limit, from 2 to 0; blocks of
     kind plan defect over an unnamed step, from 1 to 0.
6. **The supervisor lands each session's ready-check corrections as a
   ruling of their own, and readies that session at once, never waiting
   for another session's check.**
   - **Evidence:**
     - 06 was corrected by 00:03 and 04 by 00:08. Both waited for 05's
       Apply agent and one combined ruling at 00:14:42.
     - Sessions 03 to 06 started 24 minutes after 01.2 landed. 04 heads
       the longest chain (section 5, item 5).
   - **Changes:** supervisor.md 5, steps 2 and 3.
   - **Moves:** minutes from a session's needs landing to its start,
     from 24 toward its own check's time, 11 to 18 here.
7. **`plan status` renders a released run from its directory, as
   `plan metrics` and `plan retro` read the last run's, so "Done when"
   may name it.**
   - **Evidence:** item 1 failed as written, because 7.6 releases the
     run before the supervisor's checks. The condition held by the
     run's files (section 2). reliability's item 11 was of the same
     kind.
   - **Changes:** the kit (`plan status`); development.md 3.6.
   - **Moves:** "Done when" items failing as written, from 1 to 0.
8. **`plan metrics` and the pulse count a shell kit as code and its
   `test-*` files as tests, judge file lengths on code alone, take a
   Proof part's layer only from the layer words, and name each session
   whose transcript is not on this machine.**
   - **Evidence:**
     - `lines` printed "data +3418 −1904", with code +0 and tests +0.
     - The pulse said "tests +0 −0" at two checks, and flagged
       development.md as "over 800".
     - `proofs` printed "first 1".
     - The planner's and the design's sessions counted as 0 without a
       word, which made "0 before the claim" and a `cost_claude` with no
       planner (sections 3 and 5, item 10).
   - **Changes:** development.md 15.2 (`lines`' kinds); the kit
     (`numbers.sh`, `metrics.sh`).
   - **Moves:** the metric values corrected by hand in a retrospective,
     5 here, toward 0; the pulse's false lines in a check, from 2 to 0.
9. **An `AGENTS.md` gives the rule that names the graph project, from
   the checkout's path, and no one machine's name.**
   - **Evidence:** five of six threads called the graph with
     `srv-data-git-runner-src-skein`, which is not indexed here, then
     retried. skein's `AGENTS.md` gives that name "on this machine", and
     the workspace's gives it as its only example (section 5, item 11).
   - **Changes:** skein's `AGENTS.md`, and its
     `scripts/dev/workspace/AGENTS.md`.
   - **Moves:** failed graph calls at a session's start, from about 1
     to 0.

## 9. Debts left in the code

skein has no `docs/development/tech-debt.md`. These belong in one, in
smith's shape: Seen, Why it matters, Pays it back. Paths are under
`scripts/dev/` at `c611973`, which is `main`'s kit.

1. **`numbers.sh` is at 734 of 800 lines,** and every pulse or metric
   change touches it. Its `pulse` function (:557) is 177 lines. The next
   split is `pulse.sh`. Medium.
2. **test-plan asserts nothing on its time.** It ran in 37.2 to 37.8 s
   against 40 s. Its 143 checks share one linear world with about 15
   save and restore pairs, and `set -e` stops the suite at the first
   unexpected failure instead of recording it. Medium; proposal 2 for
   the time.
3. **A ring to an idle coordinator drops its prefix.** `ring`
   (plan-roles.sh:131) passes the bare message to
   `coordinator_wake_prompt`. Only the steer to a working turn starts
   "From the supervisor, by plan ring:", while development.md 7.4 says
   every ring's message does: it is the guard against a ring read as
   the user's. test-plan-watch.sh:15 checks only the working case.
   Small.
4. **`coordinator_load` resumes the thread without the profile's
   config** (plan-roles.sh:88-90). `thread/start` passes it (:186-190).
   A coordinator reloaded after a reboot or an unload may run without
   the profile's compact limit, and with the graph server on.
   Unverified: only the fake answers it. Medium.
5. **`PLAN_DRY_RUN=1` is not honoured** by `resume`,
   `start_coordinator`, `coordinator_turn`, `log_entry` and `boot_unit`.
   development.md 3.6 says it "starts and rings nothing", and
   test-plan-coordinator.sh relies on the gap. Small to reword, medium
   to honour.
6. **`plan check`'s open questions.** An "- Answer:" bullet counts as a
   second open question. A heading with words after "Design questions"
   passes, and so does a missing `decisions.md` (plan-check.sh:145-152).
   Small.
7. **Copies in production.**
   - The workspace, root and `run_kit` bootstrap is in `plan`, `heavy`
     and `land`.
   - The flock-guarded append to `log.md` is written three times.
   - The ISO-to-epoch `jq` appears ten times; the transcript lookup
     three.
   - The profile's TOML is parsed twice, with two grammars (plan:73-78,
     plan-roles.sh:166-179). A trailing `# comment` breaks both.

   Small each; medium for the bootstrap, which `kit.sh` could own.
8. **Seven file-wide `SC2154` disables** in the sourced modules hide a
   mistyped variable anywhere in about 1,700 lines. plan:43's `SC2015`
   disable is stale. Small.
9. **Settings read inline**, not once at the top as code-organization.md
   section 2 says: `PLAN_POLL`, `PLAN_QUIET`, `PLAN_GAP`,
   `PLAN_CONTEXT_LIMIT` and others. `PLAN_CODEX_FAKE`, a test hook, is
   read in production code (codex.sh:15, plan-roles.sh:308). Small.
10. **Checks tied to the clock.**
    - test-plan-watch.sh:30 bounds "three polls" with `timeout 0.3`,
      which cannot tell zero polls from three. Eight checks of an
      absence rest on it.
    - test-plan:220's `sleep 0.3` is the lock holder's window, which
      flakes under load.

    Medium and small.
11. **Behaviour covered only by fakes:**
    - the reload of an unloaded thread (`later.md` row 3);
    - the profile on a reload (item 4);
    - the queue wake's pueue JSON, written by hand;
    - the boot and pueued units, written under a stub `systemctl` and
      never loaded.
12. **The metrics' own debts** (proposal 8):
    - `lines` classes any file not ending in `.rs` as data before it asks
      whether the file is a test (numbers.sh:416-418);
    - `is_test_path` and the pulse's third copy of the test rule
      (:381, :702);
    - file-lines with no extension filter (:686-690);
    - `proof_layers` takes any first word (:78-86);
    - a missing transcript dropped without a word (:307-308);
    - "Coordinator resumed" appended before anything is known to have
      resumed (plan-roles.sh:307);
    - supervision.md's "## Loaded," still read (metrics.sh:85), though
      no document now asks anyone to write it.
13. **Documents behind the code.**
    - development.md 3.5 omits `.pueued-down`, and says the supervisor
      writes `alerts.md`, which the watch now also writes.
    - The `## Idle` heading and `PLAN_GAP` appear in no section.
    - Small.
14. **Noise in every gate:** pueue's "Failed to cleanup pid during
    shutdown" ends each test-plan run since 02.3, 103 times in this
    run. Small.

Carried from wall-clock's section 9 and not touched here: `plan
downstream --help` runs the check; the pulse's grep reads binary
goldens.

## 10. Decisions

For the user, or the director.

2026-10-18. The director, acting for the user (skein's development.md,
2.7), adopted all nine proposals. For proposal 3, a "For the user" item
is answered when the user, or the director, records a decision on it,
"no veto" included, as a `Decision:` line under the item. The designer
applied their text in skein 9c57a6b, and built each kit part, all small,
through the kit's gate: 2 in 2a9bcd3, 3 in 5f26abb, 7 in 92633c1 and 8
in 23cf08d. Nothing goes to a later kit batch. The runs in flight,
temper's client-api (kit 841f487) and forge-setup (kit c0268a8) and
hearth's render (kit 11af042), keep their kits (development.md, section
12). Each takes 1's kit premise and 6 through its supervisor, and 9 as
its sessions start. Each keeps its process for 1's Kit steps, 2, 3, 4, 5
and 7. Each run's `plan metrics`, run after its release with main's kit,
counts as 8 says. The commits' bodies say so.

This plan's own eight "For the user" items, in `decisions.md`, have no
`Decision:` line yet, and `later.md` row 1 still waits on the gate item.

| Proposal | Decision | Where |
|---|---|---|
| 1 | adopted | skein 9c57a6b: development.md, 3.6, 5.2 (the README's Kit steps) and 10.3; supervisor.md, 3 |
| 2 | adopted: a budget of 40 s with a 5 s margin for the lanes' noise | skein 9c57a6b: development.md, 5.6; skein 2a9bcd3: `test-plan`, with development.md 3.6's row |
| 3 | adopted: an item is answered by a `Decision:` line under it, by the user or the director, "no veto" included, landed with `land --docs` | skein 5f26abb: `plan check` (`plan-check.sh`), with development.md, 3.6, 5.2, 5.4, 6 and 10.4 |
| 4 | adopted | skein 9c57a6b: development.md, 5.3 and 5.6 |
| 5 | adopted | skein 9c57a6b: development.md, 5.2 |
| 6 | adopted | skein 9c57a6b: supervisor.md, 5, steps 2 and 3 |
| 7 | adopted | skein 92633c1: `plan status [PLAN_DIR]`, with development.md 3.6's row |
| 8 | adopted | skein 23cf08d: `numbers.sh` and `metrics.sh` (`lines`, the pulse's lengths, `proofs`, `cost_claude`), with development.md, 5.5, 14 and 15.2 |
| 9 | adopted | skein 9c57a6b: `AGENTS.md`, and `scripts/dev/workspace/AGENTS.md` |

# Retrospective: tls-cleanup

2026-10-12. The run of skein's `tls-cleanup` on carbon-forge, claimed at
00:23:14Z and done at 01:39Z: the first run of development.md. Written
by a fresh session that took no part in the run (development.md, section
15). Seven read-only sub-agents, one per source, read the traces: the
coordinator's Codex rollout, each implementer's rollout, the supervisor's
Claude Code transcript, the planner's (which joined as designer), and the
git history with the landed code. The plan, the run's files and
`logs/heavy.tsv` were read directly. Times are UTC. This document
proposes; the user decides, and the designer applies what is adopted.

## 1. In one page

- **Done in 1.3 hours.** 13 steps landed: 11 planned and 2 added, with
  none dropped. All eight "Done when" items hold on `main` at `dcc5d0a`.
  Planning took 53 minutes before the claim.
- **The code changed as planned.**
  - `client.rs` went from 1,114 lines to 427, and the native face from
    1,004 lines to 607 plus `output.rs` (288). `world.rs` went from
    1,043 to 752.
  - The Pulse words went from 331 lines to none.
  - The median test name went from 10 words to 8. Tests over 40 lines
    went from 7 to the 2 deferred ones.
  - Two things did not change as hoped. The Rust grew from 9,913 lines to
    10,183, against a planned net of about −500. About 50 lines are still
    repeated between the two faces.
- **The escalation channel was fast.**
  - 12 blocks were answered in a median of 1.3 minutes.
  - The doorbell rang 10 times, and the coordinator read the named file
    within 7 s each time.
  - The coordinator made 147 model requests, with no polling loops.
  - Reports were 10 lines, at the median and the 90th percentile.
  - The one hourly check found that "observed" had replaced "actual". The
    fix landed 11 minutes after its steer.
- **The blocks came from the machine and the plan, not the design.**
  - 7 were infrastructure: heavy could not read pueue's +01:00 times
    (which held up all three sessions in the first 6 minutes), the gate
    had no Chromium, and the gate's own load caused two timeouts.
  - 5 were plan defects. Four would have shown at planning time if the
    plan's commands had been run and its sketches compiled.
  - 10 of the 12 came in the first 25 minutes. None came back after the
    standing rules of rulings 16, 17 and 19.
- **The waste was in gates and in an idle designer.**
  - 31 gates ran for 13 landings. 9 failed, 5 of them on clippy. 9 more
    passed and were thrown away when `main` moved: 6 of those after the
    supervisor's Markdown-only landings.
  - The designer was never asked a question, yet used 49.3M input tokens
    watching the run, more than twice the supervisor's 20.7M.
- **The record has errors.**
  - Two of the supervisor's answers were reversed, both numbers it had not
    checked.
  - The supervisor's timestamps ran 2–12 minutes ahead of the clock.
  - `done.md` says each landing was checked for ancestry; the check
    printed nothing.
  - The browser suite ran in none of the 13 code landings. The ruling that
    dropped it rested on "no root", but passwordless sudo works.
- **The main proposals** (section 8):
  - `plan check` runs heavy and the gate once;
  - the planner runs the plan's commands and compiles its sketches;
  - the run's standing rules go into development.md;
  - `land` keeps a passing gate across Markdown-only commits;
  - `plan pulse` lists the words that rose most;
  - the designer does not watch the run.

## 2. Outcome

The plan's "Done when", run once from skein's primary checkout at
`dcc5d0a`:

| # | Item | Result |
|---|---|---|
| 1 | No file of the crate or the world crate over 800 lines | Holds: the command prints nothing. The largest is `tests/tls/src/world.rs`, 752 lines. Three files were over 800 at the start. |
| 2 | No Pulse word, in names either | Holds: prints nothing (331 lines at the start). |
| 3 | `taste.sh` once in the gate, and 3.2's gate passed | Holds: `rg -c` prints 1. `fadcbcb`'s `Gate-Result` reads "gate passed: 1940 tests / 4.995 s; 97 tests / 30.357 s; 39 s in all". That gate ran without its browser suite (ruling 14). |
| 4 | The native face shares the engine | Holds: both `rg` print nothing. |
| 5 | The native `Client` keeps no field only `Open` needs | Holds: prints `lower_read` and `withdrawn_read`, both named in `403beac`'s body, for `Failed`'s and `TransportClosing`'s withdrawal. |
| 6 | No test name over 12 words; the median compared | Holds: the second command prints nothing (19 at the start). The first prints "tests 144 median 8 max 12", against 116, 10 and 15. |
| 7 | Tests over 40 lines: only the two deferred | Holds: `fake_peer.rs`'s 56-line test and `exchanges_swept` (45). |
| 8 | 3.1's `Proof:` says `test-plan --live` passed | Holds: `541056e`'s line reads "live, scripts/dev/test-plan and test-plan --live each printed test-plan: passed within five minutes". |

The README states three more aims:
- **"Nothing a user of the crate can observe changes":** held as far as
  the tests show.
  - No expected value changed, except 1.2's `should_panic` text, which
    the plan allowed.
  - Each production increment's gate passed with its test files
    unchanged.
  - Two mutants failed the tests the plan named.
- **"The native face shares the classic client's engine":** done for the
  pieces the plan named. Starting, processing, failing, the
  `close_notify` decision and Flushing to Finish are still written twice
  (section 9).
- **"Fewer, plainer words":** plainer, yes. Fewer, no: the Rust grew by
  270 lines. Nothing in "Done when" counted lines (section 5, item 8).

## 3. Numbers

No earlier run has a `metrics.tsv`: `plan metrics --history` prints "No
metrics.tsv in any plan". The nearest comparison is the client run
(jig-client-implementation, before development.md). Its numbers were
counted by hand in `~/src/plans/development-process/proposal.md`, section
2, under other definitions, for a run with about 18 times as many
steps. Compare its column for orders of magnitude only.

| Metric | tls-cleanup | Client run | Notes |
|---|---|---|---|
| `wall_h`, `running_h` | 1.3, 1.3 | 49.6, about 36.5 | No pause, no restart. |
| `steps_planned`, `steps_added`, `steps_dropped` | 11, 2, 0 | 104 of 110, 126 added | Both additions were named by the supervisor (03.s1, 02.s1). |
| `step_h` | 0.10 | about 0.16 | |
| `escalations` | 12: infrastructure 7, plan defect 5 | 28: 15 design, 5 to 7 budgets, 3 corrections, 2 plan defects, 2 infrastructure, 1 process | 0.9 per landed step, against 0.12. |
| `escalation_wait_min` | 1.3 | 3.5 | |
| `blocked_h` | 0.2 | not counted | 15% of the run had a block open. |
| `answers_reversed` | 2 | 3 | Both were the supervisor's numbers (section 5, item 3). |
| `steers` | 1 sent, 0 points refused | not counted | |
| `restarts` | 0 | 2 power-offs, 2 continuations | |
| `tokens_coordinator` | 15.9M in (98% cached), 55K out | 811M in | About 1.2M per step, against 3.5M. |
| `tokens_implementers` | 43.2M in (98% cached), 175K out | not counted | Session 01: 16.6M; 02: 21.3M; 03: 5.3M. |
| `tokens_claude` | 227.6M in (98% cached), 1.09M out | not counted | **Not comparable:** see below. |
| `tokens_per_step` | 22.2M | not counted | Inflated by `tokens_claude`: 9.9M for the run alone. |
| `coordinator_requests` | 147, 4 after a wait ran out | 7,500, 779 after a wait ran out | Each timeout cost a second request too: 8 in all. |
| `compactions` | 0 | 33 | The coordinator's context peaked at 164K. |
| `gates_per_step` | 2.5 | about 3.4 | 31 gates and 1 `test-plan --live` in the gate lane. |
| `queue_wait_ratio` | 0.66 | not counted | Gate lane: 801 s waiting, 1,658 s running. Build lane: 320 s waiting, 41 s running. |
| `suites` | focused 1912 → 1940 tests, 9.99 → 5.00 s; fuzzy 97 tests, 41.1 → 31.5 s | fuzzy 4 → 24 s | The drop is not the code's (below). |
| `proofs` | unit 2, world 5, joint 2, process 3, live 1; 0 below plan | not counted | |
| `report_lines` | median 10, p90 10 | p90 453 | |
| `lines` | code +1,445 −1,461; tests +2,041 −1,731 | not counted | The Rust grew by 270 lines. |

**Not comparable, and why:**

- **`tokens_claude`** counts every session in `claude.tsv`, sub-agents
  included:
  - The planner's session also joined the run as designer, so its planning
    counts here: 71.5M before the claim, its six sub-agents included.
  - This retrospective's own session counts too: about 80M with its
    seven sub-agents, up to when `metrics.tsv` was written.
  - The run itself: the supervisor 23.3M and the designer 52.8M (49.3M
    between the claim and `done.md`).

  The definition should count planning, the run and the retrospective
  apart (section 8, proposal 17).
- **`queue_wait_ratio` and `gates_per_step`** miss heavy's first seven
  tasks (00:25–00:29Z): heavy failed before writing their `heavy.tsv`
  lines.

**What explains the largest differences:**

- **Escalations per step,** 0.9 against 0.12:
  - **The machine:** this was the kit's first run on carbon-forge, whose
    `Etc/UTC` zoneinfo is a copy of Lisbon's and which has no Chromium.
    That gave 5 blocks.
  - **The plan:** 5 blocks, four of them commands or sketches never run
    (section 5, item 2).
  - **The card:** development.md 8.5 and 8.7 send every gate failure
    outside the increment, and every edit outside its Files, to a
    question. Six blocks were of those kinds: four gate failures in code
    no session had touched, and two edits just outside Files, both
    allowed.
  - None was a design question, against 15 of 28 in the client run.
- **The coordinator's cost per step,** a third of the client run's. It
  waited in one 300-second `wait_agent` (32 waits: 22 ended by a report, 6
  by the doorbell, 4 timed out), with no loops or watchers.
  - 39% of its input tokens came from its setup reads carried through
    every request. development.md came back truncated and had to be
    re-read in parts.
- **The suites' times halved** between the first landing and the last,
  but not because of the code:
  - The focused suite took 9.5–10.3 s in every gate up to `403beac`
    (01:04), except `c7834bc` (5.6 s). From `7b5afa3` (01:10) on it took
    5.0–5.2 s, with about the same 1,930 tests. The fuzzy suite moved the
    same way, from about 40 s to about 31 s.
  - The two timeout blocks fell in the slow half.
  - The cause was not found. The planner's guess is the code graph
    re-indexing `main` after each landing, and the implementers indexing
    their own worktrees. Both run outside pueue, which only pauses its
    own groups.
  - So a gate's times are not a quiet machine's (section 8, proposal 16).

## 4. What worked

- **The process ran end to end on its first try.** Plan, start,
  sessions, escalations, one check, one steer, `done.md`, release,
  `STATUS.md`, retrospective: no role was restarted, and no step was
  dropped or redone.
- **The escalation channel.**
  - Blocks reached `blocked.md` in 15–35 s (11 of 12).
  - The supervisor answered in 0.4–4.6 minutes, landing its ruling before
    each unblock as 10.4 asks. It worked 17.6 of the run's 77 minutes.
  - Instructions reached the implementers 26–97 s after the ring.
  - The supervisor had the coordinator pass each standing rule to every
    session, not only the one that asked.
- **The second time became a rule** (section 14.4):
  - Ruling 17 turned the second edit beyond Files (after ruling 13) into
    a rule.
  - Ruling 16 defined boundary types.
  - Ruling 19 widened ruling 18's retry to any timeout.

  No block of those kinds came back.
- **The coordinator kept to its card.**
  - It invented no session, step or protocol.
  - It resumed sessions waiting on a dependency within 39 s.
  - Its 14 final reports match `log.md` word for word.
- **Implementers kept working while blocked.**
  - Session 01 wrote each next increment's edit script while its gate
    ran.
  - Session 02 wrote `records.rs` and `meter.rs` while 2.1's questions
    were open, and 2.3 while 2.2's was.
  - Session 02 also started a build-lane clippy before `land` on its own
    at 00:48, 32 minutes before the steer asked for it. It caught 9 lint
    and compile errors before the gate.
- **Proofs at the named layer.**
  - All 13 commits' `Proof:` lines match their increment's layer.
  - Both mutants failed the tests the plan named:
    `ready_survives_token_exhaustion_in_the_same_step` and
    `finish_spends_a_held_grant_and_sends_close_notify`.
  - The live proof ran once, in 75 s, for cents.
- **Shapes held where they were sketched.**
  - `Buffers::new`, `Answer`, `Open`, `open()`, `Pair::ready` and the
    `Peer` accessors landed as sketched.
  - The plan's predictions came true: `lower_read` and `withdrawn_read`
    stay, and `worst_case` changes (440,697 → 440,673).
  - Every test removed, renamed or split has its line in the commit body.
- **The hourly check caught taste drift within the hour.**
  - 2.2 landed "observed" in place of "actual" at 00:59.
  - The 01:17 check found it by reading, not by its numbers.
  - Step 02.s1 was added at 01:19 and landed at 01:31, taking "observed"
    in `tests/tls` from 102 to 0.
  - In the client run the same drift ("actual" 44 → 708) was never caught
    during the run.
- **The planner found and fixed machine problems before the run:**
  - `test-plan --live` refused by Claude Code in `/tmp`, which became
    3.1;
  - skein's HTTPS push URL with no credentials;
  - 16 `plan check` failures;
  - `DEV_REPOS`.

  Its audit (37 findings) and its two reviews (37 problems) found the
  scope the run then executed. The run found no code-against-design gap
  that the audit had missed.

## 5. What did not

Each item gives its cause, when it was first visible, and when it was
acted on.

1. **Infrastructure that `plan check` does not test.**
   - **heavy's times.** pueue printed `+01:00` times, because the
     machine's `/usr/share/zoneinfo/Etc/UTC` is Lisbon's, and heavy's
     `jq` reads only `Z`.
     - `set -e` ended heavy before its summary, its `heavy.tsv` line and
       its `pueue remove`, and `land`'s gate failed with it.
     - First visible at 00:25:19, in every session within 2 minutes.
     - Acted on at 00:29 (ruling 12, a `TZ=UTC0` prefix) and fixed at
       00:43:29 (`86ed5ca`).
     - `test-plan` cannot catch it: it never runs heavy, `land` or the
       gate.
   - **Chromium.**
     - The gate's last line needs Chromium, which carbon-forge lacks.
     - The planner read that line at 23:42:51 and checked nothing.
     - First visible at 00:33:38. Ruling 14 (00:35) dropped the line
       from every code landing's gate and asked the user in
       `decisions.md`.
     - The ruling's premise, "the run has no root", is false. The
       supervisor's own `sudo -n true` succeeded at 00:33:51, and
       passwordless sudo still works.
     - The browser suite ran in none of the 13 code landings, and is still
       off on carbon-forge.
   - **The cause of both:** `plan check` never queues a task and never
     runs the gate.
2. **Plan defects that running the plan would have shown.** The planner
   ran the "Done when" commands, but never a Proof or count command,
   `cargo doc` or clippy. It never searched for the users of a type
   whose shape it changed. The plan was written in about 7 minutes. The
   two reviews' fixes went in during the last 5 minutes and were not
   reviewed again.
   - **Ruling 13:** 2.1's Files allowed "its three ready-pair builders
     only" of `native_output.rs`.
     - Its Shape removed the fields that 18 other lines there read.
     - The word "only" was added at 00:21:57, 35 s before landing, as a
       review fix.
     - One `git grep '\.(server|extracted)\b'` finds the 18 lines.
   - **Ruling 16:** the sketched `Peer { Rustls(Server), Extracted(Tls12) }`
     fails `clippy::large_enum_variant`, and the plan forbade lint
     exceptions in tests. The consistency reviewer had warned that
     "nothing was compiled or run".
   - **Ruling 17:** "`cargo doc` with no warning" could not pass within
     1.2's Files.
     - Three of the four warnings already existed at `73c900b`.
     - 1.1's prescribed `pub(crate)` added the fourth.
   - **Ruling 15:** `heavy cargo nextest list … | wc -l` counts cargo's
     build lines, so it is not a test count. The command was never run.
   - All four were visible at planning time. Each was acted on within 1–5
     minutes of its block, at a cost of one round trip each (1.5–5
     minutes for the session).
3. **The supervisor's numbers.** Both reversed answers turned one of the
   plan's relative checks into an absolute number nobody had asked for,
   unchecked.
   - **"Prints 99"** (00:30) came from `wc -l` of a log whose lines it did
     not read.
   - **"62" for 2.4** (00:36) forgot the 16 tests that 2.2's splits add,
     written in the same session file.
   - Corrected at 00:36 and 01:03, after blocks from session 02.
   - development.md 4.5 already says "check an answer against the code
     before sending it".
4. **A word ban moved the filler to a synonym.**
   - 2.2's edit script first deleted the Pulse words. At 00:43:46 it put
     `re.sub(r'\bactual\b', 'observed', s)` ahead of that deletion, over
     seven files, because "removing them would break expressions".
   - "observed" went from 9 to 119 at `c7834bc` (00:59). Copied messages
     took "winner" from 50 to 90.
   - The cause:
     - 2.2's session file said only "None of the Pulse words".
     - Decision 9, which says why the words go, is not in its "Read
       first", and 8.1 says to read only those.
     - Neither `plan pulse` nor `taste.sh` can see a word that is not on
       the list.
   - Visible at 00:59; found at the 01:17 check; acted on at 01:19.
   - Left over: 02.s1 covered `tests/tls` only, so 14 lines of
     `crates/skein-tls` still say "observed" or "physical" (section 9).
     "winner" is still at 90.
5. **Gates thrown away when `main` moved.**
   - `land` reruns the whole gate after every rebase. Its gate cache is
     keyed by the whole tree, so a Markdown commit to `docs/` changes the
     key.
   - 9 passing gates were thrown away:
     - 6 after the supervisor's rulings (`51d2f4c` twice, `8e0cfaf`,
       `62af41f`, `de2f136`, `3925a5d`), about 6.4 minutes of the
       exclusive gate lane;
     - 3 after code landings.
   - 03.s1 landed on `land`'s fifth and last try. One more ruling would
     have made it exit 6.
   - Visible from 00:37:54. The supervisor noted it at the 01:17 check
     and changed nothing.
6. **Clippy found in the gate.**
   - 5 of the 9 failed gates were clippy: `large_enum_variant`,
     `used_underscore_binding` with `needless_borrow`,
     `redundant_field_names` (2.1); `doc_markdown` (1.2);
     `collapsible_if` (1.4).
   - Each failed in 5–7 s, but after up to 106 s in the gate lane's
     queue and a round trip.
   - The card asks for no clippy before `land`. The plan's "Run nothing for
     it before `land`" may have kept session 01 from running one.
   - First visible at 00:38:58. The steer came at 01:20, after session 01
     had finished.
7. **Timeouts under the gate's own load.**
   - Two blocks: `skein-world`'s "the real loop settles within 1000 ms"
     (`testing/skein-world/src/real.rs:358`) at 00:47, and the focused
     suite's 15-second global timeout at 00:56.
   - Both retries passed.
   - The cause is not known (section 3).
   - Rulings 18 and 19 made one retry a rule. The wall-clock bound is a
     debt (section 9).
8. **"Fewer words" did not mean fewer lines.**
   - The README's aim was "the same things in fewer, plainer words". No
     "Done when" item counted lines, and no rule said what the Size lines'
     net estimates bind.
   - The Size lines added up to about −500 net. Some came out the other
     way:
     - 2.1, with shared modules: +65 against −150;
     - 2.2, splitting 6 tests into 22: +194 against −200;
     - 1.3: +1 against −40.
   - Only 1.3's commit body says so.
   - Visible from 2.1 (00:46), in the pulse's line counts. Not acted on.
9. **An idle designer with a large context.**
   - The user asked the planner to "observe the plan execution", and it
     did, with its own watcher: 26 wakes, each re-reading a context of
     410–503K tokens.
   - That cost 49.3M input tokens between the claim and `done.md`. It was
     asked no question.
   - Its notes were useful to this retrospective, but a fresh session, or
     the retrospective itself, reads the same record for a tenth of that.
   - Visible from the start; not acted on.
10. **Implementers polling their own commands.**
    - Sessions waited on `land` and heavy with short tool calls that
      returned nothing:
      - session 03: 43 calls, 43% of its input tokens (2.3M) in two `land`
        waits;
      - session 01: 51 calls, about 18%;
      - session 02: about 7%.
    - Not counted by any metric, so not visible to the check.
11. **The record.**
    - **The supervisor's times.** Headings it wrote ran 2–12 minutes ahead
      of `date -u`, although it read `date -u` at every wake. The steer
      written at 01:19:56 says "01:31Z". These times went into the ledger's
      supplement notes and into later rulings' "Reverses:" lines.
    - **The ancestry check.** The coordinator ran `merge-base
      --is-ancestor`, which prints nothing, as one line of a multi-line
      script. Its result was never shown for any of the 13 landings, and
      `done.md` says it was checked.
      - All 13 are on `origin/main`: this retrospective checked.
    - **Out of order:** two reports sit at the top of `log.md`, above "##
      Claimed", in reverse order.
    - **Stale state:** one `context.md` edit by `str.replace` silently did
      nothing (00:47).
12. **The kit.**
    - `plan retro` with no argument fails after `plan release` ("no active
      run").
    - A heavy that fails leaves no `heavy.tsv` line and leaves its pueue
      task behind (tasks 0–6).
    - `test-plan --live` ran in the gate lane, which paused the other
      sessions for 80 s, though it compiles nothing.
    - `taste.sh` misses names in CamelCase or with digits (`ActualReady`,
      `actual2`). Run with no directory, it checks the whole repository.
    - The ledger's `thread` column holds `/root/w1`…`w3`, not Codex thread
      ids: `spawn_agent` returns only those.
    - A 7.2 MB binary was left in `logs/skein-01/`.
    - `land --docs` refuses `metrics.tsv` ("not Markdown"), though section
      15.1 lands it beside the retrospective with `land --docs`. This
      retrospective landed through the gate instead, with ruling 14's
      command.

## 6. Decisions taken during the run

None changed what the design asks, and none went to the designer. Where
each belongs:

| Ruling | What | In the design | Proposal |
|---|---|---|---|
| 12 | heavy reads pueue's times in UTC (step 03.s1) | Not needed. The kit's fix landed (`86ed5ca`); `plan check` would have caught it, not in development.md yet | 1 |
| 13 | 2.1 updates the 18 users of `Pair`'s peer | This plan only | 2 |
| 14 | The gate without its browser suite on carbon-forge | Still in force: Chromium is not installed. Not in development.md | 1, 15 |
| 15 | Count test lines with `grep -c`, not `wc -l` | This plan only | 2 |
| 16 | `Peer::Rustls(Box<Server>)`; a boundary type is one used outside the session's files | README rule; not in development.md (8.2) | 4 |
| 17 | Edits beyond an increment's Files, at most 20 lines, are an implementation choice | README rule; not in development.md (8.7) | 3 |
| 18, 19 | One retry for a gate that fails only on a timeout outside the session's files | README rule; not in development.md (8.5) | 5 |
| 20 | 2.4 counts from 78 | This plan only | 11 |
| 21 | Step 02.s1: the native world files in plain words | This plan only | 8, 9 |
| Steer, point 2 | Clippy in the build lane before each `land` | Not in development.md (8.1, 8.3) | 6 |

**Still with the user** (`decisions.md`, "For the user"):
- **Chromium on carbon-forge.** Passwordless sudo works, so `sudo apt-get
  install chromium` is all it takes. Until then, every code landing there
  gates without its browser suite.
- **The native face's future** (decision 4).
- **The word check workspace-wide** (`later.md`).

## 7. The proposals adopted after the last retrospective

There was no earlier retrospective. The nearest thing is the proposal
written after the client run (`~/src/plans/development-process/proposal.md`),
which went into development.md on 2026-10-11. For each of its main
proposals: what it was meant to move, the client run's number, this
run's, and whether it moved. Scale and definitions differ (section 3).

| Proposal (its section) | Meant to move | Client run | tls-cleanup | Moved |
|---|---|---|---|---|
| A supervisor apart from the designer (4) | the escalation wait; the designer's context kept for design | median 3.5 min | 1.3 min; no design question | Yes. But the waiting designer cost 49.3M tokens (section 5, item 9). |
| The hourly check and `plan pulse` (5) | time from a drift being visible to acting on it | 7 to 28 h | "observed": 18 min to the check, 21 min to the steer | Yes |
| The steer's shape (6) | points not followed | not counted | 1 steer, 2 points, both done, 0 refused | Yes |
| One wait and the doorbell (7) | requests that only saw a wait end | 779 of 7,500 | 4 of 147 (8 with their follow-ups); the doorbell answered 10 of 10 | Yes |
| Writing for a literal reader; reports of at most 40 lines (8, 11) | report length; evidence sprawl | p90 453 lines; 256 scripts, 125 polling | p90 10 lines; no polling scripts | Yes, but implementers polled their own commands (section 5, item 10) |
| Shapes, test shapes, "Don't" lists (9.1, 9.2) | taste drift | "actual" 44 → 708; median test name 12 words | Pulse words 331 → 0; median name 10 → 8 | Yes, with one substitution caught mid-run |
| Each step proves itself (9.4) | proofs below plan | not counted | 0 below; 2 mutants failed their named tests | Held |
| Steps added only by an instruction (9.3) | steps added; the coordinator's inventions | 126 added; sessions and protocols invented | 2, both the supervisor's; none invented | Yes |
| Decisions reach the design (11) | decisions left outside the design | about 25 | 10 rulings, each with its state in `done.md`; none needed the design | Held |
| Gating again when `main` moves (11, 13) | gates per landing | about 3.4 | 2.5; 9 of 31 thrown away | A little |
| Thread ids recorded (11) | traces found by the retrospective | none | the coordinator's in `thread`; the sessions' found by parent id, not from the ledger | Partly |
| A code audit while planning (9.7) | code found to differ from the design during the run | item 37 | none found in the run | Held |
| A retrospective ends every run (10) | — | none written | this one | Done |

## 8. Proposals

Each proposal gives a rule in one sentence, its evidence, the document
it changes, and the metric it should move. They are ordered by expected
effect.

1. **`plan check` queues one `heavy true` and runs the repository's gate
   once on `main`, and fails when either fails.**
   - Evidence: 7 of 12 blocks were infrastructure present before the run
     (section 5, item 1). heavy's parse held up all three sessions; the
     browser suite was dropped from every landing.
   - Changes: development.md, section 6, step 2; `plan check`.
   - Moves: `escalations` (infrastructure) and `blocked_h` down.
2. **Before the plan lands, the planner runs once, at the plan's base,
   every command the plan prescribes, compiles each type sketch under
   clippy in a scratch worktree, and lists in Files every user of a type
   whose Shape changes.**
   - The commands: Proofs, counts, "Done when", and `cargo doc` where a
     Proof asks for no warning.
   - Evidence: four of five plan-defect blocks (rulings 13, 15, 16, 17)
     come from a command, a sketch or a `git grep` that was never run
     (section 5, item 2). A plan's last-minute fixes need the same check.
   - Changes: development.md, section 5.4.
   - Moves: `escalations` (plan defect) down.
3. **An edit outside an increment's Files is an implementation choice,
   named in the commit body, when the increment's change, its Proof or
   the gate needs it, it is in the session's own files, it is at most 20
   lines, and it changes no test's expected value or name.**
   - Evidence: rulings 13 and 17. Every such question was answered "yes",
     and none came back after ruling 17.
   - Changes: development.md, sections 8.2 and 8.7.
   - Moves: `escalations` (plan defect) down.
4. **A type crosses a boundary when something outside the session's own
   files uses it; changing any other type's fields is an implementation
   choice.**
   - Evidence: session 02 read 8.2's "a type that crosses a boundary" as
     any `pub` type. Two questions (13, 16) concerned `Peer`, whose users
     were all session 02's.
   - Changes: development.md, section 8.2.
   - Moves: `escalations` down.
5. **A gate that fails only on a timeout, in tests outside the
   session's files, is run once more without asking; a second failure
   on the same tree is a question.**
   - Evidence: rulings 18 and 19. Both retries passed, and the suites'
     times varied twofold (section 3).
   - Changes: development.md, sections 8.5 and 8.7.
   - Moves: `escalations` (infrastructure) and `blocked_h` down.
6. **Before each `land` of a Rust change, the implementer runs `heavy
   cargo clippy -p <each crate it changed> --all-targets -- -D
   warnings` once, and fixes what it prints.**
   - Evidence: 5 of 9 failed gates were clippy. Session 02 did this from
     00:48 and caught 9 errors in the build lane.
   - Changes: development.md, sections 8.1 and 8.3. A plan's "Run nothing
     for it before `land`" then means no proof run, not no clippy.
   - Moves: `gates_per_step` and `queue_wait_ratio` down.
7. **`land` keeps a passing gate when the commits that moved `main`
   since its rebase change only Markdown files under `docs/`, which the
   gate does not read.**
   - It pushes with that gate's `Gate-Result`. The evidence check becomes
     "the landed tree differs from `Gate-Tree` only in `docs/`'s Markdown
     files".
   - Evidence: 6 of 9 thrown-away gates followed the supervisor's
     Markdown landings, and 03.s1 landed on its fifth and last try
     (section 5, item 5).
   - Changes: development.md, sections 9.1 and 9.2; `land`.
   - Moves: `gates_per_step` and `queue_wait_ratio` down.
8. **`plan pulse` prints the words whose count in the plan's files rose
   most since the plan's base, beside the Pulse list's words.**
   - Evidence: "observed" (9 → 119) and "winner" (50 → 90) rose unseen by
     the list. Only a reading at the check found the first.
   - Changes: development.md, section 14.2; `plan pulse`.
   - Moves: `steps_added` down (02.s1 was added for this).
9. **A session file that removes words says what takes their place:
   nothing, or what the code checks, and never another qualifier.**
   - Evidence: 2.2's file said only "None of the Pulse words", and its
     script chose "observed". 02.s1's "do not put another word in their
     place" worked.
   - Changes: development.md, section 4.5 ("Repeat what matters where it
     applies").
   - Moves: `steps_added` down.
10. **Each aim the plan's README states is a "Done when" item with its
    command, or is named there as unchecked.**
    - Evidence: "fewer words" was checked for words, not lines, and the
      Rust grew by 270 lines against a planned −500 (section 5, item 8).
    - Changes: development.md, section 5.2.
    - Moves: `lines`, net, toward the plan's aim.
11. **An answer keeps the plan's relative checks; a number it adds is
    checked against `main` at the time of writing and quoted with its
    source.**
    - Evidence: `answers_reversed` 2, both numbers that replaced a
      relative check (section 5, item 3).
    - Changes: development.md, section 10.4 ("Before an answer is sent").
    - Moves: `answers_reversed` down.
12. **The designer does not watch the run: after `plan join designer` it
    ends its turn, and only `plan wake` brings it back.**
    - Evidence: 49.3M input tokens with no question (section 5, item 9).
      An observer, if wanted, is a fresh session with a small context.
    - Changes: development.md, sections 6 (step 5) and 10.5.
    - Moves: `tokens_claude` down.
13. **An implementer waits for `land` and long heavy commands in one call
    with the longest wait its tool allows, and never polls them.**
    - Evidence: session 03 spent 43% of its input tokens in empty polls of
      two `land` runs, session 01 about 18%.
    - Changes: development.md, section 8.3.
    - Moves: `tokens_implementers` down.
14. **Every time an agent writes into a run's files is read from `date
    -u` when it writes, never estimated.**
    - Evidence: the supervisor's headings ran 2–12 minutes ahead (section
      5, item 11), and the ledger's notes and the rulings' "Reverses:"
      lines carry them.
    - Changes: development.md, section 4.5.
    - Moves: no metric. It keeps `escalation_wait_min` and the ledger
      true.
15. **A ruling that drops part of the gate reaches the user at once, in a
    push notification, with the command that would restore it.**
    - Evidence: ruling 14 dropped the browser suite on a false premise
      ("no root"), and 13 landings went without it. The fix was one
      `sudo apt-get install chromium`.
    - Changes: development.md, section 10.4 ("What goes to the user").
    - Moves: no metric. The number of gates run without a suite should
      stay at zero.
16. **heavy records the machine's one-minute load when a gate starts, and
    `plan pulse` flags a gate that started above 1.**
    - Evidence: the focused suite took 10 s or 5 s for the same tests, and
      both timeouts fell in the slow half. Nothing recorded why.
    - Changes: development.md, sections 11.2 and 14.2; heavy, `plan
      pulse`.
    - Moves: `suites` comparable between gates.
17. **`tokens_claude` is reported in three parts: before the claim, from
    the claim to `done.md`, and after.**
    - Evidence: this run's 227.6M includes 71.5M of planning and about
      80M of retrospective (section 3).
    - Changes: development.md, section 15.2; `plan metrics`.
    - Moves: makes `tokens_claude` and `tokens_per_step` comparable.
18. **The kit keeps its record true:** `plan retro` finds a released
    run; heavy writes its `heavy.tsv` line and removes its task on every
    exit; `taste.sh` refuses to run without a directory and catches
    CamelCase names; the coordinator's checks print their results (`git
    merge-base --is-ancestor <commit> origin/main && echo on-main`);
    `test-plan` runs in the build lane; and `land --docs` takes a plan's
    `metrics.tsv` with its Markdown.
    - Evidence: section 5, items 11 and 12.
    - Changes: the kit; development.md, section 7.3.
    - Moves: no metric. The record stays true.

## 9. Debts left in the code

skein has no `docs/development/tech-debt.md` yet. These are for it, or
for the next plan that touches skein-tls. Paths and lines are at
`dcc5d0a`.

- **The faces still repeat about 50 lines:**
  - starting: `client.rs:351-372` and `native.rs:284-312`;
  - processing: `client.rs:398-408` and `native.rs:314-332`;
  - failing: `close.rs:19-26` and `native.rs:443-457`;
  - the `close_notify` decision: `close.rs:40-58` and `native.rs:491-503`;
  - Flushing to Finish: `client.rs:445-448` and `native.rs:570-577`.

  Two `State`s and two `Open`s share four fields. `Writing::Finishing`
  (`client.rs:343`) is never set by the native face.
- **A lint exception in production code:**
  - `client.rs:57`, `#[cfg_attr(not(test), expect(unused_imports, …))]`
    on the `pub(crate)` `MAX_HANDSHAKE` re-export.
  - Its one user, `client/native/tests.rs:459`, can name
    `limits::MAX_HANDSHAKE` instead.
- **Docs lost in 1.3:**
  - `answer`'s doc, which said when `End` comes, became the sketch's
    placeholder line.
  - Three explanatory comments went: a record's plaintext is no longer
    than its body; a handshake message the records cannot hold whole;
    room may still be granted.
- **Functions over the guides:**
  - Production (60 lines): `client/above.rs:36` `stream`, 70 lines.
  - The world crate (60 lines): `tests/tls/src/native.rs:453` `io_event`
    (93) and `:145` `Driver::new` (80).
  - Test helpers (40 lines): `fake_peer.rs` `iterate` (89),
    `memory.rs` `next` (63), `native_memory.rs` `measure` (62).
- **The world crate's harness, still twice:**
  - `native_memory.rs:142-254` keeps its own proxy (take, pull, answer,
    extraction) beside `Pair`'s (`native_pair.rs:286-405`).
  - The `Limits` of `native_pair.rs:20` is written again at
    `tests/tls/src/native.rs:20` and `native_output.rs:508`.
  - `Peer` has four accessors that panic on the wrong variant, used 27
    times, and no methods of its own. So `native_pair.rs` and
    `native_memory.rs` match on it to dispatch, and build a throwaway
    rustls server as a placeholder (`native_pair.rs:332`,
    `native_memory.rs:211`).
- **Tests that check several behaviours under one name:**
  - The six `withdrawn_*` tests (`native_output.rs:368-493`) each repeat
    a 7-line Finish and Close tail.
  - The header and body pairs that 2.2 split differ only in
    `queued_ciphertext(false)` against `(true)`, and the bool hides
    which.
- **Expected values:**
  - `tests/config.rs:62` expects `client::FLIGHT`, the crate's constant.
  - `client/native/tests.rs:143` asserts only `>= 440_441`, although
    `403beac` gives the value, 440,673.
- **A step test missing:** none checks `Close` after a `Truncated` failure
  before the handshake. That is why 2.4 kept
  `the_stream_ending_or_failing_before_the_handshake_fails_it_when_asked`
  in `tests/tls/tests/handshake.rs`.
- **Words left:**
  - "physical" at `client/native.rs:460` and in 9 lines of
    `client/native/tests.rs`;
  - "observed" at `client/native/tests.rs:39`, `:46`, `:478`, `:491`;
  - `Case::PhysicalCloseFirst` (`tests/tls/src/native.rs:43`, 12 uses);
  - "winner", 90 uses, mostly copied messages;
  - typos left by deletions: `native_pair.rs:213` ("an maximal"), `:276`,
    `:309`, and `tests/tls/src/native.rs:250`.
- **A wall-clock bound in the gate:**
  `testing/skein-world/src/real.rs:358`, "the real loop settles within
  1000 ms", fails under the gate's own load.
- **`taste.sh`:**
  - it misses CamelCase names and names with digits;
  - with no directory it checks the whole repository;
  - its test lives only in the run's logs;
  - it does not check the Pulse `types` line.
- **`later.md`'s rows stand as written:** the native face's `waiting()`
  and requests after a failure, `session.rs`'s nesting, the tests'
  seeds and bytes, `fake_peer.rs`'s home, `fuzzy_native.rs`, smith's
  root store, lib.md and io.md's words, the word check workspace-wide,
  and `plan check`'s `DEV_REPOS` and push checks.

## 10. What was adopted

2026-10-12, after the retrospective landed. The user delegated the choice
("take reasonable decisions"); the designer applied it to development.md
and the kit, in one landing.

- **All eighteen proposals,** with three changes:
  - 2: the planner's runs go through `heavy`, like every role's (11.1).
    The planner's baseline timings for this plan, three `cargo nextest
    run` before the claim, ran beside the queue. In the run, every build,
    test and gate of the four Codex sessions and the three Claude Code
    sessions went through `heavy` or `land`, and no two gates overlapped.
  - 3 and 4 are one rule in 8.2: what crosses a boundary, and the edits
    beyond Files that need no question.
  - 16: a gate's log records the load and the three busiest processes as
    the gate starts alone. No flag yet: on eight cores a load above 1
    says little, and a threshold waits for data.
- **`later.md`'s two `plan check` rows,** with proposal 1: `plan check`
  checks the plan's repository and those upstream of it, and that
  `origin` takes a push.
- **Ruling 14 is over.** Chromium was installed on carbon-forge at
  07:24Z, and the browser suite passes there (6 of 6, through `heavy
  --gate`).

Still with the user: the native face's future (decision 4), and the word
check workspace-wide (`later.md`).

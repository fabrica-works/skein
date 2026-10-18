# kit-batch: decisions

The decisions this plan rests on, numbered; the supervisor's rulings
during the run follow them, from 101; the choices left to the user close
the file. The changes themselves were adopted by the user, or by the
director acting for them (development.md, 2.7), in the retrospectives'
section 10 (findings/inventory.md); the planner decided what follows
(the brief of 2026-10-17: "Decide what's the planner's to decide").

## The plan's own

1. **Scope: skein only, the kit and the documents that describe it.**
   `scripts/dev/**` (with `workspace/`), development.md, supervisor.md
   and implementer.md: every adopted kit change still waiting, as
   findings/inventory.md lists them, and nothing else. The director's
   `experiments.md` is not on this machine: candidate 7 is taken from the
   brief's words. No other repository changes; the plan has no repin
   (bash and Markdown only).
2. **`kit-after-parity` is re-applied, not rebased.** Its one commit,
   d7e6ee3, has a kit part that applies to main with no conflict and
   passes `test-plan` (findings/base-runs.md, 3), and a document part
   that main moved under. 01.1 applies the kit part, and writes the
   document rules afresh where main lacks them; 02.1 writes the rules
   with no kit part. Four rules are dropped, each superseded on main:
   `findings/planning.md` and `plan check`'s warning about it (parity 10
   put the commands' output in `findings/`); the README and
   code-organization read by cited sections, and "Rules that bind it"
   repeating them all (5.2's "the two or three"); at most eight
   increments a session (5.3); the implementer splitting an outgrown
   increment (implementer.md 8.6's tolerance; protocol 11's `split`
   state is kept, in 05).
3. **The fence: the run starts no session while temper's client-api run
   is active.** That run claimed before skein e6fd9fb, so it runs main's
   kit, and it lands in skein: each of its landings brings main's kit
   into its machine's checkout mid-run (parity, section 10). The
   coordinator checks `git -C ~/src/rust/skein show
   origin/main:docs/design/STATUS.md | grep -c "^| temper's client-api |
   active on"` before starting any session: while it prints 1, it runs
   `plan idle "fence: temper's client-api is active"`; the supervisor's
   hourly check rings it once the row has changed. No session is held
   mid-work for it: an implementer that cannot land would wait hours
   with its context open. Lifted by ruling 101, 2026-10-17: the run's
   kit is pinned (temper's ruling 295).
4. **The other runs keep their process.** hearth's render and temper's
   forge-setup were claimed after e6fd9fb and run their claim's `kit/`
   (development.md, 3.6). Each landing's body says, under 4.4: "hearth's
   render and temper's forge-setup keep their process." A run active when
   a step lands that is not in that list is named the same way.
5. **The gate: `test-plan` and shellcheck, not `scripts/check.sh`.** No
   step changes Rust, and check.sh reads nothing in `scripts/dev/`
   (base-runs.md). A step changing code lands with the README's command,
   `LAND_PREBUILD= land --gate '<test-plan, then shellcheck of every kit
   file>'`; a Markdown-only step with `land --docs`. shellcheck runs at `-S warning`: its info findings at
   the base (SC1091, SC2012, SC2013, SC2016, SC2119) are not this plan's.
   The one warning at the base, SC2120 on test-plan's `watch`, is 01.1's.
6. **What `plan check` refuses (forgejo-17 1), as the process designer
   would read 5.4:** a plan whose `findings/` holds no base run, a line
   starting `$ ` in a fenced block of a `findings/*.md` file (the shape
   this plan's base-runs.md uses); or whose `decisions.md` has a
   `## Design questions` section with an item lacking an `Answer:` line.
   A plan with no such section has no open question. development.md 5.4
   names both shapes in 03.2.
7. **The coordinator's fixed lines are the kit's (protocol 6):** `plan
   landed STEP COMMIT` runs 7.3's three checks and sets `landed` only when
   all pass, else prints which failed and exits 1; `plan unblocked`
   files blocked.md and unblock.md under `## Blocked and unblocked,
   <time>` and removes the blocks its `Answer to` line names (10.3); `plan steered` files steer.md under `##
   Steered, <time>`; `plan handover REPOSITORY NN CONTEXT` writes 7.3
   step 4's line beside the supervisor's `plan handover supervisor`, told
   apart by its three arguments; `plan resume [CAUSE]` writes `##
   Coordinator resumed, <time>: <cause>` itself.
8. **A resume's cause:** the argument given; else `reboot` when the
   machine booted after the heartbeat (`uptime -s` against the
   heartbeat's time); else `the user's request` when `paused.md` is
   there; else `unknown` (7.6; protocol 8).
9. **One way to reach the coordinator's thread.** `plan ring`, `plan
   resume` and every ring of `plan ruling` and `plan ready` call one
   function, which first asks `thread/loaded/list`; a thread not in it is
   loaded with `thread/resume` before the turn is read and sent, and a
   send the daemon refuses loads and sends once more (client-composition
   4: `thread/read` answers from the rollout, so it cannot tell a loaded
   thread). Each load appends `##
   Coordinator loaded, <time>: by <command>` to log.md; `restarts` counts
   it with cause `loaded` (05). A ring's message begins "From the
   supervisor, by plan ring:" (protocol 8). The watch rings through it
   (parity 5's queue tasks), so it loads through it too.
10. **A Proof's layer is its first word, up to its first hyphen**
    ("process-tier" is process); each part after a semicolon counts
    (client-composition 9).
11. **Prices per model.** `claude_cost`, which 05.1 moves to
    `metrics.sh` and which the pulse calls too, holds a table of Opus
    5.5, Sonnet 5.5 and Haiku 5.5's rates (checked against
    platform.claude.com's pricing page on 2026-10-17), with its sources and date; Haiku's change
    past 100K tokens of prompt. Each message is priced by its own
    `.message.model`; a model not in the table is priced as Opus and
    named in the output. `PLAN_CLAUDE_PRICES` goes (wall-clock 8).
12. **Design sessions:** the `Designer-Session:` trailers of the commits
    that change a `docs/design/` or `docs/foundation/` path the README's
    Inputs name, in the plan's own repository, reachable from the plan's
    landing (its `Planner-Session:` commit), and newer than the
    repository's previous `Planner-Session:` commit; with their
    sub-agents (wall-clock 8; client-composition 9; parity 14). A skein
    plan's design is `docs/foundation/`, so it counts. A trailer may name
    a sub-agent, as `agent-<id>` or `<id>` (parity's decision 14 lets a
    bounded task of the designer's run as one); its own sub-agents count
    with it (parity 15). 5.2 says the Inputs name each design document by
    its full path, and `plan check` warns when they name none (05.2).
13. **The machine's settings are a file: `$DEV_TMP/machine.env`.** Lines
    `KEY=integer`, for `PLAN_MIN_MEM_GB`, `PLAN_MIN_DISK_GB`,
    `HEAVY_BUILD_SLOTS` and `HEAVY_BUILD_JOBS` only; read, never sourced;
    the environment wins over it. kit.sh reads it, since plan, land and
    heavy source kit.sh; `plan check` prints the values it used and where
    each came from. It survives `plan resume`, which env vars did not.
14. **The hourly check is a watch event, `hourly:`** (parity 11), printed
    when supervision.md's last `## Check, ` entry, or its first `##
    Process, ` when it has none, is over 65 minutes old. supervisor.md's
    check writes its `## Check, ` heading first, before its sub-agent, so
    that a check that stops halfway still leaves the next one due, as the
    cron did. `check:` stays the ready check's. supervisor.md's cron step, and development.md's
    cron text, go.
15. **A daemon down is an alert:** the watch prints `alert: pueued is
    down` and appends it to alerts.md once an outage (parity 8, with
    01.1's alerts.md).
16. **`plan context [SESSION]`** prints a Claude Code session's context,
    its compactions against `PLAN_COMPACTIONS` (2) and the word to hand
    over at them, by default this session's (skein-ui 9). The planner and
    the designer run it after each sub-agent's report (2.1, 2.2).
17. **`plan downstream`'s worktrees go to `<run>/downstream/<repo>`,**
    outside `wt/`, and a trap removes them on every exit (forgejo-17 6,
    skein-ui 10).
18. **The coordinator's thread takes the profile's settings.**
    `start_coordinator` passes in `config` every key of
    `coordinator.config.toml` but `model`, among them
    `model_auto_compact_token_limit`, and the profile gets
    `mcp_servers.codebase-memory-mcp.enabled = false` (protocol 4 and 5;
    findings/inventory.md, 3). The workspace `AGENTS.md` says
    implementers make none of `list_projects`, `index_status`,
    `check_index_coverage`, `index_repository`, and that a design or plan
    session is never a sub-agent (parity 14).
19. **Files stay within 800 lines.** `plan` is 1,361 lines after 01.1:
    01.2 moves the checks into `plan-check.sh`, the watch into
    `plan-watch.sh` and the roles' code into `plan-roles.sh`, sourced as
    numbers.sh is, leaving `plan` 561 lines (findings/base-runs.md, 9); sessions 04 and 06,
    which add most, then change mostly different files: both change
    `handover` and `resume`, and 06.3 waits for 04.2 (README, Shared
    files). 05.1 moves the metrics
    out of numbers.sh into `metrics.sh`. Pure moves, each its own
    increment, so that every later diff reads.
20. **The live layer's budget:** each live proof is one `test-plan
    --live` run (about $0.05 and 81 s at the base), and one more after a
    fix: at most eight in the run.
21. **heavy writes its row inside its task (wall-clock 7).** The queued
    command is heavy itself (`heavy --task`), which runs the command,
    writes the log and the row on its exit, so that a client killed
    while it waits loses nothing; the client only waits and prints. The
    pulse reads what pueue still holds, `pueue status --json`, beside
    heavy.tsv.
22. **A fake Codex daemon for the process layer.** No client call
    unloads a thread: `thread/unsubscribe` leaves it loaded
    (findings/base-runs.md, 5), and a daemon restart would unload the
    run's own coordinator. So `codex.sh` takes `PLAN_CODEX_FAKE`, a
    script that answers `codex_call METHOD JSON` from a state directory,
    and `test-plan` tests the loading, the rings, the resume's heading
    and the thread's settings against it (04.1). `thread/loaded/list`,
    which names the loaded threads, is how the kit tells a loaded thread;
    the live layer checks that the real daemon answers it as the fake
    does.
23. **A split's parts are lettered and count as planned** (protocol 11,
    which parity's decision 15 takes in, over parity 15's "split steps as
    added"): a session that splits `<NN>.<M>` sets its row `split` and
    lands `<NN>.<M>a`, `b`, ...; the `split` row counts in none, and is
    done, for 5.7's repin and 7.6's end, once its parts are. A
    lettered part of a row not `split`, and every `.s<k>`, is added. 7.3
    says so (05.3), so parity's numeric split-offs (10.5 and 10.6 from
    10.1) do not recur, and the metrics need not tell them by name.
24. **unblock.md names the blocks it answers whole:** its `Answer to`
    line lists each heading as blocked.md has it, without `## `,
    separated by `; ` (10.3). `plan unblocked` removes each `## ` entry
    of blocked.md whose heading occurs in that line, with no split, since
    a heading may hold a comma or a semicolon (06.3).
25. **The coordinator's start hold reads the machine's floors:** 11.4's
    "below 6 GB of memory or 40 GB of disk" becomes the floors `plan
    check` prints, `PLAN_MIN_MEM_GB` and `PLAN_MIN_DISK_GB` (6 and 40
    unless the machine file says otherwise), and the build lane widens
    again above the memory floor; the 3, 1.5, 15 and 60 GB lines stay
    (03.1). Else hel8, whose floor is 4, would hold every start. The
    pulse's disk line names the same ladder, the floor included (05.5).
26. **The watch's ring is the supervisor's:** when the queue tasks the
    coordinator waits on end, the watch rings "From the supervisor, by
    plan watch: the queue tasks you wait on have ended: <ids>", and 7.4
    tells the coordinator to read their results and act, as on a task it
    saw end (04.3). The supervisor resumes a quiet coordinator with `plan
    resume crash` (01.1's text; decisions.md 8).

## Rulings

101. **The fence is lifted** (steer of 2026-10-17T23:46:41Z).
     The fence (decisions.md, 3; README, "Rules of this plan") is lifted, at the director's word of 2026-10-17T23:45Z: "the fence on temper's client-api can be lifted now. client-api's kit is pinned: on carbon-forge, ~/src/tmp/temper/client-api/kit is a worktree at skein 841f487 (its ruling 295, temper 0e25fc8d1, 23:43Z 10-17). Its plan, land and heavy no longer read carbon's skein checkout, so the kit batch's landings can't reach it. Every other rule of the plan stands."

     Decision 3 rested on "That run claimed before skein e6fd9fb, so it runs main's kit, and it lands in skein: each of its landings brings main's kit into its machine's checkout mid-run." temper's ruling 295 removes that premise: "from now on `plan`, `land` and `heavy`, run from the copy on PATH, hand over to `kit/scripts/dev`'s (`kit.sh`, `run_kit`), as for a run claimed after `e6fd9fb`, and skein's kit batch may land without reaching this run."

     From now until the run's end, the coordinator runs no fence command before a start, and starts sessions as the README's "How many at once, and in what order" says: skein 01 and 02 at once. The README's fence bullet is replaced by "The fence is lifted (decisions.md, 3; ruling 101)"; its "if the fence allows (Rules)" goes, with nothing in its place. Unchanged: every other rule of the plan, the landing body's line under decision 4 among them.

     Checked:
     - `git -C ~/src/rust/temper show --format='%h %s%n%b' 0e25fc8d1 | head -2` gives `0e25fc8d1 Ruling 295 of client-api` and `the run's kit pinned at skein 841f487 (infrastructure, the director's request of 2026-10-17T23:40Z).`
     - `git -C ~/src/rust/skein show origin/main:docs/design/STATUS.md | grep -c "^| temper's client-api | active on"` at 23:17Z gave `1`: the row stays, and the fence's own command would hold the run.

102. **test-plan's world keeps off the machine: skein-01.s1** (plan defect, ready checks of skein-05.1 and 06.1).
     A plan defect of skein-01.1, found by the ready checks of skein-05.1 and 06.1: test-plan's world reaches the machine. The README's rule "No step runs a real role but through `test-plan --live`, and none restarts pueued or the Codex daemon of the machine it runs on" assumes a world that keeps off the machine; 01.1's test "check --fix fast-forwards" (`test-plan:404` at `817b15c`) runs `plan check --fix` with the machine's `$HOME` and user bus, so each `test-plan` run, a land gate's among them, relinks `~/.local/bin/{heavy,land,plan,test-plan}` to its own tree, which dangles once that tree is removed.

     Supplement step skein-01.s1 (`skein/01-s1-test-world.md`) gives the offline layer its own `HOME` and no user bus, and waits for the world's pueued before its first `pueue group add`. It needs nothing; start it now, before any other session. The README gains the rule "test-plan keeps off the machine", which binds every session, since each changes `test-plan`; until skein-01.s1 lands, after each `test-plan` run and before a worktree is removed, relink: `for t in heavy land plan test-plan; do ln -sfn ~/src/rust/skein/scripts/dev/$t ~/.local/bin/$t; done`. Unchanged: `plan-check.sh`, and every other rule.

     Checked:
     - `ls -l ~/.local/bin/plan` at 2026-10-18T00:01Z gave `-> /home/free/src/rust/tmp/skein/scratch-check04-01/scripts/dev/plan`, a ready check's scratch worktree; relinked to `~/src/rust/skein/scripts/dev/plan` at 00:01:54Z.
     - `git log --format='%h %s' -S'check --fix fast-forwards' origin/main -- scripts/dev/test-plan` gives `9ffbcfd Apply the adopted kit rules from kit-after-parity` (01.1).
     - `sed -n '195p' scripts/dev/plan-check.sh` gives `local bin=$HOME/.local/bin home=${DEV_KIT_HOME:-$kit}`; `:274` gives `systemctl --user restart pueued && sleep 1`.
     - `sed -n '35,36p' scripts/dev/test-plan` gives `pueued -d` and `pueue group add build >/dev/null`; the checks of 05 and 06 each saw their first run fail at `test-plan:36`, a rerun pass.

103. **03's, 04's, 05's and 06's Shapes against main** (ready check of skein-03.1, 04.1, 05.1 and 06.1).
     The ready checks of skein-03.1, 04.1, 05.1 and 06.1, against skein `817b15c` (01.1, 01.2, 02.1 to 02.3 landed), found no plan defect beyond ruling 102's and corrections in each session file; each file now carries them, and cites a place by its content with its line "at 817b15c":
     - **03:** 03.4's `errors=$(` guards its grep (`{ grep -E … || true; }`), or `set -euo pipefail` (plan:39) aborts `plan downstream` before "fails" prints; `made` and `unmake` are global, so the EXIT trap sees them; 03.3's boot unit names an absolute `ExecStart`; 03.3's tests run in skein-01.s1's world; 03.1 needs skein-01.s1 (and the README's row says so); the cites moved by 01.2's split are corrected.
     - **04:** cites moved by 01.2 (plan-roles.sh, plan-watch.sh); 04.1's heading is written in 7.4, which 7.5's bullet names; 04.3's Files gain `plan`'s usage (`plan idle REASON [--tasks ID…]`) and 7.4's idle bullet; a dirty worktree fails test-plan's `heavy_run --gate true` check (:203): commit first.
     - **05:** metrics.sh opens with `# shellcheck shell=bash` and disables SC2154 and SC2034; development.md 3.6 (:455-456) names `metrics.sh`; `jsonl [--arg N V]... PROGRAM [FILE...]`, stdin with no FILE; of numbers.sh's eight jq lines, :99 and :131 stay `head -n1` reads; 05.4's fixture records carry `payload.thread_id` and `payload.response_id`, and `codex_requests` counts distinct `response_id`s (smith `rollouts.rs:171-181`); `restart_causes` takes the whole text after the heading's time, not numbers.sh:685's regex; the new functions are in Files.
     - **06:** 06.1's and 06.3's Files gain `plan`'s usage and dispatch (plan:534); 06.3 reads each of unblock.md's `Answer to` lines (`plan ruling` appends a second draft, plan:436-438) and logs the two files as one body (`log_entry HEADING [BODY_FILE]`); 06.4's Files gain development.md 3.5's `thread`, `threads` row, and its test restores `$run/threads` with `$run/thread` (plan-roles.sh:138 appends to it; numbers.sh:90 reads it).
     A further copy a session finds stale is corrected in its commit, and named in the commit body.

     Checked:
     - `git diff --stat 817b15c 0a1700b -- scripts docs/foundation` prints nothing: the cites hold at 0a1700b.
     - `sed -n '509p' scripts/dev/plan` gives `errors=$(grep -E '^[^ ]+:[0-9]+:[0-9]+: error' "${log:-/dev/null}" …`; `sed -n '39p'` gives `set -euo pipefail`.
     - `sed -n '436,438p' scripts/dev/plan` gives `if [[ -f $run/unblock.md ]]; then` / `sed '1{/^# Unblock/d}' "$draft" >>"$run/unblock.md"`.
     - `sed -n '138p' scripts/dev/plan-roles.sh` gives `printf '%s\t%s\n' "$thread" "$(now)" >>"$run/threads"`.
     - Suites: test-plan passed at 817b15c in each check's scratch worktree (tasks 3796, 3797 and others), after a first failure at test-plan:36 (ruling 102); `shellcheck -S warning -x` clean.

## For the user

Long-term choices the user may veto. Each was decided by the planner
under the director's brief.

- **This plan's gate is `test-plan` and shellcheck,** not skein's
  `scripts/check.sh`, for the steps that change only `scripts/dev/` and
  Markdown (decision 5). skein's `AGENTS.md` still names check.sh for
  every landing; earlier kit changes ran it (2.5 to 3 minutes on
  carbon-forge, none of it reading the kit). The alternative was
  check.sh on each of 20 code landings.
- **What counts as a base run and an open question** for `plan check`
  (decision 6): a `$ ` line in a fenced block of `findings/*.md`, and an
  item of decisions.md's `## Design questions` without `Answer:`. Every
  plan from here is written to these shapes.
- **The machine file** `~/src/rust/tmp/machine.env` and its four keys
  (decision 13). Each machine's settings live there from now on: hel8's
  `PLAN_MIN_MEM_GB=4`.
- **The hourly check's word is `hourly:`,** and the cron job goes
  (decision 14).
- **`plan handover` takes two forms** (decision 7): `supervisor`, and
  a session's line.
- **The fence** (decision 3): the run idles while temper's client-api is
  active. Lifted by ruling 101, at the director's word.
- **experiments.md was not read** (decision 1): candidate 7 is planned
  from the brief's words.
- **Process text the planner wrote as the process designer would,**
  where the trace found the design silent: a split's parts are lettered
  and count as planned, and a `split` row is done once its parts are
  (23); `docs/foundation/` counts as a skein plan's design, a plan's
  Inputs name design documents by full path, and a trailer may name a
  sub-agent (12); 15.2's `coordinator_requests` is restated to the kit's
  count, the requests after a wait ran out (05.4); unblock.md's `Answer
  to` line separates its headings with `; ` (24); the coordinator's
  start hold follows the machine file's floors (25); the hourly check
  writes its heading first (14); the watch's ring and `plan resume
  crash` (26).

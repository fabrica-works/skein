# Session 02: `land` and `heavy`, and the rules with no kit

2026-10-17. Three faults of the tools every session runs. A push that
lost a race for origin's ref lock exits 7, an infrastructure fault, where
it is main moving (protocol 12). `plan set --help` made a step named
`--help` (protocol 12). And `heavy`'s client writes its task's row after
the task: a client killed while it waits loses the row, and seven suites,
21.8 hours of the gate lane, had none in wall-clock (its proposal 7).
02.1 first writes the rules of kit-after-parity that have no kit, so that
the session starts at once.

## Read first

- skein `docs/foundation/development.md`, sections 9.1 (`land`), 11
  (`heavy`, and 11.2's `heavy.tsv`), 14.2 (`plan pulse`), 15.2
  (`queue_wait_ratio`, `gate_lane_other`).
- `findings/inventory.md`, section 1 (the rules 02.1 writes);
  `findings/base-runs.md`, section 6 (a private queue).
- `decisions.md`, 2 and 21; `code-organization.md`, sections 3 and 4.
- skein `testing-strategy.md`, sections 4 (fakes and their neighbours)
  and 10.

## What the design asks

- **5.2, 10.4, 7.6; implementer.md 8.1 to 8.3, 8.5, 8.7; supervisor.md
  8:** d7e6ee3's rules 1, 4, 7, 8, 9, 13 and 14k, as findings/inventory.md
  lists them.
- **9.1:** `land`'s exit 6 is "main kept moving"; a lost ref lock is main
  moving.
- **11.2:** one row per task in `logs/heavy.tsv`, whatever becomes of the
  client.
- **14.2, 15.2:** the queue's figures count every task of the run.

## Choices this plan makes

- `land` retries on `cannot lock ref` as on a rejected push: fetch,
  rebase, push again, at most `--tries` times, then exit 6.
- `heavy` queues itself: `heavy --task TOKEN -- command…`, an internal
  mode like `--exclusive`, runs inside the pueue task and writes the log
  and the row (decisions.md, 21). The client waits, prints and removes
  the task. A task with no row when its client looks (killed with
  SIGKILL) gets the row from pueue's record, as today.
- The pulse and the metrics read the queue through one function,
  `queue_rows RUN`: heavy.tsv's rows, and a row for each finished task
  pueue still holds that queued after the run's claim and is not a
  `heavy --task` (which wrote its own).

## Rules that bind it

- **The gate** (README, Rules): from 02.2, the README's command;
  02.1 lands with `land --docs`.
- **No real queue** (code-organization.md, 4 and 5): the tests start
  their own `pueued`; never the machine's.

## Increments

### 02.1 The rules with no kit
Scope: d7e6ee3's rules 1, 4, 7, 8, 9, 13 and 14k (findings/inventory.md,
  1).
Needs: nothing.
Files: `docs/foundation/development.md` (5.2, 7.6, 10.4),
  `docs/foundation/implementer.md` (8.1, 8.2, 8.3, 8.5, 8.7),
  `docs/foundation/supervisor.md` (8).
Shape: each rule, as `git show d7e6ee3 -- docs/foundation/` words it,
  written where main's section says the same thing, only where main
  lacks it: the README's longest landing chain in hours at the last
  runs' `step_h` (rule 1); a find-the-cause step's budget of runs and
  minutes (4); `cargo nextest run -E 'rdeps(<crate>)'` once before
  `land` (7); edits that follow the Shape need no question, listed under
  "Users outside Files:" (8); diagnostics never committed (9); a live
  probe's sentence in Done when (13); a failing Done-when command
  reaches the user as an alert (14k).
Test shape: none: Markdown.
Proof: none, Markdown only: `land --docs` lands it, and nothing runs.
Don't: repeat a rule main already words; change a section's numbering.
Size: 0 lines of code; about 60 of Markdown.
Removes: nothing.

### 02.2 A lost ref lock is main moving; `plan set` refuses an option
Scope: protocol 12; 9.1.
Needs: skein 01.1 (d7e6ee3's `land`; the SC2120 fix the gate needs).
Files: `scripts/dev/land` (the push's error test, land:295 after 01.1;
  the exit list, :34–44), `scripts/dev/plan` (`set_step`),
  `scripts/dev/test-plan`, `docs/foundation/development.md` 9.1.
Shape: land's retry test becomes `grep -qE 'non-fast-forward|fetch
  first|stale info|cannot lock ref'`; its header's exit 6 reads "main
  kept moving, or its ref stayed locked". In 9.1, exit 6's row of the
  exit table (about :1668-1679) reads "main kept moving, or its ref
  stayed locked: a lost ref-lock race is retried like a moved main, and
  exits 6 when the retries run out". `set_step` opens with `[[ $step !=
  -* ]] || die "a step is not an option: $step"`.
Test shape: in test-plan, after the ledger's checks, a land world: a bare
  repository `$t/origin.git` with one commit on main, and a clone
  `$t/clone` (its own `user.name` and `user.email`, as the fixture's
  alpha has, test-plan:43–44) with `DEV_TMP=$t/land-tmp`; each check
  lands a fresh branch from origin's main. "A locked ref is main moving":
  a Markdown commit, `touch $t/origin.git/refs/heads/main.lock`, then
  `land --docs --tries 2` exits 6 with "land: main moved 2 times; giving
  up", and the lock file is removed after. The refusals d7e6ee3's `land`
  adds (01.1) are tested here, each exiting 2: "land refuses an unread
  trailer" (`land --docs` on a commit with `Step: alpha-01.1` in a
  paragraph before the last: "git reads no trailer in:", "its Step
  line"), and "land refuses a Pulse word" (`$t/land-tmp/active-plan`
  holds `alpha/demo` and `$dir`, whose Pulse forbids `actual|exact`; a
  commit with a `Plan: demo` trailer adds `let exact = 1;` to `w.rs`;
  `LAND_PREBUILD= land --gate true`: "the plan's Pulse forbids these
  words (actual|exact)", "w.rs: let exact = 1;"). That one is a code
  landing, so it takes the turn: a background `flock` holds
  `$t/clone/.git/land-turn.lock` for 1 s (the test waits until it is
  held), and the output also has "land: another landing in clone has its
  turn; waiting for it". Then `plan set --help state=ready` exits 2
  (`die`) and leaves the ledger as it was.
Proof: process, those checks in `test-plan`, under 6 s.
Don't: retry on any other push error.
Size: about 10 lines.
Removes: exit 7 for a lost race; steps named by an option.

### 02.3 `heavy` writes its row from inside its task
Scope: wall-clock 7; decisions.md 21; 11.2.
Needs: skein 01.1 (test-plan as d7e6ee3 leaves it; `heavy` is as on main).
Files: `scripts/dev/heavy`; `scripts/dev/test-plan`;
  `docs/foundation/development.md` 11, 11.2.
Shape: the client makes `token=$(date +%s%N)-$$` and queues
  `"$kit/heavy" --task "$token" -- "${task[@]}"`. `--task` (parsed
  before the HEAVY_TASK check, like `--exclusive`): finds its own id,
  `pueue status --json | jq` on the task whose command holds the token;
  names the log `<UTC time>-<id>.log` as today; traps EXIT, TERM, INT and
  HUP to write the row (id, lane, end, waited from the token's time to
  its start, ran, status, MemAvailable, PWD, log, command, label: the 11
  columns of today, heavy:152–154); runs the command with output to the
  log and to stdout; exits with its status. The client, after `pueue
  wait`, takes the last row whose id is the task's; with none, writes it
  from `pueue log --json` as heavy:130–154 do now; then removes the task,
  tails the log and exits as now.
Test shape: test-plan gets a private queue (findings/base-runs.md, 6)
  at its top, beside DEV_TMP's export (:21): `export
  PUEUE_CONFIG_PATH=$t/pueue/pueue.yml`, `pueued -d`, groups `build` and
  `gate`, shut down in the EXIT trap whether `$t` is kept or not; so
  every check, the pulse and the metrics (:211–352) among them, reads it
  and never the machine's queue. heavy runs with `DEV_TMP=$t/heavy-tmp`.
  Checks: "heavy writes a row and passes the status" (`heavy sh -c 'exit
  3'`: exit 3, one row with status 3); "a killed client loses no row"
  (`heavy sleep 1 &`; once `pueue status --json` shows the task
  `Running`, polled every 0.1 s for at most 5 s, kill the client; `pueue
  wait`: one row, status 0); "the gate lane writes its row too" (`heavy
  --gate true`). Each under 2 s.
Proof: process, those checks, under 6 s; live, once, in the report:
  `heavy --tail 3 true` from the worktree on the machine's queue, its row
  in `$DEV_TMP/logs/heavy.tsv`, under 30 s.
Don't: write the row in two places for one task; start or restart the
  machine's pueued.
Size: about 70 lines.
Removes: rows lost with their client.

### 02.4 The pulse and the metrics read what pueue still holds
Scope: wall-clock 7; 14.2, 15.2.
Needs: 02.3 (the private queue, `heavy --task`); skein 05.1 (`metrics.sh`).
Files: `scripts/dev/numbers.sh` (`land_gates`, the pulse's queue lines,
  numbers.sh:454, 550, 572 after 01.1); `scripts/dev/metrics.sh`
  (`metrics_of`'s `queue_wait_ratio` and `gate_lane_other`,
  numbers.sh:718–723 before 05.1); `scripts/dev/test-plan`;
  development.md 14.2, 15.2.
Shape: `queue_rows RUN` in numbers.sh prints `$RUN/logs/heavy.tsv`,
  then, for each task of `TZ=UTC0 pueue status --json` that is done
  (`.status | keys[0]` is `Done`), queued at or after the run's claim
  (active-plan's fourth line), and whose command holds no `heavy
  --task`, a row in heavy.tsv's columns from `.status.Done`'s
  `enqueued_at`, `start`, `end` and `result`, read as heavy:132–147 read
  them. A `pueue status` that fails adds no row. The four readers of heavy.tsv read `queue_rows "$run"`
  instead. The pulse adds `pueue holds N finished tasks no client
  removed` when N > 0.
Test shape: in test-plan's private queue (02.3's, so the earlier pulse
  and metrics checks read it empty and print as before): a task added
  with `pueue add` (no `--task`) after the claim and finished; `expect "the pulse counts a
  task pueue still holds"` on the pulse's queue line, with the fixture's
  20 s and 200 s plus that task's.
Proof: process, that check, under 2 s.
Don't: remove a task from pueue; count a `heavy --task` twice.
Size: about 40 lines.
Removes: queue figures that leave out held tasks.

## Interface sketches

```bash
# heavy, inside the task: the row, whatever ends the command.
heavy --task TOKEN -- command…   # TOKEN: <enqueued epoch ns>-<client pid>
# numbers.sh: heavy.tsv's rows, then the finished tasks pueue holds that
# wrote none; 11 tab-separated columns, as heavy.tsv.
queue_rows RUN
```

## Questions to raise when reached

None.

## Done when

02.1 to 02.4 have landed, and `test-plan` passes on main with the private
queue's checks.

# Session 05: the numbers

2026-10-17. What `plan pulse` and `plan metrics` count, and at what price,
against section 15.2. A torn line in a transcript stopped `jq`, and the
metrics with it (exit 5, parity 15). Opus 5.5's prices went on every
message, though $37.45 of wall-clock's supervisor was Sonnet's (wall-clock
8). The design's sessions and their sub-agents, $23.76 in wall-clock and
$28.13 in client-composition, were not counted (4.4; client-composition
9). Hand loads were not restarts, a Proof counted its first word only, a
lettered part of a split step was a step of its own, and a pin was read
from the checkout, not from origin's `main` (parity 15, protocol 11). The
pulse still held two old rules: a context limit of 200,000 (parity 6) and
no word on disk (parity 7). Five increments: the metrics move to their own
file, then what they read, how they price it, what they call a step, a
restart and a pin, and last the pulse's two rules.

## Read first

- skein `docs/foundation/development.md`, sections 3.6 (`plan contexts`,
  :488), 4.4 (:593-597), 7.3 (the states, :1505-1517), 11.4 (disk,
  :2160-2183), 14.2 (:2398), 15.1 (:2462-2474), 15.2 (:2507-2552).
- The retrospectives, section 8 and its section 10 decisions:
  `smith/docs/development/plans/parity/retrospective.md` (6, 7, 14, 15),
  `smith/docs/development/plans/wall-clock/retrospective.md` (8),
  `smith/docs/development/plans/reliability/retrospective.md` (17, its
  rest), `temper/docs/development/plans/protocol/retrospective.md` (11),
  `jig/docs/development/plans/client-composition/retrospective.md` (9).
- `decisions.md`, 10 to 12, 23 and 25; `findings/inventory.md`, section 2 (protocol
  11, parity 6 and 7); `findings/base-runs.md`.
- `code-organization.md`, sections 1 (the file table), 3 (`metrics.sh`,
  `line_counts`), 4 and 5 (tests).
- The kit after 01.1 (findings/base-runs.md, 9): `scripts/dev/numbers.sh`
  (786 lines; 747 at the base), `scripts/dev/plan`,
  `scripts/dev/test-plan`. Line numbers below are after 01.1: "before
  01.2" for `plan`, and "before 05.1" for numbers.sh.

## What the design asks

- **3.6:** `plan contexts` marks the sessions past `PLAN_CONTEXT_LIMIT`.
- **4.4:** the design's landing names its sessions in `Designer-Session:`
  trailers, so that a plan's whole cost can be counted.
- **7.3:** the ledger's states; a step its session split is a `split` row;
  a step added by a steer is `.s<k>`.
- **11.4:** `kache gc` at every disk check below 80 GB; at most six
  sessions below 60 GB; starts held below the disk floor (40 GB unless
  the machine file says otherwise); the line stops below 15 GB.
- **14.2:** the pulse's lines: pins, contexts, the machine.
- **15.1:** the transcripts the retrospective reads, the designer's
  included.
- **15.2:** the metrics: `restarts`, `tokens_*`, `cost_claude`,
  `coordinator_requests`, `steps_*`, `proofs`, `lines`, `pin_lag`.

## Choices this plan makes

- A Proof's layer is its first word up to its first hyphen: `process-tier`
  is process. Each part after a semicolon counts (decisions.md, 10).
- Prices are per model, by the message's own `.message.model`, in a table
  in `metrics.sh` with its sources and the date 2026-10-17 (decisions.md,
  11). A model not in the table is priced as Opus and named. A message
  with no model is priced as Opus and not named.
- The design's sessions are the `Designer-Session:` trailers of the
  commits that change a `docs/design/` or `docs/foundation/` path the
  README's Inputs name, newer than the repository's previous
  `Planner-Session:` commit, read in the plan's own repository
  (decisions.md, 12). A trailer may name a sub-agent, `agent-<id>` or
  `<id>`: a bounded task of the designer's may run as one (parity's
  decision 14), and its own sub-agents count with it (parity 15).
- A restart's cause is the whole text after the time. A coordinator
  loaded by hand is cause "loaded".
- A lettered part of a step whose ledger row is `split` is planned; any
  other lettered part and every `.s<k>` step is added; the `split` row
  counts in none. This is protocol 11's rule where parity 15 and
  reliability 17 say "added", as the parts of a split are the plan's
  work, and parity's decision 15 takes protocol 11 in. 7.3 names a
  split's parts by letter, so a numeric step is a planned one or a
  `.s<k>` (decisions.md, 23). A `-repin` step is planned.
- `pin_lag` reads `origin/main`'s `Cargo.lock`. Only `plan metrics`
  fetches first, best effort; the pulse does not.
- `PLAN_CONTEXT_LIMIT` is 150,000 wherever it is read. The Claude Code
  roles' `PLAN_CLAUDE_AUTOCOMPACT` stays 200,000.
- `running_h` is left as the kit counts it (later.md).

## Rules that bind it

- **The gate** (README, Rules): `LAND_PREBUILD= land --gate
  'scripts/dev/test-plan && shellcheck -S warning -x $(git ls-files
  scripts/dev ":!scripts/dev/workspace")'`.
- **Shared files.** `numbers.sh` and `test-plan` are also changed by 02.3
  and 02.4 (the pulse's and the queue's readers), 03 (`$t/stub`) and 04.1
  (`record_check`, the `Coordinator loaded` heading). Rebase on `main`,
  touch only the functions an increment names, keep both sides.
- **The design's text is edited in the increment** that changes the
  definition, and no increment restates it. Every file stays at most 800
  lines.

## Increments

### 05.1 `metrics.sh`, a move
Scope: code-organization.md, sections 1 and 3: numbers.sh is 786 lines,
  and 05.2 to 05.5 add about 270.
Needs: skein 01.2 (`plan` split; 01.1's `line_counts`, `record_check` and
  `pulse_setting` in numbers.sh).
Files: new `scripts/dev/metrics.sh`; `scripts/dev/numbers.sh`
  (`claude_cost` and `supervisor_sessions`, :221-284 with their comments,
  `metrics_of`, :637-754, and `metrics`, :756-786, leave; the header
  comment, :1-10, says so); `scripts/dev/plan` (the `source` of
  numbers.sh, plan:1268-1269 before 01.2).
Shape: the four functions move unchanged, in order, to `metrics.sh`,
  under `# shellcheck shell=bash` and a comment saying that `plan` sources
  it after numbers.sh. `plan` gains `# shellcheck source=metrics.sh` and
  `source "$kit/metrics.sh"` below numbers.sh's. Functions resolve at call
  time, so numbers.sh's pulse still calls `claude_cost`, and
  `metrics_of` calls numbers.sh's readers. `claude_tokens`,
  `median_and_p90`, `report_lengths`, `land_gates`, `pin_lag`,
  `run_ended`, `running_seconds` and `thread_contexts` stay.
Test shape: none new. test-plan copies the whole kit into its fake
  skein (test-plan:88-92), so the run's kit has `metrics.sh` too.
Proof: process, `scripts/dev/heavy --tail 15 scripts/dev/test-plan`
  prints `test-plan: passed`, with test-plan unchanged; the gate's
  shellcheck is silent.
Don't: change a body or a comment of a moved function; move any other.
Size: about 8 new lines (213 moved).
Removes: numbers.sh's growth toward the ceiling.

### 05.2 Torn lines, the model, prices, the design's sessions
Scope: parity 15 (the torn line; a named sub-agent's own sub-agents),
  parity 14 (its metrics part), wall-clock 8, client-composition 9;
  decisions.md, 11 and 12; 14.2 (cost so far); 5.2 (Inputs); 15.2
  `tokens_claude` and `cost_claude`.
Needs: 05.1.
Files: `scripts/dev/numbers.sh` (new `jsonl`, `claude_files`, `design_inputs`,
  `designer_sessions`; `claude_roles`, :164-176; `claude_messages`,
  :183-212; the other jq sites on transcripts and rollouts: :117, :132,
  :145, :315; the pulse's cost label, :623); `scripts/dev/metrics.sh`
  (`claude_cost`, :221-241 before 05.1, rewritten; `supervisor_sessions`'
  jq, :257 before 05.1); `scripts/dev/plan-check.sh` (`check_plan`,
  plan:240 before 01.2); `scripts/dev/test-plan`; development.md :515
  (drop `PLAN_CLAUDE_PRICES`), 4.4 (:593-597, "does not read them yet"),
  5.2's Inputs (:737: they name each design document by its full path in
  the repository, as the metrics read them), 14.2's cost-so-far bullet
  (:2391-2393: "since the planning" becomes "since the design", as the
  count includes the design's sessions), 15.1's transcripts list (the
  designer's sessions), 15.2 `tokens_claude` (the designer's sessions,
  sub-agents of sub-agents; the part before the claim names "the design
  and the planning") and `cost_claude` (prices per model; the cache-read
  sentence says "at Opus's and Sonnet's rates").
Shape: `jsonl PROGRAM FILE...` runs `jq -R -r` over each line as
  `fromjson? | select(type == "object") | (PROGRAM)`, so a torn line is
  skipped and the rest read; all six jq sites on transcripts and
  rollouts use it. (A file's last line without a newline joins the next
  file's first, and both are skipped: a loss of two lines.)
  `claude_messages` takes the message's `.message.model`
  as column 8 (the first line with the id decides; the other columns stay,
  so `claude_tokens` is untouched) and skips `<synthetic>` messages.
  `claude_files SESSION` prints the transcripts of a session: `$s.jsonl`,
  `agent-${s#agent-}.jsonl` (the trailer may name a sub-agent either way),
  `*/$s/subagents/*.jsonl`, and the closure of the agents in that
  directory whose `agent-<id>.meta.json` `toolUseId` appears in a named
  agent's transcript (a sub-agent's sub-agents sit flat beside it, at
  `spawnDepth` 2; later.md). `designer_sessions` is in Interface
  sketches; `claude_roles` appends its rows after `claude.tsv`'s, so the
  roles stay planner, supervisor, designer. `claude_cost` keeps its
  output and adds `; unknown models, priced as Opus: <model> <messages>,
  ...` when one is not in the table. Its awk BEGIN block holds the prices
  per MTok (fresh, 5-minute write, 1-hour write, read, output), matched
  by model prefix so a dated name prices alike, under a comment naming
  the three sources and 2026-10-17: Opus 5.5 4, 5, 8, 0.20, 20; Sonnet 5.5
  2, 2.50, 4, 0.10, 10; Haiku 5.5 at most 100,000 prompt tokens (fresh +
  writes + reads of the message) 0.10, 0.125, 0.20, 0.01, 0.50, over
  100,000 0.50, 0.625, 1, 0.05, 2.50. Sources: platform.claude.com
  `/docs/en/about-claude/pricing`, `/docs/en/models/haiku-5-5/overview`,
  and anthropic.com `/claude-sonnet-5-5`. The pulse's cost label (:623)
  reads "since the design", as `claude_cost` counts the design's
  sessions. `check_plan` warns "the README's Inputs name no docs/design/
  or docs/foundation/ path: the metrics will count no design session"
  when `design_inputs` prints nothing: one function in numbers.sh, which
  `designer_sessions` calls too, so the rule is written once.
Test shape: three checks in test-plan, `expect` (:26) throughout.
  1. "a torn line": one line between the supervisor's messages
     (:195-198) and `boundary` (:200); the supervisor's "2 compactions"
     (:241) fails at the base, as the boundaries come after it, and
     passes after:
     ```
     printf '{"type":"assistant","message":{"id":"t","usage":{"input_tok\n' >>"$CLAUDE_CONFIG_DIR/projects/p/$session.jsonl"
     ```
  2. "the design's sessions". Before "Plan the demo" (:84-86): c1 changes
     `docs/design/thing.md` with `Designer-Session: $d_old`; c2 is
     `git commit --allow-empty` with `Planner-Session: $planner_old`; c3
     changes `docs/design/thing.md` with `Designer-Session: $designer` and
     `Designer-Session: agent-a1`; c4 changes `docs/design/other.md` with
     `Designer-Session: $other`. The README heredoc (:45-55) gains
     `## Inputs` and a line naming `` `docs/design/thing.md` ``. Transcripts,
     stamped 2020: `$designer.jsonl` holds d1 (100 in, 1 out);
     `$designer/subagents/agent-1.jsonl` d2 (40, 1); under a session no
     trailer names, `$host/subagents/agent-a1.jsonl` d3 (100, 1),
     `agent-a2.jsonl` d4 (100, 0) and `agent-a3.jsonl` d5 (1000, 9), each
     with a meta.json, a2's `toolUseId` in a1's text and a3's in none, so
     only the closure finds a2 and it leaves a3 out; `$d_old` and
     `$other` hold one small message each, which count not.
     The metrics expectation (:234-247) changes `tokens_claude` to
     `$'tokens_claude\t1960 in (5% cached), 23 out; in: 1840 before the claim, 120 in the run, 0 after'`;
     the pulse's "Claude Code 120/17" stays, and its expectation at :225
     reads "since the design". The demo README's Inputs leave `plan check`
     without the warning; the warning itself: at the first `plan check`
     (:309), `sed -i '/^## Inputs/,/^$/d' "$dir/README.md"` strips the
     Inputs (heading, line and the blank after), `PLAN_NO_GATE=1 "$plan"
     check "$dir" 2>&1` must hold `warn  the README's Inputs name no
     docs/design/ or docs/foundation/ path`, and `git -C "$repo" checkout
     -- "$dir/README.md"` restores it.
  3. "prices by model", one world, reference check:
     ```
     priced() { printf '{"type":"assistant","message":{"id":"%s","model":"%s","usage":{"input_tokens":%d,"cache_creation":{"ephemeral_5m_input_tokens":%d,"ephemeral_1h_input_tokens":%d},"cache_read_input_tokens":%d,"output_tokens":%d}}}\n' "$@"; }
     m=$t/claude-prices/projects/p; mkdir -p "$m"
     priced p1 claude-opus-5-5 1000000 1000000 1000000 10000000 1000000 >"$m/$planner.jsonl"
     { priced s1 claude-sonnet-5-5 2000000 2000000 0 10000000 1000000
       priced s2 claude-future-9 1000000 0 0 0 0
       priced s3 '<synthetic>' 0 0 0 0 0; } >"$m/$session.jsonl"
     awk 'BEGIN { for (i = 0; i < 1100; i++) printf "{\"type\":\"assistant\",\"message\":{\"id\":\"h%d\",\"model\":\"claude-haiku-5-5\",\"usage\":{\"input_tokens\":%d,\"output_tokens\":20000}}}\n", i, (i < 1000 ? 100000 : 100001) }' >"$m/$fork.jsonl"
     expect "prices by model" "$(CLAUDE_CONFIG_DIR=$t/claude-prices "$plan" metrics)" \
         $'cost_claude\t$93: planner $39, supervisor $24, designer $30; cache reads $3, cache writes $18, fresh input $27, output $45; unknown models, priced as Opus: claude-future-9 1'
     ```
     Ignoring the tier would give a designer of $22 or $110. In the main
     world the messages carry no model and cost Opus's: the call at :234
     drops `PLAN_CLAUDE_PRICES`, and its `cost_claude` expectation becomes
     `$'cost_claude\t$0: planner $0, supervisor $0, designer $0; cache reads $0, cache writes $0, fresh input $0, output $0'`.
Proof: process, `scripts/dev/heavy --tail 15 scripts/dev/test-plan`
  prints `test-plan: passed`, which runs the three checks;
  `git grep -n PLAN_CLAUDE_PRICES -- scripts docs/foundation` prints
  nothing.
Don't: widen `designer_sessions` beyond the plan's own repository
  (decisions.md, 12); keep `PLAN_CLAUDE_PRICES` as an override; read a
  sub-agent's meta.json for anything but `toolUseId`.
Size: about 104 lines.
Removes: `PLAN_CLAUDE_PRICES`; `jq` calls that die on a torn line.

### 05.3 `split`, planned and added, every Proof word
Scope: parity 15, reliability 17 (its rest), protocol 11; decisions.md,
  10 and 23; 5.5, 5.7, 7.3, 7.6, 14.2, 15.2 `steps_*`, `proofs` and
  `lines`.
Needs: 05.2 (they share fixtures and `jsonl`).
Files: `scripts/dev/plan` (state whitelist and its message, plan:566-567
  before 01.2: add `split`; `status`'s counts, plan:647-690, skip `split`
  rows); `scripts/dev/numbers.sh` (`layer_rank`, :38; `planned_layer`,
  :47; `proof_layer`, :64; the pulse's progress awk, :494-512, proofs
  block, :527-546, and line count, :574); `scripts/dev/metrics.sh`
  (`metrics_of`'s counts, :644, and proofs and lines, :727-750);
  `scripts/dev/test-plan`; development.md 7.3's states (add `split`:
  "replaced by its parts, named by letter, `<NN>.<M>a`, `b`, ...; counts
  in none") and its "Steps not in the plan" (:1495-1499: the coordinator,
  when a session splits its increment, sets the row `split` (note: the
  parts) and adds `<NN>.<M>a`, `b`, ... with the note "part of
  <NN>.<M>"; `.s<k>` stays for a step not in the plan); 10.4 (:1865) and
  2.5 (:219), which name unplanned steps `.s<k>`, only where their text
  would contradict that; 7.6's end (:1606) and 5.7's "When it starts"
  (:1248), which say a run ends, or its closing repin starts, once every
  step has landed (or been dropped): each says "landed, dropped or split
  (a `split` row is done once its parts are)", and no code changes for
  it, as the kit decides no end itself (the coordinator does, by `plan
  status`); 5.5's Proof line (:1071-1076) and 15.2 `proofs`: a layer is
  the first word up to its first hyphen ("process-tier" is process),
  each part after a semicolon counts, and a split increment is judged at
  its last part; 14.2's Proofs and Tests bullets (:2374-2375,
  :2380-2382), which say docs landings are not counted; 15.2 `steps_*`,
  `lines`.
Shape: `ledger_kinds` reads the ledger in two passes (`FNR == NR`
  collects the `split` rows) and prints each row but the `split` ones with
  a 12th column, `planned` or `added`: `.s<k>` is always added; a part
  (`^(.*[0-9])[a-z]$`) is planned when its parent row is `split`, else
  added; `-repin` and every other step is planned. The pulse awk and
  `metrics_of`'s count use `$12 == "added"`, not their own pattern.
  `proof_layers PROOF` prints one lower-case word per part: PROOF split at
  semicolons, each part's first word cut at the first of space, comma,
  colon, period or hyphen. `proof_layer` prints the highest-ranked of them
  (`layer_rank`), else the first word, else `-`. `planned_layer` prints
  the heading's Proof through `proof_layer` and reads a part's heading
  without its letter (`beta-02.1b` reads `### 02.1`). `gated_commits`
  prints `plan_commits` rows whose Step is not `-` and whose gate does
  not start `none`; `proofs` and `lines` read from it, so a docs landing
  adds nothing. The pulse reads it too, as `gated_commits "$since"`
  (passed on to `plan_commits`) into `gated`, for its Proofs block
  (:528-541) and its line count (:574), so a docs landing is no "no Proof
  line" there; `$commits` stays for its landings, last gate and code
  signals. `below_plan STEP PROOF` prints the increment's layer when
  STEP has no later lettered part in the ledger and PROOF's layer ranks
  below the increment's; the pulse and the metrics call it,
  and an earlier part is tallied, not judged. `proofs` prints the known
  layers in rank order, any other word in first-seen order, `none`, then
  `N below plan`.
Test shape: `commit` (:125) and `expect` (:26); two changes and two worlds.
  Alpha-01.1's Proof (:136) becomes "world, the story first, passed",
  as "the" would now count as a layer. Both worlds save the ledger and
  restore it, so "metrics after release" (:352) and the rest hold.
  1. "split steps". `cp "$run/ledger.tsv" "$t/ledger.save"; rm
     "$run/.pulse"`, then append 11-column rows (updated `$stamp`):
     `alpha-04.4` split; `alpha-04.4a`, `alpha-04.4b`, `alpha-04.5` and
     `alpha-01.s2` landed; `alpha-04.6` dropped. Expect, from `status`,
     "7 of 9 steps landed; 1 running"; from `pulse`, "- Progress: 5
     planned and 2 added steps landed since; in all 5 of 6 planned
     landed, 2 added (0 to go)"; from `metrics`, `steps_planned\t5\t`,
     `steps_added\t2\t`, `steps_dropped\t1\t`.
  2. "every Proof word". A `beta` repository made as alpha is (:42-44),
     whose `bcommit STEP PROOF GATE` is `commit` (:125) with
     `--allow-empty`, run with
     `DEV_REPOS="alpha beta" "$plan" metrics`: 01.1 "unit, the units;
     world, the worlds", 01.2 "process-tier, the machinery", 01.3
     "codecs, the wire; Live, one run", 01.4 "joint, together", 01.5 no
     Proof, 01.6 a docs landing of a 30-line `notes.md` with gate "none,
     Markdown only", 02.1a "unit, the first half", 02.1b "unit, the
     second half". A temporary `$dir/beta/02-parts.md` has `### 02.1` and
     `Proof: process, the whole`; the ledger gets landed rows `beta-02.1a`
     and `beta-02.1b`, so only 02.1b is judged. Expect `proofs\tunit
     4, world 2, joint 1, process 1, live 1, codecs 1, none 2, 2 below
     plan` (alpha-01.2 and beta-02.1b; 02.1a is unit, below, and not
     judged) and `data +1 -0` unchanged (the docs landing adds nothing).
     Then the pulse on this world, after `rm "$run/.pulse"` as in 1:
     `p=$(DEV_REPOS="alpha beta" "$plan" pulse | grep '^- Proofs:')` must
     hold `beta-01.5` (its "no Proof line") and, by `[[ $p != *beta-01.6*
     ]]`, not the docs landing.
Proof: process, `scripts/dev/heavy --tail 15 scripts/dev/test-plan`
  prints `test-plan: passed`, with both worlds.
Don't: judge an earlier part; count a `split` row anywhere; add a state
  beyond `split`; make the kit decide a run's end.
Size: about 71 lines.
Removes: `proof_layer`'s first-word rule; the steps pattern repeated in
  the pulse and the metrics.

### 05.4 Restarts, Codex usage records, pins
Scope: parity 15 (hand loads; pins from origin), protocol 11 (restarts
  with their whole cause; the coordinator's requests; `pin_lag` after
  fetching), client-composition 9 (hand loads); 15.2 `restarts`,
  `tokens_coordinator`, `tokens_implementers`, `coordinator_requests`,
  `pin_lag`.
Needs: 05.3. (04.1 writes the `Coordinator loaded` heading, decisions.md
  9; this increment's test writes its own.)
Files: `scripts/dev/numbers.sh` (`codex_tokens`, :112; `pin_lag`, :404;
  the pulse's call, :628); `scripts/dev/metrics.sh` (`restarts`, :685;
  the requests loop, :696; the call, :751); `scripts/dev/test-plan`;
  development.md 15.2 on the five rows above, its `coordinator_requests`
  row restated "the coordinator's model requests, from its usage
  records, and how many followed a wait that ran out" (the kit counts
  those after a `timed_out` wait, not those that only saw a wait end);
  14.2's pins bullet (:2394-2395: "behind the upstream mains, as of the
  last fetch (`plan metrics` fetches; the pulse does not)").
Shape: `restart_causes` prints `cause N, ...` sorted by cause. It counts
  `## Coordinator resumed, <time>: <cause>` by the whole text after `Z: `;
  each `## Coordinator loaded, ` heading in `log.md` as `loaded`; and
  each `## Loaded, ` in `supervision.md` (`docs/foundation/supervisor.md`
  :133) as `loaded`.
  `codex_tokens THREAD` reads, through `jsonl`, the last
  `token_usage_record`'s `payload.thread_token_usage` (input, output,
  cached input), else the last `total_token_usage` as before.
  `codex_requests FILE` counts `token_usage_record` lines, else
  `total_token_usage` lines. `thread_contexts` keeps `last_token_usage`.
  `pin_lag [fetch]` reads `git show origin/main:Cargo.lock` of each
  repository, not the checkout's; with `fetch`, `metrics_of` first runs
  `timeout 20 git fetch -q origin main` in each upstream, ignoring a
  failure.
Test shape: three worlds, `expect` (:26); each restores what it changes.
  1. "restarts". Save `log.md` and append `## Coordinator resumed,
     2026-10-11T12:20:00Z: the user's request: a pause` and `##
     Coordinator loaded, 2026-10-11T12:30:00Z: by plan ring`; write
     `$run/supervision.md` with two `## Loaded, 2026-10-11T12:40:00Z: by
     hand` entries. Expect `$'restarts\tloaded 3, reboot 1, the user\'s
     request: a pause 1\t'`; restore, `rm supervision.md`.
  2. "usage records". `CODEX_HOME=$t/codex-records`, thread `t-coord-1`
     (:157 names it): three `token_usage_record` events whose
     `thread_token_usage` grows to 3100 input, 1800 cached, 310 output,
     and one `token_count` event with 9999 input, to show the records
     win. Expect `$'tokens_coordinator\t3100 in (58% cached), 310 out'`
     and `$'coordinator_requests\t3, 0 after a wait ran out'`.
  3. "pin lag":
     ```
     s=$DEV_ROOT/skein; g=(git -c user.email=test@example.com -c user.name=test)
     c1=$("${g[@]}" -C "$s" commit-tree -p HEAD -m a 'HEAD^{tree}')
     c2=$("${g[@]}" -C "$s" commit-tree -p "$c1" -m b 'HEAD^{tree}')
     git -C "$s" update-ref refs/remotes/origin/main "$c2"
     git init -q -b main "$DEV_ROOT/smith"; w=$DEV_ROOT/smith
     lock() { printf 'source = "git+https://example.invalid/forgetic/skein.git#%s"\n' "$1" >"$w/Cargo.lock"; }
     lock "$(git -C "$s" rev-parse HEAD)"; "${g[@]}" -C "$w" add -A; "${g[@]}" -C "$w" commit -q -m lock
     git -C "$w" update-ref refs/remotes/origin/main HEAD; lock "$c2"
     expect "pin lag" "$("$plan" metrics)" $'pin_lag\tsmith: skein 2\t'
     rm -rf "$w"; git -C "$s" update-ref -d refs/remotes/origin/main
     ```
     The checkout's lock names the newest rev, so only the origin read
     gives 2. Afterwards `pin_lag\tnone` (:247) holds.
Proof: process, `scripts/dev/heavy --tail 15 scripts/dev/test-plan`
  prints `test-plan: passed`, with the three worlds.
Don't: write a `Coordinator loaded` heading (04.1 does); fetch from the
  pulse; read the Codex shape from anywhere but a rollout fixture.
Size: about 65 lines.
Removes: the metrics' count of hand loads as nothing; the checkout read.

### 05.5 The context limit and the disk line
Scope: parity 6, parity 7; 3.6, 11.4, 14.2; decisions.md, 25.
Needs: 05.4; skein 03.1 (`machine_setting`, for the disk floor).
Files: `scripts/dev/numbers.sh` (the pulse's Contexts line, :629, and
  Machine line, :632; new `context_limit`, `disk_rule`);
  `scripts/dev/plan` (`contexts`, plan:1325 before 01.2, and its comment,
  :1318-1320 "200,000 tokens"); `scripts/dev/test-plan`; development.md
  3.6 (:488, 150,000) and 14.2 (:2398, the machine bullet: heartbeat,
  memory, disk and the rung of 11.4's disk ladder it is on).
Shape: `context_limit` prints `${PLAN_CONTEXT_LIMIT:-150000}`; the pulse
  and `plan contexts` use it. `claude_autocompact` (plan:81) and
  `workspace/*` stay at 200,000. `disk_rule GB` prints 11.4's whole
  ladder: `no rule applies` at 80 or more; else `kache gc at this check`,
  then, each joined by `; `, `at most six sessions run` below 60, `starts
  held` below the disk floor (`read -r floor _ <<<"$(machine_setting
  PLAN_MIN_DISK_GB 40)"`: 40 unless the machine file or the environment
  says otherwise) and `the line stops` below 15. The Machine line
  (heartbeat and memory) drops its disk figure, and a new line follows
  it: `- Disk: N GB free; <rule>.`
Test shape: the expectations at :228 and :251 say "past 150K". Reference
  check, a `df` stub (03.3 and 04.3 make `$t/stub` too, so `mkdir -p`),
  the pulse's mark saved and restored (:229 reads it), the disk floor at
  its default, 40:
  ```
  mkdir -p "$t/stub"; printf '#!/bin/sh\necho Avail; echo "${DF_GB}G"\n' >"$t/stub/df"; chmod +x "$t/stub/df"
  cp "$run/.pulse" "$t/pulse.save"
  for gb in 90 70 50 35 10; do rm "$run/.pulse"; PATH=$t/stub:$PATH DF_GB=$gb "$plan" pulse >"$t/pulse.$gb"; done
  expect "disk, 90" "$(cat "$t/pulse.90")" "- Disk: 90 GB free; no rule applies."
  expect "disk, 70" "$(cat "$t/pulse.70")" "- Disk: 70 GB free; kache gc at this check."
  expect "disk, 50" "$(cat "$t/pulse.50")" "- Disk: 50 GB free; kache gc at this check; at most six sessions run."
  expect "disk, 35" "$(cat "$t/pulse.35")" "- Disk: 35 GB free; kache gc at this check; at most six sessions run; starts held."
  expect "disk, 10" "$(cat "$t/pulse.10")" "- Disk: 10 GB free; kache gc at this check; at most six sessions run; starts held; the line stops."
  cp "$t/pulse.save" "$run/.pulse"
  ```
  and the boundary: `PLAN_CONTEXT_LIMIT=260000 "$plan" contexts` must
  not contain "hand over" (`[[ $out != *"hand over"* ]]`; the peak is
  260K).
Proof: process, `scripts/dev/heavy --tail 15 scripts/dev/test-plan`
  prints `test-plan: passed`; `git grep -n 'PLAN_CONTEXT_LIMIT:-'
  -- scripts/dev ':!scripts/dev/test-plan'` prints only `context_limit`.
Don't: change `PLAN_CLAUDE_AUTOCOMPACT`; run `kache gc` from the pulse
  (it names the rule, as the coordinator acts on it).
Size: about 44 lines.
Removes: the 200,000 default of the context limit.

## Interface sketches

```
jsonl PROGRAM FILE...      # jq -R -r; torn lines and non-objects are skipped
claude_files SESSION       # a session's transcripts, sub-agents of sub-agents included
design_inputs README       # the docs/design/ and docs/foundation/ paths its Inputs name
designer_sessions          # "designer<TAB>id", the plan's repository, decision 12
claude_cost                # "$N: role $n, ...; cache reads ..., ...[; unknown models, ...]"
ledger_kinds               # ledger rows but split ones, plus a 12th column planned|added
proof_layers PROOF         # one lower-case layer word per semicolon part
proof_layer PROOF          # the highest-ranked layer, else the first word, else -
below_plan STEP PROOF      # the increment's layer when its last part ranks below it
gated_commits [SINCE]      # plan_commits rows with a Step and a gate that ran
restart_causes             # "cause N, ..." from log.md and supervision.md
codex_requests FILE        # a rollout's model requests
pin_lag [fetch]            # "repo: up N, ...; ..." or none
context_limit              # 150000 unless PLAN_CONTEXT_LIMIT
disk_rule GB               # the rule for the free disk
```

`designer_sessions`, the range being what goes wrong:

```
design_inputs() {
    awk '/^## /{inside=($0=="## Inputs")} inside{while(match($0,/`docs\/(design|foundation)\/[^`]*`/)){print substr($0,RSTART+1,RLENGTH-2); $0=substr($0,RSTART+RLENGTH)}}' "$1"
}

designer_sessions() {
    local top prefix inputs landing prev
    inputs=$(design_inputs "$plan_dir/README.md")
    [[ -n $inputs ]] || return 0          # no pathspec would mean every commit
    top=$(git -C "$plan_dir" rev-parse --show-toplevel) || return 0
    prefix=$(cd "$plan_dir" && git rev-parse --show-prefix)
    landing=$(git -C "$top" log -n1 --format=%H --grep='^Planner-Session:' main -- "${prefix%/}")
    [[ -n $landing ]] || return 0
    prev=$(git -C "$top" log -n1 --format=%H --grep='^Planner-Session:' "$landing^" 2>/dev/null || true)
    # shellcheck disable=SC2086 # one path per line, none with spaces
    git -C "$top" log "${prev:+$prev..}$landing" --format='%(trailers:key=Designer-Session,valueonly,separator=%x0A)' -- $inputs |
        awk 'NF { print "designer\t" $1 }'
}
```

`prev..landing` leaves out whatever `prev` reaches, so a design commit
older than the previous plan's landing does not count; a commit outside
the Inputs does not either, by the pathspec.

## Questions to raise when reached

None. The planner decided the five the draft held: `running_h` and the
layout of a sub-agent's sub-agents go to later.md; a split's parts are
lettered, a trailer may name a sub-agent either way, and
`docs/foundation/` counts as design (decisions.md, 12 and 23).

## Done when

- `scripts/dev/heavy --tail 15 scripts/dev/test-plan` prints
  `test-plan: passed`.
- `git ls-files scripts/dev ':!scripts/dev/workspace' | xargs wc -l | awk
  '$2 != "total" && $1 > 800'` prints nothing (after 01.2).
- `git grep -n PLAN_CLAUDE_PRICES -- scripts docs/foundation` prints
  nothing; `git grep -n 'PLAN_CONTEXT_LIMIT:-' -- scripts/dev
  ':!scripts/dev/test-plan'` prints one line, `context_limit`'s.
- `git grep -n 'does not read them yet' docs/foundation/development.md`
  prints nothing.

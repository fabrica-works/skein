# shellcheck shell=bash
# shellcheck source-path=SCRIPTDIR
# shellcheck disable=SC2154,SC2034 # test-plan owns these checks and fixtures
# Sourced by test-plan for its part "numbers": the queue, the pulse, the
# metrics, the contexts, the disk rungs and downstream.

# These commands share one queue and one row file; clients wait together.
heavy_run() (cd "$repo" || exit; HEAVY_TASK='' DEV_TMP=$t/heavy-tmp "$kit/heavy" "$@")
heavy_capture() {
    local label=$1
    shift
    capture status heavy_run "$@" >"$t/$label.out"
    head -n1 "$t/$label.out" >"$t/$label.status"
}
heavy_row() {
    awk -F'\t' -v command="$1" '$10 == command { n++; lane=$2; status=$6 }
        END { printf "rows=%d lane=%s status=%s", n, lane, status }' "$t/heavy-tmp/logs/heavy.tsv"
}
heavy_capture failure sh -c 'exit 3' &
failure_pid=$!
HEAVY_TASK='' DEV_TMP=$t/heavy-tmp "$kit/heavy" sleep 0.5 >"$t/client.log" 2>&1 &
client_pid=$!
running_client() {
    pueue status --json | jq -er '.tasks[] | select((.status | has("Running")) and (.command | contains("sleep 0.5"))) | .id'
}
task_id=$(poll_until 50 running_client)
kill "$client_pid"
wait "$client_pid" 2>/dev/null || true
heavy_capture gate --gate true &
gate_pid=$!
wait "$failure_pid" "$gate_pid"
expect "heavy passes the failed command's status" "$(cat "$t/failure.status")" 'status=3'
expect "heavy writes one failure row" "$(heavy_row 'sh -c exit 3')" 'rows=1 lane=build status=3'
expect "a killed client loses no row" "$(heavy_row 'sleep 0.5')" 'rows=1 lane=build status=0'
expect "the gate lane passes the status too" "$(cat "$t/gate.status")" 'status=0'
expect "the gate lane writes its row too" "$(heavy_row true)" 'rows=1 lane=gate status=0'

# The pulse, with the queue's statistics after these commands.
TZ=UTC0 pueue status --json >"$t/queue-stats.json"
printf '#!/bin/sh\necho Avail; echo "${DF_GB}G"\n' >"$t/stub/df"; chmod +x "$t/stub/df"
pulse=$(DF_GB=90 plan_numbers pulse)
rm "$t/stub/df"
expect "the pulse names its disk rung" "$pulse" "- Disk: 90 GB free; no rule applies."
expect pulse "$pulse" \
    "- Progress: 2 planned and 1 added steps landed since; in all 2 of 3 planned landed, 1 added (0 to go)" \
    "- Running: alpha-01.3 (w1)" \
    "below plan: alpha-01.2 (unit, planned joint)" \
    "no Proof line: alpha-01.s1" \
    "reports 1 lines median, 3 p90" "1 land gates for 4 landings, 1 other gate-lane tasks" "queue 20 s waiting, 200 s running" \
    "- Record: headings no shape names: 1 (the first: \"## Unblock handled, 2026-10-11T12:06:00Z\"); Kind values not in section 10.2: none; context.md lines past 200 characters: 1." \
    "code +65 -0, tests +6 -0, generated +1 -0, data +61 -0." \
    "last gate: scripts/check.sh passed: 13 tests / 1.1 s; 2 tests / 20.0 s; 30 s in all; within four fifths of the budgets" \
    "Landings past the aims: alpha-01.2 (focused 1.1 s past 1.05); alpha-01.s1 (focused 1.1 s past 1.05)." \
    "SLOW lines in gate logs: 2" \
    "over 50 lines: src/big.rs (62)" "new lint exceptions: 1" "forbidden words: actual 1, exact 1" \
    "types like a parallel one: OwnerThing" "test helpers in more than three files: text (5 files)" \
    "tokens in/out: coordinator 54000/5400, sessions 700/70, Claude Code 120/17" \
    "- Short waits so far, run out in less than 300 s: coordinator 1, sessions 1." \
    "Claude Code at list prices, since the design: \$0: planner \$0, supervisor \$0, designer \$0" \
    "1 steers in all, 1 points refused" \
    "- Rising words: line 0 to 60" "- Pins behind upstream main, in commits: none." \
    "- Contexts: t-impl-1 100K (past 100K: t-impl-1); Claude Code: supervisor 1K, designer 0K."
expect "a document is judged by no length" "$(if [[ $pulse != *docs/long.md* ]]; then echo absent; fi)" absent
expect "pulse mark" "$(cat "$run/.pulse")" " "
expect "pulse again" "$(plan_numbers pulse)" "- Progress: 0 planned and 0 added steps landed since" "no gate since"
sed -i.save '/^aims: alpha /d' "$dir/README.md"
rm "$run/.pulse"
expect "a repository without aims names no landing" "$(plan_numbers pulse)" "Landings past the aims: none."
sed -i '/^aims:/d' "$dir/README.md"
rm "$run/.pulse"
expect "no aims, no line" "$(if [[ $(plan_numbers pulse) != *'past the aims'* ]]; then echo absent; fi)" absent
mv "$dir/README.md.save" "$dir/README.md"

# The metrics.
metrics=$(plan_numbers metrics --out "$dir/metrics.tsv"; cat "$dir/metrics.tsv")
expect metrics "$metrics" \
    $'running_h\t0.5\th' $'steps_planned\t2\t' $'steps_added\t1\t' \
    $'escalations\tdesign gap 1, plan defect 1\t' $'escalation_wait_min\t60.0\t' $'blocked_h\t2.0\th' \
    $'answers_reversed\t1\t' $'steers\t1 sent, 1 points refused' $'restarts\tloaded 1, reboot 1\t' \
    $'tokens_coordinator\t54000 in (0% cached), 5400 out' $'tokens_implementers\t700 in (0% cached), 70 out' $'tokens_claude\t1960 in (5% cached), 23 out; in: 1840 before the claim, 120 in the run, 0 after' \
    $'cost_claude\t$0: planner $0, supervisor $0, designer $0; cache reads $0, cache writes $0, fresh input $0, output $0\t' \
    $'supervisor\t1 sessions, 0.0 h, 0.0M weighted tokens per hour, 100% in the 40 requests after each start or compaction, 2 compactions' \
    $'coordinator_requests\t4, 1 after a wait ran out' $'compactions\t1\t' \
    $'gates_per_step\t0.3\t' $'gate_lane_other\t1\t' $'queue_wait_ratio\t0.10\t' \
    $'suites\talpha first: scripts/check.sh passed: 10 tests / 1.0 s; 2 tests / 50.0 s; 60 s in all; last: scripts/check.sh passed: 13 tests' \
    $'proofs\tunit 1, world 1, none 1, 1 below plan' $'report_lines\t1 median, 3 p90' \
    "code +65 -0, tests +6 -0, generated +1 -0, data +61 -0"$'\tlines' \
    $'pin_lag\tnone\t' $'contexts\t260K median, 260K p90 of the sessions\' peaks; 0 handovers'
# Model pricing includes each Haiku prompt tier and names an unknown model.
priced() { printf '{"type":"assistant","message":{"id":"%s","model":"%s","usage":{"input_tokens":%d,"cache_creation":{"ephemeral_5m_input_tokens":%d,"ephemeral_1h_input_tokens":%d},"cache_read_input_tokens":%d,"output_tokens":%d}}}\n' "$@"; }
m=$t/claude-prices/projects/p; mkdir -p "$m"
priced p1 claude-opus-5-5 1000000 1000000 1000000 10000000 1000000 >"$m/$planner.jsonl"
{ priced s1 claude-sonnet-5-5 2000000 2000000 0 10000000 1000000
  priced s2 claude-future-9 1000000 0 0 0 0
  priced s3 '<synthetic>' 0 0 0 0 0; } >"$m/$session.jsonl"
awk 'BEGIN { for (i = 0; i < 1100; i++) printf "{\"type\":\"assistant\",\"message\":{\"id\":\"h%d\",\"model\":\"claude-haiku-5-5\",\"usage\":{\"input_tokens\":%d,\"output_tokens\":20000}}}\n", i, (i < 1000 ? 100000 : 100001) }' >"$m/$fork.jsonl"
expect history "$(plan_numbers metrics --history)" "| Metric | demo |" "| steps_planned | 2 |"

# The fixture above records 20 seconds waiting and 200 seconds running.
fixture_wait=20 fixture_run=200
held_id=$(pueue add --print-task-id --group gate --label measure -- true)
finished_task() {
    TZ=UTC0 pueue status --json | jq -er --arg id "$held_id" \
        '.tasks[$id].status.Done // empty | [.enqueued_at, .start, .end] | @tsv'
}
done_row=$(poll_until 50 finished_task)
IFS=$'\t' read -r queued_at started_at ended_at <<<"$done_row"
held_wait=$(($(date -d "$started_at" +%s) - $(date -d "$queued_at" +%s)))
held_run=$(($(date -d "$ended_at" +%s) - $(date -d "$started_at" +%s)))
rm "$run/.pulse"
TZ=UTC0 pueue status --json >"$t/queue-stats.json"
held_pulse=$(plan_numbers pulse)
expect "the pulse counts a task pueue still holds" "$held_pulse" \
    "queue $((fixture_wait + held_wait)) s waiting, $((fixture_run + held_run)) s running" \
    '2 other gate-lane tasks' 'pueue holds 1 finished tasks no client removed'
pueue remove "$task_id" >/dev/null
TZ=UTC0 pueue status --json >"$t/queue-stats.json"
# Usage records take precedence over token counts and deduplicate responses.
mkdir -p "$t/codex-records/sessions"
{
    printf '{"type":"session_meta","payload":{"id":"%s"}}\n' "$coordinator"
    while read -r response input cached output; do
        printf '{"type":"token_usage_record","payload":{"thread_id":"%s","response_id":"%s","thread_token_usage":{"input_tokens":%d,"cached_input_tokens":%d,"output_tokens":%d}}}\n' "$coordinator" "$response" "$input" "$cached" "$output"
    done <<'RECORDS'
r1 1000 500 100
r2 2100 1200 210
r2 2100 1200 210
r3 3100 1800 310
RECORDS
    printf '{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":9999}}}}\n'
} >"$t/codex-records/sessions/rollout-x-$coordinator.jsonl"
# Hand loads and multi-word restart causes keep their full meaning.
cp "$run/log.md" "$t/log.save"
cat >>"$run/log.md" <<'RESTARTS'
## Coordinator resumed, 2026-10-11T12:20:00Z: the user's request: a pause
## Coordinator loaded, 2026-10-11T12:30:00Z: by plan ring
RESTARTS
printf '## Loaded, 2026-10-11T12:40:00Z: by hand\n%.0s' 1 2 >"$run/supervision.md"

# The checkout's lock is ahead of origin's; pin lag reads origin's lock.
s=$DEV_ROOT/skein; g=(git -c user.email=test@example.com -c user.name=test)
c1=$("${g[@]}" -C "$s" commit-tree -p HEAD -m a 'HEAD^{tree}')
c2=$("${g[@]}" -C "$s" commit-tree -p "$c1" -m b 'HEAD^{tree}')
git -C "$s" update-ref refs/remotes/origin/main "$c2"
w=$DEV_ROOT/smith; mkdir -p "$w"; repository "$w"
lock() { printf 'source = "git+https://example.invalid/forgetic/skein.git#%s"\n' "$1" >"$w/Cargo.lock"; }
lock "$(git -C "$s" rev-parse HEAD)"; "${g[@]}" -C "$w" add -A; "${g[@]}" -C "$w" commit -q -m lock
git -C "$w" update-ref refs/remotes/origin/main HEAD; lock "$c2"

shared_metrics=$(CODEX_HOME=$t/codex-records CLAUDE_CONFIG_DIR=$t/claude-prices plan_numbers metrics)
expect "prices by model" "$shared_metrics" \
    $'cost_claude\t$93: planner $39, supervisor $24, designer $30; cache reads $3, cache writes $18, fresh input $27, output $45; unknown models, priced as Opus: claude-future-9 1; no transcript on this machine, counted as $0: designer design-current, designer agent-a1\t'
expect "restarts retain causes and hand loads" "$shared_metrics" $'restarts\tloaded 4, reboot 1, the user\'s request: a pause 1\t'
expect "usage records" "$shared_metrics" \
    $'tokens_coordinator\t3100 in (58% cached), 310 out' $'coordinator_requests\t3, 0 after a wait ran out'

expect "pin lag from origin" "$shared_metrics" $'pin_lag\tsmith: skein 2\t'
cp "$t/log.save" "$run/log.md"; rm "$run/supervision.md"
rm -rf "$w"; git -C "$s" update-ref -d refs/remotes/origin/main

# Split rows are replaced by planned parts; a supplement remains added.
cp "$run/ledger.tsv" "$t/ledger.save"
cp "$run/.pulse" "$t/pulse.save"
rm "$run/.pulse"
while read -r step state; do
    printf '%s\talpha\t04\t%s\t\t\t\t\t%s\t\t\n' "$step" "$state" "$stamp" >>"$run/ledger.tsv"
done <<'ROWS'
alpha-04.4 split
alpha-04.4a landed
alpha-04.4b landed
alpha-04.5 landed
alpha-01.s2 landed
alpha-04.6 dropped
ROWS
expect "split steps in status" "$("$plan" status)" "7 of 9 steps landed; 1 running"
expect "split steps in pulse" "$(plan_numbers pulse)" "- Progress: 5 planned and 2 added steps landed since; in all 5 of 6 planned landed, 2 added (0 to go)"
expect "split steps in metrics" "$(plan_numbers metrics)" $'steps_planned\t5\t' $'steps_added\t2\t' $'steps_dropped\t1\t'
cp "$t/ledger.save" "$run/ledger.tsv"

# Each Proof part counts, docs contribute no lines, and only the last split part is judged.
mkdir -p "$DEV_ROOT/beta" "$dir/beta"
repository "$DEV_ROOT/beta"
bcommit() { local repo=$DEV_ROOT/beta; commit "$@" --allow-empty; }
bcommit beta-01.1 "unit, the units; world, the worlds" passed
bcommit beta-01.2 "process-tier, the machinery" passed
bcommit beta-01.3 "codecs, the wire; Live, one run" passed
bcommit beta-01.4 "joint, together (in 101 s; first run found it)" passed
bcommit beta-01.5 "" passed
notes_lines=30
awk -v lines="$notes_lines" 'BEGIN { for (i=0; i<lines; i++) print "note" }' >"$DEV_ROOT/beta/notes.md"
bcommit beta-01.6 "" "none, Markdown only"
bcommit beta-02.1a "unit, the first half" passed
bcommit beta-02.1b "unit, the second half" passed
printf '### 02.1\nProof: process, the whole\n' >"$dir/beta/02-parts.md"
for part in a b; do printf 'beta-02.1%s\tbeta\t02\tlanded\t\t\t\t\t%s\t\t\n' "$part" "$stamp" >>"$run/ledger.tsv"; done
expect "every Proof word" "$(DEV_REPOS="alpha beta" plan_numbers metrics)" \
    $'proofs\tunit 4, world 2, joint 1, process 1, live 1, codecs 1, none 2, 2 below plan' 'data +61 -0'
rm "$run/.pulse"
p=$(DEV_REPOS="alpha beta" plan_numbers pulse | grep '^- Proofs:')
expect "a gated step without Proof is named" "$p" beta-01.5
expect "docs do not count as missing Proof" "$(if [[ $p != *beta-01.6* ]]; then echo absent; fi)" absent
cp "$t/ledger.save" "$run/ledger.tsv"
cp "$t/pulse.save" "$run/.pulse"
rm -rf "$DEV_ROOT/beta" "$dir/beta"

expect contexts "$("$plan" contexts)" "  100K, peak 260K  past 100K: hand over"
# The rollout fixture's peak is 260K: reaching the configured limit holds.
context_handover() { local out; out=$(PLAN_CONTEXT_LIMIT=$1 "$plan" contexts); if [[ $out == *"hand over"* ]]; then echo handover; else echo clear; fi; }
fixture_peak=260000
expect "the context limit holds at the peak" "$(context_handover "$fixture_peak")" clear
expect "one token past the limit hands over" "$(context_handover "$((fixture_peak - 1))")" handover

# Section 11.4's rungs hold at each bound and apply one GB below it.
# shellcheck source=kit.sh
source "$kit/kit.sh"
# shellcheck source=numbers.sh
source "$kit/numbers.sh"
export tmp=$DEV_TMP
while IFS='|' read -r gb want; do
    expect "disk ladder, gb=$gb" "<$(PLAN_MIN_DISK_GB=40 disk_rule "$gb")>" "<$want>"
done <<'DISK'
80|no rule applies
79|kache gc at this check
60|kache gc at this check
59|kache gc at this check; at most six sessions run
40|kache gc at this check; at most six sessions run
39|kache gc at this check; at most six sessions run; starts held
15|kache gc at this check; at most six sessions run; starts held
14|kache gc at this check; at most six sessions run; starts held; the line stops
DISK

# Nothing in the workspace pins alpha.
expect downstream "$("$plan" downstream)" "downstream: no repository pins alpha"

# A failed downstream probe removes its temporary worktree too.
beta=$DEV_ROOT/beta
mkdir -p "$beta" "$t/pueue-none"
git -C "$beta" init -q -b main
printf '[[package]]\nname = "alpha-package"\nsource = "git+https://example.com/alpha.git#main"\n' >"$beta/Cargo.lock"
git -C "$beta" add Cargo.lock
git -C "$beta" -c user.email=test@example.com -c user.name=test commit -qm "Pin alpha"
git clone -q --bare "$beta" "$t/beta.git"
git -C "$beta" remote add origin "$t/beta.git"
cat >"$t/pueue-none/pueue.yml" <<EOF
shared:
  pueue_directory: $t/pueue-none
  runtime_directory: $t/pueue-none
  use_unix_socket: true
  unix_socket_path: $t/pueue-none/pueue.socket
EOF
downstream=$(DEV_REPOS="alpha beta" PUEUE_CONFIG_PATH=$t/pueue-none/pueue.yml "$plan" downstream)
expect "downstream leaves no worktree" "$(printf '%s\n' "$downstream"; if [[ ! -e $run/downstream ]]; then printf 'worktrees=%s downstream=removed\n' "$(git -C "$beta" worktree list | wc -l)"; fi)" "beta against alpha main: fails" "worktrees=1 downstream=removed"

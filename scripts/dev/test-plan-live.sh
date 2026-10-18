# shellcheck shell=bash
# shellcheck disable=SC2154 # test-plan supplies its workspace, helpers and fixtures
# shellcheck disable=SC2034 # test-plan reads the shared failure flag after sourcing
# Live agent checks, sourced by test-plan only for --live.

# The longest live turn sleeps 70 s; allow two minutes, polling each tenth.
live_polls=1200 live_poll=0.1
wait_idle() {
    local poll
    for ((poll=0; poll<live_polls; poll++)); do
        [[ $(PLAN_NO_ROLES='' "$plan" status || true) != *"- coordinator: $thread, idle"* ]] || { echo idle=true; return; }
        sleep "$live_poll"
    done
    echo idle=false
}
live_text() { jq -r 'select(.type == "response_item" and .payload.type == "message" and .payload.role == "assistant") | .payload.content[]?.text // empty' "$file"; }
live_outputs() { jq -r 'select(.type == "response_item" and (.payload.type == "function_call_output" or .payload.type == "custom_tool_call_output")) | .payload.output' "$file"; }

# Real agents, on cheap settings, with the real Codex and Claude Code homes.
unset CODEX_HOME CLAUDE_CONFIG_DIR PLAN_CODEX_FAKE
export HOME=$machine_home
[[ -z $machine_runtime_dir ]] || export XDG_RUNTIME_DIR=$machine_runtime_dir
[[ -z $machine_bus_address ]] || export DBUS_SESSION_BUS_ADDRESS=$machine_bus_address
export PLAN_START_UNCHECKED=1 PLAN_CLAUDE_MODEL=${PLAN_TEST_CLAUDE_MODEL:-claude-haiku-5-5} PLAN_CLAUDE_EFFORT=low
export PLAN_CODEX_EFFORT=low PLAN_CODEX_SANDBOX=read-only
export PLAN_SUPERVISOR_PROMPT="This is a test of plan start. Reply OK and end your turn."
export PLAN_COORDINATOR_PROMPT='This is a test of plan start. First run the shell command sleep 6 and wait for it to complete, so the initial doorbell reaches you. After receiving Doorbell test, execute exactly this JavaScript in one exec cell:
// @exec: {"yield_time_ms": 1000}
text("codebase_tools=" + JSON.stringify(ALL_TOOLS.filter(t => t.name.includes("codebase_memory")).map(t => t.name)));
const task = await tools.exec_command({cmd:"sleep 70",yield_time_ms:30000});
if (task.session_id) text(await tools.write_stdin({session_id:task.session_id,chars:"",yield_time_ms:300000})); else text(task);
Then call wait exactly once for that cell with yield_time_ms: 300000. When it completes, reply with one line: every message you received after this one, quoted word for word, or none. End your turn.'
export PLAN_COORDINATOR_RESUME="This is a test of plan resume. Reply RESUMED and end your turn."
export PLAN_RETRO_PROMPT="This is a test of plan retro. Reply OK and end your turn."
rm -f "$run/thread" "$run/claude.tsv" "$run/done.md"
started=$(XDG_CONFIG_HOME=$t/config PATH=$t/stub:$PATH "$plan" start "$dir" 2>&1 || true)
expect start "$started" "supervisor: Claude Code session" "coordinator: Codex thread" "rang the coordinator: Doorbell test"
expect "start enables its boot unit" "$(cat "$boot" "$t/systemctl.log")" "ExecStart=$kit/plan resume" "--user enable plan-resume.service"
thread=$(cat "$run/thread" || true)
loaded=$(
    source "$kit/codex.sh"
    codex_open && codex_call thread/loaded/list '{}' && codex_close
)
expect "the real loaded list names the thread" "$(jq -r --arg t "$thread" '.data | index($t) != null' <<<"$loaded")" true
supervisor=$(awk -F'\t' '$1 == "supervisor" { print $2 }' "$run/claude.tsv" || true)
expect "attach running supervisor" "$(PLAN_DRY_RUN=1 "$plan" attach supervisor || true)" "claude attach ${supervisor:0:8}"
expect "the start turn finishes" "$(wait_idle)" 'idle=true'
file=$(find "$HOME/.codex/sessions" "$HOME/.codex/archived_sessions" -name "rollout-*-$thread.jsonl" 2>/dev/null | head -n1 || true)
expect "the coordinator heard the doorbell" "$(live_text || true)" "Doorbell test"
cell_wait=$(jq -sr '
    def output_text: if type == "array" then map(.text // "") | join("\n") else . end;
    def seconds: sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601;
    [.[] | select(.type == "response_item")] as $rows |
    ([$rows[] | select(.payload.type == "custom_tool_call" and .payload.name == "exec" and (.payload.input | contains("sleep 70")))] | first) as $sleep |
    ([$rows[] | select(.payload.call_id == $sleep.payload.call_id and .payload.type == "custom_tool_call_output") | .payload.output | output_text | capture("cell ID (?<id>[^\\n]+)").id] | first) as $cell |
    [$rows[] | select(.payload.type == "function_call" and .payload.name == "wait") | select((.payload.arguments | fromjson).cell_id == $cell)] as $waits |
    ([$rows[] | select(.payload.call_id == $waits[0].payload.call_id and .payload.type == "function_call_output")] | first) as $done |
    "waits=\($waits | length) yield_ms=\($waits[0].payload.arguments | fromjson | .yield_time_ms) completed=\($done.payload.output | output_text | startswith("Script completed")) elapsed=\((($done.timestamp | seconds) - ($sleep.timestamp | seconds)) >= 70)"
' "$file")
expect "the cell waits 300 s" "$cell_wait" 'waits=1 yield_ms=300000 completed=true elapsed=true'
expect "the profile disables graph tools" "$(live_outputs)" 'codebase_tools=[]'
expect resume "$("$plan" resume || true)" "coordinator: resumed"
expect "resume records its cause" "$(grep '^## Coordinator resumed, ' "$run/log.md" | tail -n1)" ": unknown"
expect "the resume turn finishes" "$(wait_idle)" 'idle=true'
expect "the coordinator resumed" "$(live_text || true)" "RESUMED"
# An idle coordinator ended its turn; a ring starts its next one.
"$plan" idle "This is a test of plan idle." >/dev/null
expect "idle ring" "$(PLAN_COORDINATOR_WAKE="This is a test of a ring to an idle coordinator. Reply RUNG and end your turn." "$plan" ring "Rang." || true)" "woke the idle coordinator: Rang."
expect "idle cleared" "$(ls "$run")" "ledger.tsv"
[[ ! -f $run/idle.md ]] || { echo "FAIL idle.md stayed after the ring"; failed=1; }
expect "the ring turn finishes" "$(wait_idle)" 'idle=true'
expect "the idle coordinator answered" "$(live_text || true)" "RUNG"
expect wake "$("$plan" wake supervisor "This is a test of plan wake. Reply WOKEN and end your turn." || true)" "supervisor: Claude Code session"
expect retro "$("$plan" retro "$dir" || true)" "retrospective: Claude Code session"
# Clean up: the coordinator's thread archived, the Claude Code sessions removed.
(
    source "$kit/codex.sh"
    codex_open && codex_call thread/archive "$(jq -cn --arg t "$thread" '{threadId: $t}' || true)" >/dev/null && codex_close
) || echo "could not archive thread $thread"
cut -f2 "$run/claude.tsv" | sort -u | while read -r session; do
    claude stop "${session:0:8}" >/dev/null 2>&1 || true
    claude rm "${session:0:8}" >/dev/null 2>&1 || true
done

# shellcheck shell=bash
# shellcheck disable=SC2154 # test-plan owns the fixture variables this suite reads
# shellcheck disable=SC2034 # test-plan reads the failure flag this suite writes
# Sourced by test-plan for the coordinator commands.
expect "context of a session" "$("$plan" context "$session")" 'context: 1K (compactions: 2): hand over now'
out=$(CLAUDE_CODE_SESSION_ID='' capture status "$plan" context)
expect "context needs a session" "$out" 'status=2' 'plan:' 'name a session'

# Coordinator headings and bodies are written by the kit, whole.
cat >"$run/blocked.md" <<'EOF'
# Blocked

## first, with a comma
First question.
## second heading
Second question.
## third heading
Third question.
EOF
cat >"$run/unblock.md" <<'EOF'
# Unblock

Answer to the blocked report of 2026-10-11T10:00:00Z (first, with a comma; second heading).
The answer.
EOF
cat "$run/blocked.md" "$run/unblock.md" >"$t/answered-body"
log_lines=$(wc -l <"$run/log.md")
"$plan" unblocked
archived=$(tail -n +"$((log_lines + 1))" "$run/log.md")
retained=$(cat "$run/blocked.md")
expected_retained=$'# Blocked\n\n## third heading\nThird question.'
expect "unblocked files the answer" "$(if [[ $archived == *"$(cat "$t/answered-body")"* && $retained == "$expected_retained" && ! -e $run/unblock.md ]]; then echo 'archived=whole retained=third answer=removed'; fi)" 'archived=whole retained=third answer=removed'
expect "unblocked names its heading" "$archived" '## Blocked and unblocked, '
rm "$run/blocked.md"
printf '# Steer\nThe instruction.\n' >"$run/steer.md"
"$plan" steered
expect "steered files steer.md" "$(if [[ ! -e $run/steer.md ]]; then tail -n 5 "$run/log.md"; fi)" '## Steered, ' '# Steer' 'The instruction.'
"$plan" handover alpha 01 151K
expect "handover logs a session's line" "$(tail -n1 "$run/log.md")" 'Handover, ' ': alpha 01 at 151K'

# All resume causes use the same completed fake turn and capture.
resume_case() {
    printf 'turn-1\tcompleted\n' >"$PLAN_CODEX_FAKE/turns/$coordinator"
    : >"$PLAN_CODEX_FAKE/calls"
    PLAN_DRY_RUN=1 capture status "$plan" resume "$@"
    grep '^## Coordinator resumed, ' "$run/log.md" | tail -n1
    grep '^turn/start ' "$PLAN_CODEX_FAKE/calls"
}
cp -p "$run/heartbeat" "$t/heartbeat.before-resume"
boot=$(date -d "$(uptime -s)" +%s)
touch -d "@$((boot - 1))" "$run/heartbeat"
expect "resume names a reboot" "$(resume_case)" 'status=0' ': reboot' 'turn/start ' 'Resume the run of'
touch "$run/heartbeat"
echo '# Paused' >"$run/paused.md"
expect "resume names the user's request" "$(resume_case)" 'status=0' ": the user's request" 'turn/start '
rm "$run/paused.md"
touch -d "@$((boot - 1))" "$run/heartbeat"
expect "resume takes a given cause" "$(resume_case crash)" 'status=0' ': crash' 'turn/start '
rm "$run/heartbeat"
expect "resume with no heartbeat" "$(resume_case)" 'status=0' ': unknown' 'turn/start '
mv "$t/heartbeat.before-resume" "$run/heartbeat"

# A fresh coordinator takes typed profile settings through the fake daemon.
mv "$run/thread" "$t/thread.before-profile"
[[ ! -f $run/threads ]] || mv "$run/threads" "$t/threads.before-profile"
PLAN_DRY_RUN=1 "$plan" resume >/dev/null
expect "the thread takes the profile" "$(jq -r '
    "compact_limit=\(.config.model_auto_compact_token_limit) graph_enabled=\(.config["mcp_servers.codebase-memory-mcp.enabled"])"
' "$PLAN_CODEX_FAKE/started")" 'compact_limit=200000 graph_enabled=false'
mv "$t/thread.before-profile" "$run/thread"
rm -f "$run/threads"
[[ ! -f $t/threads.before-profile ]] || mv "$t/threads.before-profile" "$run/threads"

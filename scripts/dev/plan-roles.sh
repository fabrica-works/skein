# shellcheck shell=bash
# shellcheck disable=SC2154 # plan sets the variables these functions read
# Sourced by plan, not run.

# The run's roles (development.md, section 2): the coordinator's Codex
# thread, in the run's thread file, and the Claude Code sessions, one line
# each in claude.tsv: role, session id, time recorded.

# Any Claude Code session's context and handover limit (development.md 2.1).
context() {
    local session=${1:-${CLAUDE_CODE_SESSION_ID:-}} tokens compactions suffix=""
    [[ -n $session ]] || die "name a session, or run plan context in a Claude Code session"
    read -r tokens _ <<<"$(claude_context "$session")"
    compactions=$(claude_compactions "$session")
    ((compactions < compaction_limit)) || suffix=": hand over now"
    echo "context: $((tokens / 1000))K (compactions: $compactions)$suffix"
}

# Every Claude Code session this machine knows, as JSON.
claude_sessions() { claude agents --json --all 2>/dev/null || echo '[]'; }

# The latest session id recorded for a role.
role_session() {
    [[ -f $run/claude.tsv ]] || return 0
    awk -F'\t' -v role="$1" '$1 == role { id = $2 } END { print id }' "$run/claude.tsv"
}

record_role() {
    printf '%s\t%s\t%s\n' "$1" "$2" "$(now)" >>"$run/claude.tsv"
}

# A Claude Code session's state: running, idle, or gone.
session_state() {
    claude_sessions | jq -r --arg s "$1" '
        [.[] | select(.sessionId == $s)] | last
        | if . == null then "gone" else (.status // .state // "running") end'
}

# Starts a background Claude Code session for a role, in the workspace's
# directory, and records it. With a session id, continues that session.
# The supervisor and the designer compact in place (development.md,
# section 10.5); the retrospective is short-lived.
claude_role() {
    local role=$1 prompt=$2 resume=${3:-} out id session
    if [[ ${PLAN_DRY_RUN:-} == 1 ]]; then
        if [[ -n $resume ]]; then
            echo "$role: would resume $resume: $prompt"
        else
            echo "$role: would start a fresh session: $prompt"
        fi
        return
    fi
    local args=(--bg -n "$plan $role" --model "$claude_model" --effort "$claude_effort"
        --dangerously-skip-permissions)
    [[ -n $resume ]] && args+=(--resume "$resume")
    [[ $role == retrospective ]] || args+=(--autocompact "$claude_autocompact")
    out=$(cd "$claude_dir" && claude "${args[@]}" "$prompt" 2>&1) ||
        die "claude did not start the $role: $out"
    id=$(grep -oE '\b[0-9a-f]{8}\b' <<<"$out" | head -n1 || true)
    session=$(claude_sessions | jq -r --arg id "$id" --arg name "$plan $role" '
        [.[] | select((.id == $id and $id != "") or .name == $name)] | last | .sessionId // empty')
    [[ -n $session ]] || session=$resume
    [[ -n $session ]] || die "cannot find the $role's session: $out"
    record_role "$role" "$session"
    echo "$role: Claude Code session $session"
}

# The coordinator's thread on the Codex daemon: loaded, with its turn's state.
coordinator_state() {
    local thread
    thread=$(cat "$run/thread" 2>/dev/null || true)
    [[ -n $thread ]] || { echo "none"; return; }
    (
        # shellcheck source=codex.sh
        source "$kit/codex.sh"
        codex_open >/dev/null 2>&1 || { echo "unreachable"; exit; }
        codex_call thread/read "$(jq -cn --arg t "$thread" '{threadId: $t, includeTurns: true}')" 2>/dev/null |
            jq -r '[.thread.turns[]? | .status] | last // "idle" | if . == "inProgress" then "working" else "idle" end' ||
            echo "not loaded"
        codex_close
    )
}

ring() {
    load
    local message=${1:?message} thread turn
    if [[ ${PLAN_DRY_RUN:-} == 1 ]]; then
        echo "would ring the coordinator: $message"
        return
    fi
    thread=$(cat "$run/thread" 2>/dev/null) || die "no coordinator thread in $run/thread"
    # shellcheck source=codex.sh
    source "$kit/codex.sh"
    codex_open || exit 1
    turn=$(codex_call thread/read "$(jq -cn --arg t "$thread" '{threadId: $t, includeTurns: true}')" |
        jq -r '[.thread.turns[]? | select(.status == "inProgress")] | last | .id // empty') ||
        { echo "plan: the coordinator's thread is not loaded: plan resume" >&2; exit 3; }
    if [[ -z $turn && -f $run/idle.md ]]; then
        # An idle coordinator ended its turn (plan idle): the ring starts the next.
        codex_call turn/start "$(jq -cn --arg t "$thread" --arg p "$(coordinator_wake_prompt "$message")" \
            '{threadId: $t, input: [{type: "text", text: $p}]}')" >/dev/null ||
            { echo "plan: the Codex daemon did not start the idle coordinator's turn" >&2; exit 3; }
        rm -f "$run/idle.md"
        codex_close
        echo "woke the idle coordinator: $message"
        return
    fi
    [[ -n $turn ]] || { echo "plan: the coordinator has no turn in progress: plan resume" >&2; exit 3; }
    codex_call turn/steer "$(jq -cn --arg t "$thread" --arg u "$turn" --arg m "$message" \
        '{threadId: $t, expectedTurnId: $u, input: [{type: "text", text: $m}]}')" >/dev/null
    codex_close
    echo "rang the coordinator: $message"
}

# The roles' prompts. Tests replace them through PLAN_*_PROMPT.
coordinator_prompt() {
    local text="Coordinate the run of $plan_dir, as skein's docs/foundation/development.md, section 7, says."
    echo "${PLAN_COORDINATOR_PROMPT:-$text}"
}

coordinator_resume_prompt() {
    local text="Resume the run of $plan_dir as skein's docs/foundation/development.md, section 7.6, says."
    [[ -z ${1:-} ]] || text+=" The cause, for your log.md heading: $1."
    echo "${PLAN_COORDINATOR_RESUME:-$text}"
}

# The turn a ring starts for an idle coordinator: the ring's message, then
# what to do with it.
coordinator_wake_prompt() {
    local text="You were idle (plan idle). Re-read context.md, run the wake-up command of skein's docs/foundation/development.md, section 7.4, and act on what it shows."
    echo "$1"
    echo
    echo "${PLAN_COORDINATOR_WAKE:-$text}"
}

start_coordinator() {
    local result thread
    # shellcheck source=codex.sh
    source "$kit/codex.sh"
    codex_open || die "the Codex daemon does not answer: codex app-server daemon start"
    result=$(codex_call thread/start "$(jq -cn --arg cwd "$root" --arg model "$codex_model" \
        --arg effort "$codex_effort" --argjson threads "${codex_threads:-8}" --arg sandbox "$codex_sandbox" \
        '{cwd: $cwd, model: $model, sandbox: $sandbox, approvalPolicy: "never",
          config: {model_reasoning_effort: $effort, agents: {max_threads: $threads}}}')") ||
        die "the Codex daemon did not start the coordinator's thread"
    thread=$(jq -r '.thread.id' <<<"$result")
    printf '%s\n' "$thread" >"$run/thread"
    printf '%s\t%s\n' "$thread" "$(now)" >>"$run/threads"
    codex_call thread/name/set "$(jq -cn --arg t "$thread" --arg n "$plan coordinator" \
        '{threadId: $t, name: $n}')" >/dev/null || true
    codex_call turn/start "$(jq -cn --arg t "$thread" --arg p "$(coordinator_prompt)" \
        '{threadId: $t, input: [{type: "text", text: $p}]}')" >/dev/null ||
        die "the Codex daemon did not start the coordinator's turn"
    codex_close
    echo "coordinator: Codex thread $thread"
}

supervisor_prompt() {
    local text="Supervise the run of $plan_dir as skein's docs/foundation/supervisor.md says. The run's directory is $run."
    echo "${PLAN_SUPERVISOR_PROMPT:-$text}"
}

supervisor_resume_prompt() {
    local text="Your session restarted. Start your watch and your hourly check again (skein's docs/foundation/supervisor.md, section 1), read the end of supervision.md, and go on supervising the run of $plan_dir."
    echo "${PLAN_SUPERVISOR_RESUME:-$text}"
}

supervisor_handover_prompt() {
    local text="Take over the supervision of the run of $plan_dir from a session that handed over, as skein's docs/foundation/supervisor.md, section 1, says for a successor. The run's directory is $run."
    echo "${PLAN_SUPERVISOR_HANDOVER:-$text}"
}

# A designer started fresh for one question, from a brief instead of the
# whole history of the session before it (development.md, section 10.4).
designer_fresh_prompt() {
    local text="You are the designer of the run of $plan_dir, started fresh for one question, as skein's docs/foundation/development.md, sections 2.1, 4.4, 4.5 and 10.4, say. The run's directory is $run. Read $run/question.md, the design sections and plan files it names, and the earlier questions and answers of this run that touch it (the \"Question and answer\" sections of $run/supervision.md); read no other file of the run."
    echo "${PLAN_DESIGNER_PROMPT:-$text}"
}

retro_prompt() {
    local text="Write the retrospective of the run of $plan_dir, as skein's docs/foundation/development.md, section 15, says. The run's directory is $run."
    echo "${PLAN_RETRO_PROMPT:-$text}"
}

# A boot unit belongs to the run whose DEV_TMP it records.
boot_unit() {
    local action=$1 unit=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/plan-resume.service
    if [[ $action == enable ]]; then
        mkdir -p "$(dirname "$unit")"
        printf '%s\n' '[Unit]' 'Description=Resume the development run' 'After=pueued.service' '' \
            '[Service]' 'Type=oneshot' "Environment=DEV_ROOT=$root DEV_TMP=$tmp" \
            "ExecStart=$kit/plan resume" '' '[Install]' 'WantedBy=default.target' >"$unit"
        systemctl --user daemon-reload
        systemctl --user enable plan-resume.service
    elif [[ -f $unit ]]; then
        if awk -v want="DEV_TMP=$tmp" '/^Environment=/ { for (i = 1; i <= NF; i++) if ($i == want) found = 1 }
            END { exit !found }' "$unit"; then
            systemctl --user disable plan-resume.service
            rm "$unit"
            systemctl --user daemon-reload
        else
            echo "plan: warning: $unit names another DEV_TMP; left in place" >&2
        fi
    fi
}

start() {
    local dir=${1:?plan directory}
    if [[ ${PLAN_START_UNCHECKED:-} != 1 ]]; then
        check "$dir" || die "the machine is not ready: fix what check says (plan check --fix $dir), then start again"
    fi
    claim "$dir" >/dev/null
    load
    [[ ! -s $run/thread ]] || die "the run has a coordinator already: plan resume"
    claude_role supervisor "$(supervisor_prompt)"
    start_coordinator
    boot_unit enable
    sleep "${PLAN_RING_DELAY:-5}"
    ring "Doorbell test: log it as development.md, section 7.4, says, and go on." ||
        echo "plan: the doorbell did not ring; steers wait for the coordinator's next wake-up" >&2
    [[ ! -d $run/kit ]] || echo "kit: skein $(git -C "$run/kit" rev-parse --short HEAD), the claim's"
    echo "started $name: plan status, plan attach supervisor, plan attach coordinator"
}

attach() {
    load
    local role=${1:?role: coordinator, supervisor, designer or retrospective} session line
    if [[ $role == coordinator ]]; then
        line=(codex resume --remote unix:// "$(cat "$run/thread")")
    else
        session=$(role_session "$role")
        [[ -n $session ]] || die "no $role recorded in $run/claude.tsv"
        if [[ $(session_state "$session") == gone ]]; then
            line=(claude --resume "$session")
        else
            line=(claude attach "${session:0:8}")
        fi
    fi
    if [[ ${PLAN_DRY_RUN:-} == 1 ]]; then
        echo "${line[*]}"
        return
    fi
    exec "${line[@]}"
}

# After a restart, or a role's crash: start what is not running again. The
# cause, when the caller knows it (reboot, crash, the user's request), goes
# to the coordinator for its log.
resume() {
    load
    local cause=${1:-}
    [[ ! -f $run/done.md ]] || die "the run is done"
    [[ ! -d $run/kit ]] || echo "kit: skein $(git -C "$run/kit" rev-parse --short HEAD), the claim's"
    pueue status >/dev/null 2>&1 ||
        { systemctl --user start pueued 2>/dev/null || pueued --daemonize >/dev/null 2>&1 || true; }
    codex app-server daemon version >/dev/null 2>&1 || codex_daemon_start || true

    local session state thread
    session=$(role_session supervisor)
    state=$(session_state "${session:-none}")
    if [[ -z $session ]]; then
        claude_role supervisor "$(supervisor_prompt)"
    elif [[ $state == gone || $state == exited || $state == stopped || $state == failed ]]; then
        # A reboot leaves the session "failed": its process died with the machine.
        claude_role supervisor "$(supervisor_resume_prompt)" "$session"
    else
        echo "supervisor: running ($state)"
    fi

    thread=$(cat "$run/thread" 2>/dev/null || true)
    if [[ -z $thread ]]; then
        start_coordinator
        return
    fi
    # shellcheck source=codex.sh
    source "$kit/codex.sh"
    codex_open || die "the Codex daemon does not answer: codex app-server daemon start"
    # After a restart the daemon no longer holds the thread, and thread/read
    # answers an error: load it, rather than read its last turn as "none".
    local read
    if read=$(codex_call thread/read "$(jq -cn --arg t "$thread" '{threadId: $t, includeTurns: true}')" 2>/dev/null); then
        state=$(jq -r '[.thread.turns[]? | .status] | last // "none"' <<<"$read")
    else
        state="not loaded"
    fi
    if [[ $state == "not loaded" ]]; then
        codex_call thread/resume "$(jq -cn --arg t "$thread" --arg sandbox "$codex_sandbox" \
            '{threadId: $t, sandbox: $sandbox, approvalPolicy: "never"}')" >/dev/null ||
            die "the Codex daemon cannot load the coordinator's thread $thread"
    fi
    if [[ $state == inProgress ]]; then
        echo "coordinator: working"
    else
        codex_call turn/start "$(jq -cn --arg t "$thread" --arg p "$(coordinator_resume_prompt "$cause")" \
            '{threadId: $t, input: [{type: "text", text: $p}]}')" >/dev/null ||
            die "the Codex daemon did not start the coordinator's turn"
        echo "coordinator: resumed, Codex thread $thread"
    fi
    codex_close
}

join() {
    load
    local role=${1:?role}
    [[ -n ${CLAUDE_CODE_SESSION_ID:-} ]] || die "run plan join from the Claude Code session itself"
    record_role "$role" "$CLAUDE_CODE_SESSION_ID"
    echo "$role: Claude Code session $CLAUDE_CODE_SESSION_ID"
    [[ $role != designer ]] ||
        echo "End your turn now, and start no watch: plan wake brings you back with each question (development.md, section 10.4)."
}

# Wakes a role's Claude Code session in the background with a message
# (development.md, section 10.4). A session at work is never woken: a
# second wake would fork a copy of it. A designer compacted to its limit,
# or whose session has been idle longer than its cache lives, starts fresh
# from a brief; any other session is resumed.
wake() {
    load
    local role=${1:?role} message=${2:?message} session at idle_for compactions
    session=$(role_session "$role")
    [[ -n $session ]] || die "no $role recorded in $run/claude.tsv (plan join $role)"
    if [[ $(session_state "$session") == busy ]]; then
        echo "$role: at work in session $session; not woken. Send it a message (SendMessage) naming the file to read." >&2
        return 3
    fi
    if [[ $role == designer ]]; then
        read -r _ at <<<"$(claude_context "$session")"
        idle_for=$(($(date +%s) - at))
        compactions=$(claude_compactions "$session")
        if ((compactions >= compaction_limit || (at > 0 && idle_for > designer_idle))); then
            echo "designer: compactions $compactions, idle $((idle_for / 60)) min: a fresh session"
            claude_role designer "$(designer_fresh_prompt) Then: $message"
            return
        fi
    fi
    claude_role "$role" "$message" "$session"
}

# The supervisor hands over to a fresh session (development.md, section
# 10.5), from its own session, once its watch and cron job are stopped and
# its handover is in supervision.md.
handover() {
    load
    local role=${1:?role: supervisor} old
    [[ $role == supervisor ]] || die "only the supervisor hands over; plan wake starts a fresh designer"
    old=$(role_session supervisor)
    [[ -z ${CLAUDE_CODE_SESSION_ID:-} || $CLAUDE_CODE_SESSION_ID == "$old" ]] ||
        die "this session is not the run's supervisor; $old is"
    printf '## Handed over, %s\n\nFrom session %s.\n\n' "$(now)" "${old:-none}" >>"$run/supervision.md"
    [[ -z $old ]] || printf '%s\n' "$old" >>"$run/handed-over"
    claude_role supervisor "$(supervisor_handover_prompt)"
    echo "This session supervises no more: with its watch and its cron job stopped, end your turn."
}

retro() {
    if (($#)); then
        locate "$1"
    else
        last_run
    fi
    [[ -d $run ]] || die "no run directory at $run"
    claude_role retrospective "$(retro_prompt)"
}

roles() {
    local session thread
    echo
    echo "## Roles"
    echo
    thread=$(cat "$run/thread" 2>/dev/null || true)
    echo "- coordinator: ${thread:-not started}, $(coordinator_state); heartbeat $(($(date +%s) - $(stat -c %Y "$run/heartbeat" 2>/dev/null || date +%s))) s ago"
    local role
    for role in supervisor designer retrospective; do
        session=$(role_session "$role")
        [[ -n $session ]] && echo "- $role: $session, $(session_state "$session")"
    done
    local file
    for file in blocked unblock steer question answer paused "done"; do
        [[ -f $run/$file.md ]] && echo "- $file.md: $(grep -c '^## ' "$run/$file.md" || true) headings, since $(date -u -d "@$(stat -c %Y "$run/$file.md")" +%FT%TZ)"
    done
    if [[ -f $run/supervision.md ]]; then
        echo "- last check: $(grep '^## ' "$run/supervision.md" | tail -n1 | sed 's/^## //')"
    fi
    return 0
}

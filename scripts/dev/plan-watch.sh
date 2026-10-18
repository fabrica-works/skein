# shellcheck shell=bash
# shellcheck disable=SC2154 # plan sets the variables these functions read
# Sourced by plan, not run.

# The coordinator, with nothing running and nothing to do, ends its turn
# instead of sleeping (development.md, section 7.4): idle.md says since when
# and why, the watch does not take its silence for a fault, and the next
# ring starts its next turn.
idle() {
    load
    local reason=${1:?reason} at
    at=$(now)
    printf '%s\n%s\n' "$at" "$reason" >"$run/idle.md"
    printf '## Idle, %s\n\n%s\n\n' "$at" "$reason" >>"$run/log.md"
    echo "idle since $at: end your turn now; the doorbell starts your next one"
}

# Wait until the supervisor has something to do (development.md, section
# 10.5): a new blocked report, the designer's answer, steps to check before
# they start, the coordinator gone quiet (once, and again after as long)
# unless it is idle, a pause or the end. Prints what, then the supervisor's
# context, and exits. A session that has handed over is told so and shown
# nothing. The designer has no watch: plan wake brings it back.
watch() {
    load
    local seen=$run/.watch
    touch "$seen"
    while :; do
        handed_over && return 4
        if watch_once "$seen"; then
            supervisor_context
            return 0
        fi
        sleep "${PLAN_POLL:-30}"
    done
}

# One look for an event not yet shown: prints it and succeeds, or fails.
watch_once() {
    local seen=$1 quiet=${PLAN_QUIET:-600} sig beat age step repo session file shown=0
    if [[ -f $run/done.md ]] && ! grep -qx "done" "$seen"; then
        echo "done" >>"$seen"
        echo "done: $run/done.md"
        return 0
    fi
    if [[ -f $run/paused.md ]]; then
        sig=paused:$(stat -c %Y "$run/paused.md")
        if ! grep -qxF "$sig" "$seen"; then
            echo "$sig" >>"$seen"
            echo "paused: $run/paused.md"
            return 0
        fi
    fi
    if [[ -f $run/blocked.md && ! -f $run/unblock.md ]]; then
        sig=blocked:$(grep '^## ' "$run/blocked.md" | sha1sum | cut -c1-12)
        if ! grep -qxF "$sig" "$seen"; then
            echo "$sig" >>"$seen"
            echo "blocked: $run/blocked.md"
            grep '^## ' "$run/blocked.md" || true
            return 0
        fi
    fi
    if [[ -f $run/answer.md ]]; then
        sig=answer:$(stat -c %Y "$run/answer.md")
        if ! grep -qxF "$sig" "$seen"; then
            echo "$sig" >>"$seen"
            echo "answer: $run/answer.md"
            return 0
        fi
    fi
    # Steps the coordinator holds for the supervisor's check before they
    # start (development.md, section 7.2), each shown once, all at once.
    while IFS=$'\t' read -r step repo session; do
        sig=check:$step
        ! grep -qxF "$sig" "$seen" || continue
        echo "$sig" >>"$seen"
        file=$(find "$plan_dir/$repo" -maxdepth 1 -name "$session-*.md" 2>/dev/null | sort | head -n1)
        echo "check: $step${file:+, $file}"
        shown=1
    done < <(awk -F'\t' 'NR > 1 && $4 == "check" { print $1 "\t" $2 "\t" $3 }' "$run/ledger.tsv" 2>/dev/null)
    ((!shown)) || return 0
    if [[ -f $run/heartbeat && ! -f $run/paused.md && ! -f $run/done.md && ! -f $run/idle.md ]]; then
        beat=$(stat -c %Y "$run/heartbeat")
        age=$(($(date +%s) - beat))
        for sig in quiet2:$beat quiet:$beat; do
            if [[ $sig == quiet2:* ]] && { ((age <= 2 * quiet)) || ! grep -qxF "quiet:$beat" "$seen"; }; then
                continue
            fi
            if ((age > quiet)) && ! grep -qxF "$sig" "$seen"; then
                echo "quiet:$beat" >>"$seen"
                echo "$sig" >>"$seen"
                if [[ $sig == quiet2:* ]]; then
                    echo "quiet again: no heartbeat from the coordinator since $(date -u -d "@$beat" +%TZ)"
                else
                    echo "quiet: no heartbeat from the coordinator since $(date -u -d "@$beat" +%TZ)"
                fi
                return 0
            fi
        done
    fi
    return 1
}

# True, and says so, when this Claude Code session is a supervisor that
# handed over (development.md, section 10.5): plan handover names it in
# the run's handed-over file. A resumed session, which Claude Code may give
# a new id, is not one.
handed_over() {
    local me=${CLAUDE_CODE_SESSION_ID:-}
    [[ -n $me ]] && grep -qxF "$me" "$run/handed-over" 2>/dev/null || return 1
    echo "handed over: session $(role_session supervisor) supervises the run now. End your turn, and start no watch, check or cron job."
}

# The supervisor's context, the input of its last request, with its
# compactions, and at their limit the word to hand over (development.md,
# section 10.5).
supervisor_context() {
    local session
    session=$(role_session supervisor)
    [[ -n $session ]] || return 0
    context "$session" | sed "s/^context: /context: the supervisor's is /; s/: hand over now$/: hand over now (supervisor.md, section 7)/"
}

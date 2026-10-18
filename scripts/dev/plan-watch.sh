# shellcheck shell=bash
# shellcheck disable=SC2154 # plan sets the variables these functions read
# Sourced by plan, not run.

# The coordinator, with nothing running and nothing to do, ends its turn
# instead of sleeping (development.md, section 7.4): idle.md says since when
# and why, the watch does not take its silence for a fault, and the next
# ring starts its next turn.
idle() {
    load
    local reason=${1:?reason} at tasks=''
    shift
    if (($#)); then
        [[ $1 == --tasks ]] || die "usage: plan idle REASON [--tasks ID…]"
        shift
        (($#)) || die "--tasks needs queue task ids"
        tasks=$*
    fi
    at=$(now)
    printf '%s\n%s\n' "$at" "$reason" >"$run/idle.md"
    [[ -z $tasks ]] || printf 'tasks: %s\n' "$tasks" >>"$run/idle.md"
    printf '## Idle, %s\n\n%s\n\n' "$at" "$reason" >>"$run/log.md"
    echo "idle since $at: end your turn now; the doorbell starts your next one"
}

# Queue completions wake the coordinator without ending the supervisor's watch.
wake_idle_tasks() {
    local tasks queue=$1 message
    [[ -f $run/idle.md && ! -f $run/paused.md && ! -f $run/done.md ]] || return 0
    tasks=$(sed -n '3s/^tasks: //p' "$run/idle.md")
    [[ -n $tasks ]] || return 0
    jq -e --arg ids "$tasks" '
        [.tasks as $tasks | ($ids | split(" ")[]) as $id
         | $tasks[$id].status? | objects | keys[0]
         | select(. == "Queued" or . == "Running" or . == "Paused")]
        | length == 0' <<<"$queue" >/dev/null || return 0
    message="From the supervisor, by plan watch: the queue tasks you wait on have ended: $tasks"
    coordinator_turn watch "$message" "$(coordinator_wake_prompt "$message")" >/dev/null || return 0
    rm -f "$run/idle.md"
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

# Each event carries at most 150 material lines, with its source for the rest.
watch_lines() {
    awk -v file="$1" 'NR <= 150 { print "  " $0 }
        END { if (NR > 150) printf "  … (%d more lines: %s)\n", NR-150, file }'
}

# A step's ledger row and session path; a block also needs the increment.
watch_step() {
    local step=$1 detail=${2:-} row repo session file report
    row=$(awk -F'\t' -v s="$step" '$1 == s' "$run/ledger.tsv")
    IFS=$'\t' read -r _ repo session _ <<<"$row"
    [[ -n $repo && -n $session ]] || return 0
    printf 'ledger: %s\n' "$row"
    file=$(find "$plan_dir/$repo" -maxdepth 1 -name "$session-*.md" 2>/dev/null | sort | head -n1)
    printf 'session: %s\n' "$file"
    [[ -n $detail && -n $file ]] || return 0
    awk -v s="${step#*-}" '/^### / { show = ($2 == s) } show' "$file"
    report=$(find "$run/reports" -maxdepth 1 -name "$repo-$session-*.md" 2>/dev/null | sort | tail -n1)
    [[ -z $report ]] || printf 'report: %s\n' "$report"
}

watch_blocked() {
    local seen=$1 heading sig
    local entries=()
    [[ -f $run/blocked.md && ! -f $run/unblock.md ]] || return 1
    while IFS= read -r heading; do
        sig=blocked:$heading
        grep -qxF "$sig" "$seen" && continue
        echo "$sig" >>"$seen"
        entries+=("$heading")
    done < <(grep '^## ' "$run/blocked.md" || true)
    ((${#entries[@]})) || return 1
    echo "blocked: $run/blocked.md"
    for heading in "${entries[@]}"; do
        awk -v h="$heading" '/^## / { show = ($0 == h) } show' "$run/blocked.md"
        if [[ $heading =~ ^##\ ([^[:space:]]+)\ ([0-9]+\.[^:[:space:]]+) ]]; then
            watch_step "${BASH_REMATCH[1]}-${BASH_REMATCH[2]}" increment
        fi
    done | watch_lines "$run/blocked.md"
}

# Ready notes retain the checked commits even if the running note changes.
watch_started() {
    local seen=$1 step state note sig repo commit shown=0
    while IFS=$'\t' read -r step state note; do
        if [[ $state == ready && $note == *'checked '* ]]; then
            sig=ready:$step:$note
            grep -qxF "$sig" "$seen" || echo "$sig" >>"$seen"
            continue
        fi
        [[ $state == running ]] || continue
        [[ $note == *'checked '* ]] || note=$(awk -v p="ready:$step:" 'index($0,p)==1 { n=substr($0,length(p)+1) } END { print n }' "$seen")
        [[ $note == *'checked '* ]] || continue
        sig=started:$step
        grep -qxF "$sig" "$seen" && continue
        echo "$sig" >>"$seen"
        echo "started: $step"
        while read -r repo commit; do
            [[ -e $root/$repo/.git ]] || continue
            echo "$repo since $commit:"
            if git -C "$root/$repo" fetch --quiet origin main; then
                git -C "$root/$repo" diff --stat "$commit" origin/main
            else
                echo "fetch failed: $repo"
            fi
        done < <(awk '{ gsub(/[,;]/," "); for (i=1;i<NF;i++) if ($(i+1) ~ /^[0-9a-f]+$/ && length($(i+1)) >= 7) print $i,$(i+1) }' <<<"${note#*checked }") | watch_lines "$run/ledger.tsv"
        shown=1
    done < <(awk -F'\t' 'NR > 1 && ($4 == "ready" || $4 == "running") { print $1 "\t" $4 "\t" $11 }' "$run/ledger.tsv")
    ((shown))
}

# The hourly heading and queue outage persist across successive watches.
watch_health() {
    local seen=$1 stamp at queue shown=0
    stamp=$(awk '/^## Check, / { check=$3 } /^## Process, / && !process { process=$3 }
        END { print (check ? check : process) }' "$run/supervision.md" 2>/dev/null || true)
    if [[ -n $stamp ]] && at=$(date -d "${stamp%:}" +%s 2>/dev/null) &&
        (($(date +%s) - at > 3900)) && ! grep -qxF "hourly:$stamp" "$seen"; then
        echo "hourly:$stamp" >>"$seen"
        echo "hourly: $run/supervision.md since ${stamp%:}"
        shown=1
    fi
    if queue=$(pueue status --json 2>/dev/null); then
        rm -f "$run/.pueued-down"
        wake_idle_tasks "$queue"
    elif [[ ! -f $run/.pueued-down ]]; then
        touch "$run/.pueued-down"
        echo "alert: pueued is down"
        printf '%s pueued is down\n' "$(now)" >>"$run/alerts.md"
        shown=1
    fi
    ((shown))
}

# One look for an event not yet shown: prints it and succeeds, or fails.
watch_once() {
    local seen=$1 quiet=${PLAN_QUIET:-600} sig beat age step repo session file shown=0
    if [[ -f $run/done.md ]] && ! grep -qx "done" "$seen"; then
        echo "done" >>"$seen"
        echo "done: $run/done.md"
        watch_lines "$run/done.md" <"$run/done.md"
        shown=1
    fi
    if [[ -f $run/paused.md ]]; then
        sig=paused:$(stat -c %Y "$run/paused.md")
        if ! grep -qxF "$sig" "$seen"; then
            echo "$sig" >>"$seen"
            echo "paused: $run/paused.md"
            watch_lines "$run/paused.md" <"$run/paused.md"
            shown=1
        fi
    fi
    if watch_blocked "$seen"; then shown=1; fi
    if [[ -f $run/answer.md ]]; then
        sig=answer:$(stat -c %Y "$run/answer.md")
        if ! grep -qxF "$sig" "$seen"; then
            echo "$sig" >>"$seen"
            echo "answer: $run/answer.md"
            watch_lines "$run/answer.md" <"$run/answer.md"
            shown=1
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
        watch_step "$step" | watch_lines "$run/ledger.tsv"
        shown=1
    done < <(awk -F'\t' 'NR > 1 && $4 == "check" { print $1 "\t" $2 "\t" $3 }' "$run/ledger.tsv" 2>/dev/null)
    if watch_started "$seen"; then shown=1; fi
    ((!shown)) || return 0
    if watch_health "$seen"; then return 0; fi
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
                { printf 'coordinator: %s\n' "$(coordinator_state)"; tail -n 20 "$run/log.md"; } | watch_lines "$run/log.md"
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
    echo "handed over: session $(role_session supervisor) supervises the run now. End your turn, and start no watch or check."
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

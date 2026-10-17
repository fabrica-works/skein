# shellcheck shell=bash
# shellcheck disable=SC2154 # plan sets root, repos, name, plan, plan_dir and run
# The run's numbers: `plan pulse`, what changed since the supervisor's last
# check (development.md, section 14.2), and `plan metrics`, the run's
# metrics (section 15.2). Sourced by plan, not run.
#
# They read the run's directory, the plan's commits on each repository's
# main (their Plan, Step, Proof and Gate-Result trailers), the queue's
# statistics (logs/heavy.tsv), the coordinator's and its sessions' Codex
# rollouts, and the Claude Code sessions' transcripts.

codex_home=${CODEX_HOME:-$HOME/.codex}
claude_home=${CLAUDE_CONFIG_DIR:-$HOME/.claude}

# The repositories the plan may change: the workspace's, and the plan's own.
plan_repos() {
    # shellcheck disable=SC2086 # the repositories, one word each
    printf '%s\n' $repos "${name%%/*}" | awk '!seen[$0]++'
}

# The plan's commits on main, oldest first: repository, hash, commit time,
# Step, Proof, Gate-Result, with "-" for a missing trailer (read collapses
# empty fields between tabs). With an argument, only those made after that
# second.
plan_commits() {
    local since=${1:-} repo
    [[ $since != 0 && -n $since ]] && since=$((since + 1)) || since=
    for repo in $(plan_repos); do
        [[ -d $root/$repo/.git ]] || continue
        git -C "$root/$repo" log main --reverse ${since:+--since="@$since"} --format="%H%x09%ct%x09%(trailers:key=Plan,valueonly,separator=)%x09%(trailers:key=Step,valueonly,separator=)%x09%(trailers:key=Proof,valueonly,separator=)%x09%(trailers:key=Gate-Result,valueonly,separator=)" 2>/dev/null |
            awk -F'\t' -v OFS='\t' -v plan="$plan" -v repo="$repo" '$3 == plan {
                for (i = 4; i <= 6; i++) if ($i == "") $i = "-"
                print repo, $1, $2, $4, $5, $6 }' || true
    done | sort -s -t$'\t' -k3,3n
}

# A layer's rank (development.md, section 5.5); 0 when unknown.
layer_rank() {
    case ${1,,} in
    unit) echo 1 ;; world) echo 2 ;; joint) echo 3 ;; process) echo 4 ;; live) echo 5 ;; *) echo 0 ;;
    esac
}

# The layer a step's increment names in its session file's Proof line. Its
# heading may write the session as the step does ("### 06.1") or without
# the zero ("### 6.1").
planned_layer() {
    local step=$1 repo session item file
    repo=${step%-*}
    session=${step##*-}
    item=${session#*.}
    session=${session%%.*}
    [[ $session =~ ^[0-9]+$ ]] || return 0
    for file in "$plan_dir/$repo/$session"-*.md; do
        [[ -f $file ]] || continue
        awk -v id="$((10#$session)).$item" -v raw="$session.$item" '
            /^### / { inside = ($2 == id || $2 == raw) }
            inside && /^Proof:/ { sub(/^Proof: */, ""); sub(/[ ,.;:].*/, ""); print; exit }' "$file"
        return
    done
}

# A Proof line's layer: its first word, without punctuation.
proof_layer() {
    local layer=${1%%[ ,;:.]*}
    echo "${layer:--}"
}

# Codex's rollout directories that exist.
rollout_dirs() {
    local dir
    for dir in "$codex_home/sessions" "$codex_home/archived_sessions"; do
        [[ -d $dir ]] && echo "$dir"
    done
    return 0
}

# A thread's Codex rollout files.
rollout_of() {
    local dirs
    mapfile -t dirs < <(rollout_dirs)
    ((${#dirs[@]})) || return 0
    find "${dirs[@]}" -name "rollout-*-$1.jsonl" 2>/dev/null || true
}

# The coordinator's threads (the thread file and the history plan keeps),
# then every thread spawned under them, at any depth.
run_threads() {
    local parents children file
    parents=$( { cat "$run/thread" "$run/threads" 2>/dev/null || true; } | awk 'NF { print $1 }' | sort -u)
    [[ -n $parents ]] || return 0
    printf '%s\n' "$parents" | sed 's/^/coordinator\t/'
    local all
    local dirs
    mapfile -t dirs < <(rollout_dirs)
    ((${#dirs[@]})) || return 0
    all=$({ find "${dirs[@]}" -name 'rollout-*.jsonl' -newermt "@$(run_started)" 2>/dev/null || true; } |
        while read -r file; do
            head -n1 "$file" | jq -r '[.payload.id, (.payload.parent_thread_id // .payload.source.subagent.thread_spawn.parent_thread_id // "")] | @tsv' 2>/dev/null || true
        done)
    children=$parents
    while [[ -n $children ]]; do
        children=$(LIST=$children awk -F'\t' '
            BEGIN { n = split(ENVIRON["LIST"], p, "\n"); for (i = 1; i <= n; i++) want[p[i]] = 1 }
            $2 in want { print $1 }' <<<"$all" | sort -u)
        [[ -z $children ]] || printf '%s\n' "$children" | sed 's/^/session\t/'
    done
}

# A Codex thread's tokens, from its last token count: input, output, and
# the part of the input read from the cache.
codex_tokens() {
    local file
    file=$(rollout_of "$1" | head -n1)
    [[ -n $file ]] || { echo "0 0 0"; return; }
    grep '"total_token_usage"' "$file" | tail -n1 |
        jq -r '.payload.info.total_token_usage | "\(.input_tokens // 0) \(.output_tokens // 0) \(.cached_input_tokens // 0)"' 2>/dev/null || echo "0 0 0"
}

# Each Codex thread's context, the input of its last model request, and
# the largest it reached: kind, slot (the agent's path), thread, last,
# peak. The coordinator's threads first, then the sessions'.
thread_contexts() {
    local kind thread file slot sizes
    while IFS=$'\t' read -r kind thread; do
        [[ -n $thread ]] || continue
        file=$(rollout_of "$thread" | head -n1)
        [[ -n $file ]] || continue
        slot=coordinator
        [[ $kind == coordinator ]] ||
            slot=$(head -n1 "$file" | jq -r --arg t "${thread:0:13}" '.payload.agent_path // .payload.source.subagent.thread_spawn.agent_path // $t' 2>/dev/null || true)
        sizes=$(grep '"last_token_usage"' "$file" | jq -r '.payload.info.last_token_usage.input_tokens // 0' 2>/dev/null |
            awk '{ last = $1; if ($1 > peak) peak = $1 } END { printf "%d\t%d", last, peak }' || true)
        printf '%s\t%s\t%s\t%s\n' "$kind" "${slot:-?}" "$thread" "${sizes:-0	0}"
    done < <(run_threads)
}

# A Claude Code session's context, from its transcript: the input of its
# last request, and when that request was made (seconds since the epoch);
# "0 0" when it has none.
claude_context() {
    local file last
    file=$(find "$claude_home/projects" -maxdepth 2 -name "$1.jsonl" 2>/dev/null | head -n1)
    [[ -n $file ]] || { echo "0 0"; return; }
    last=$(tac "$file" | head -n 400 | jq -r 'select(.type == "assistant" and .message.usage != null and .message.model != "<synthetic>")
        | [(.message.usage | (.input_tokens // 0) + (.cache_creation_input_tokens // 0) + (.cache_read_input_tokens // 0)),
           ((.timestamp // "") | if . == "" then 0 else (sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601) end)]
        | "\(.[0]) \(.[1])"' 2>/dev/null | head -n1 || true)
    echo "${last:-0 0}"
}

# How many times a Claude Code session's context was compacted, from its
# transcript's compaction boundaries (development.md, section 10.5).
claude_compactions() {
    local file
    file=$(find "$claude_home/projects" -maxdepth 2 -name "$1.jsonl" 2>/dev/null | head -n1)
    [[ -n $file ]] || { echo 0; return; }
    grep -c '"subtype":"compact_boundary"' "$file" || true
}

# The run's Claude Code sessions, role and session id, each once: the
# planner's first, named by the Planner-Session trailers of the plan's
# commits (development.md, section 5.4), then claude.tsv's, in its order.
claude_roles() {
    {
        local top prefix
        if top=$(git -C "$plan_dir" rev-parse --show-toplevel 2>/dev/null); then
            prefix=$(cd "$plan_dir" && git rev-parse --show-prefix)
            git -C "$top" log main --format='%(trailers:key=Planner-Session,valueonly,separator=%x0A)' -- "${prefix%/}" 2>/dev/null |
                awk 'NF { print "planner\t" $1 }'
        fi
        awk -F'\t' 'NF >= 2 { print $1 "\t" $2 }' "$run/claude.tsv" 2>/dev/null || true
    } | awk -F'\t' '!seen[$2]++'
}

# The Claude Code sessions' messages, their sub-agents' included, each
# message counted once across every session: a resumed or forked session's
# transcript copies the messages before it, which count for the first role
# that has them. One line each: role, part (1 before the claim, 2 from the
# claim to done.md, 3 after; a message without a time counts in the run),
# fresh input, 5-minute cache writes, 1-hour cache writes, cache reads,
# output.
claude_messages() {
    local role session files
    while IFS=$'\t' read -r role session; do
        [[ -n $session ]] || continue
        files=$(find "$claude_home/projects" \( -name "$session.jsonl" -o -path "*/$session/subagents/*.jsonl" \) 2>/dev/null || true)
        [[ -n $files ]] || continue
        # shellcheck disable=SC2086 # one path per line, none with spaces
        jq -r --arg role "$role" 'select(.type == "assistant" and .message.usage != null) | .message as $m | $m.usage as $u
            | ($u.cache_creation // {}) as $c
            | (if ($c.ephemeral_1h_input_tokens // null) == null and ($c.ephemeral_5m_input_tokens // null) == null
               then ($u.cache_creation_input_tokens // 0) else ($c.ephemeral_1h_input_tokens // 0) end) as $hour
            | [$role, $m.id, ($u.input_tokens // 0), ($c.ephemeral_5m_input_tokens // 0), $hour,
               ($u.cache_read_input_tokens // 0), ($u.output_tokens // 0),
               ((.timestamp // "") | if . == "" then "" else (sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601 | tostring) end)]
            | @tsv' $files 2>/dev/null || true
    done < <(claude_roles) |
        awk -F'\t' -v OFS='\t' -v start="$(run_started)" -v end="$(run_ended)" '
            !($2 in role) { role[$2] = $1; order[++n] = $2 }
            { for (i = 3; i <= 7; i++) if ($i + 0 > v[$2, i] + 0) v[$2, i] = $i
              if ($8 != "" && !($2 in t)) t[$2] = $8 }
            END {
                for (j = 1; j <= n; j++) {
                    k = order[j]
                    part = (!(k in t)) ? 2 : (t[k] < start) ? 1 : (t[k] <= end) ? 2 : 3
                    print role[k], part, v[k, 3] + 0, v[k, 4] + 0, v[k, 5] + 0, v[k, 6] + 0, v[k, 7] + 0
                }
            }'
}

# The Claude Code sessions' tokens: input (fresh, cache writes and cache
# reads), output, and the cache reads, for each part (development.md,
# section 15.2). Nine numbers: three for each part.
claude_tokens() {
    claude_messages | awk -F'\t' '
        { a[$2] += $3 + $4 + $5 + $6; b[$2] += $7; d[$2] += $6 }
        END { printf "%d %d %d %d %d %d %d %d %d\n", a[1], b[1], d[1], a[2], b[2], d[2], a[3], b[3], d[3] }'
}

# What the Claude Code sessions' tokens cost at list prices (development.md,
# section 15.2): in all, by role, and by kind. PLAN_CLAUDE_PRICES gives the
# dollars per million tokens of fresh input, 5-minute cache writes, 1-hour
# cache writes, cache reads and output; Opus 5.5's by default. A cache read
# costs a fortieth of a 1-hour write, so token counts alone hide most of
# what a long context costs.
claude_cost() {
    claude_messages | awk -F'\t' -v prices="${PLAN_CLAUDE_PRICES:-4 5 8 0.20 20}" '
        BEGIN { split(prices, p, " ") }
        {
            if (!($1 in by)) roles[++n] = $1
            c = ($3 * p[1] + $4 * p[2] + $5 * p[3] + $6 * p[4] + $7 * p[5]) / 1e6
            by[$1] += c; total += c
            fresh += $3 * p[1] / 1e6; writes += ($4 * p[2] + $5 * p[3]) / 1e6
            reads += $6 * p[4] / 1e6; out += $7 * p[5] / 1e6
        }
        END {
            printf "$%.0f", total
            for (i = 1; i <= n; i++) printf "%s%s $%.0f", (i == 1 ? ": " : ", "), roles[i], by[roles[i]]
            printf "; cache reads $%.0f, cache writes $%.0f, fresh input $%.0f, output $%.0f\n", reads, writes, fresh, out
        }'
}

# The supervisor's sessions (development.md, sections 10.5 and 15.2): how
# many, their hours, each from its first request to its last, their
# weighted tokens per hour, the share of those in the 40 requests after
# each session's start or compaction (its bootstrap, or its re-read), and
# their compactions. Weights: fresh input 1,
# cache reads 0.1, cache writes 1.25, output 5. Sub-agents are left out,
# and a message counts once, for the first session that has it.
supervisor_sessions() {
    local session file
    awk -F'\t' '$1 == "supervisor" && !seen[$2]++ { print $2 }' "$run/claude.tsv" 2>/dev/null |
        while read -r session; do
            file=$(find "$claude_home/projects" -maxdepth 2 -name "$session.jsonl" 2>/dev/null | head -n1)
            [[ -n $file ]] || continue
            jq -r --arg s "$session" '
                if .type == "system" and .subtype == "compact_boundary" then [$s, "compaction", (.uuid // .timestamp // "")]
                elif .type == "assistant" and .message.usage != null and .message.model != "<synthetic>" then
                    .message.usage as $u
                    | [$s, "request", .message.id,
                       ((.timestamp // "") | if . == "" then "" else (sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601) end),
                       ($u.input_tokens // 0) + 0.1 * ($u.cache_read_input_tokens // 0)
                       + 1.25 * ($u.cache_creation_input_tokens // 0) + 5 * ($u.output_tokens // 0)]
                else empty end | @tsv' "$file" 2>/dev/null || true
        done | awk -F'\t' '
            $2 == "compaction" { if (!($3 in boundary)) { boundary[$3]; compactions++; since[$1] = 0 } next }
            !($3 in owner) { owner[$3] = $1; early[$3] = ++since[$1] <= 40; keys[++n] = $3 }
            owner[$3] != $1 { next }
            { if ($5 > w[$3]) w[$3] = $5
              if (!($1 in requests_of)) { requests_of[$1]; sessions++ }
              if ($4 != "" && (!($1 in t0) || $4 < t0[$1])) t0[$1] = $4
              if ($4 != "" && $4 > t1[$1]) t1[$1] = $4 }
            END {
                for (i = 1; i <= n; i++) {
                    k = keys[i]; total += w[k]
                    if (early[k]) boot += w[k]
                }
                for (s in t0) seconds += t1[s] - t0[s]
                hours = seconds / 3600
                printf "%d sessions, %.1f h, %.1fM weighted tokens per hour, %d%% in the 40 requests after each start or compaction, %d compactions\n",
                    sessions, hours, (hours > 0 ? total / hours / 1e6 : 0), (total > 0 ? 100 * boot / total : 0), compactions
            }'
}

# When the run started: its claim, from log.md.
run_started() {
    local at
    at=$(sed -nE 's/^## Claimed, (.*)$/\1/p' "$run/log.md" 2>/dev/null | head -n1)
    date -u -d "${at:-now}" +%s
}

run_ended() {
    if [[ -f $run/done.md ]]; then stat -c %Y "$run/done.md"; else date +%s; fi
}

# Seconds the coordinator worked: the span of its rollouts' events, less
# every gap of more than ten minutes.
running_seconds() {
    local files thread
    files=$(for thread in $( { cat "$run/thread" "$run/threads" 2>/dev/null || true; } | awk 'NF { print $1 }' | sort -u); do
        rollout_of "$thread"
    done)
    if [[ -z $files ]]; then
        echo $(($(run_ended) - $(run_started)))
        return
    fi
    # shellcheck disable=SC2086 # one path per line, none with spaces
    jq -r '.timestamp // empty | sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601' $files 2>/dev/null | sort -n |
        awk -v gap="${PLAN_GAP:-600}" 'NR > 1 { d = $1 - last; if (d <= gap) total += d } { last = $1 } END { print total + 0 }'
}

# The plan's README settings for the pulse (development.md, section 5.2):
# lines `words: …`, `types: …`, `file-lines: N`, `budgets: FOCUSED FUZZY`.
pulse_setting() {
    sed -nE "s/^ *$1: *(.*[^ ]) *$/\\1/p" "$plan_dir/README.md" 2>/dev/null | head -n1
}

is_test_path() {
    [[ $1 =~ (^|/)(tests?|testing)(/|$) || $1 =~ tests?\.rs$ || $1 =~ _tests?\.rs$ ]]
}

# The reports' lengths in log.md, in lines that are not blank: sections
# headed `## <repository> <NN>, <time>` (development.md, section 7.3), from
# line $1 on.
report_lengths() {
    awk -v from="${1:-1}" '
        NR < from { next }
        /^## / {
            if (inside) print n
            inside = ($0 ~ /^## [A-Za-z0-9_-]+ [0-9]+, /); n = 0; next
        }
        inside && NF { n++ }
        END { if (inside) print n }' "$run/log.md" 2>/dev/null
}

median_and_p90() {
    sort -n | awk '{ v[NR] = $1 } END {
        if (!NR) { print "0 0"; exit }
        m = v[int((NR + 1) / 2)]; p = v[int(NR * 0.9 + 0.999)]; if (p == "") p = v[NR]
        print m, p }'
}

# Lines added and removed in code and in tests, by the given commits
# (repository and hash per line): code+ code- tests+ tests-.
line_counts() {
    local repo hash added removed path ca=0 cr=0 ta=0 tr=0
    while IFS=$'\t' read -r repo hash; do
        while IFS=$'\t' read -r added removed path; do
            [[ $added == - ]] && continue
            if is_test_path "$path"; then
                ta=$((ta + added)); tr=$((tr + removed))
            else
                ca=$((ca + added)); cr=$((cr + removed))
            fi
        done < <(git -C "$root/$repo" show --format= --numstat "$hash")
    done
    echo "$ca $cr $ta $tr"
}

# The words of a set of files at a revision, one per line, lower case:
# identifiers split at underscores, digits and case changes; words of four
# letters or more. Reads repository, revision, then paths on stdin.
words_at() {
    local repo=$1 rev=$2 path
    while read -r path; do
        git -C "$root/$repo" show "$rev:$path" 2>/dev/null || true
    done | sed -E 's/([a-z])([A-Z])/\1 \2/g' | grep -oE '[A-Za-z]{4,}' | tr '[:upper:]' '[:lower:]'
}

# The words that rose most in the files the plan's commits changed, from
# before its first commit to main (development.md, section 14.2): a word
# the plan forbids can move to a synonym the list does not name. At most
# eight, each up by ten or more and by half or more.
rising_words() {
    local commits repo first
    commits=$(plan_commits 0)
    for repo in $(cut -f1 <<<"$commits" | sort -u); do
        [[ -n $repo ]] || continue
        first=$(awk -F'\t' -v r="$repo" '$1 == r { print $2; exit }' <<<"$commits")
        local paths
        paths=$(awk -F'\t' -v r="$repo" '$1 == r { print $2 }' <<<"$commits" |
            while read -r hash; do git -C "$root/$repo" show --format= --name-only "$hash"; done | sort -u)
        { words_at "$repo" "$first^" <<<"$paths" | sed 's/^/0 /'; words_at "$repo" main <<<"$paths" | sed 's/^/1 /'; }
    done | awk '{ n[$2, $1]++; w[$2] = 1 }
        END {
            for (k in w) { b = n[k, 0] + 0; a = n[k, 1] + 0; if (a - b >= 10 && a >= 1.5 * b) print a - b, k, b, a }
        }' | sort -k1,1nr -k2,2 | head -n 8 | awk '{ printf "%s%s %d to %d", (NR > 1 ? ", " : ""), $2, $3, $4 }
        END { if (!NR) printf "none" }'
}

# How far each repository's pins are behind the upstream mains, in
# commits (development.md, section 5.7): "jig: skein 145, smith 338". A
# repository that pins two revisions of one upstream counts the older.
pin_lag() {
    local repo up rev lag most out="" line
    for repo in $all_repos; do
        [[ -f $root/$repo/Cargo.lock ]] || continue
        line=""
        for up in $all_repos; do
            [[ $up != "$repo" ]] || break
            most=""
            for rev in $(sed -nE "s|^source = \"git\+[^\"]*/$up\.git#([0-9a-f]+)\"$|\1|p" "$root/$repo/Cargo.lock" | sort -u); do
                lag=$(git -C "$root/$up" rev-list --count "$rev..origin/main" 2>/dev/null) || lag="?"
                if [[ $lag == "?" || -z $most ]] || { [[ $most != "?" ]] && ((lag > most)); }; then most=$lag; fi
            done
            [[ -z $most ]] || line+="${line:+, }$up $most"
        done
        [[ -z $line ]] || out+="${out:+; }$repo: $line"
    done
    echo "${out:-none}"
}

# Gate-lane tasks since a time (all of them, without one): land's gates,
# then the others (measurements, probes, live runs, plan check's gate), by
# the label in heavy.tsv's eleventh column. A queue whose lines carry no
# label counts every gate-lane task as a gate.
land_gates() {
    awk -F'\t' -v s="${1:-}" '
        $2 == "gate" && (s == "" || $3 >= s) { g++; if (NF >= 11) { lab = 1; if ($11 ~ /^land /) l++ } }
        END { if (!lab) l = g; print l + 0, g - l + 0 }' "$run/logs/heavy.tsv" 2>/dev/null || echo "0 0"
}

# Each long-lived Claude Code role's context: its latest session's, the
# input of its last request (development.md, section 10.5).
claude_role_contexts() {
    local role session tokens out=""
    for role in supervisor designer; do
        session=$(role_session "$role")
        [[ -n $session ]] || continue
        read -r tokens _ <<<"$(claude_context "$session")"
        out+="${out:+, }$role $((tokens / 1000))K"
    done
    echo "${out:-none}"
}

pulse() {
    load
    ledger_upgrade
    local mark=$run/.pulse since from now_s
    now_s=$(date +%s)
    if [[ -f $mark ]]; then
        read -r since from <"$mark"
    else
        since=$(($(run_started) - 1))
        from=1
    fi
    # Everything counted happened after the mark's second.
    local after
    after=$(date -u -d "@$((since + 1))" +%FT%TZ)
    local hours
    hours=$(awk -v s="$since" -v n="$now_s" 'BEGIN { printf "%.1f", (n - s) / 3600 }')
    echo "# Pulse of $name, $(now), since $after ($hours h)"
    echo

    # Progress.
    local ledger=$run/ledger.tsv
    awk -F'\t' -v since="$since" -v three="$((now_s - 10800))" -v now="$now_s" '
        function epoch(t,   c) { c = "date -u -d \"" t "\" +%s"; c | getline e; close(c); return e }
        NR == 1 { next }
        {
            added = ($1 ~ /\.s[0-9]+$/)
            if ($4 == "landed") {
                if (added) la++; else lp++
                t = epoch($9)
                if (t > since) { if (added) na++; else np++ }
                if (t >= three && !added) n3++
            } else if ($4 != "dropped") { if (added) ra++; else rp++ }
            if ($4 == "running") run[++r] = $1 " (" $5 ") " sprintf("%.1f", (now - epoch($9)) / 3600)
        }
        END {
            printf "- Progress: %d planned and %d added steps landed since; in all %d of %d planned landed, %d added (%d to go); %d planned in the last 3 h.\n",
                np, na, lp, lp + rp, la, ra, n3
            line = ""
            for (i = 1; i <= r; i++) { split(run[i], f, " "); flag = (f[3] > 2) ? "!" : ""; line = line (i > 1 ? "; " : "") f[1] " " f[2] " " f[3] " h" flag }
            printf "- Running: %s\n", (r ? line : "none")
        }' "$ledger"

    # The channel.
    local file line=""
    for file in blocked question steer unblock answer; do
        [[ -f $run/$file.md ]] || continue
        line+="$file.md $(grep -c '^## ' "$run/$file.md" || true) headings, $(((now_s - $(stat -c %Y "$run/$file.md")) / 60)) min old; "
    done
    line+=$(awk -F'\t' -v now="$now_s" '
        function epoch(t,   c, e) { c = "date -u -d \"" t "\" +%s"; c | getline e; close(c); return e }
        NR > 1 && $4 == "check" { s = s (n++ ? ", " : "") $1 " " int((now - epoch($9)) / 60) " min" }
        END { if (n) printf "%d to check (%s); ", n, s }' "$run/ledger.tsv" 2>/dev/null || true)
    [[ ! -f $run/idle.md ]] || line+="the coordinator idle since $(head -n1 "$run/idle.md"); "
    echo "- Channel: ${line:-nothing open; }$(grep -c '^## Steered' "$run/log.md" 2>/dev/null || true) steers in all, $(grep -cE '^Steer .*point [0-9]+: cannot' "$run/log.md" 2>/dev/null || true) points refused"

    # Proofs, and the commits since.
    local commits repo hash at step proof layer want proofs="" missing="" lower=""
    commits=$(plan_commits "$since")
    while IFS=$'\t' read -r repo hash at step proof _; do
        [[ -n $repo ]] || continue
        if [[ $proof == - ]]; then
            missing+="$step "
            continue
        fi
        layer=$(proof_layer "$proof")
        want=$(planned_layer "$step")
        proofs+="$step $layer; "
        if [[ -n $want ]] && (($(layer_rank "$layer") < $(layer_rank "$want"))); then
            lower+="$step ($layer, planned $want) "
        fi
    done <<<"$commits"
    echo "- Proofs: ${proofs:-none landed}${lower:+ below plan: $lower}${missing:+ no Proof line: $missing}"

    # Waste.
    local log_lines reports gates landed_since wait_run files scripts
    log_lines=$(wc -l <"$run/log.md" 2>/dev/null || echo 0)
    reports=$(report_lengths "$from" | median_and_p90)
    landed_since=$(grep -c . <<<"$commits" || true)
    gates=$(land_gates "$after")
    wait_run=$(awk -F'\t' -v s="$after" '$3 >= s { w += $4; r += $5 } END { printf "%d s waiting, %d s running", w, r }' "$run/logs/heavy.tsv" 2>/dev/null || echo "no queue statistics")
    files=$(find "$run/logs" -type f -newermt "@$((since + 1))" 2>/dev/null | wc -l)
    scripts=$(find "$run/logs" -type f \( -name '*.sh' -o -name '*.py' -o -name '*.js' \) -newermt "@$((since + 1))" 2>/dev/null | wc -l)
    echo "- Waste: log +$((log_lines - from + 1)) lines; reports ${reports/ / lines median, } p90; ${gates% *} land gates for $landed_since landings, ${gates#* } other gate-lane tasks; queue $wait_run; logs/ +$files files, $scripts scripts."

    # Tests.
    local budgets focused fuzzy last_gate counts
    budgets=$(pulse_setting budgets)
    read -r focused fuzzy <<<"${budgets:-15 60}"
    last_gate=$(awk -F'\t' '$6 != "" && $6 != "-" { g = $6 } END { print g }' <<<"$commits")
    if [[ -n $last_gate ]]; then
        echo "- Tests: last gate: $last_gate; $(awk -v g="$last_gate" -v f="$focused" -v z="$fuzzy" 'BEGIN {
            n = split(g, part, ";"); out = ""
            for (i = 1; i <= n; i++) if (match(part[i], /[0-9.]+ s/)) {
                t = substr(part[i], RSTART, RLENGTH - 2) + 0; b = (i == 1) ? f : (i == 2) ? z : 0
                if (b && t > 0.8 * b) out = out sprintf("suite %d at %.0f%% of its %d s; ", i, 100 * t / b, b)
            }
            print (out == "" ? "within four fifths of the budgets" : out) }')"
    else
        echo "- Tests: no gate since"
    fi
    local slow
    slow=$(awk -F'\t' -v s="$after" '$2 == "gate" && $3 >= s { print $9 }' "$run/logs/heavy.tsv" 2>/dev/null |
        while read -r file; do grep -c 'SLOW \[' "$file" 2>/dev/null || true; done | awk '{ n += $1 } END { print n + 0 }')
    counts=$(cut -f1,2 <<<"$commits" | { grep . || true; } | line_counts)
    read -r ca cr ta tr <<<"$counts"
    echo "  SLOW lines in gate logs: $slow; lines added and removed: code +$ca -$cr, tests +$ta -$tr."

    # Code signals in the added lines.
    local limit words types added big exceptions word_hits type_hits helpers
    limit=$(pulse_setting file-lines)
    limit=${limit:-800}
    words=$(pulse_setting words)
    types=$(pulse_setting types)
    added=$(while IFS=$'\t' read -r repo hash _; do
        [[ -n $repo ]] || continue
        git -C "$root/$repo" show --format= -U0 "$hash" | awk -v repo="$repo" '
            /^\+\+\+ / { path = substr($2, 3); next } /^\+/ { print repo "\t" path "\t" substr($0, 2) }'
    done <<<"$commits")
    big=$(cut -f1,2 <<<"$added" | sort -u | while IFS=$'\t' read -r repo path; do
        [[ -n $path ]] || continue
        n=$(git -C "$root/$repo" show "main:$path" 2>/dev/null | wc -l)
        if ((n > limit)); then echo "$path ($n)"; fi
    done | paste -sd ' ' -)
    exceptions=$(cut -f3 <<<"$added" | grep -cE '(expect|allow)\(clippy::too_many_(lines|arguments)' || true)
    word_hits=""
    [[ -z $words ]] || word_hits=$(cut -f3 <<<"$added" | { grep -oiwE "$words" || true; } | tr '[:upper:]' '[:lower:]' | sort | uniq -c | awk '{ printf "%s%s %d", (NR > 1 ? ", " : ""), $2, $1 }')
    type_hits=""
    [[ -z $types ]] || type_hits=$(cut -f3 <<<"$added" | sed -nE 's/.*\b(struct|enum|type) ([A-Za-z0-9_]+).*/\2/p' | grep -E "$types" | sort -u | paste -sd ' ' - || true)
    helpers=$(awk -F'\t' '{ print $1 "\t" $2 "\t" $3 }' <<<"$added" | while IFS=$'\t' read -r repo path text; do
        is_test_path "$path" || continue
        if [[ $text =~ ^[[:space:]]*(pub(\(crate\))?[[:space:]]+)?fn[[:space:]]+([a-z_0-9]+)\( ]]; then
            echo "$repo ${BASH_REMATCH[3]}"
        fi
    done | sort -u | while read -r repo fn; do
        n=$(git -C "$root/$repo" grep -lE "fn $fn\\(" main -- '*tests*' '*testing*' 2>/dev/null | wc -l)
        if ((n > 3)); then echo "$fn ($n files)"; fi
    done | paste -sd ' ' -)
    echo "- Code: over $limit lines: ${big:-none}; new lint exceptions: $exceptions; forbidden words: ${word_hits:-none}; types like a parallel one: ${type_hits:-none}; test helpers in more than three files: ${helpers:-none}."
    echo "- Rising words: $(rising_words)."

    # Cost so far, and the machine.
    local coordinator_in=0 coordinator_out=0 sessions_in=0 sessions_out=0 claude_in=0 claude_out=0 kind thread a b
    while IFS=$'\t' read -r kind thread; do
        [[ -n $thread ]] || continue
        read -r a b _ <<<"$(codex_tokens "$thread")"
        if [[ $kind == coordinator ]]; then
            coordinator_in=$((coordinator_in + a)); coordinator_out=$((coordinator_out + b))
        else
            sessions_in=$((sessions_in + a)); sessions_out=$((sessions_out + b))
        fi
    done < <(run_threads)
    read -r _ _ _ claude_in claude_out _ <<<"$(claude_tokens)"
    echo "- Cost so far: $(awk -v s="$(run_started)" -v n="$now_s" 'BEGIN { printf "%.1f", (n - s) / 3600 }') h since the claim, $(awk -v r="$(running_seconds)" 'BEGIN { printf "%.1f", r / 3600 }') h working; tokens in/out: coordinator $coordinator_in/$coordinator_out, sessions $sessions_in/$sessions_out, Claude Code $claude_in/$claude_out; Claude Code at list prices, since the planning: $(claude_cost)."
    local beat mem disk
    beat=$(stat -c %Y "$run/heartbeat" 2>/dev/null || echo "$now_s")
    mem=$(awk '/MemAvailable/ { printf "%.1f", $2 / 1048576 }' /proc/meminfo)
    disk=$(df -BG --output=avail "$root" | tail -n1 | tr -dc 0-9)
    echo "- Pins behind upstream main, in commits: $(pin_lag)."
    echo "- Contexts: $(thread_contexts | awk -F'\t' -v limit="${PLAN_CONTEXT_LIMIT:-200000}" '
        $1 == "session" { line = line (n++ ? ", " : "") $2 " " int($4 / 1000) "K"; if ($4 > limit) past = past (p++ ? ", " : "") $2 }
        END { printf "%s", (n ? line : "no session yet"); if (p) printf " (past %dK: %s)", limit / 1000, past }'); Claude Code: $(claude_role_contexts)."
    echo "- Machine: heartbeat $((now_s - beat)) s ago; $mem GB memory and $disk GB disk free."

    printf '%s %s\n' "$now_s" "$((log_lines + 1))" >"$mark"
}

# The run's metrics (development.md, section 15.2): name, value, unit.
metrics_of() {
    ledger_upgrade
    local started ended running landed planned added dropped
    started=$(run_started)
    ended=$(run_ended)
    running=$(running_seconds)
    read -r planned added dropped <<<"$(awk -F'\t' 'NR > 1 {
        if ($4 == "landed") { if ($1 ~ /\.s[0-9]+$/) a++; else p++ } else if ($4 == "dropped") d++ }
        END { print p + 0, a + 0, d + 0 }' "$run/ledger.tsv" 2>/dev/null || echo "0 0 0")"
    landed=$((planned + added))
    m() { printf '%s\t%s\t%s\n' "$1" "$2" "$3"; }
    m wall_h "$(awk -v s="$started" -v e="$ended" 'BEGIN { printf "%.1f", (e - s) / 3600 }')" h
    m running_h "$(awk -v r="$running" 'BEGIN { printf "%.1f", r / 3600 }')" h
    m steps_planned "$planned" steps
    m steps_added "$added" steps
    m steps_dropped "$dropped" steps
    m step_h "$(awk -v r="$running" -v n="$landed" 'BEGIN { printf "%.2f", n ? r / 3600 / n : 0 }')" "h per step"

    # Escalations: each Kind and Since line belongs to the "Blocked and
    # unblocked" heading above it (development.md, section 10.3). One that
    # stays open is listed again in each later section, until the one that
    # answers it: it counts once, answered at its last listing.
    local escalations
    escalations=$(awk -F'\t' '
        /^## Blocked and unblocked, / { at = $0; sub(/^## Blocked and unblocked, */, "", at); kind = ""; heading = ""; next }
        /^## [A-Za-z0-9_-]+ [^ ]+: .*, [0-9T:-]+Z$/ && at != "" { heading = $0 }
        /^- Kind: / && at != "" { kind = $0; sub(/^- Kind: */, "", kind) }
        /^- Since: / && at != "" { since = $0; sub(/^- Since: */, "", since); sub(/\. .*$/, "", since)
            key = (heading != "" ? heading : since "\t" kind)
            if (!(key in last)) order[++n] = key
            last[key] = at; first[key] = since; kinds[key] = kind; heading = "" }
        END { for (i = 1; i <= n; i++) print last[order[i]] "\t" first[order[i]] "\t" kinds[order[i]] }
        ' "$run/log.md" 2>/dev/null |
        while IFS=$'\t' read -r at since kind; do
            a=$(date -u -d "$at" +%s 2>/dev/null) || continue
            b=$(date -u -d "$since" +%s 2>/dev/null) || continue
            printf '%s\t%s\t%s\n' "$b" "$a" "${kind:-unknown}"
        done)
    m escalations "$(cut -f3 <<<"$escalations" | grep . | sort | uniq -c | awk '{ k = $2; for (i = 3; i <= NF; i++) k = k " " $i; printf "%s%s %d", (NR > 1 ? ", " : ""), k, $1 }')" "by kind"
    m escalation_wait_min "$(awk -F'\t' 'NF == 3 { print ($2 - $1) / 60 }' <<<"$escalations" | median_and_p90 | awk '{ printf "%.1f", $1 }')" "min, median"
    m blocked_h "$(awk -F'\t' 'NF == 3 { print $1 "\t" $2 }' <<<"$escalations" | sort -n |
        awk -F'\t' '{ if ($1 > end) { total += end - start; start = $1; end = $2 } else if ($2 > end) end = $2 }
            END { total += end - start; printf "%.1f", total / 3600 }')" h
    m answers_reversed "$(grep -cE '^Reverses: [^n]' "$run/log.md" 2>/dev/null || true)" answers
    m steers "$(grep -c '^## Steered' "$run/log.md" 2>/dev/null || true) sent, $(grep -cE '^Steer .*point [0-9]+: cannot' "$run/log.md" 2>/dev/null || true) points refused" ""
    m restarts "$(sed -nE 's/^## Coordinator resumed, .*: ([^:]*)$/\1/p' "$run/log.md" 2>/dev/null | sort | uniq -c | awk '{ printf "%s%s %d", (NR > 1 ? ", " : ""), $2, $1 }')" "by cause"

    # Tokens.
    local kind thread a b c ci=0 co=0 cc=0 si=0 so=0 sc=0 li=0 lo=0 lc=0 requests=0 idle=0 compactions=0 file
    local pa pb pc ra rb rc xa xb xc bi=0 ri=0 ro=0 xi=0
    while IFS=$'\t' read -r kind thread; do
        [[ -n $thread ]] || continue
        read -r a b c <<<"$(codex_tokens "$thread")"
        if [[ $kind == coordinator ]]; then
            ci=$((ci + a)); co=$((co + b)); cc=$((cc + c))
            for file in $(rollout_of "$thread"); do
                requests=$((requests + $(grep -c '"total_token_usage"' "$file" || true)))
                idle=$((idle + $(grep -cE 'Wait timed out|\\"timed_out\\":true' "$file" || true)))
                compactions=$((compactions + $(grep -c '"type":"compacted"' "$file" || true)))
            done
        else
            si=$((si + a)); so=$((so + b)); sc=$((sc + c))
        fi
    done < <(run_threads)
    read -r pa pb pc ra rb rc xa xb xc <<<"$(claude_tokens)"
    li=$((pa + ra + xa)); lo=$((pb + rb + xb)); lc=$((pc + rc + xc))
    bi=$pa; ri=$ra; ro=$rb; xi=$xa
    share() { awk -v c="$1" -v a="$2" 'BEGIN { printf "%d%%", a ? 100 * c / a : 0 }'; }
    m tokens_coordinator "$ci in ($(share "$cc" "$ci") cached), $co out" tokens
    m tokens_implementers "$si in ($(share "$sc" "$si") cached), $so out" tokens
    m tokens_claude "$li in ($(share "$lc" "$li") cached), $lo out; in: $bi before the claim, $ri in the run, $xi after" tokens
    m cost_claude "$(claude_cost)" "dollars at list prices"
    m supervisor "$(supervisor_sessions)" ""
    m tokens_per_step "$(((ci + co + si + so + ri + ro) / (landed ? landed : 1)))" "tokens per step, the run's"
    m coordinator_requests "$requests, $idle after a wait ran out" requests
    m compactions "$compactions" compactions

    # The queue.
    local tsv=$run/logs/heavy.tsv
    local gates
    gates=$(land_gates)
    m gates_per_step "$(awk -v g="${gates% *}" -v n="$landed" 'BEGIN { printf "%.1f", n ? g / n : 0 }')" "land gates per step"
    m gate_lane_other "${gates#* }" "gate-lane tasks besides land's gates"
    m queue_wait_ratio "$(awk -F'\t' '{ w += $4; r += $5 } END { printf "%.2f", r ? w / r : 0 }' "$tsv" 2>/dev/null || echo 0)" "waiting per running"

    # The commits.
    local commits
    commits=$(plan_commits 0)
    m suites "$(awk -F'\t' '$6 != "" && $6 != "-" && $6 !~ /^none/ {
            if (!($1 in first)) { first[$1] = $6; order[++n] = $1 } last[$1] = $6 }
        END { for (i = 1; i <= n; i++) printf "%s%s first: %s; last: %s", (i > 1 ? " | " : ""), order[i], first[order[i]], last[order[i]] }' <<<"$commits")" "Gate-Result, by repository"
    local repo hash at step proof layer want below=0
    local -A by_layer=()
    while IFS=$'\t' read -r repo hash at step proof _; do
        [[ -n $repo ]] || continue
        layer=$(proof_layer "$proof")
        [[ $layer != - ]] || layer=none
        by_layer[$layer]=$((${by_layer[$layer]:-0} + 1))
        want=$(planned_layer "$step")
        if [[ -n $want && $layer != none ]] && (($(layer_rank "$layer") < $(layer_rank "$want"))); then
            below=$((below + 1))
        fi
    done <<<"$commits"
    m proofs "$(for layer in unit world joint process live none; do [[ -n ${by_layer[$layer]:-} ]] && printf '%s %s, ' "$layer" "${by_layer[$layer]}"; done)$below below plan" steps
    m report_lines "$(report_lengths 1 | median_and_p90 | awk '{ print $1 " median, " $2 " p90" }')" lines
    local ca cr ta tr
    read -r ca cr ta tr <<<"$(cut -f1,2 <<<"$commits" | grep . | line_counts)"
    m lines "code +$ca -$cr, tests +$ta -$tr" lines
    m pin_lag "$(pin_lag)" "commits behind upstream main"
    m contexts "$(thread_contexts | awk -F'\t' '$1 == "session" { print $5 }' | median_and_p90 |
        awk '{ printf "%dK median, %dK p90", $1 / 1000, $2 / 1000 }') of the sessions' peaks; $(grep -c '^Handover, ' "$run/log.md" 2>/dev/null || true) handovers" ""
}

metrics() {
    if [[ ${1:-} == --history ]]; then
        local file
        for file in "$root"/*/docs/development/plans/*/metrics.tsv; do
            [[ -f $file ]] || continue
            awk -F'\t' -v plan="$(basename "$(dirname "$file")")" '{ print plan "\t" $1 "\t" $2 }' "$file"
        done | awk -F'\t' '
            { if (!($1 in p)) { p[$1] = 1; plans[++np] = $1 } if (!($2 in k)) { k[$2] = 1; keys[++nk] = $2 } v[$1, $2] = $3 }
            END {
                if (!np) { print "no metrics.tsv in any plan"; exit }
                printf "| Metric |"; for (j = 1; j <= np; j++) printf " %s |", plans[j]; print ""
                printf "|---|"; for (j = 1; j <= np; j++) printf "---|"; print ""
                for (i = 1; i <= nk; i++) { printf "| %s |", keys[i]; for (j = 1; j <= np; j++) printf " %s |", v[plans[j], keys[i]]; print "" }
            }'
        return
    fi
    local out=""
    [[ ${1:-} == --out ]] && { out=${2:?file}; shift 2; }
    if (($#)); then
        locate "$1"
    else
        last_run
    fi
    [[ -d $run ]] || die "no run directory at $run"
    if [[ -n $out ]]; then
        metrics_of >"$out"
        echo "wrote $out"
    else
        metrics_of
    fi
}

# shellcheck shell=bash
# shellcheck disable=SC2154 # plan sets root, repos, name, plan, plan_dir and run
# The run's numbers: `plan pulse`, what changed since the supervisor's last
# check (development.md, section 14.2), and the readers metrics.sh uses
# for the run's metrics (section 15.2). Sourced by plan, not run.
#
# They read the run's directory, the plan's commits on each repository's
# main (their Plan, Step, Proof and Gate-Result trailers), the queue's
# statistics (logs/heavy.tsv), the coordinator's and its sessions' Codex
# rollouts, and the Claude Code sessions' transcripts.

codex_home=${CODEX_HOME:-$HOME/.codex}
claude_home=${CLAUDE_CONFIG_DIR:-$HOME/.claude}

# Read complete JSON objects line by line, skipping torn transcript records.
jsonl() {
    local -a args=()
    while [[ ${1:-} == --arg ]]; do args+=("$1" "$2" "$3"); shift 3; done
    local program=$1
    shift
    jq -R -r "${args[@]}" "fromjson? | select(type == \"object\") | ($program)" "$@"
}

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

# Only gated steps contribute proofs and code/test line counts.
gated_commits() {
    plan_commits "${1:-}" | awk -F'\t' '$4 != "-" && $6 !~ /^none/'
}

# A split's lettered parts remain planned; the replaced row counts in none.
ledger_kinds() {
    awk -F'\t' -v OFS='\t' '
        FNR == NR { if ($4 == "split") parents[$1] = 1; next }
        $4 != "split" {
            parent = $1; sub(/[a-z]$/, "", parent)
            kind = ($1 ~ /\.s[0-9]+$/ || ($1 ~ /[0-9][a-z]$/ && !(parent in parents))) ? "added" : "planned"
            print $0, kind
        }' "$run/ledger.tsv" "$run/ledger.tsv"
}

# Each semicolon part names a layer; a hyphen ends its first word.
proof_layers() {
    awk -v proof="$1" 'BEGIN {
        n = split(proof, part, ";")
        for (i = 1; i <= n; i++) {
            sub(/^[ \t]+/, "", part[i]); sub(/[ ,:.\t-].*$/, "", part[i])
            if (part[i] != "") print tolower(part[i])
        }
    }'
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
    [[ $item =~ [0-9][a-z]$ ]] && item=${item%[a-z]}
    session=${session%%.*}
    [[ $session =~ ^[0-9]+$ ]] || return 0
    for file in "$plan_dir/$repo/$session"-*.md; do
        [[ -f $file ]] || continue
        proof_layer "$(awk -v id="$((10#$session)).$item" -v raw="$session.$item" '
            /^### / { inside = ($2 == id || $2 == raw) }
            inside && /^Proof:/ { sub(/^Proof: */, ""); print; exit }' "$file")"
        return
    done
}

# Judge the highest-ranked layer, retaining the first word when unknown.
proof_layer() {
    local layer best=- rank=0 n
    while read -r layer; do
        n=$(layer_rank "$layer")
        if [[ $best == - ]] || ((n > rank)); then best=$layer; rank=$n; fi
    done < <(proof_layers "$1")
    echo "$best"
}

# A split is judged only at its last part, against the whole increment.
below_plan() {
    local step=$1 proof=$2 parent=$1 want layer
    [[ $step =~ [0-9][a-z]$ ]] && parent=${step%[a-z]}
    if awk -F'\t' -v parent="$parent" -v step="$step" '
        substr($1, 1, length(parent)) == parent && substr($1, length(parent)+1) ~ /^[a-z]$/ && $1 > step { found = 1 }
        END { exit !found }' "$run/ledger.tsv"; then return; fi
    want=$(planned_layer "$step"); layer=$(proof_layer "$proof")
    if [[ -n $want && $layer != - ]] && (($(layer_rank "$layer") < $(layer_rank "$want"))); then echo "$want"; fi
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

# Prefer the last usage record's thread total; token_count totals are the fallback.
codex_tokens() {
    local file
    file=$(rollout_of "$1" | head -n1)
    [[ -n $file ]] || { echo "0 0 0"; return; }
    jsonl '
        if .type == "token_usage_record" and .payload.thread_token_usage != null then ["record", .payload.thread_token_usage]
        elif .payload.info.total_token_usage != null then ["total", .payload.info.total_token_usage]
        else empty end
        | [.[0], (.[1].input_tokens // 0), (.[1].output_tokens // 0), (.[1].cached_input_tokens // 0)] | @tsv' "$file" |
        awk -F'\t' '{ last[$1] = $2 " " $3 " " $4 } END { print ("record" in last ? last["record"] : "total" in last ? last["total"] : "0 0 0") }'
}

# A response has one request even when its usage record is written twice.
codex_requests() {
    jsonl 'if .type == "token_usage_record" and .payload.response_id != null then ["record", .payload.response_id]
        elif .payload.info.total_token_usage != null then ["total"] else empty end | @tsv' "$1" |
        awk -F'\t' '$1 == "record" { seen[$2] = 1 } $1 == "total" { totals++ }
            END { for (id in seen) n++; print (n ? n : totals + 0) }'
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
        sizes=$(grep '"last_token_usage"' "$file" | jsonl '.payload.info.last_token_usage.input_tokens // 0' 2>/dev/null |
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
    last=$(tac "$file" | head -n 400 | jsonl 'select(.type == "assistant" and .message.usage != null and .message.model != "<synthetic>")
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

# The Inputs' design paths bound the design sessions to this plan's repository.
design_inputs() {
    awk '/^## /{inside=($0=="## Inputs")} inside{while(match($0,/`docs\/(design|foundation)\/[^`]*`/)){print substr($0,RSTART+1,RLENGTH-2); $0=substr($0,RSTART+RLENGTH)}}' "$1"
}

designer_sessions() {
    local top prefix inputs landing prev
    inputs=$(design_inputs "$plan_dir/README.md")
    [[ -n $inputs ]] || return 0
    top=$(git -C "$plan_dir" rev-parse --show-toplevel) || return 0
    prefix=$(cd "$plan_dir" && git rev-parse --show-prefix)
    landing=$(git -C "$top" log -n1 --format=%H --grep='^Planner-Session:' main -- "${prefix%/}")
    [[ -n $landing ]] || return 0
    prev=$(git -C "$top" log -n1 --format=%H --grep='^Planner-Session:' "$landing^" 2>/dev/null || true)
    # shellcheck disable=SC2086 # one path per line, none with spaces
    git -C "$top" log "${prev:+$prev..}$landing" --format='%(trailers:key=Designer-Session,valueonly,separator=%x0A)' -- $inputs |
        awk 'NF { print "designer\t" $1 }'
}

# A named agent's descendants share its directory; toolUseId joins them.
claude_files() {
    local session=$1 file meta tool next
    local -a pending=()
    local -A seen=()
    mapfile -t pending < <(find "$claude_home/projects" \( -name "$session.jsonl" -o -name "agent-${session#agent-}.jsonl" -o -path "*/$session/subagents/*.jsonl" \) 2>/dev/null || true)
    while ((${#pending[@]})); do
        file=${pending[0]}; pending=("${pending[@]:1}")
        [[ -z ${seen[$file]:-} ]] || continue
        seen[$file]=1
        printf '%s\n' "$file"
        [[ $file == */subagents/agent-*.jsonl ]] || continue
        for meta in "${file%/*}"/agent-*.meta.json; do
            [[ -f $meta ]] || continue
            next=${meta%.meta.json}.jsonl
            [[ -f $next && -z ${seen[$next]:-} ]] || continue
            tool=$(jq -r '.toolUseId // empty' "$meta")
            if [[ -n $tool ]] && grep -qF -- "$tool" "$file"; then pending+=("$next"); fi
        done
    done
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
        designer_sessions
    } | awk -F'\t' '!seen[$2]++'
}

# The Claude Code sessions' messages, their sub-agents' included, each
# message counted once across every session: a resumed or forked session's
# transcript copies the messages before it, which count for the first role
# that has them. One line each: role, part (1 before the claim, 2 from the
# claim to done.md, 3 after; a message without a time counts in the run),
# fresh input, 5-minute cache writes, 1-hour cache writes, cache reads,
# output and model.
claude_messages() {
    local role session files
    while IFS=$'\t' read -r role session; do
        [[ -n $session ]] || continue
        files=$(claude_files "$session")
        [[ -n $files ]] || continue
        # shellcheck disable=SC2086 # one path per line, none with spaces
        jsonl --arg role "$role" 'select(.type == "assistant" and .message.usage != null and .message.model != "<synthetic>") | .message as $m | $m.usage as $u
            | ($u.cache_creation // {}) as $c
            | (if ($c.ephemeral_1h_input_tokens // null) == null and ($c.ephemeral_5m_input_tokens // null) == null
               then ($u.cache_creation_input_tokens // 0) else ($c.ephemeral_1h_input_tokens // 0) end) as $hour
            | [$role, $m.id, ($u.input_tokens // 0), ($c.ephemeral_5m_input_tokens // 0), $hour,
               ($u.cache_read_input_tokens // 0), ($u.output_tokens // 0),
               ((.timestamp // "") | if . == "" then "" else (sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601 | tostring) end), ($m.model // "")]
            | @tsv' $files 2>/dev/null || true
    done < <(claude_roles) |
        awk -F'\t' -v OFS='\t' -v start="$(run_started)" -v end="$(run_ended)" '
            !($2 in role) { role[$2] = $1; model[$2] = $9; order[++n] = $2 }
            { for (i = 3; i <= 7; i++) if ($i + 0 > v[$2, i] + 0) v[$2, i] = $i
              if ($8 != "" && !($2 in t)) t[$2] = $8 }
            END {
                for (j = 1; j <= n; j++) {
                    k = order[j]
                    part = (!(k in t)) ? 2 : (t[k] < start) ? 1 : (t[k] <= end) ? 2 : 3
                    print role[k], part, v[k, 3] + 0, v[k, 4] + 0, v[k, 5] + 0, v[k, 6] + 0, v[k, 7] + 0, model[k]
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

# When the run started: its claim, from log.md.
run_started() {
    local at
    at=$(sed -nE 's/^## Claimed, (.*)$/\1/p' "$run/log.md" 2>/dev/null | head -n1)
    date -u -d "${at:-now}" +%s
}

# When the run ended: done.md's "Recorded <time>" line, which the
# coordinator writes first (development.md, section 7.6), or else when
# done.md last changed; now, while it runs.
run_ended() {
    local at
    [[ -f $run/done.md ]] || { date +%s; return; }
    at=$(sed -nE 's/^Recorded:? ([0-9]{4}-[0-9T:-]+Z).*$/\1/p' "$run/done.md" | head -n1)
    if [[ -n $at ]]; then date -u -d "$at" +%s; else stat -c %Y "$run/done.md"; fi
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
    jsonl '.timestamp // empty | sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601' $files 2>/dev/null | sort -n |
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

# Lines added and removed, by the given commits (repository and hash per
# line), in four kinds: production code, tests, generated code (under a
# generated/ directory), and data (any file not Rust: goldens, lock files,
# fixtures). Eight numbers: added and removed for each, in that order.
line_counts() {
    local repo hash
    while IFS=$'\t' read -r repo hash; do
        git -C "$root/$repo" show --format= --numstat "$hash"
    done | awk -F'\t' '
        $1 == "-" { next }
        {
            if ($3 ~ /(^|\/)generated(\/|$)/) k = "g"
            else if ($3 !~ /\.rs$/) k = "d"
            else if ($3 ~ /(^|\/)(tests?|testing)(\/|$)/ || $3 ~ /tests?\.rs$/) k = "t"
            else k = "c"
            a[k] += $1; r[k] += $2
        }
        END { printf "%d %d %d %d %d %d %d %d\n", a["c"], r["c"], a["t"], r["t"], a["g"], r["g"], a["d"], r["d"] }'
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
    local repo up rev lag most out="" line lock
    if [[ ${1:-} == fetch ]]; then
        for up in $all_repos; do
            [[ -d $root/$up/.git ]] || continue
            timeout 20 git -C "$root/$up" fetch -q origin main 2>/dev/null || true
        done
    fi
    for repo in $all_repos; do
        lock=$(git -C "$root/$repo" show origin/main:Cargo.lock 2>/dev/null) || continue
        line=""
        for up in $all_repos; do
            [[ $up != "$repo" ]] || break
            most=""
            for rev in $(sed -nE "s|^source = \"git\+[^\"]*/$up\.git#([0-9a-f]+)\"$|\1|p" <<<"$lock" | sort -u); do
                lag=$(git -C "$root/$up" rev-list --count "$rev..origin/main" 2>/dev/null) || lag="?"
                if [[ $lag == "?" || -z $most ]] || { [[ $most != "?" ]] && ((lag > most)); }; then most=$lag; fi
            done
            [[ -z $most ]] || line+="${line:+, }$up $most"
        done
        [[ -z $line ]] || out+="${out:+; }$repo: $line"
    done
    echo "${out:-none}"
}

# The coordinator's record against its shapes (development.md, sections
# 7.3, 7.5, 10.2 and 10.3), from line $1 of log.md on: headings no shape
# names, Kind values section 10.2 does not list, and context.md's lines
# past 200 characters. The metrics read the record by those shapes.
record_check() {
    local headings kinds long
    headings=$(awk -v from="${1:-1}" '
        NR < from || !/^## / { next }
        /^## [A-Za-z0-9_-]+ ([0-9]+|repin), [0-9T:-]+Z$/ { next }
        /^## (Blocked and unblocked|Steered|Coordinator resumed|Coordinator loaded|Claimed|Released|Idle), / { next }
        /^## [A-Za-z0-9_-]+ [^ ]+: .*, [0-9T:-]+Z$/ { next }
        /^## (Decision|Landed|Instructions|Checked)$/ || /^## [0-9]+\. / { next }
        { n++; if (n == 1) first = $0 }
        END { if (n) printf "%d (the first: \"%s\")", n, first; else printf "none" }' "$run/log.md" 2>/dev/null)
    kinds=$(awk -v from="${1:-1}" '
        NR >= from && /^- Kind: / {
            k = $0; sub(/^- Kind: */, "", k)
            if (k !~ /^(design gap|plan defect|cross-session scope|process|infrastructure|credentials)$/) c[k]++
        }
        END { for (k in c) printf "%s\"%s\" %d", (n++ ? ", " : ""), k, c[k]; if (!n) printf "none" }' "$run/log.md" 2>/dev/null)
    long=$(awk 'length($0) > 200 { n++ } END { print n + 0 }' "$run/context.md" 2>/dev/null || echo 0)
    echo "headings no shape names: ${headings:-none}; Kind values not in section 10.2: ${kinds:-none}; context.md lines past 200 characters: $long"
}

# Logged tasks and finished commands whose client left them in pueue.
queue_rows() {
    local task_run=$1 claim json
    if [[ ${queue_run:-} == "$task_run" ]]; then
        [[ -z $queue_snapshot ]] || printf '%s\n' "$queue_snapshot"
        return 0
    fi
    cat "$task_run/logs/heavy.tsv" 2>/dev/null || true
    [[ -f $tmp/active-plan ]] || return 0
    claim=$(sed -n 4p "$tmp/active-plan")
    json=$(TZ=UTC0 pueue status --json 2>/dev/null) || return 0
    jq -r --arg claim "$claim" '
        def epoch: sub("\\.[0-9]+Z$"; "Z") | fromdate;
        .tasks[] | select((.status | keys[0]) == "Done")
        | select(.command | contains("heavy --task") | not)
        | . as $task | .status.Done as $done
        | select(($done.enqueued_at | epoch) >= ($claim | epoch))
        | [$task.id, $task.group, ($done.end | epoch | strftime("%FT%TZ")),
           (($done.start | epoch) - ($done.enqueued_at | epoch)),
           (($done.end | epoch) - ($done.start | epoch)),
           (if $done.result == "Success" then 0
            elif ($done.result | type) == "object" then ($done.result.Failed // 1) else 1 end),
           0, $task.path, "/dev/null", $task.command, ($task.label // "")]
        | @tsv' <<<"$json"
}

# Gate-lane tasks since a time (all of them, without one): land's gates,
# then the others (measurements, probes, live runs, plan check's gate), by
# the label in heavy.tsv's eleventh column. A queue whose lines carry no
# label counts every gate-lane task as a gate.
land_gates() {
    awk -F'\t' -v s="${1:-}" '
        $2 == "gate" && (s == "" || $3 >= s) { g++; if (NF >= 11) { lab = 1; if ($11 ~ /^land /) l++ } }
        END { if (!lab) l = g; print l + 0, g - l + 0 }' < <(queue_rows "$run")
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
    ledger_kinds | awk -F'\t' -v since="$since" -v three="$((now_s - 10800))" -v now="$now_s" '
        function epoch(t,   c) { c = "date -u -d \"" t "\" +%s"; c | getline e; close(c); return e }
        NR == 1 { next }
        {
            added = ($12 == "added")
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
        }'

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
    echo "- Record: $(record_check "$from")."
    echo "- Channel: ${line:-nothing open; }$(grep -c '^## Steered' "$run/log.md" 2>/dev/null || true) steers in all, $(grep -cE '^Steer .*point [0-9]+: cannot' "$run/log.md" 2>/dev/null || true) points refused"

    # Proofs, and the commits since.
    local commits gated repo hash at step proof layer want proofs="" missing="" lower=""
    commits=$(plan_commits "$since")
    gated=$(gated_commits "$since")
    while IFS=$'\t' read -r repo hash at step proof _; do
        [[ -n $repo ]] || continue
        if [[ $proof == - ]]; then
            missing+="$step "
            continue
        fi
        layer=$(proof_layer "$proof")
        want=$(below_plan "$step" "$proof")
        proofs+="$step $layer; "
        if [[ -n $want ]]; then
            lower+="$step ($layer, planned $want) "
        fi
    done <<<"$gated"
    echo "- Proofs: ${proofs:-none landed}${lower:+ below plan: $lower}${missing:+ no Proof line: $missing}"

    # Waste.
    local log_lines reports gates landed_since wait_run files scripts
    log_lines=$(wc -l <"$run/log.md" 2>/dev/null || echo 0)
    reports=$(report_lengths "$from" | median_and_p90)
    landed_since=$(grep -c . <<<"$commits" || true)
    # Read one queue snapshot for all figures in this pulse.
    local queue_snapshot queue_run=
    queue_snapshot=$(queue_rows "$run")
    queue_run=$run
    gates=$(land_gates "$after")
    wait_run=$(awk -F'\t' -v s="$after" '$3 >= s { w += $4; r += $5 } END { printf "%d s waiting, %d s running", w, r }' < <(queue_rows "$run"))
    files=$(find "$run/logs" -type f -newermt "@$((since + 1))" 2>/dev/null | wc -l)
    scripts=$(find "$run/logs" -type f \( -name '*.sh' -o -name '*.py' -o -name '*.js' \) -newermt "@$((since + 1))" 2>/dev/null | wc -l)
    echo "- Waste: log +$((log_lines - from + 1)) lines; reports ${reports/ / lines median, } p90; ${gates% *} land gates for $landed_since landings, ${gates#* } other gate-lane tasks; queue $wait_run; logs/ +$files files, $scripts scripts."

    local held logged
    held=$(queue_rows "$run" | wc -l)
    logged=$(wc -l <"$run/logs/heavy.tsv" 2>/dev/null || echo 0)
    held=$((held - logged))
    ((held == 0)) || echo "  pueue holds $held finished tasks no client removed"

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
    slow=$(awk -F'\t' -v s="$after" '$2 == "gate" && $3 >= s { print $9 }' < <(queue_rows "$run") |
        while read -r file; do grep -c 'SLOW \[' "$file" 2>/dev/null || true; done | awk '{ n += $1 } END { print n + 0 }')
    counts=$(cut -f1,2 <<<"$gated" | { grep . || true; } | line_counts)
    read -r ca cr ta tr ga gr da dr <<<"$counts"
    echo "  SLOW lines in gate logs: $slow; lines added and removed: code +$ca -$cr, tests +$ta -$tr, generated +$ga -$gr, data +$da -$dr."

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
    echo "- Cost so far: $(awk -v s="$(run_started)" -v n="$now_s" 'BEGIN { printf "%.1f", (n - s) / 3600 }') h since the claim, $(awk -v r="$(running_seconds)" 'BEGIN { printf "%.1f", r / 3600 }') h working; tokens in/out: coordinator $coordinator_in/$coordinator_out, sessions $sessions_in/$sessions_out, Claude Code $claude_in/$claude_out; Claude Code at list prices, since the design: $(claude_cost)."
    local beat mem disk
    beat=$(stat -c %Y "$run/heartbeat" 2>/dev/null || echo "$now_s")
    mem=$(awk '/MemAvailable/ { printf "%.1f", $2 / 1048576 }' /proc/meminfo)
    disk=$(df -BG --output=avail "$root" | tail -n1 | tr -dc 0-9)
    echo "- Pins behind upstream main, in commits: $(pin_lag "")."
    echo "- Contexts: $(thread_contexts | awk -F'\t' -v limit="${PLAN_CONTEXT_LIMIT:-200000}" '
        $1 == "session" { line = line (n++ ? ", " : "") $2 " " int($4 / 1000) "K"; if ($5 > limit) past = past (p++ ? ", " : "") $2 }
        END { printf "%s", (n ? line : "no session yet"); if (p) printf " (past %dK: %s)", limit / 1000, past }'); Claude Code: $(claude_role_contexts)."
    echo "- Machine: heartbeat $((now_s - beat)) s ago; $mem GB memory and $disk GB disk free."

    printf '%s %s\n' "$now_s" "$((log_lines + 1))" >"$mark"
}


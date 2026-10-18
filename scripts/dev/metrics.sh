# shellcheck shell=bash
# shellcheck disable=SC2154 # numbers.sh sets claude_home; plan sets root and run
# shellcheck disable=SC2034 # hash in metrics_of's commit reader is unused
# The run's metrics. Sourced by plan after numbers.sh, not run.

# List prices per MTok, checked 2026-10-17: platform.claude.com/docs/en/about-claude/pricing,
# platform.claude.com/docs/en/models/haiku-5-5/overview and anthropic.com/claude-sonnet-5-5.
# Each model's prefix covers its dated names; unknown and absent models use Opus.
claude_cost() {
    claude_messages | awk -F'\t' '
        BEGIN {
            split("4 5 8 0.20 20", opus, " ")
            split("2 2.50 4 0.10 10", sonnet, " ")
            split("0.10 0.125 0.20 0.01 0.50", haiku, " ")
            split("0.50 0.625 1 0.05 2.50", haiku_long, " ")
        }
        {
            if (!($1 in by)) roles[++n] = $1
            model = ($8 ~ /^claude-sonnet-5-5/ ? "sonnet" : $8 ~ /^claude-haiku-5-5/ ? "haiku" : "opus")
            if ($8 != "" && $8 !~ /^claude-(opus|sonnet|haiku)-5-5/) {
                if (!($8 in unknown)) models[++nm] = $8
                unknown[$8]++
            }
            for (j = 1; j <= 5; j++) p[j] = model == "sonnet" ? sonnet[j] : model == "haiku" ? (($3+$4+$5+$6 > 100000) ? haiku_long[j] : haiku[j]) : opus[j]
            c = ($3 * p[1] + $4 * p[2] + $5 * p[3] + $6 * p[4] + $7 * p[5]) / 1e6
            by[$1] += c; total += c
            fresh += $3 * p[1] / 1e6; writes += ($4 * p[2] + $5 * p[3]) / 1e6
            reads += $6 * p[4] / 1e6; out += $7 * p[5] / 1e6
        }
        END {
            printf "$%.0f", total
            for (i = 1; i <= n; i++) printf "%s%s $%.0f", (i == 1 ? ": " : ", "), roles[i], by[roles[i]]
            printf "; cache reads $%.0f, cache writes $%.0f, fresh input $%.0f, output $%.0f", reads, writes, fresh, out
            for (i = 1; i <= nm; i++) printf "%s%s %d", (i == 1 ? "; unknown models, priced as Opus: " : ", "), models[i], unknown[models[i]]
            print ""
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
            jsonl --arg s "$session" '
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

# Restart causes retain their full text; hand loads have the cause loaded.
restart_causes() {
    {
        sed -nE -e 's/^## Coordinator resumed, [^ ]+Z: //p' -e 's/^## Coordinator loaded, .*/loaded/p' "$run/log.md"
        [[ ! -f $run/supervision.md ]] || sed -nE 's/^## Loaded, .*/loaded/p' "$run/supervision.md"
    } | sort | uniq -c | awk '{ n = $1; sub(/^[[:space:]]*[0-9]+[[:space:]]+/, ""); printf "%s%s %d", (NR > 1 ? ", " : ""), $0, n }'
}

# The run's metrics (development.md, section 15.2): name, value, unit.
metrics_of() {
    ledger_upgrade
    local started ended running landed planned added dropped
    started=$(run_started)
    ended=$(run_ended)
    running=$(running_seconds)
    read -r planned added dropped <<<"$(ledger_kinds | awk -F'\t' 'NR > 1 {
        if ($4 == "landed") { if ($12 == "added") a++; else p++ } else if ($4 == "dropped") d++ }
        END { print p + 0, a + 0, d + 0 }' 2>/dev/null || echo "0 0 0")"
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
            # A Since written after its answer is a record's slip, not a wait.
            ((a >= b)) || continue
            printf '%s\t%s\t%s\n' "$b" "$a" "${kind:-unknown}"
        done)
    m escalations "$(cut -f3 <<<"$escalations" | grep . | sort | uniq -c | awk '{ k = $2; for (i = 3; i <= NF; i++) k = k " " $i; printf "%s%s %d", (NR > 1 ? ", " : ""), k, $1 }')" "by kind"
    m escalation_wait_min "$(awk -F'\t' 'NF == 3 { print ($2 - $1) / 60 }' <<<"$escalations" | median_and_p90 | awk '{ printf "%.1f", $1 }')" "min, median"
    m blocked_h "$(awk -F'\t' 'NF == 3 { print $1 "\t" $2 }' <<<"$escalations" | sort -n |
        awk -F'\t' '{ if ($1 > end) { total += end - start; start = $1; end = $2 } else if ($2 > end) end = $2 }
            END { total += end - start; printf "%.1f", total / 3600 }')" h
    m answers_reversed "$(grep -cE '^Reverses: [^n]' "$run/log.md" 2>/dev/null || true)" answers
    m steers "$(grep -c '^## Steered' "$run/log.md" 2>/dev/null || true) sent, $(grep -cE '^Steer .*point [0-9]+: cannot' "$run/log.md" 2>/dev/null || true) points refused" ""
    m restarts "$(restart_causes)" "by cause"

    # Tokens.
    local kind thread a b c ci=0 co=0 cc=0 si=0 so=0 sc=0 li=0 lo=0 lc=0 requests=0 idle=0 compactions=0 file
    local pa pb pc ra rb rc xa xb xc bi=0 ri=0 ro=0 xi=0
    while IFS=$'\t' read -r kind thread; do
        [[ -n $thread ]] || continue
        read -r a b c <<<"$(codex_tokens "$thread")"
        if [[ $kind == coordinator ]]; then
            ci=$((ci + a)); co=$((co + b)); cc=$((cc + c))
            for file in $(rollout_of "$thread"); do
                requests=$((requests + $(codex_requests "$file")))
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
    local gates
    gates=$(land_gates)
    m gates_per_step "$(awk -v g="${gates% *}" -v n="$landed" 'BEGIN { printf "%.1f", n ? g / n : 0 }')" "land gates per step"
    m gate_lane_other "${gates#* }" "gate-lane tasks besides land's gates"
    m queue_wait_ratio "$(awk -F'\t' '{ w += $4; r += $5 } END { printf "%.2f", r ? w / r : 0 }' < <(queue_rows "$run"))" "waiting per running"

    # The commits.
    local commits gated
    commits=$(plan_commits 0)
    gated=$(gated_commits 0)
    m suites "$(awk -F'\t' '$6 != "" && $6 != "-" && $6 !~ /^none/ {
            if (!($1 in first)) { first[$1] = $6; order[++n] = $1 } last[$1] = $6 }
        END { for (i = 1; i <= n; i++) printf "%s%s first: %s; last: %s", (i > 1 ? " | " : ""), order[i], first[order[i]], last[order[i]] }' <<<"$commits")" "Gate-Result, by repository"
    local repo hash at step proof layer want below=0
    local -A by_layer=()
    local -a unknown=()
    local parts
    # Each step once, by its last commit: a step landed in several commits
    # proves itself at its tip.
    while IFS=$'\t' read -r repo hash at step proof _; do
        [[ -n $repo ]] || continue
        parts=$(proof_layers "$proof")
        for layer in ${parts:-none}; do
            if [[ -z ${by_layer[$layer]:-} && $layer != none ]] && (($(layer_rank "$layer") == 0)); then unknown+=("$layer"); fi
            by_layer[$layer]=$((${by_layer[$layer]:-0} + 1))
        done
        want=$(below_plan "$step" "$proof")
        [[ -z $want ]] || below=$((below + 1))
    done < <(awk -F'\t' '{ last[$4] = $0; if (!($4 in seen)) { seen[$4] = 1; order[++n] = $4 } }
        END { for (i = 1; i <= n; i++) print last[order[i]] }' <<<"$gated")
    m proofs "$(for layer in unit world joint process live "${unknown[@]}" none; do [[ -n ${by_layer[$layer]:-} ]] && printf '%s %s, ' "$layer" "${by_layer[$layer]}"; done)$below below plan" steps
    m report_lines "$(report_lengths 1 | median_and_p90 | awk '{ print $1 " median, " $2 " p90" }')" lines
    local ca cr ta tr ga gr da dr
    read -r ca cr ta tr ga gr da dr <<<"$(cut -f1,2 <<<"$gated" | grep . | line_counts)"
    m lines "code +$ca -$cr, tests +$ta -$tr, generated +$ga -$gr, data +$da -$dr" lines
    m pin_lag "$(pin_lag fetch)" "commits behind upstream main"
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

# shellcheck shell=bash
# shellcheck disable=SC2154 # plan sets the variables these functions read
# Sourced by plan, not run.

check() {
    local fix=0 failed=0
    [[ ${1:-} == --fix ]] && { fix=1; shift; }
    ok() { echo "ok    $*"; }
    warn() { echo "warn  $*"; }
    fail() { echo "FAIL  $*"; failed=1; }
    if (($#)); then
        check_plan "$1"
        check_decisions
    fi

    # Every primary checkout a plan builds on: on main, clean, equal to
    # origin's main, and origin takes a push. Without DEV_REPOS, the plan's
    # repository and those upstream of it; without a plan, all of them.
    # --fix fast-forwards a clean checkout that is only behind, and keeps
    # the code graph's index out of git status.
    local repo dir ahead behind graph state checked=$repos
    [[ -n ${DEV_REPOS:-} || -z ${project:-} ]] || checked=$(upstream "$project")
    for repo in $checked; do
        dir=$root/$repo
        [[ -d $dir/.git ]] || { warn "$repo: no checkout at $dir"; continue; }
        if [[ $(git -C "$dir" symbolic-ref --quiet --short HEAD || true) != main ]]; then
            fail "$repo: primary checkout is not on main"
            continue
        fi
        if ((fix)) && ! grep -qsx '.codebase-memory/' "$dir/.git/info/exclude"; then
            mkdir -p "$dir/.git/info" && echo '.codebase-memory/' >>"$dir/.git/info/exclude"
        fi
        [[ -z $(git -C "$dir" status --porcelain) ]] ||
            fail "$repo: primary checkout has changes"
        git -C "$dir" fetch --quiet origin || { fail "$repo: cannot fetch origin"; continue; }
        read -r behind ahead < <(git -C "$dir" rev-list --left-right --count origin/main...main)
        if ((fix && behind && !ahead)) && [[ -z $(git -C "$dir" status --porcelain) ]] &&
            git -C "$dir" merge --quiet --ff-only origin/main; then
            ok "$repo: main fast-forwarded $behind commits to origin/main"
            behind=0
        fi
        if ((ahead || behind)); then
            fail "$repo: main is $ahead ahead and $behind behind origin/main"
        else
            ok "$repo: main is origin/main"
        fi
        read -r graph state < <(graph_index "$repo")
        case $state in
            none) ok "$repo: the graph has no index of it ($graph)" ;;
            older) warn "$repo: the graph's index is older than origin/main: re-index $dir (project $graph)" ;;
            *) ok "$repo: the graph's index is as new as origin/main" ;;
        esac
        if GIT_TERMINAL_PROMPT=0 git -C "$dir" push --dry-run --quiet origin main >/dev/null 2>&1; then
            ok "$repo: origin takes a push"
        else
            fail "$repo: origin refuses a push from here: git -C $dir push --dry-run origin main"
        fi
    done

    # One run per machine.
    if [[ ! -f $active ]]; then
        ok "no run is active"
    elif [[ $(sed -n 1p "$active") == "${name:-}" ]]; then
        ok "this run is the active one"
    elif [[ -n ${name:-} ]]; then
        fail "another run is active: $(sed -n 1p "$active")"
    else
        warn "a run is active: $(sed -n 1p "$active")"
    fi

    check_machine
    [[ -z ${project:-} ]] || check_gate
    return "$failed"
}

# The queue and the plan's gate, run once on main (development.md, section
# 6): a machine that cannot run them blocks every session at its first
# landing. A gate that passed on main's tree is not run again.
check_gate() {
    [[ ${PLAN_NO_GATE:-} != 1 ]] || { warn "PLAN_NO_GATE: the queue and the gate are not checked"; return 0; }
    local out
    if out=$("$kit/heavy" --tail 0 true 2>&1); then
        ok "heavy runs a task through the queue"
    else
        fail "heavy cannot run a task: $(tail -n1 <<<"$out")"
        return 0
    fi
    local dir=$root/$project gate=scripts/check.sh tree key cache
    if [[ ! -x $dir/$gate ]]; then
        warn "$project has no $gate: its gate is not checked"
        return 0
    fi
    tree=$(git -C "$dir" rev-parse 'main^{tree}')
    key=$(printf '%s\n%s\n%s\n' "$tree" "$gate" "$(rustc -V 2>/dev/null || true)" | sha1sum | cut -c1-40)
    cache=$tmp/gates/$project/$key
    if [[ -f $cache ]]; then
        ok "$project's gate passed on main's tree: $(cat "$cache")"
    elif out=$(cd "$dir" && HEAVY_LABEL="$project gate" "$kit/heavy" --gate --tail 20 bash -c "$gate" 2>&1); then
        mkdir -p "$(dirname "$cache")"
        echo "plan check passed on main" >"$cache"
        ok "$project's gate passes on main"
    else
        fail "$project's gate fails on main: $(grep '^heavy: task ' <<<"$out" | tail -n1)"
    fi
}

# The plan: complete, committed, on origin, and ready in STATUS.md.
check_plan() {
    locate "$1"
    local file
    git -C "$top_dir" fetch --quiet origin || fail "$project: cannot fetch origin"
    for file in README.md code-organization.md; do
        [[ -f $plan_dir/$file ]] && ok "plan has $file" || fail "plan has no $file"
    done
    if [[ -n $(git -C "$top_dir" status --porcelain -- "$rel") ]]; then
        fail "plan has uncommitted changes"
    elif git -C "$top_dir" cat-file -e "origin/main:$rel/README.md" 2>/dev/null; then
        ok "plan is on origin/main"
    else
        fail "plan is not on origin/main"
    fi

    [[ -n $(design_inputs "$plan_dir/README.md") ]] ||
        warn "the README's Inputs name no docs/design/ or docs/foundation/ path: the metrics will count no design session"
    check_plan_readiness

    # The plan's row in its repository's STATUS.md: ready, or this
    # machine's run being resumed.
    local row state
    row=$(grep -E "^\| *$plan *\|" "$top_dir/docs/design/STATUS.md" 2>/dev/null | head -n1 || true)
    state=$(awk -F'|' '{ gsub(/^ +| +$/, "", $3); print $3 }' <<<"$row")
    case $state in
    "") warn "no row for $plan in $project's docs/design/STATUS.md" ;;
    ready | "active on $(host)"*) ok "STATUS.md says $state" ;;
    *) fail "STATUS.md says $state" ;;
    esac
    check_status_done "$top_dir/docs/design/STATUS.md"
}

# The items of section 2 of a status file still left to a plan that its
# section 1 marks done (development.md, section 3.3): "<doc> (<plan>)" for
# each, on one line. A plan is named as Left names it, ": <plan>" after
# its items, or "(<plan>" or "(the <plan> plan" in them; one named for its
# `later.md` is not.
done_items() {
    awk -F'|' '
        BEGIN {
            np = split(": \001(\001(the ", pre, "\001")
            nq = split(")\001;\001,\001 |\001 (\001 plan)\001 plan,\001 plan;", post, "\001")
        }
        /^## [0-9]+\./ { section = $0; sub(/^## /, "", section); sub(/\..*/, "", section); next }
        section == 1 && /^\|/ {
            name = $2; state = $3
            gsub(/^ +| +$/, "", name); gsub(/^ +| +$/, "", state)
            if (state ~ /^done/) done[++n] = name
            next
        }
        section == 2 && /^\| `/ {
            doc = $2; gsub(/^ +| +$/, "", doc)
            line = $0
            for (i = 1; i <= n; i++) {
                found = 0
                for (a = 1; a <= np && !found; a++)
                    for (b = 1; b <= nq && !found; b++)
                        if (index(line, pre[a] done[i] post[b])) found = 1
                if (found) printf "%s%s (%s)", (k++ ? ", " : ""), doc, done[i]
            }
        }' "$1" 2>/dev/null || true
}

check_status_done() {
    local items
    [[ -f $1 ]] || return 0
    items=$(done_items "$1")
    if [[ -n $items ]]; then
        warn "STATUS.md section 2 still leaves items to done plans: $items (the designer clears them, development.md, section 3.3)"
    else
        ok "STATUS.md section 2 leaves nothing to a done plan"
    fi
}

# The items of decisions.md's sections whose heading matches $1 that have
# no line starting $2 before the next item or heading.
open_items() {
    awk -v heading="$1" -v mark="^[[:space:]]*$2" '
        /^#/ { if (item && !answered) open++; item = 0; section = ($0 ~ heading); next }
        section && /^(- |[0-9]+\. )/ {
            if (item && !answered) open++
            item = 1; answered = 0
        }
        section && $0 ~ mark { answered = 1 }
        END { if (item && !answered) open++; print open + 0 }' "$plan_dir/decisions.md" 2>/dev/null || true
}

# Only base commands, the Design questions and the user's decisions decide
# readiness (development.md, section 5.4).
check_plan_readiness() {
    local open
    if awk '
        FNR == 1 { fenced = 0 }
        /^(```|~~~)/ { fenced = !fenced; next }
        fenced && /^\$ / { found = 1 }
        END { exit !found }' "$plan_dir"/findings/*.md 2>/dev/null; then
        ok "plan has a base run in findings/"
    else
        fail "plan has no base run in findings/ (development.md, section 5.4)"
    fi
    open=$(open_items '^## Design questions$' 'Answer:')
    if ((open)); then
        fail "plan has $open open design questions in decisions.md"
    else
        ok "plan has no open design questions in decisions.md"
    fi
    open=$(open_items '^#+ ([0-9.]+ )?For the user' 'Decision:')
    if ((open)); then
        fail "plan has undecided \"For the user\" items in decisions.md: $open (development.md, section 5.4)"
    else
        ok "plan has a decision on each \"For the user\" item in decisions.md"
    fi
}

# The last retrospective's proposals, each decided (development.md,
# sections 6 and 15.1): the retrospective added last to any repository's
# origin/main (its main, without one), other than this plan's own, has a row in its section 10 for
# each proposal of its section 8, and no row that is still open.
check_decisions() {
    local repo ref at path best_at=0 best_repo="" best_ref="" best_path="" text open where
    # shellcheck disable=SC2086 # the repositories, one word each
    for repo in $(printf '%s\n' $all_repos $repos | awk '!seen[$0]++'); do
        [[ -d $root/$repo/.git ]] || continue
        ref=origin/main
        git -C "$root/$repo" rev-parse --verify --quiet "$ref" >/dev/null || ref=main
        while read -r at path; do
            [[ -n $path && $repo/$path != "$project/$rel/retrospective.md" ]] || continue
            if ((at > best_at)); then best_at=$at; best_repo=$repo; best_ref=$ref; best_path=$path; fi
        done < <(git -C "$root/$repo" log "$ref" --diff-filter=A --format='%ct' --name-only \
            -- 'docs/development/plans/*/retrospective.md' 2>/dev/null |
            awk '/^[0-9]+$/ { at = $1; next } NF { print at, $0 }')
    done
    if [[ -z $best_path ]]; then
        ok "no retrospective yet: no proposal waits for a decision"
        return 0
    fi
    where="$best_repo's $(basename "$(dirname "$best_path")")"
    text=$(git -C "$root/$best_repo" show "$best_ref:$best_path")
    open=$(awk '
        /^## / { section = $2 }
        section == "8." && /^[0-9]+\. \*\*/ { n = $1; sub(/\.$/, "", n); order[++k] = n }
        section == "10." && /^\| *[0-9]+ *\|/ {
            split($0, f, "|"); n = f[2]; d = f[3]
            gsub(/^ +| +$/, "", n); gsub(/^ +| +$/, "", d)
            if (d != "" && tolower(d) !~ /^open/) decided[n] = 1
        }
        END { for (i = 1; i <= k; i++) if (!(order[i] in decided)) printf "%s%s", (j++ ? ", " : ""), order[i] }' <<<"$text")
    if ! grep -q '^## 10\. ' <<<"$text"; then
        fail "the last retrospective, $where, has no section 10 for the user's decisions (development.md, section 15.1)"
    elif [[ -n $open ]]; then
        fail "the last retrospective, $where, has proposals not yet decided: $open (its section 10; development.md, section 15.1)"
    else
        ok "the last retrospective's proposals are decided ($where)"
    fi
}

# Codex's daemon, and its updater, keep the environment of whoever starts
# them, and give it to every command the coordinator runs. Started from an
# agent's session, they would carry its variables (CODEX_CI, CODEX_THREAD_ID,
# CLAUDECODE, ...). So start them from a clean login shell, whoever calls.
codex_daemon_start() {
    local var pass=()
    for var in HOME USER LOGNAME LANG XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS \
        CODEX_HOME DEV_ROOT DEV_TMP DEV_REPOS; do
        [[ -z ${!var:-} ]] || pass+=("$var=${!var}")
    done
    # shellcheck disable=SC2016 # $1 expands in the login shell
    env -i "${pass[@]}" SHELL=/bin/bash \
        bash -lc 'cd "$1" && codex app-server daemon start' _ "$root" >/dev/null 2>&1
}

# The queue's user unit is repairable without restarting a running daemon.
check_units() {
    local unit=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/pueued.service
    if ((bus)) && { ! grep -qsx 'StartLimitIntervalSec=0' "$unit" ||
        ! grep -qsx 'Restart=always' "$unit" || ! grep -qsx 'RestartSec=5' "$unit"; }; then
        warn "pueued's unit needs StartLimitIntervalSec=0, Restart=always and RestartSec=5 (--fix writes it)"
        if ((fix)); then
            mkdir -p "$(dirname "$unit")"
            printf '%s\n' '[Unit]' 'Description=pueue daemon' 'StartLimitIntervalSec=0' '' '[Service]' \
                "ExecStart=$(command -v pueued) --verbose" 'Restart=always' 'RestartSec=5' '' \
                '[Install]' 'WantedBy=default.target' >"$unit"
            systemctl --user daemon-reload
        fi
    fi
    if ((!bus)); then
        if pueue status >/dev/null 2>&1; then
            ok "pueued is running (no systemd user bus: plan resume starts it after a restart)"
        elif ((fix)); then
            pueued --daemonize >/dev/null 2>&1 && sleep 1
            pueue status >/dev/null 2>&1 && ok "pueued started" || fail "pueued does not start"
        else
            fail "pueued is not running (--fix starts it)"
        fi
    elif systemctl --user is-active --quiet pueued; then
        ok "pueued is running as a user service"
        if [[ $(systemctl --user show pueued -p UnitFileState --value) != enabled ]]; then
            if ((fix)); then systemctl --user enable --quiet pueued
            else warn "pueued's unit is not enabled: it will not start after a reboot (--fix enables it)"; fi
        fi
    elif ((fix)); then
        systemctl --user enable --quiet pueued
        systemctl --user restart pueued && sleep 1
        systemctl --user is-active --quiet pueued && ok "pueued started" || fail "pueued does not start"
    else
        fail "pueued is not running as a user service (--fix starts it)"
    fi
    ((!bus)) || [[ $(loginctl show-user "$USER" -p Linger --value 2>/dev/null) == yes ]] ||
        warn "no lingering: user services stop when $USER logs out (loginctl enable-linger)"
}

# The tools, the workspace's files, the queue and room to work.
check_machine() {
    # Tools.
    local tool
    for tool in git jq pueue flock cargo; do
        command -v "$tool" >/dev/null && ok "has $tool" || fail "no $tool"
    done
    cargo nextest --version >/dev/null 2>&1 && ok "has cargo-nextest" || fail "no cargo-nextest"
    # The tools and files link to the kit on PATH, never to a run's (kit.sh).
    local bin=$HOME/.local/bin home=${DEV_KIT_HOME:-$kit}
    [[ $home != "$(readlink -f "$tmp")"/*/*/kit/scripts/dev ]] || home=$root/skein/scripts/dev
    for tool in heavy land plan test-plan; do
        if [[ $(readlink -f "$bin/$tool" 2>/dev/null) == "$home/$tool" ]]; then
            ok "$tool is on PATH"
        elif ((fix)); then
            mkdir -p "$bin" && ln -sfn "$home/$tool" "$bin/$tool" && ok "linked $bin/$tool"
        else
            fail "$bin/$tool does not link to $home/$tool (--fix links it)"
        fi
    done

    # The workspace's own files live in skein, beside the tools.
    local pair link target
    for pair in "$root/AGENTS.md=AGENTS.md" "$root/CLAUDE.md=CLAUDE.md" \
        "$root/.claude/settings.json=claude-settings.json" \
        "${CODEX_HOME:-$HOME/.codex}/coordinator.config.toml=coordinator.config.toml"; do
        link=${pair%%=*}
        target=$home/workspace/${pair#*=}
        if [[ $(readlink -f "$link" 2>/dev/null) == "$target" ]]; then
            ok "$link is skein's"
        elif [[ -e $link && ! -L $link ]]; then
            fail "$link is a file of its own; replace it with a link to $target"
        elif ((fix)); then
            mkdir -p "$(dirname "$link")" && ln -sfn "$target" "$link" && ok "linked $link"
        else
            fail "$link does not link to $target (--fix links it)"
        fi
    done

    # Cargo reaches the workspace's repositories on GitHub.
    if grep -qs 'git\.ekanayaka\.io' "$root"/*/Cargo.toml; then
        git config --global --get-regexp '^url\..*\.insteadof$' 2>/dev/null |
            grep -q ' https://git\.ekanayaka\.io/ai/$' &&
            ok "git rewrites git.ekanayaka.io to GitHub" ||
            fail "Cargo.toml names git.ekanayaka.io: git config --global url.git@github.com:forgetic/.insteadOf https://git.ekanayaka.io/ai/"
    fi
    grep -qs 'git-fetch-with-cli *= *true' "$HOME/.cargo/config.toml" &&
        ok "cargo fetches with git" ||
        fail "cargo must fetch with git: [net] git-fetch-with-cli = true in ~/.cargo/config.toml"
    local bus=0
    systemctl --user show-environment >/dev/null 2>&1 && bus=1
    if grep -qs kache "$HOME/.cargo/config.toml"; then
        if ((bus)); then
            systemctl --user is-active --quiet kache && ok "kache is running" ||
                warn "kache is configured but not running"
        else
            ok "kache is configured"
        fi
    else
        warn "no kache rustc-wrapper in ~/.cargo/config.toml"
    fi

    # The queue: the daemon as a user service, and its two lanes. Without a
    # systemd user bus (some containers), pueued runs on its own, and `plan
    # resume` starts it again after a restart.
    check_units
    if apt-config dump 2>/dev/null | grep -E '^Unattended-Upgrade::Automatic-Reboot "true";' >/dev/null; then
        warn 'the machine may reboot unattended (Unattended-Upgrade::Automatic-Reboot)'
    fi
    local group want have status slots slots_source line source
    read -r slots slots_source < <(machine_setting HEAVY_BUILD_SLOTS 2)
    while IFS= read -r line; do warn "machine.env: ignored line $line"; done < <(
        awk 'NF && $0 !~ /^(PLAN_MIN_MEM_GB|PLAN_MIN_DISK_GB|HEAVY_BUILD_SLOTS|HEAVY_BUILD_JOBS)=[0-9]+$/ { print NR ": " $0 }' "$tmp/machine.env" 2>/dev/null || true)
    for group in gate build; do
        want=1 source=default
        [[ $group == build ]] && { want=$slots; source=$slots_source; }
        have=$(pueue group --json 2>/dev/null | jq -r --arg g "$group" '.[$g].parallel_tasks // empty')
        if [[ -z $have ]] && ((fix)); then
            pueue group add "$group" >/dev/null
            pueue parallel "$want" --group "$group" >/dev/null
            have=$want
        fi
        if [[ -z $have ]]; then
            fail "queue lane $group is missing (--fix adds it)"
            continue
        fi
        if [[ $have != "$want" ]] && ((fix)); then
            pueue parallel "$want" --group "$group" >/dev/null && have=$want
        fi
        [[ $have == "$want" ]] && ok "queue lane $group runs $want at once ($source)" ||
            warn "queue lane $group runs $have at once, not $want ($source; --fix sets it)"
        status=$(pueue group --json | jq -r --arg g "$group" '.[$g].status')
        if [[ $status != Running ]] && ((fix)); then
            pueue start --group "$group" >/dev/null && status=Running
        fi
        [[ $status == Running ]] && ok "queue lane $group is running" ||
            fail "queue lane $group is $status (--fix starts it)"
    done

    # Room to work.
    local disk mem disk_min disk_source mem_min mem_source
    read -r disk_min disk_source < <(machine_setting PLAN_MIN_DISK_GB 40)
    read -r mem_min mem_source < <(machine_setting PLAN_MIN_MEM_GB 6)
    disk=$(df -BG --output=avail "$root" | tail -n1 | tr -dc 0-9)
    mem=$(awk '/MemAvailable/ { print int($2 / 1048576) }' /proc/meminfo)
    ((disk >= disk_min)) && ok "disk: ${disk} GB free, want $disk_min ($disk_source)" ||
        fail "disk: ${disk} GB free, want $disk_min ($disk_source)"
    ((mem >= mem_min)) && ok "memory: ${mem} GB available, want $mem_min ($mem_source)" ||
        fail "memory: ${mem} GB available, want $mem_min ($mem_source)"

    # The roles' programs: Claude Code trusts the workspace, where the
    # supervisor starts, and Codex's daemon answers.
    if command -v claude >/dev/null; then
        jq -e --arg d "$claude_dir" '.projects[$d].hasTrustDialogAccepted == true' "$HOME/.claude.json" >/dev/null 2>&1 &&
            ok "Claude Code trusts $claude_dir" ||
            fail "Claude Code does not trust $claude_dir: run claude there once, and accept"
    else
        fail "no claude"
    fi
    if command -v codex >/dev/null; then
        codex app-server daemon version >/dev/null 2>&1 || { ((fix)) && codex_daemon_start; } || true
        codex app-server daemon version 2>/dev/null | jq -e '.status == "running"' >/dev/null &&
            ok "Codex's app-server daemon is running" ||
            fail "Codex's app-server daemon is not running (--fix starts it)"
    else
        fail "no codex"
    fi

    # The coordinator's Codex profile.
    local profile=${CODEX_HOME:-$HOME/.codex}/coordinator.config.toml
    grep -qs max_threads "$profile" && ok "Codex profile coordinator sets max_threads" ||
        fail "no Codex profile at $profile setting [agents] max_threads"

}

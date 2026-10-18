# shellcheck shell=bash
# shellcheck disable=SC2154,SC2034 # test-plan owns these checks and fixtures
# Sourced by test-plan for its fake daemon and watch checks.

# The coordinator's daemon, kept in this world's state directory.
export PLAN_CODEX_FAKE=$t/codex-fake
mkdir -p "$PLAN_CODEX_FAKE/turns"
"$plan" idle "the ring test" >/dev/null
expect "ring loads an unloaded thread" "$("$plan" ring hello)" "woke the idle coordinator: hello"
expect "ring loads before sending" "$(cut -d' ' -f1 "$PLAN_CODEX_FAKE/calls" | paste -sd, -)" \
    "thread/loaded/list,thread/resume,thread/read,turn/start"
expect "ring records its load" "$(cat "$run/log.md")" ": by plan ring"
: >"$PLAN_CODEX_FAKE/calls"
expect "ring steers a turn in progress" "$("$plan" ring hello)" "rang the coordinator: hello"
expect "the ring names the supervisor" "$(cat "$PLAN_CODEX_FAKE/calls")" '"text":"From the supervisor, by plan ring: hello"'
printf 'turn/steer\n' >"$PLAN_CODEX_FAKE/refuse"
: >"$PLAN_CODEX_FAKE/calls"
expect "a refused steer loads and sends again" "$("$plan" ring again 2>&1)" "rang the coordinator: again"
expect "a refused steer retries once" "$(cut -d' ' -f1 "$PLAN_CODEX_FAKE/calls" | paste -sd, -)" \
    "thread/loaded/list,thread/read,turn/steer,thread/resume,turn/steer"
printf 'turn-1\tcompleted\n' >"$PLAN_CODEX_FAKE/turns/$coordinator"
ring_result=$("$plan" ring stopped 2>&1) && ring_code=0 || ring_code=$?
expect "ring with no turn says resume" "$ring_code: $ring_result" "3: plan: the coordinator has no turn in progress: plan resume"

# The watch, each of its events.
watch() { timeout 10 "$plan" watch || echo "watch exited $?"; }
# One call gathers all open events; a second call has none to show.
: >"$run/.watch"
printf '# Blocked\n\n## alpha 01.2: an open question\n' >"$run/blocked.md"
echo '# Answer' >"$run/answer.md"
"$plan" set alpha-01.2 state=check
expect "the first watch prints every open event" "$(watch)" "blocked:" "answer:" "check: alpha-01.2"
# Three polls suffice to show that no event is repeated (PLAN_POLL=0.1).
quiet_watch=$(timeout 0.3 "$plan" watch) && quiet_status=0 || quiet_status=$?
expect "the next watch repeats no event" "status=$quiet_status output=$quiet_watch" "status=124 output="
rm "$run/blocked.md" "$run/answer.md"
"$plan" set alpha-01.2 state=landed
: >"$run/.watch"
echo "# Blocked" >"$run/blocked.md"; echo "## alpha 01.3: a question, $stamp" >>"$run/blocked.md"
expect "watch blocked" "$(watch)" "blocked: $run/blocked.md" "## alpha 01.3: a question"
echo "# Answer" >"$run/answer.md"
expect "watch answer" "$(watch)" "answer: $run/answer.md"
rm "$run/blocked.md" "$run/answer.md"
touch -d '-5 seconds' "$run/heartbeat"
expect "watch quiet" "$(PLAN_QUIET=2 watch)" "quiet: no heartbeat"
expect "watch quiet again" "$(PLAN_QUIET=2 watch)" "quiet again: no heartbeat"
touch "$run/heartbeat"
echo "# Paused" >"$run/paused.md"
expect "watch paused" "$(PLAN_COMPACTIONS=3 watch)" "paused: $run/paused.md" "context: the supervisor's is 1K (compactions: 2)"
rm "$run/paused.md"
"$plan" set alpha-02.1 repo=alpha session=02 state=check title=third
expect "status check" "$("$plan" status)" "1 to check"
expect "watch check" "$(watch)" "check: alpha-02.1, $dir/alpha/02-second.md" "(compactions: 2): hand over now"
expect "ready" "$(PLAN_DRY_RUN=1 "$plan" ready alpha-02.1)" "would ring the coordinator: ready: alpha-02.1"
expect "ready in the ledger" "$(grep alpha-02.1 "$run/ledger.tsv")" $'\tready\t' "checked "
expect "ready twice" "$(PLAN_DRY_RUN=1 "$plan" ready alpha-02.1 2>&1 || true)" "is not waiting for its check"
# An idle coordinator's silence is no event.
expect idle "$("$plan" idle "waiting for the user")" "end your turn now"
touch -d '-5 seconds' "$run/heartbeat"
expect "watch idle" "$(PLAN_QUIET=2 timeout 3 "$plan" watch || echo "watch exited $?")" "watch exited 124"
rm "$run/idle.md"
touch "$run/heartbeat"

# A report, logged by its implementer.
report=$(printf 'Session: alpha 01\nStep: alpha-01.3, landed\n' | "$plan" log alpha 01)
expect "log" "$report" "$run/reports/alpha-01-"
expect "logged" "$(tail -n 5 "$run/log.md")" "## alpha 01, " "Step: alpha-01.3, landed"

# A ruling, written once in the draft unblock, then landed in decisions.md
# with the plan edit waiting in the supervisor's worktree.
printf '# Unblock\n\nAnswer to the blocked report of 2026-10-11T10:00:00Z.\nReverses: nothing.\n\n## Decision\n\nDo the thing, as ruling {ruling} says.\n\n## Landed\n\n## Instructions\n\n1. Go on.\n' >"$run/unblock.draft.md"
docs=$("$plan" docs)
echo "Amended." >>"$docs/docs/development/plans/demo/alpha/01-first.md"
ruled=$(PLAN_DRY_RUN=1 "$plan" ruling "The thing, ruling {ruling}" "plan defect, alpha 01's block of 2026-10-11T10:00:00Z" 2>&1)
expect ruling "$ruled" "ruling 101 landed as alpha " "would ring the coordinator: unblock.md is ready"
expect "ruling in decisions.md" "$(git -C "$repo" show main:docs/development/plans/demo/decisions.md)" \
    "101. **The thing, ruling 101** (plan defect, alpha 01's block of 2026-10-11T10:00:00Z)." "     Do the thing, as ruling 101 says." $'\n\n## For the user'
expect "ruling's plan edit" "$(git -C "$repo" show main:docs/development/plans/demo/alpha/01-first.md)" "Amended."
expect "unblock from the draft" "$(cat "$run/unblock.md")" "## Landed" "ruling 101, in alpha/01-first.md, decisions.md." "1. Go on."
printf 'The second.\n' >"$t/ruling.txt"
expect "ruling from a file" "$("$plan" ruling --text "$t/ruling.txt" "Second" "ready check of alpha-02.1" 2>/dev/null)" "ruling 102 landed as alpha "
rm "$run/unblock.md"

# The user's decisions on the last retrospective's proposals.
older=$repo/docs/development/plans/older
mkdir -p "$older"
printf '# Retrospective\n\n## 8. Proposals\n\n1. **One.** Why.\n\n2. **Two.** Why.\n   1. A sub-item.\n\n## 10. Decisions\n\n| Proposal | Decision | Where |\n|---|---|---|\n| 1 | adopted | here |\n| 2 | open | |\n' >"$older/retrospective.md"
git -C "$repo" add -A
git -C "$repo" commit -q -m "An older retrospective"
sed -i '/^## Inputs/,/^$/d' "$dir/README.md"
expect "Inputs with no design warn" "$(PLAN_NO_GATE=1 "$plan" check "$dir" 2>&1 || true)" "warn  the README's Inputs name no docs/design/ or docs/foundation/ path"
git -C "$repo" checkout -- "$dir/README.md"
expect "decisions open" "$(PLAN_NO_GATE=1 "$plan" check "$dir" 2>&1 || true)" "FAIL  the last retrospective, alpha's older, has proposals not yet decided: 2"
sed -i 's/^| 2 | open | |$/| 2 | rejected: not now | |/' "$older/retrospective.md"
git -C "$repo" commit -q -am "Decide the older retrospective's proposals"
expect "decisions made" "$(PLAN_NO_GATE=1 "$plan" check "$dir" 2>&1 || true)" "ok    the last retrospective's proposals are decided (alpha's older)"

# A plan is ready only after its base commands and design answers.
unfinished=$repo/docs/development/plans/unfinished
cp -r "$dir" "$unfinished"
rm "$unfinished/findings/base-runs.md"
expect "a plan with no base run is refused" "$(PLAN_NO_GATE=1 "$plan" check "$unfinished" 2>&1 || true)" "FAIL  plan has no base run in findings/ (development.md, section 5.4)"
cp "$dir/findings/base-runs.md" "$unfinished/findings/base-runs.md"
printf '\n## Design questions\n\n1. Which?\n' >>"$unfinished/decisions.md"
expect "an open question is refused" "$(PLAN_NO_GATE=1 "$plan" check "$unfinished" 2>&1 || true)" "FAIL  plan has 1 open design question"
printf '\nAnswer: this.\n' >>"$unfinished/decisions.md"
expect "an answered question passes" "$(PLAN_NO_GATE=1 "$plan" check "$unfinished" 2>&1 || true)" "ok    plan has a base run in findings/" "ok    plan has no open design questions in decisions.md"
rm -rf "$unfinished"

# Per-machine minimums survive a resume; an environment value takes priority.
printf '%s\n' 'PLAN_MIN_MEM_GB=1' 'BOGUS=3' >"$DEV_TMP/machine.env"
expect "the machine file sets a minimum" "$(PLAN_NO_GATE=1 "$plan" check "$dir" 2>&1 || true)" "want 1 (machine.env)" "warn  machine.env: ignored line 2: BOGUS=3"
expect "the environment wins" "$(PLAN_MIN_MEM_GB=2 PLAN_NO_GATE=1 "$plan" check "$dir" 2>&1 || true)" "want 2 (environment)"
rm "$DEV_TMP/machine.env"
# A landing's checks use the session's own worktree and leave refusals alone.
cp "$run/ledger.tsv" "$t/ledger.before-landed"
git clone -q --bare "$repo" "$t/alpha.git"
git -C "$repo" remote add origin "$t/alpha.git"
git -C "$repo" worktree add -q -b b4 "$run/wt/alpha-01" main
"$plan" set alpha-01.4 repo=alpha session=01 state=running branch=b4
landed_before=$(cat "$run/ledger.tsv")
landing_commit() {
    git -C "$run/wt/alpha-01" commit -q --allow-empty -m "Landing" -m "Step: $1"
    landing=$(git -C "$run/wt/alpha-01" rev-parse HEAD)
}
refuse_landing() {
    local what=$1 reason=$2 status=0 out
    out=$("$plan" landed alpha-01.4 "$landing" 2>&1) || status=$?
    expect "$what" "$status $out" '1 ' "$reason"
    expect "$what leaves the ledger" "$(if [[ $(cat "$run/ledger.tsv") == "$landed_before" ]]; then echo unchanged; fi)" unchanged
}
landing_commit alpha-01.4
refuse_landing "landed refuses a commit off main" 'is not on origin/main'
landing_commit alpha-01.3
git -C "$run/wt/alpha-01" push -q origin HEAD:main
refuse_landing "landed refuses another step's commit" 'does not have Step: alpha-01.4'
landing_commit alpha-01.4
git -C "$run/wt/alpha-01" push -q origin HEAD:main
echo dirty >>"$run/wt/alpha-01/src/big.rs"
refuse_landing "landed refuses a dirty worktree" 'worktree on b4 is dirty'
git -C "$run/wt/alpha-01" restore src/big.rs
expect "landed sets a checked step" "$("$plan" landed alpha-01.4 "$landing")" "landed alpha-01.4 $landing"
expect "landed records its commit" "$(awk -F'\t' '$1 == "alpha-01.4" { print $4, $8 }' "$run/ledger.tsv")" "landed $landing"
git -C "$repo" worktree remove "$run/wt/alpha-01"
git -C "$repo" branch -D b4 >/dev/null
git -C "$repo" remote remove origin
mv "$t/ledger.before-landed" "$run/ledger.tsv"


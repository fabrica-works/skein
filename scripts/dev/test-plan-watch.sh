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
ring_result=$(capture status "$plan" ring stopped)
expect "ring with no turn says resume" "$ring_result" "status=3" "plan: the coordinator has no turn in progress: plan resume"

# The watch, each of its events.
watch() { PATH=$t/stub:$PATH timeout 10 "$plan" watch || echo "watch exited $?"; }
# A bounded observation of three polls (PLAN_POLL=0.1).
silent_watch() {
    local captured output
    captured=$(PATH=$t/stub:$PATH capture status timeout 0.3 "$plan" watch)
    output=${captured#"${captured%%$'\n'*}"}
    output=${output#$'\n'}
    printf '%s bytes=%s' "${captured%%$'\n'*}" "${#output}"
}
fake_starts() { awk '$1 == "turn/start" { count++ } END { print count+0 }' "$PLAN_CODEX_FAKE/calls"; }
# One call gathers all open events; a second call has none to show.
: >"$run/.watch"
printf '# Blocked\n\n## alpha 01.2: an open question\nThe block needs an answer.\n' >"$run/blocked.md"
echo '# Answer' >"$run/answer.md"
"$plan" set alpha-01.2 state=check
first_watch=$(watch)
expect "the first watch prints every open event" "$first_watch" "blocked:" "answer:" "check: alpha-01.2"
expect "blocked prints the increment" "$first_watch" "  The block needs an answer." $'ledger: alpha-01.2\t' "  ### 01.2 The second increment" "session: $dir/alpha/01-first.md"
# Three polls suffice to show that no event is repeated (PLAN_POLL=0.1).
expect "the next watch repeats no event" "$(silent_watch)" "status=124 bytes=0"
rm "$run/blocked.md" "$run/answer.md"
"$plan" set alpha-01.2 state=landed
: >"$run/.watch"
echo "# Blocked" >"$run/blocked.md"; echo "## alpha 01.3: a question, $stamp" >>"$run/blocked.md"
expect "watch blocked" "$(watch)" "blocked: $run/blocked.md" "## alpha 01.3: a question"
echo "# Answer" >"$run/answer.md"
expect "answer prints its text" "$(watch)" "answer: $run/answer.md" "  # Answer"
# development.md 10.5 bounds material at 150 lines, including the bound.
while read -r rows expected_omitted; do
    awk -v n="$rows" 'BEGIN { for (i=1;i<=n;i++) print "line" }' >"$run/answer.md"
    touch -d "@$rows" "$run/answer.md"
    material=$(watch)
    omitted=$(sed -nE 's/^  … \(([0-9]+) more lines:.*/\1/p' <<<"$material")
    expect "event material is bounded at rows=$rows" \
        "lines=$(grep -c '^  line$' <<<"$material") omitted=${omitted:-0}" "lines=150 omitted=$expected_omitted"
done <<'ROWS'
150 0
151 1
ROWS
rm "$run/blocked.md" "$run/answer.md"
touch -d '-5 seconds' "$run/heartbeat"
expect "watch quiet" "$(PLAN_QUIET=2 watch)" "quiet: no heartbeat"
expect "watch quiet again" "$(PLAN_QUIET=2 watch)" "quiet again: no heartbeat"
touch "$run/heartbeat"
echo "# Paused" >"$run/paused.md"
expect "watch paused" "$(watch)" "paused: $run/paused.md" "context: the supervisor's is 1K (compactions: 2)"
rm "$run/paused.md"
"$plan" set alpha-02.1 repo=alpha session=02 state=check title=third
expect "status check" "$("$plan" status)" "1 to check"
check_watch=$(watch)
expect "watch check" "$check_watch" "check: alpha-02.1, $dir/alpha/02-second.md" "(compactions: 2)"
expect "compactions never hand over" "$(if [[ $check_watch != *"hand over"* ]]; then echo absent; fi)" absent
expect "ready" "$(PLAN_DRY_RUN=1 "$plan" ready alpha-02.1)" "would ring the coordinator: ready: alpha-02.1"
expect "ready in the ledger" "$(grep alpha-02.1 "$run/ledger.tsv")" $'\tready\t' "checked "
expect "ready twice" "$(PLAN_DRY_RUN=1 "$plan" ready alpha-02.1 2>&1 || true)" "is not waiting for its check"
# The ready check's commit precedes a file now on origin/main.
checked=$(git -C "$repo" rev-parse HEAD)
git clone -q --bare "$repo" "$t/watch-origin.git"
git -C "$repo" remote add origin "$t/watch-origin.git"
"$plan" set alpha-01.2 state=ready note="checked alpha $checked"
# The watch sees ready and retains its note before the coordinator changes it.
expect "ready waits for the start" "$(silent_watch)" "status=124 bytes=0"
printf 'fn watch() {}\n' >"$repo/src/watch.rs"
git -C "$repo" add src/watch.rs
git -C "$repo" commit -qm "Move after the ready check"
git -C "$repo" push -q origin main
"$plan" set alpha-01.2 state=running note=started
expect "started prints what moved" "$(watch)" "started: alpha-01.2" "src/watch.rs"
"$plan" set alpha-01.2 state=landed
git -C "$repo" remote remove origin
# An idle coordinator's silence is no event.
expect idle "$("$plan" idle "waiting for the user")" "end your turn now"
touch -d '-5 seconds' "$run/heartbeat"
expect "watch idle" "$(PLAN_QUIET=2 silent_watch)" "status=124 bytes=0"
rm "$run/idle.md"
touch "$run/heartbeat"

# A bare heading still dates the next check after an interrupted check.
old=$(date -u -d '-66 minutes' +%FT%TZ)
for heading in Check Process; do
    : >"$run/.watch"
    printf '## %s, %s%s\n' "$heading" "$old" "$(if [[ $heading == Process ]]; then echo ': skein abc123'; fi)" >"$run/supervision.md"
    hourly=$(watch)
    expect "hourly after 65 minutes from $heading" "$hourly" "hourly: $run/supervision.md since $old"
    expect "hourly is printed once for $heading" "$(silent_watch)" "status=124 bytes=0"
done
rm "$run/supervision.md"
touch "$t/pueue.down"
down=$(watch)
expect "the queue down alerts" "$down" "alert: pueued is down"
expect "the queue down alerts once" "$(silent_watch)" "status=124 bytes=0"
expect "the outage is recorded once" "rows=$(wc -l <"$run/alerts.md")" "rows=1"
rm "$t/pueue.down"
expect "the recovered queue stays quiet" "$(silent_watch)" "status=124 bytes=0"
touch "$t/pueue.down"
down=$(watch)
expect "a second outage alerts again" "$down" "alert: pueued is down"
expect "the second outage is recorded" "rows=$(wc -l <"$run/alerts.md")" "rows=2"
rm "$t/pueue.down"

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
# A ruling that did not land stays in the worktree, which plan refuses to
# move, or to rule in, until it lands or goes.
git -C "$docs" commit -q --allow-empty -m "Ruling 103"
expect "an unlanded ruling" "$(capture status "$plan" ruling --text "$t/ruling.txt" "Third" "ready check of alpha-02.1")" \
    "status=2" "has commits that main lacks"
expect "kept" "$(git -C "$docs" log -1 --format=%s)" "Ruling 103"
git -C "$docs" reset -q --hard main

# The user's decisions on the last retrospective's proposals.
older=$repo/docs/development/plans/older
mkdir -p "$older"
printf '# Retrospective\n\n## 8. Proposals\n\n1. **One.** Why.\n\n2. **Two.** Why.\n   1. A sub-item.\n\n## 10. Decisions\n\n| Proposal | Decision | Where |\n|---|---|---|\n| 1 | adopted | here |\n| 2 | open | |\n' >"$older/retrospective.md"
git -C "$repo" add -A
git -C "$repo" commit -q -m "An older retrospective"
sed -i '/^## Inputs/,/^$/d' "$dir/README.md"
open_decisions=$(plan_check "$dir")
expect "Inputs with no design warn" "$open_decisions" "warn  the README's Inputs name no docs/design/ or docs/foundation/ path"
git -C "$repo" checkout -- "$dir/README.md"
expect "decisions open" "$open_decisions" "FAIL  the last retrospective, alpha's older, has proposals not yet decided: 2"
sed -i 's/^| 2 | open | |$/| 2 | rejected: not now | |/' "$older/retrospective.md"
git -C "$repo" commit -q -am "Decide the older retrospective's proposals"
expect "an adopted proposal not applied" "$(plan_check "$dir")" "FAIL  the last retrospective, alpha's older, has adopted proposals not yet applied: 1 (no commit on main"
echo "This run applies older's proposal 1 in its first session." >>"$dir/README.md"
expect "a README that applies it" "$(plan_check "$dir")" "ok    the last retrospective's proposals are decided (alpha's older)"
git -C "$repo" checkout -- "$dir/README.md"
sed -i "s/^| 1 | adopted | here |\$/| 1 | adopted | alpha $(git -C "$repo" rev-parse --short HEAD) |/" "$older/retrospective.md"
git -C "$repo" commit -q -am "Say where the older retrospective's proposal went"

# A plan is ready only after its base commands and design answers.
unfinished=$repo/docs/development/plans/unfinished
cp -r "$dir" "$unfinished"
rm "$unfinished/findings/base-runs.md"
printf '\n## Design questions\n\n1. Which?\n' >>"$unfinished/decisions.md"
sed -i 's/^Nothing\.$/- **A choice.** Its default./' "$unfinished/decisions.md"
unfinished_check=$(plan_check "$unfinished")
expect "a plan with no base run is refused" "$unfinished_check" "FAIL  plan has no base run in findings/ (development.md, section 5.4)"
expect "an open question is refused" "$unfinished_check" "FAIL  plan has 1 open design question"
expect "an undecided item for the user is refused" "$unfinished_check" 'FAIL  plan has undecided "For the user" items in decisions.md: 1'
cp "$dir/findings/base-runs.md" "$unfinished/findings/base-runs.md"
printf '\nAnswer: this.\n' >>"$unfinished/decisions.md"
sed -i '/^- \*\*A choice\.\*\*/a\  Decision: no veto, by the director, 2026-10-18.' "$unfinished/decisions.md"

# Per-machine minimums survive a resume; an environment value takes priority.
printf '%s\n' 'PLAN_MIN_MEM_GB=1' 'BOGUS=3' >"$DEV_TMP/machine.env"
answered_check=$(plan_check "$unfinished")
expect "decisions made" "$answered_check" "ok    the last retrospective's proposals are decided (alpha's older)"
expect "an answered question passes" "$answered_check" "ok    plan has a base run in findings/" "ok    plan has no open design questions in decisions.md" \
    'ok    plan has a decision on each "For the user" item in decisions.md'
expect "the machine file sets a minimum" "$answered_check" "want 1 (machine.env)" "warn  machine.env: ignored line 2: BOGUS=3"
expect "the environment wins" "$(PLAN_MIN_MEM_GB=2 plan_check "$dir")" "want 2 (environment)"
rm "$DEV_TMP/machine.env"
rm -rf "$unfinished"
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
    local what=$1 reason=$2 out
    out=$(capture status "$plan" landed alpha-01.4 "$landing")
    expect "$what" "$out" 'status=1' "$reason"
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
expect "landed records its commit" "$(awk -F'\t' '$1 == "alpha-01.4" { print "state=" $4 " commit=" $8 }' "$run/ledger.tsv")" "state=landed commit=$landing"
git -C "$repo" worktree remove "$run/wt/alpha-01"
git -C "$repo" branch -D b4 >/dev/null
git -C "$repo" remote remove origin
mv "$t/ledger.before-landed" "$run/ledger.tsv"

# The coordinator waits for a named queue task, then receives its completion.
task=12
: >"$PLAN_CODEX_FAKE/calls"
"$plan" idle "the gate" --tasks "$task" >/dev/null
printf '{"tasks":{"%s":{"status":{"Running":{}}}}}\n' "$task" >"$t/pueue.json"
expect "running tasks keep the coordinator idle" "$(silent_watch); starts=$(fake_starts)" "status=124 bytes=0; starts=0"
printf '{"tasks":{"%s":{"status":{"Done":{}}}}}\n' "$task" >"$t/pueue.json"
expect "completed tasks keep the supervisor watching" "$(silent_watch)" "status=124 bytes=0"
expect "the watch rings when the tasks end" "starts=$(fake_starts); idle=$(if [[ -f $run/idle.md ]]; then echo present; else echo removed; fi)" "starts=1; idle=removed"
expect "the queue wake names the supervisor" "$(cat "$PLAN_CODEX_FAKE/calls")" 'From the supervisor, by plan watch: the queue tasks you wait on have ended: 12'

# Each role waits for landings, its run's or another's, and is told once
# their Plan and Step trailers have all reached origin/main.
git clone -q --bare "$repo" "$t/await.git"
git -C "$repo" remote add origin "$t/await.git"
git clone -q "$t/await.git" "$t/await"
land_step() { git -C "$t/await" -c user.email=test@example.com -c user.name=test commit -q --allow-empty -m Landing -m "$1"; git -C "$t/await" push -q origin HEAD:main; }
printf 'turn-1\tcompleted\n' >"$PLAN_CODEX_FAKE/turns/$coordinator"
: >"$PLAN_CODEX_FAKE/calls"
"$plan" idle "other's 03.1" >/dev/null
"$plan" await coordinator alpha-01.5 other/alpha-03.1 >/dev/null
"$plan" await supervisor other/alpha-03.1 >/dev/null
land_step $'Plan: demo\nStep: alpha-01.5'
land_step $'Plan: demo\nStep: alpha-03.1'
expect "a landing still awaited tells no one" "$(silent_watch); starts=$(fake_starts)" "status=124 bytes=0; starts=0"
land_step $'Plan: other\nStep: alpha-03.1'
expect "the landings tell the supervisor" "$(watch)" "landed: other/alpha-03.1"
expect "and ring the coordinator" "starts=$(fake_starts); idle=$(if [[ -f $run/idle.md ]]; then echo present; else echo removed; fi); awaits=$(wc -c <"$run/awaits.tsv")" \
    "starts=1; idle=removed; awaits=0"
expect "the landing wake names the steps" "$(cat "$PLAN_CODEX_FAKE/calls")" 'the landings you wait on have reached origin/main: demo/alpha-01.5 other/alpha-03.1'
git -C "$repo" remote remove origin
rm -rf "$t/await" "$run/awaits.tsv"

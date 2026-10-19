# shellcheck shell=bash
# shellcheck disable=SC2154,SC2034 # test-plan owns these checks and fixtures
# Sourced by test-plan for its part "roles": land, check, the roles, the
# release and the numbers after it.

# Land against an origin owned by this world, never a remote checkout.
git init -q --bare --initial-branch=main "$t/origin.git"
git clone -q "$t/origin.git" "$t/clone" 2>/dev/null
git -C "$t/clone" config user.email test@example.com
git -C "$t/clone" config user.name test
echo '# Start' >"$t/clone/README.md"
git -C "$t/clone" add README.md
git -C "$t/clone" commit -qm Start
git -C "$t/clone" push -q origin main
land_branch() {
    git -C "$t/clone" switch -qc "$1" origin/main
    echo "$1" >>"$t/clone/README.md"
    git -C "$t/clone" add README.md
    git -C "$t/clone" commit -qm "$2"
}
land_run() (cd "$t/clone" || exit; DEV_TMP=$t/land-tmp LAND_PREBUILD='' "$kit/land" "$@")
land_branch locked 'A Markdown change'
touch "$t/origin.git/refs/heads/main.lock"
out=$(capture status land_run --docs --tries 2)
rm "$t/origin.git/refs/heads/main.lock"
expect "A locked ref is main moving" "$out" 'status=6' 'land:' 'land: main moved 2 times; giving up'
land_branch unread $'Unread trailer\n\nStep: alpha-01.1\n\nAnother paragraph.'
out=$(capture status land_run --docs)
expect "land refuses an unread trailer" "$out" 'status=2' 'land:' 'git reads no trailer in:' 'its Step line'
land_branch unread-planner $'Unread session\n\nPlanner-Session: 3f2a\n\nAnother paragraph.'
out=$(capture status land_run --docs)
expect "land refuses an unread Planner-Session" "$out" 'status=2' 'land:' 'git reads no trailer in:' 'its Planner-Session line'
land_branch unread-designer $'Unread session\n\nDesigner-Session: 3f2a\n\nAnother paragraph.'
out=$(capture status land_run --docs)
expect "land refuses an unread Designer-Session" "$out" 'status=2' 'land:' 'git reads no trailer in:' 'its Designer-Session line'
land_branch forbidden $'A forbidden word\n\nPlan: demo'
echo 'let exact = 1;' >"$t/clone/w.rs"
git -C "$t/clone" add w.rs
git -C "$t/clone" commit -qm $'Add the word\n\nPlan: demo'
mkdir -p "$t/land-tmp"
printf 'alpha/demo\n%s\n' "$dir" >"$t/land-tmp/active-plan"
flock "$t/clone/.git/land-turn.lock" sh -c 'touch "$1"; sleep 0.3' sh "$t/turn-held" &
turn_pid=$!
poll_until 50 test -f "$t/turn-held"
out=$(capture status land_run --gate true)
wait "$turn_pid"
expect "land refuses a Pulse word" "$out" 'status=2' 'land:' "the plan's Pulse forbids these words (actual|exact)" 'w.rs: let exact = 1;'
expect "land waits for another landing's turn" "$out" 'land: another landing in clone has its turn; waiting for it'
land_branch and-name $'A test of two behaviours\n\nPlan: demo'
printf '#[test]\n#[ignore]\nfn reads_and_writes() {}\n' >"$t/clone/t.rs"
git -C "$t/clone" add t.rs
git -C "$t/clone" commit -qm $'Add the test\n\nPlan: demo'
out=$(capture status land_run --gate true)
expect "land refuses a test name the Pulse's pattern matches" "$out" 'status=2' 'land:' "the plan's Pulse names tests matching (_and_)" 't.rs: reads_and_writes'
before=$(cat "$run/ledger.tsv")
out=$(capture status "$plan" set --help state=ready)
expect "plan set refuses an option" "$out" 'status=2' 'plan:' 'a step is not an option: --help'
expect "an option leaves the ledger unchanged" "$(if [[ $(cat "$run/ledger.tsv") == "$before" ]]; then echo unchanged; fi)" unchanged

# A clean primary checkout behind its origin is advanced by --fix.
head=$(git -C "$repo" rev-parse HEAD)
git clone -q --bare "$repo" "$t/ff.git"
git -C "$repo" remote add origin "$t/ff.git"
git clone -q "$t/ff.git" "$t/ff"
git -C "$t/ff" -c user.email=test@example.com -c user.name=test commit -q --allow-empty -m "Advance origin"
git -C "$t/ff" push -q origin main
expect "check --fix fast-forwards" "$(plan_check --fix "$dir")" "ok    alpha: main fast-forwarded 1 commits to origin/main"
expect "check --fix links in the world" "$(readlink "$HOME/.local/bin/plan")" "$kit/plan"
# The code graph's index of the checkout is compared with origin/main's tip.
graph=$(readlink -f "$repo" | sed 's|^/||; s|/|-|g')
echo "grep -o 'forgetic/[a-z]*.git#[0-9a-f]*' Cargo.lock" >"$dir/alpha/02-pins.md"
out=$(plan_check "$dir")
rm "$dir/alpha/02-pins.md"
expect "check says the graph has no index" "$out" "ok    alpha: the graph has no index of it ($graph)"
expect "check refuses a pin check that names an organization" "$out" "FAIL  the plan's pin checks name an organization: alpha/02-pins.md:1:forgetic/[a-z]*.git#"
mkdir -p "$HOME/.cache/codebase-memory-mcp"
touch -d 2020-01-01 "$HOME/.cache/codebase-memory-mcp/$graph.db"
expect "check warns of an older index" "$(plan_check "$dir")" "warn  alpha: the graph's index is older than origin/main: re-index $repo (project $graph)"
expect "the pulse names an older index" "$(plan_numbers pulse)" "- Graph: index older than origin/main: alpha."
touch "$HOME/.cache/codebase-memory-mcp/$graph.db"
expect "check accepts a fresh index" "$(plan_check "$dir")" "ok    alpha: the graph's index is as new as origin/main"
expect "the pulse names no fresh index" "$(plan_numbers pulse)" "- Graph: index older than origin/main: none."
# A done plan that section 2 of the status file still names is warned of,
# but not one named for its later.md (development.md, section 3.3).
cat >"$repo/docs/design/STATUS.md" <<'EOF'
# Status

## 1. Plans

| Plan | State | Takes the design to | Where |
|---|---|---|---|
| demo | ready | x | y |
| old | done 2026-10-01 | x | y |

## 2. The design

| Document | Built | Left |
|---|---|---|
| `thing.md` | partly | 2: a part: old; 3: another: not planned |
| `other.md` | partly | 4: a later part: not planned (old's `later.md`) |
| `third.md` | partly | 5: a third part (the old plan) |
| `fourth.md` | partly | 6: a fourth part: older |
EOF
done_check=$(plan_check "$dir")
expect "check warns of a done plan's items" "$done_check" 'warn  STATUS.md section 2 still leaves items to done plans: `thing.md` (old), `third.md` (old) (the designer'
expect "a later.md is no done plan's item" "$(if [[ $done_check != *'`other.md` (old)'* ]]; then echo absent; fi)" absent
rm "$repo/docs/design/STATUS.md"
git -C "$repo" reset -q --hard "$head"
git -C "$repo" remote remove origin
rm -rf "$t/ff.git" "$t/ff"

# The roles, offline.
expect "attach coordinator" "$(PLAN_DRY_RUN=1 "$plan" attach coordinator)" "codex resume --remote unix:// $coordinator"
expect "attach supervisor" "$(PATH=$t/stub:$PATH PLAN_DRY_RUN=1 "$plan" attach supervisor)" "claude --resume $session"
expect "join outside a session" "$(CLAUDE_CODE_SESSION_ID='' "$plan" join designer 2>&1 || true)" "run plan join from the Claude Code session itself"
cp "$run/claude.tsv" "$t/claude.save"
sed -i '/^designer\t/d' "$run/claude.tsv"
expect "wake with no designer joined starts one" "$(PATH=$t/stub:$PATH PLAN_DRY_RUN=1 "$plan" wake designer "question.md")" "designer: none joined: a fresh session" "designer: would start a fresh session: You are the designer"
mv "$t/claude.save" "$run/claude.tsv"
expect "join" "$(CLAUDE_CODE_SESSION_ID=fake-designer "$plan" join designer)" "designer: Claude Code session fake-designer" "End your turn now"
expect "roles" "$(PATH=$t/stub:$PATH PLAN_NO_ROLES='' "$plan" status)" "## Roles" "- coordinator: $coordinator" "- supervisor: $session, gone"
expect "wake resumes" "$(PATH=$t/stub:$PATH PLAN_DRY_RUN=1 "$plan" wake designer "question.md")" "designer: would resume fake-designer: question.md"
# A designer compacted twice is resumed; one idle past its cache starts fresh.
big=33333333-4444-5555-6666-777777777777
{
    printf '{"type":"assistant","timestamp":"%s","message":{"id":"b1","usage":{"input_tokens":1,"cache_read_input_tokens":150000,"output_tokens":1}}}\n' "$(date -u +%FT%T.000Z)"
    boundary b2; boundary b3
} >"$CLAUDE_CONFIG_DIR/projects/p/$big.jsonl"
printf 'designer\t%s\t%s\n' "$big" "$stamp" >>"$run/claude.tsv"
expect "wake resumes after compactions" "$(PATH=$t/stub:$PATH PLAN_DRY_RUN=1 "$plan" wake designer "question.md")" "designer: would resume $big: question.md"
stale=44444444-5555-6666-7777-888888888888
printf '{"type":"assistant","timestamp":"%s","message":{"id":"b1","usage":{"input_tokens":1,"output_tokens":1}}}\n' "$(date -u -d '-2 hours' +%FT%T.000Z)" >"$CLAUDE_CONFIG_DIR/projects/p/$stale.jsonl"
printf 'designer\t%s\t%s\n' "$stale" "$stamp" >>"$run/claude.tsv"
expect "wake fresh" "$(PATH=$t/stub:$PATH PLAN_DRY_RUN=1 "$plan" wake designer "question.md")" "designer: idle 120 min: a fresh session" "designer: would start a fresh session: You are the designer" "as $run/kit/docs/foundation/development.md, sections 2.1" "in the design rulings of $dir/decisions.md" "never log.md, supervision.md"
# The supervisor hands over; the old session's watch then shows nothing.
expect handover "$(PATH=$t/stub:$PATH CLAUDE_CODE_SESSION_ID=$session PLAN_DRY_RUN=1 "$plan" handover supervisor)" "supervisor: would start a fresh session: Take over" "supervises no more" "as $run/kit/docs/foundation/supervisor.md, section 1" "Read development.md and testing-strategy.md there too"
expect "handover clears the events seen" "$(if [[ ! -s $run/.watch ]]; then echo empty; fi)" empty
expect "handover by another" "$(PATH=$t/stub:$PATH CLAUDE_CODE_SESSION_ID=someone "$plan" handover supervisor 2>&1 || true)" "this session is not the run's supervisor"
# A job's new session id is recorded, once, and its session may hand over.
# An earlier run's job of the same name, and another plan's, are not.
jobs='[{"id":"11111111","name":"demo supervisor","sessionId":"job-supervisor","startedAt":2},{"id":"fake-des","name":"demo designer","sessionId":"fake-designer","startedAt":1},{"id":"0ld0ld00","name":"demo supervisor","sessionId":"0ld0ld00-run","startedAt":0},{"id":"other000","name":"other supervisor","sessionId":"other","startedAt":3}]'
expect "handover takes the job's new session" "$(STUB_JOBS=$jobs PATH=$t/stub:$PATH CLAUDE_CODE_SESSION_ID=job-supervisor PLAN_DRY_RUN=1 "$plan" handover supervisor)" "supervisor: would start a fresh session: Take over"
expect "claude.tsv gains the job's session once" "job=$(grep -c job-supervisor "$run/claude.tsv") old=$(grep -c 0ld0ld00 "$run/claude.tsv" || true) other=$(grep -c other "$run/claude.tsv" || true)" "job=1 old=0 other=0"
printf 'supervisor\tnew-supervisor\t%s\n' "$stamp" >>"$run/claude.tsv"
expect "handed over" "$(PATH=$t/stub:$PATH CLAUDE_CODE_SESSION_ID=$session "$plan" watch || echo "watch exited $?")" "handed over: session new-supervisor" "watch exited 4"

echo "- 2026-10-11T12:00:00Z: the coordinator is quiet; plan resume" >"$run/alerts.md"
expect alerts "$("$plan" status)" "## Alerts" "the coordinator is quiet"
printf 'Recorded %s.\n\nDone.\n' "$(date -u -d "@$(($(sed -nE 's/^## Claimed, (.*)$/\1/p' "$run/log.md" | xargs -I{} date -u -d {} +%s) + 7200))" +%FT%TZ)" >"$run/done.md"
expect "the end recorded, where running_h ends too" "$(plan_numbers metrics)" $'wall_h\t2.0\th' $'running_h\t0.5\th'

# User services stay inside this test's world.
mkdir -p "$t/stub" "$t/config/systemd/user"
cat >"$t/stub/systemctl" <<EOF
#!/usr/bin/env bash
printf '%s\\n' "\$*" >>"$t/systemctl.log"
case \$2 in daemon-reload | enable | disable) exit 0 ;; *) exit 1 ;; esac
EOF
chmod +x "$t/stub/systemctl"
boot=$t/config/systemd/user/plan-resume.service
cp "$DEV_TMP/active-plan" "$t/active-plan"
printf 'Environment=DEV_ROOT=%s DEV_TMP=%s\n' "$DEV_ROOT" "$DEV_TMP" >"$boot"
XDG_CONFIG_HOME=$t/config PATH=$t/stub:$PATH "$plan" release >/dev/null
expect "release removes the boot unit" "$(if [[ ! -e $boot ]]; then cat "$t/systemctl.log"; fi)" "--user disable plan-resume.service"
[[ ! -e $run/kit ]] || { echo "FAIL release: $run/kit is left"; failed=1; }
cp "$t/active-plan" "$DEV_TMP/active-plan"
printf 'Environment=DEV_ROOT=%s DEV_TMP=%s/another\n' "$DEV_ROOT" "$DEV_TMP" >"$boot"
expect "release leaves another run's boot unit" "$(XDG_CONFIG_HOME=$t/config PATH=$t/stub:$PATH "$plan" release 2>&1; cat "$boot")" "names another DEV_TMP; left in place" "DEV_TMP=$DEV_TMP/another"

# The successor's 42 requests over an hour, after a copy of the first's
# compaction and of its message m2, which count once: the 41st is past its
# bootstrap, and the 42nd, after its own compaction, in its re-read.
request() { printf '{"type":"assistant","timestamp":"2026-10-17T%s:00.000Z","message":{"id":"%s","usage":{"input_tokens":100000,"output_tokens":0}}}\n' "$@"; }
{
    boundary u1; assistant m2 2 0 0 7
    for i in $(seq 0 40); do request "$(printf '10:%02d' "$i")" "n$i"; done
    boundary u3; request 11:00 n41
} >"$CLAUDE_CONFIG_DIR/projects/p/new-supervisor.jsonl"
expect "metrics after release" "$(plan_numbers metrics)" $'steps_planned\t2\t' \
    $'supervisor\t2 sessions, 1.0 h, 4.2M weighted tokens per hour, 97% in the 40 requests after each start or compaction, 3 compactions'
released_status=$(PLAN_NO_ROLES='' "$plan" status "$dir")
expect "status after release" "$released_status" "# alpha/demo on" "from ledger.tsv, after its release." "steps landed;"
expect "status of the last run" "$("$plan" status)" "# alpha/demo on" "after its release."
expect "the retrospective reads the checkout once the kit is gone" "$(PATH=$t/stub:$PATH PLAN_DRY_RUN=1 "$plan" retro "$dir")" "as $DEV_ROOT/skein/docs/foundation/development.md, section 15"
[[ $released_status != *"## Roles"* ]] || { echo "FAIL status after release: it lists the roles"; failed=1; }
expect "the machine's links stay" "$(if [[ $(readlink "$machine_home/.local/bin/plan" || true) == "$machine_plan_link" ]]; then echo unchanged; fi)" unchanged

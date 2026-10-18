# Supplement skein-01.s2: test-plan back within its 40 s

Added by ruling 107, from the hourly checks of 2026-10-18T00:17:02Z and
01:18:47Z. Read first: skein's `AGENTS.md`; code-organization.md,
sections 4 (Time) and 6 (Size); this plan's README, "Rules of this plan".

Scope: `test-plan` took 12 to 13 s at the base and 54 to 55 s after
06.3, against the Time rule's about 40 s; a trace of 05.s1 shows 25
`plan` calls at about 1.1 s each (26 s), a `timeout 3 plan watch` and
2.5 s of sleeps. Its captures (`status=0; out=$(…) || status=$?`) and
poll loops are copied across `test-plan`, `test-plan-watch.sh` and
`test-plan-coordinator.sh`. Budget: 3 traced runs and 45 minutes for
the cause; past it, report what each part costs and ask.
Needs: skein 04.5 and 06.5 (the last increments that edit `test-plan`).
Files: `scripts/dev/test-plan` and its sourced `test-plan-*.sh`; `plan`
and its sourced files only where one `plan` call's own start costs.
Shape: trace a run (`PS4='+ $EPOCHREALTIME ' bash -x`); cut what costs
most first: a check asserts from a capture its world already took
rather than a new `plan` call; checks that read the same state share
one world; idle waits use the shortest timeout their check needs. One
capture helper (command in, `name=code` out) and one poll helper replace
every copy; pairs print as `name=value`; "land refuses a Pulse word"
loses its wait assertion to a check of its own.
Test shape: every check and refusal the base and each increment added
still runs; their count, before and after, is in the report.
Proof: process, `test-plan` through `heavy` under 40 s, twice in a row,
both `passed`, with its time before and after in Evidence.
Don't: drop a check or a refusal; change a check's assertion; change
`plan`'s behaviour.

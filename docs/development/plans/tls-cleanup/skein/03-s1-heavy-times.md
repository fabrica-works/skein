# Session 03, step s1: heavy reads pueue's times in UTC

A step not in the plan, added by the supervisor on 2026-10-12
(`decisions.md`, ruling 12). Session 03 owns it, as it owns `scripts/**`.

On carbon-forge, `/usr/share/zoneinfo/Etc/UTC` is a copy of
Europe/Lisbon, so the local zone is +01:00, and `pueue log --json` prints
times like `2026-10-12T01:25:19.319694199+01:00`. heavy's `jq`
(`scripts/dev/heavy`, lines 113-118 at `6916bd0`) reads only times that
end in `Z`. It fails, and `set -e` ends heavy before it writes
`heavy.tsv`, removes the task or prints the log's tail. pueue's client
prints the times in its own zone: `TZ=UTC0 pueue log --json` prints them
ending in `Z`.

### s1 heavy reads pueue's times in UTC
Needs: nothing.
Files: `scripts/dev/heavy`.
Shape: line 106 becomes `json=$(TZ=UTC0 pueue log --json --full "$id")`,
  under a comment of one line: pueue prints its times in the client's
  zone, and the `jq` below reads only UTC.
Test shape: none.
Proof: process, once, seconds: from the worktree, run
  `DEV_ROOT=/srv/data/git/runner/src scripts/dev/heavy true`; it prints
  `heavy: task <id> (build) exit 0`. Nothing else is wanted. Land with
  `TZ=UTC0 land`: the gate runs through the primary checkout's heavy,
  which does not have this fix until it lands. The commit says
  `Proof: process, the worktree's heavy true exits 0 with its summary`.
Don't: change the `jq`, any other line of heavy, or another script.
Size: at most 3 lines.

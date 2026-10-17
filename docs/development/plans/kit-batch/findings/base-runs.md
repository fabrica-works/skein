# The prescribed commands, run once at the base

Each through `heavy`, on debian-8gb-hel1-1 (hel8: 4 cores, 7.7 GB),
with `DEV_TMP=~/src/rust/tmp/plan/kit-batch/devtmp` so that hearth's
render run, active on that machine, kept its queue's records. "Base" is
skein `b30cc95`; "base + d7e6ee3" is the base with `git diff bc26df2
d7e6ee3 -- scripts/dev | git apply --3way` applied (increment 1.1's
start). Logs are in that `devtmp/logs/`.

## 1. The process layer at the base

```
$ scripts/dev/heavy --tail 15 scripts/dev/test-plan
test-plan: passed
heavy: task 27 (build) exit 0, waited 0s, ran 12s, log …/20261017T201405-27.log
```

## 2. shellcheck at the base

ShellCheck 0.10.0. Without `-S`, it exits 1 on info findings (SC1091 ×9,
SC2012, SC2013 ×2, SC2016, SC2119 ×6) and one warning; at `-S warning`
only the warning stays.

```
$ scripts/dev/heavy bash -c 'shellcheck -x scripts/dev/plan scripts/dev/land scripts/dev/heavy scripts/dev/test-plan scripts/dev/numbers.sh scripts/dev/kit.sh scripts/dev/codex.sh'
In scripts/dev/test-plan line 246:
watch() { timeout 10 "$plan" watch "$@" || echo "watch exited $?"; }
^-- SC2120 (warning): watch references arguments, but none are ever passed.
heavy: task 27 (build) exit 1, waited 0s, ran 4s, log …/20261017T201413-27.log
```

## 3. Base + d7e6ee3

`git apply --3way` applied 4 files, +186/−39, with no conflict.

```
$ scripts/dev/heavy --tail 15 scripts/dev/test-plan
test-plan: passed
heavy: task 29 (build) exit 0, waited 2s, ran 13s, log …/20261017T202444-29.log

$ scripts/dev/heavy bash -c 'shellcheck -S warning -x scripts/dev/plan scripts/dev/land scripts/dev/heavy scripts/dev/numbers.sh scripts/dev/test-plan scripts/dev/kit.sh scripts/dev/codex.sh'
In scripts/dev/test-plan line 257:
watch() { timeout 10 "$plan" watch "$@" || echo "watch exited $?"; }
^-- SC2120 (warning): watch references arguments, but none are ever passed.
heavy: task 32 (build) exit 1, waited 0s, ran 4s, log …/20261017T202448-32.log
```

So the plan's gate (README, Rules) fails at the base on SC2120 alone,
and 1.1 fixes it.

## 4. The live layer at the base

Real Codex (effort low) and Claude (Haiku), as `test-plan --live` sets
them; about $0.05.

```
$ scripts/dev/heavy --tail 15 scripts/dev/test-plan --live
find: '/home/free/.codex/archived_sessions': No such file or directory
test-plan: passed
heavy: task 27 (build) exit 0, waited 0s, ran 81s, log …/20261017T203219-27.log
```

The `find` line is test-plan:358's search of a directory this machine's
Codex home lacks; it changes no result.

## 5. Can a client unload a Codex thread?

codex-cli 0.162.1, on this machine's daemon, an ephemeral thread of the
probe's own (no other thread touched). Not through heavy: it compiles
and runs nothing, and makes one request to the daemon a call.

```
$ bash probe.sh   # codex_open; thread/start {ephemeral: true}; thread/loaded/list; thread/unsubscribe; thread/loaded/list; codex_close; codex_open; thread/loaded/list; thread/read
started 01a14b97-d633-70e0-a639-3075e6ec91ca
loaded after start: true
unsubscribe: {"status":"unsubscribed"}
loaded after unsubscribe: true
loaded on a new connection: true
read: {"thread":{"id":"01a14b97-d633-70e0-a639-3075e6ec91ca", …
```

So `thread/loaded/list` names the loaded threads, and no client call
unloads one: only the daemon does, when idle, or a restart. Hence the
fake daemon of decisions.md, 22.

## 6. A private queue for test-plan

pueue 4.0.4. Both `pueue` and `pueued` read `PUEUE_CONFIG_PATH`, so a
test can run the real `heavy` against a daemon of its own, leaving the
machine's queue (hearth's render's, here) as it was. Not through heavy:
it is the queue.

```
$ export PUEUE_CONFIG_PATH=$d/pueue.yml   # shared: pueue_directory, runtime_directory: $d; use_unix_socket: true; unix_socket_path: $d/pueue.socket
$ pueued -d; pueue group add build; id=$(pueue add --print-task-id --group build -- echo hi); pueue wait --quiet $id
$ pueue status --json | jq -c '[.tasks[] | {id, command, status: (.status | keys[0])}]'
[{"id":0,"command":"echo hi","status":"Done"}]
$ pueue shutdown; unset PUEUE_CONFIG_PATH; pueue status --json | jq -c '{tasks: (.tasks | length), groups: (.groups | keys)}'
{"tasks":2,"groups":["build","default","gate"]}
```

## 7. The Done-when and Proof commands at the base

Not through heavy, but the second: each reads git or runs `plan`'s
checks, and builds nothing. Run 2026-10-17 on hel8, from the plan's
worktree, with `DEV_ROOT=~/src/rust` and the isolated `DEV_TMP`.

```
$ git -C ~/src/rust/skein log origin/main --grep='^Plan: kit-batch$' --format='%(trailers:key=Step,valueonly,separator=)' | sort -u | wc -l   # Done when 2
0
$ git ls-files scripts/dev ':!scripts/dev/workspace' | xargs wc -l | awk '$2 != "total" && $1 > 800'   # Done when 6
  1340 scripts/dev/plan
$ git -C ~/src/rust/skein show origin/main:docs/foundation/development.md | grep -cE '^\| `plan (landed|unblocked|steered|context)[ `]'   # Done when 7
0
$ git -C ~/src/rust/skein show origin/main:docs/foundation/supervisor.md | grep -c CronCreate   # Done when 8
1
$ git diff --numstat origin/main -- scripts/dev ':!scripts/dev/test-plan' ':!scripts/dev/test-codex' ':!scripts/dev/workspace/*.md' | awk '{ a += $1 } END { print a + 0 }'   # the size count; 155 in base + d7e6ee3
0
$ wc -l scripts/dev/plan scripts/dev/*.sh   # base + d7e6ee3, before 01.2
  1363 scripts/dev/plan
   108 scripts/dev/codex.sh
    22 scripts/dev/kit.sh
   786 scripts/dev/numbers.sh
$ heavy --tail 3 true   # 02.3's Proof; 2.7 s
heavy: queued task 29 in the build lane
heavy: task 29 (build) exit 0, waited 1s, ran 0s, log …/devtmp/logs/20261017T205946-29.log
$ tail -n1 $DEV_TMP/logs/heavy.tsv
29	build	2026-10-17T20:59:46Z	1	0	0	5.0	…/kit-batch/skein	…/20261017T205946-29.log	true	skein
$ PLAN_NO_GATE=1 plan check ~/src/rust/tmp/plan/kit-batch/skein/docs/development/plans/kit-batch   # 03.1's and 03.2's Proofs; 2.7 s; "has" and link lines left out
ok    plan has README.md
ok    plan has code-organization.md
FAIL  plan has uncommitted changes
warn  no row for kit-batch in skein's docs/design/STATUS.md
ok    the last retrospective's proposals are decided (smith's wall-clock)
ok    skein: main is origin/main
ok    pueued is running as a user service
ok    queue lane build runs 2 at once
ok    disk: 42 GB free
FAIL  memory: 4 GB available, want 6
warn  PLAN_NO_GATE: the queue and the gate are not checked
```

Done when 7's first pattern, without `[ `]`, counted `plan contexts`:
the README's has it. The memory line is hel8's `PLAN_MIN_MEM_GB` (03.1).
The two others are this plan's own, before it lands.

## 8. A queue no daemon serves (03.4's test)

Not through heavy: it is the queue. A config naming `$d` for its
directories and socket, where no `pueued` ever ran; the call fails before
it reaches any socket, and never the machine's daemon.

```
$ cat $d/pueue.yml
shared:
  pueue_directory: $d
  runtime_directory: $d
  use_unix_socket: true
  unix_socket_path: $d/pueue.socket
$ PUEUE_CONFIG_PATH=$d/pueue.yml timeout 10 pueue add --group build -- true; echo "exit $?"
Error:
   0: I/O error at path "$d/shared_secret" while opening secret file. Did you start the daemon at least once?:
      No such file or directory (os error 2)
exit 1
```

Under a second. `heavy`'s `pueue add` then fails, and `plan downstream`
prints the repository's line as `fails`.

## 9. 01.1 and 01.2 in a scratch worktree

01.1 is d7e6ee3's kit (`git show d7e6ee3 -- scripts/dev/`, applied with
no conflict), less `plan check`'s planning.md warning (plan:247–248) and
with test-plan:310's dangling argument fixed (SC2120). The gate, through
heavy:

```
$ heavy -- bash -c 'scripts/dev/test-plan && shellcheck -S warning -x $(git ls-files scripts/dev ":!scripts/dev/workspace") && echo shellcheck-ok'
test-plan: passed
shellcheck-ok
heavy: task 27 (build) exit 0, waited 0s, ran 16s
```

`plan` is then 1,361 lines and test-plan 409. 01.2's split on top of it,
moved by line ranges as 01.2's Shape names them, with no body changed:

```
$ wc -l scripts/dev/plan scripts/dev/plan-*.sh
  561 scripts/dev/plan
  341 scripts/dev/plan-check.sh
  352 scripts/dev/plan-roles.sh
  128 scripts/dev/plan-watch.sh
$ heavy -- bash -c 'scripts/dev/test-plan && shellcheck -S warning -x $(git ls-files scripts/dev ":!scripts/dev/workspace") scripts/dev/plan-*.sh && echo shellcheck-ok'
test-plan: passed
shellcheck-ok
heavy: task 27 (build) exit 0, waited 0s, ran 18s
```

Without a file-wide `# shellcheck source-path=SCRIPTDIR`, shellcheck
does not follow the new `source` lines and exits 1: SC2034 on the eleven
settings only the moved functions read (plan:68–73, 75, 81–83, 116). The
directive works only among the file-wide ones, before `set -euo
pipefail` (beside plan:37's `disable=SC2015`); as line 2 it shifts
usage's header, and above the `source` lines it is ignored. usage
prints `sed -n '3,31p'`, which on main already ends mid-paragraph, at
"PLAN_DIR is the plan's directory…"; printing from line 3 to the first
`# shellcheck` line prints the whole header, 34 lines, and needs no
change when a command is added. Run with `DEV_TMP` set: without it,
`plan` re-runs the active run's pinned kit (hearth's render, here).

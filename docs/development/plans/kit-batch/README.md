# kit-batch: every adopted kit change still waiting

2026-10-17. Start commit: skein `b30cc95` ("Apply the wall-clock
retrospective's text decisions"), each session starting from skein's
`origin/main`. No other repository changes.

The kit catches up with what its documents and the last six
retrospectives adopted: the rows of `kit-after-parity`, `land` and
`heavy` that lose nothing when a race or a killed client hits them, a
`plan check` that refuses an unfinished plan and reads a machine's own
settings, a watch that prints what each event needs and loads the
coordinator's thread itself, the coordinator's fixed lines written by
the kit, its profile reaching its thread, and `plan metrics` as 15.2
defines it.

## Inputs

- **skein's foundation:** `docs/foundation/development.md` (this plan
  follows it, and changes it where the kit changes),
  `docs/foundation/supervisor.md`, `docs/foundation/implementer.md`.
- **The retrospectives' decisions** (each one's sections 8 and 10):
  temper's `forgejo-17` and `protocol`; jig's `client-composition` and
  `skein-ui`; smith's `parity`, `wall-clock` and `reliability` (its 17);
  skein's branch `kit-after-parity` (d7e6ee3).
- **`decisions.md`:** what this plan settled, and what the user may veto.
- **`findings/`:** `inventory.md` (each adopted change, against main,
  with its increment); `base-runs.md` (every prescribed command, run at
  the base).

## The sessions

A session waits for what "Needs at start" names; an increment that needs
more says so in its row and its file. Each increment also builds on the
ones before it in its session.

| File | Session | Needs at start | Increments that need more |
|---|---|---|---|
| `skein/01-base.md` | skein 01: kit-after-parity's kit, and `plan` in four files | nothing | |
| `skein/01-s1-test-world.md` | skein 01.s1: test-plan's world keeps off the machine (ruling 102) | nothing | |
| `skein/01-s2-test-time.md` | skein 01.s2: test-plan back within its 40 s (ruling 107) | skein 04.5 and 06.5 | |
| `skein/02-land-heavy.md` | skein 02: `land` and `heavy`, and the rules with no kit | nothing (02.1 is Markdown) | 02.2, 02.3: skein 01.1 (d7e6ee3's `land`, the SC2120 fix); 02.4: skein 05.1 (`metrics.sh`) |
| `skein/03-check.md` | skein 03: the machine file, `plan check`'s refusals, the units, `plan downstream` | skein 01.2 (`check_machine` in `plan-check.sh`); skein 01.s1 (test-plan's world, ruling 102) | |
| `skein/04-watch.md` | skein 04: the coordinator's thread, and the watch | skein 01.2 (`ring` and `coordinator_state` in `plan-roles.sh`) | |
| `skein/05-numbers.md` | skein 05: the numbers | skein 01.2 (numbers.sh as 01.1 leaves it) | 05.5: skein 03.1 (`machine_setting`) |
| `skein/06-coordinator.md` | skein 06: the coordinator's commands and its profile | skein 01.2 (`set_step` in `plan` after the move) | 06.3: skein 04.2 (`coordinator_turn`, `log_entry`, the fake Codex; `handover`'s and `resume`'s `.watch` line) |

### How many at once, and in what order

- **At the start:** skein 01 and 02; 02 lands its Markdown, then waits
  for 01.1.
- **Then:** skein 03, 04, 05 and 06 once 01.2 has landed. Five at once
  is the most; the build lane runs 2 at once and the gate lane 1.
- **When slots are short, start first:** skein 04 and 05, the two
  longest chains (06.3 waits on 04.2, 02.4 on 05.1), then 06, 02, 03.
- **The longest chains:** 01.1 → 01.2 → 04.1 → 04.2 → 06.3 → 06.4 →
  06.5, and 01.1 → 01.2 → 05.1 → … → 05.5, seven landings each; at a
  kit step's hour or so, about 7 hours.

## Rules of this plan

- **test-plan keeps off the machine** (ruling 102): its offline layer
  never writes the machine's `$HOME` nor reaches its user bus; a test
  that runs `plan check --fix` or `systemctl` does so in the world
  skein-01.s1 sets up. Until skein-01.s1 lands, each `test-plan` run
  relinks the machine's tools to its tree: after each, and before a
  worktree is removed, run `for t in heavy land plan test-plan; do ln
  -sfn ~/src/rust/skein/scripts/dev/$t ~/.local/bin/$t; done`.
- **test-plan's checks** (ruling 107): a check asserts one behaviour,
  from a capture its world already took where one holds the line, rather
  than a new `plan` call; a capture or a poll written twice becomes a
  helper; pairs print as `name=value`. A `.s<N>` step is named in its
  report's Next, and the coordinator adds its ledger row, before it
  starts.
- **The fence is lifted** (decisions.md, 3; ruling 101): temper's
  client-api runs its own kit, pinned at skein `841f487` (temper's ruling
  295), so this run's landings no longer reach it. The coordinator runs
  no fence command, and starts sessions as the rest of this README says.
- **The gate** (decisions.md, 5): a step that changes code lands, from
  its worktree, with

  ```
  LAND_PREBUILD= land --gate 'scripts/dev/test-plan && shellcheck -S warning -x $(git ls-files scripts/dev ":!scripts/dev/workspace")'
  ```

  and a Markdown-only step with `land --docs`. Never `scripts/check.sh`:
  it builds Rust, which no step changes.
- **Each landing's body** says "hearth's render and temper's forge-setup
  keep their process." (development.md, 4.4; decisions.md, 4), naming
  any other run whose row reads `active on` the same way.
- **The kit and its documents change together:** an increment that
  changes what a tool does changes the development.md, supervisor.md or
  implementer.md text that says it, in the same commit, and 3.6's table
  for a new command or option.
- **A Proof's layer** (development.md, 5.5): above `test-plan`'s process
  layer the kit has only live, so most Proofs are `process`, the
  increment's own checks; a live part is added where the real daemon,
  Claude or the machine can answer otherwise than the fake or the
  fixture (decisions.md, 20, 22). Each part after a semicolon starts
  with its layer's word (decisions.md, 10).
- **No step runs a real role but through `test-plan --live`,** and none
  restarts pueued or the Codex daemon of the machine it runs on: the
  run's own coordinator and the queue live there.
- **Shared files,** which parallel sessions change (the coordinator
  expects rebases): `scripts/dev/plan` (every session's dispatch and
  usage lines), `scripts/dev/plan-check.sh` (03, 05.2's Inputs
  warning), `scripts/dev/plan-roles.sh` (03.3, 04, 06),
  `scripts/dev/plan-watch.sh` (04, 06.2), `scripts/dev/test-plan` (every
  session), `scripts/dev/numbers.sh` and `scripts/dev/metrics.sh` (02.4,
  05), `scripts/dev/heavy` (02.3, 03.1), `docs/foundation/development.md`
  (every session), `docs/foundation/supervisor.md` (01.1, 02.1, 04),
  `docs/foundation/implementer.md` (01.1, 02.1, 06.5; 01.1 and 02.1 both
  in 8.5). Two functions of plan-roles.sh are changed by both 04 and 06:
  `handover` and `resume`. 06.3 needs 04.2, which changes them first;
  04.5 changes only their text lines (the cron line,
  `supervisor_resume_prompt`), so a rebase across 06.3 is a line's.

## Live services

The real Codex and Claude Code, through `test-plan --live` only (Codex at
effort low, Claude on Haiku): about $0.05 and 81 s a run at the base. At
most eight runs in all (decisions.md, 20): one per live Proof, one more
after a fix. Nine had run by 06.5's Proof; no step runs another, and
the last is "Done when" 4's, at the end (ruling 108).

## One test of a suite that needs a built artifact

None: the kit builds nothing. `test-plan` is the process layer, `heavy
--tail 15 scripts/dev/test-plan` from the worktree.

## Repin

None: the plan changes no crate and no pin. The workspace's repositories
take the kit from skein's checkout (development.md, 3.6).

## Pulse

```
words: legacy|compat|hack|workaround
types: ^(Legacy|Old|Compat)[A-Z]|V2$
file-lines: 800
budgets: 40 60
```

## Done when

1. Every step is landed or dropped: `plan status` lists none waiting,
   ready, running or blocked.
2. The plan's commits: `git -C ~/src/rust/skein log origin/main
   --grep='^Plan: kit-batch$' --format='%(trailers:key=Step,valueonly,separator=)'
   | sort -u` lists 01.1 to 06.5, each once or more.
3. The process layer passes on main: in a worktree at `origin/main`,
   `heavy --tail 15 scripts/dev/test-plan` ends with `test-plan: passed`.
4. The live layer passes on main, once: in the same worktree, `heavy
   --tail 15 scripts/dev/test-plan --live` ends with `test-plan: passed`.
5. shellcheck finds no warning: in the same worktree, `heavy bash -c
   'shellcheck -S warning -x $(git ls-files scripts/dev
   ":!scripts/dev/workspace")'` exits 0.
6. No kit file is over 800 lines: `cd ~/src/rust/skein && git ls-files
   scripts/dev ':!scripts/dev/workspace' | xargs wc -l | awk '$2 !=
   "total" && $1 > 800'` prints nothing, on origin/main.
7. Every new command is in 3.6's table: `git -C ~/src/rust/skein show
   origin/main:docs/foundation/development.md | grep -cE '^\| `plan
   (landed|unblocked|steered|context)[ `]'` prints 4.
8. The cron job is gone: `git -C ~/src/rust/skein show
   origin/main:docs/foundation/supervisor.md | grep -c CronCreate`
   prints 0.
9. The metrics price each model and the context limit is 150,000:
   `git -C ~/src/rust/skein grep -n -e PLAN_CLAUDE_PRICES -e
   'PLAN_CONTEXT_LIMIT:-200000' origin/main -- scripts docs/foundation`
   prints nothing.
10. The aims stated above are these items; none other is checked.

# Session 03: the machine file, `plan check`'s refusals, the units, `plan downstream`

2026-10-17. What `plan check` reads and refuses, and what the kit leaves
on a machine. A machine's settings are environment variables today, which
`plan resume` and a reboot forget: hel8 has 7.7 GB and needs
`PLAN_MIN_MEM_GB=4` typed at each check. A plan with no base runs or with
open design questions passes the check (forgejo-17 1). pueued's unit gave
up within a second of a full disk, and stayed down five hours (parity 8);
a reboot stopped protocol's run for 98 minutes (protocol 8). `plan
downstream`'s worktrees stay in `wt/`, where they block `plan release`
(forgejo-17 6, skein-ui 10).

## Read first

- skein `docs/foundation/development.md`, sections 3.5, 3.6 (`plan
  check`, `plan downstream`), 5.4 (before the run), 7.6 (resuming), 11.1
  (the queue), 14.1.
- `decisions.md`, 6, 8, 13, 17; `findings/inventory.md`, section 2.
- `code-organization.md`, sections 1, 3 (`machine_setting`) and 4.
- skein `testing-strategy.md`, section 10.

## What the design asks

- **11.1, 3.6:** the lanes' widths and the machine's minimums, per
  machine, kept across resumes and reboots.
- **5.4:** a plan runs only once its commands ran at the base and its
  design questions are answered.
- **7.6, 11.1:** the queue's daemon restarts itself without limit; a
  reboot resumes the run.
- **14.1:** `plan downstream` leaves nothing behind.

## Choices this plan makes

- `$DEV_TMP/machine.env`, lines `KEY=integer` for four keys, parsed by
  `machine_setting` in kit.sh, never sourced; the environment wins over
  it, and it over the default (decisions.md, 13).
- The base-run and open-question shapes are decisions.md 6's.
- The boot unit is `plan-resume.service`, under
  `${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/`, as pueued's unit
  then is too; it runs `plan resume` with no cause, which 06.3 reads as a
  reboot from the heartbeat.
- `plan downstream`'s worktrees are `<run>/downstream/<repository>`,
  made fresh at each call and removed by an EXIT trap (decisions.md, 17).

## Rules that bind it

- **The gate** (README, Rules).
- **No machine state from a test** (code-organization.md, 5): a
  `systemctl` stub in `$t/stub` records its arguments; `XDG_CONFIG_HOME`
  is `$t/config`; the downstream check's queue is a `PUEUE_CONFIG_PATH`
  that no daemon serves (findings/base-runs.md, 8).

## Increments

### 03.1 The machine file
Scope: the brief's item 8; decisions.md 13; 11.1.
Needs: skein 01.2 (`check_machine` in `plan-check.sh`).
Files: `scripts/dev/kit.sh`; `scripts/dev/plan-check.sh`
  (`check_machine`'s lanes and room, plan:420–454 after 01.1);
  `scripts/dev/heavy` (`build_jobs`, heavy:35); `scripts/dev/test-plan`;
  development.md 3.6, 11.1, 11.4 (:2166–2171).
Shape: `machine_setting KEY DEFAULT` prints `value source`: the
  environment's `$KEY` if set (`environment`), else the last `KEY=` line of
  `$tmp/machine.env` whose value is digits (`machine.env`), else DEFAULT
  (`default`). A line with any other key, or a value not digits, is
  ignored, and `plan check` warns of it, naming the line. `check_machine`
  reads `PLAN_MIN_MEM_GB` (6), `PLAN_MIN_DISK_GB` (40) and
  `HEAVY_BUILD_SLOTS` (2) through it, and its lines say where each came
  from: `ok    memory: 7 GB available, want 4 (machine.env)` (plan:451–454
  say `want N` on a `fail` line only; the `ok` lines gain it). heavy reads
  `HEAVY_BUILD_JOBS` (4) through it. development.md 11.4's coordinator
  hold, "Below 6 GB of memory or 40 GB of disk", reads: below the memory
  and disk floors `plan check` prints (the machine file's `PLAN_MIN_MEM_GB`
  and `PLAN_MIN_DISK_GB`, 6 and 40 GB unless it says otherwise), it holds
  new starts; and the build lane widens again above the memory floor, not
  "above 6 GB". The 3 GB, 1.5 GB, 15 GB and 60 GB lines stay.
Test shape: test-plan, beside "decisions made" (:312): with
  `$DEV_TMP/machine.env` holding `PLAN_MIN_MEM_GB=1` and `BOGUS=3`,
  "the machine file sets a minimum" expects `want 1 (machine.env)` and
  a warn naming `BOGUS=3`; with `PLAN_MIN_MEM_GB=2` in the environment
  too, "the environment wins" expects `want 2 (environment)`.
Proof: process, those checks, under 2 s; live, `plan check` on this
  machine, once, in the report, naming the source of each.
Don't: `source` or `eval` the file; add a key beyond the four.
Size: about 40 lines.
Removes: settings that a resume forgets.

### 03.2 `plan check` refuses an unfinished plan
Scope: forgejo-17 1; decisions.md 6; 5.4.
Needs: skein 01.2 (`check_plan` in `plan-check.sh`).
Files: `scripts/dev/plan-check.sh` (`check_plan`); `scripts/dev/test-plan`
  (the demo plan, :38–87); development.md 5.4.
Shape: `check_plan` fails `plan has no base run in findings/
  (development.md, section 5.4)` unless a `findings/*.md` holds a line
  starting `$ ` inside a fenced block; and fails `plan has N open design
  questions in decisions.md` when its `## Design questions` section has
  items (lines starting `- ` or `<n>. `) with no `Answer:` line before
  the next item or section. Each passes with an `ok` line.
Test shape: the demo plan gains `findings/base-runs.md` with a fenced `$
  true`, so the earlier checks hold. Then copies of it: none, "a plan
  with no base run is refused"; `## Design questions` with `1. Which?`,
  "an open question is refused" (`1 open design question`); with
  `Answer: this.` under it, "an answered question passes".
Proof: process, those checks, under 2 s; live, `plan check` on this
  plan's directory, once, in the report: both `ok`.
Don't: read any other section of decisions.md.
Size: about 30 lines.
Removes: nothing.

### 03.3 The units: pueued without a limit, the boot unit, the warning
Scope: parity 8 (its unit); protocol 8 (the boot unit, the warning);
  7.6, 11.1.
Needs: 03.1 (`check_machine` as 03.1 leaves it).
Files: `scripts/dev/plan-check.sh` (`check_machine`'s unit, plan:386–419
  after 01.1); `scripts/dev/plan-roles.sh` (`start`); `scripts/dev/plan`
  (`release`); `scripts/dev/test-plan`; development.md 6, 7.6, 11.1.
Shape: the unit's lines move out of `check_machine` into `check_units`,
  which it calls, so that neither grows past code-organization.md 6.
  pueued's unit is read at `${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/`
  (plan:389 says `$HOME/.config`), as the boot unit's is. It gains `StartLimitIntervalSec=0` under `[Unit]` and
  `RestartSec=5` with `Restart=always`. `check_machine` warns of a unit
  that lacks them; `--fix` rewrites it and runs `daemon-reload`, and
  restarts pueued only when it is not running. It warns `the machine may
  reboot unattended (Unattended-Upgrade::Automatic-Reboot)` when
  `apt-config dump` says `"true"`. `boot_unit enable` writes
  `plan-resume.service` (`Type=oneshot`, `After=pueued.service`,
  `Environment=DEV_ROOT=<root> DEV_TMP=<tmp>`, `ExecStart=<kit>/plan
  resume`, `WantedBy=default.target`) and enables it; `start` calls it
  once its roles have started. `boot_unit disable` disables and removes
  the unit only when it names this run's `DEV_TMP`; `release` calls it
  before it removes active-plan. A unit naming another `DEV_TMP` is left,
  with a warning.
Test shape: a `systemctl` stub in `$t/stub` (not `$t/bin`, so that
  nothing else sees it) appends its arguments to `$t/systemctl.log` and
  exits 0 for `daemon-reload`, `enable` and `disable`, 1 otherwise. The
  release at test-plan:341 runs with `XDG_CONFIG_HOME=$t/config` and
  `PATH=$t/stub:$PATH`, after the test writes
  `$t/config/systemd/user/plan-resume.service` naming `DEV_TMP=$DEV_TMP`:
  "release removes the boot unit" expects the file gone and `--user
  disable plan-resume.service` in the log. A second unit naming another
  `DEV_TMP` is left ("release leaves another run's boot unit").
Proof: process, those checks, under 2 s; live, `test-plan --live`'s
  `plan start`, run with the same two variables, writes the unit under
  `$t/config` with `ExecStart=<kit>/plan resume`, and the stub logs its
  enable. One live run, about $0.05 and 90 s. The stub is on PATH for
  release and that start only, never for `plan check`, whose
  `check_machine` reads the machine's own units.
Don't: restart a running pueued; let a test reach the machine's
  `~/.config/systemd` or its `systemctl`.
Size: about 50 lines.
Removes: a queue daemon that gives up; a run that stays stopped after a
  reboot.

### 03.4 `plan downstream` removes its worktrees
Scope: forgejo-17 6, skein-ui 10; decisions.md 17; 14.1.
Needs: skein 01.2 (`downstream` in `plan` after the move).
Files: `scripts/dev/plan` (`downstream`, plan:1270–1316 after 01.1, which 01.2 leaves in `plan`);
  `scripts/dev/test-plan` (beside "downstream", :254);
  development.md 3.5, 14.1.
Shape: `dir=$run/downstream/$repo`, added fresh with `git worktree add
  --detach` at origin/main; a `trap` set before the first add removes
  every one made (`git worktree remove --force`, then `rmdir
  $run/downstream`) on any exit. Its loops read `$repos`, the workspace's
  repositories in order (`DEV_REPOS`, else all five), not `$all_repos`,
  so that a test workspace is probed. The lines it prints do not change.
Test shape: a repository `beta` beside alpha in the fixture workspace,
  with a bare origin, whose `Cargo.lock` holds a package with `source =
  "git+…/alpha.git#…"`; `DEV_REPOS="alpha beta"` and, for the one call,
  `PUEUE_CONFIG_PATH=$t/pueue-none/pueue.yml`, a config whose directories
  and socket are in `$t/pueue-none` and no daemon ever started there, so
  that `heavy`'s `pueue add` exits 1 at once (findings/base-runs.md, 8):
  "downstream leaves no worktree" expects `beta against alpha main:
  fails`, no `$run/downstream`, and one line from `git -C beta worktree
  list`. No stub: `$t/stub/pueue` is 04.3's.
Proof: process, that check, under 2 s.
Don't: reuse a worktree across calls; leave one in `wt/`.
Size: about 15 lines.
Removes: `<run>/wt/<repo>-downstream`.

## Interface sketches

```bash
# kit.sh: one machine setting, with where it came from.
machine_setting KEY DEFAULT   # prints "<value> <environment|machine.env|default>"
# plan-roles.sh: the boot unit of the active run.
boot_unit enable|disable
```

## Questions to raise when reached

None.

## Done when

03.1 to 03.4 have landed; `plan check` on main names each setting's
source; hel8's `~/src/rust/tmp/machine.env` can then hold
`PLAN_MIN_MEM_GB=4` (the user's, "For the user").

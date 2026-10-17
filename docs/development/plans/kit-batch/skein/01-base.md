# Session 01: kit-after-parity's kit, and `plan` in four files

2026-10-17. The kit part of skein's branch `kit-after-parity` (d7e6ee3)
has waited 135 commits: `land`'s turn and its refusals, the alerts file,
`plan check --fix`'s fast-forward, the resume's cause and the metrics'
`wall_h`, `proofs` and `lines`. It applies to main with no conflict and
passes `test-plan` (findings/base-runs.md, 3). 01.1 lands it with the
document rules that describe it. `plan` is then 1,363 lines, past the
Pulse's 800: 01.2 moves its checks, its watch and its roles' code into
three files it sources, so that sessions 03, 04 and 06 change different
files. Every later session needs this one.

## Read first

- skein `docs/foundation/development.md`, sections 3.5 (the run's
  files), 3.6 (the kit), 5.5 (the proof's layers), 9.1 (`land`), 15.2
  (the metrics).
- `findings/inventory.md`, section 1 (each rule of d7e6ee3, against
  main); `findings/base-runs.md`, sections 2 and 3.
- `decisions.md`, 2 (re-applied, four rules dropped), 5 (the gate), 19
  (the split); `code-organization.md`, sections 1 and 6.
- skein `testing-strategy.md`, section 10 (a test's shape);
  `programming-model.md`: none, the kit is bash.

## What the design asks

- **3.5, 10.5; supervisor.md 2:** every alert also goes to the run's
  `alerts.md`, and `plan status` prints its last ten lines first
  (d7e6ee3's rule 5).
- **3.6:** `plan check --fix` fast-forwards a clean primary checkout that
  is only behind origin/main, and keeps `.codebase-memory/` out of git
  status (rule 14b).
- **7.6, 3.5:** done.md opens "Recorded <UTC time>." (rule 3).
- **9.1, 13 ("Gating again"); implementer.md 8.4, 8.5:** a code landing
  holds its repository's turn on the machine from its rebase to its
  push; `land` refuses Plan, Step or Proof lines git does not read as
  trailers, and code adding a word the active plan's Pulse forbids
  (rule 10).
- **14.2; 15.2:** the pulse's record bullet; `wall_h`, `proofs` counted
  by each step's last commit, `lines` in four kinds (rules 11, 12).

## Choices this plan makes

- The document text is written afresh against main's sections, from
  d7e6ee3's (`git show d7e6ee3 -- docs/foundation/`), not applied: main
  moved under it. Rules 2, 6, 14d and 14e are not written (decisions.md,
  2).
- `plan check`'s warning for a missing `findings/planning.md` is not
  applied, nor its expectation in `test-plan`.
- 01.2 moves, and changes nothing: same function bodies, same order
  within each file.

## Rules that bind it

- **The gate** (README, Rules): 01.1 lands with the README's command;
  shellcheck must exit 0, so 01.1 fixes SC2120.
- **The kit and its documents change together** (README, Rules).
- **Size** (code-organization.md, 6): a pure move counts only its new
  lines, and its report names the functions it moved.

## Increments

### 01.1 kit-after-parity's kit, with its rules
Scope: d7e6ee3's rules 3, 5, 10, 11, 12 and 14b (findings/inventory.md,
  1), and the kit's `plan resume [CAUSE]`; kit and documents.
Needs: nothing.
Files: `scripts/dev/land`, `scripts/dev/plan`, `scripts/dev/numbers.sh`,
  `scripts/dev/test-plan` (from `git diff bc26df2 d7e6ee3 --
  scripts/dev`); `docs/foundation/development.md` (3.5, 3.6, 7.6, 9.1,
  10.5, 13, 14.2, 15.2), `docs/foundation/implementer.md` (8.4, 8.5),
  `docs/foundation/supervisor.md` (2, the `quiet:` row, about :112).
Shape: `git diff bc26df2 d7e6ee3 -- scripts/dev | git apply --3way`
  (4 files, +186/−39, no conflict). Then remove from `check_plan` the two
  lines that warn of a missing `findings/planning.md` (`warn "plan has
  no findings/planning.md, …"`), and their expectation in test-plan.
  Then test-plan's `watch()` (test-plan:257 after the apply), which
  shellcheck flags SC2120 because no call passes it arguments: the
  calls pass none, so drop `"$@"` from it. The documents: each rule's
  text in the section the table names, as development.md words it
  elsewhere: the command, what it does, why in one clause. And the
  cause: 3.6's `plan resume` row reads `plan resume [CAUSE]`; 7.6 says
  the cause (reboot, crash, the user's request), when the caller knows
  it, goes into the coordinator's prompt for its log.md heading, where
  it now says the prompt names none (about :1585); the supervisor's own
  resume of a quiet coordinator is `plan resume crash`, in supervisor.md's
  `quiet:` row (about :112, "run `plan resume` yourself, once") and in
  10.5's matching text (about :1997-2000).
Test shape: the expectations d7e6ee3 adds to `test-plan`, kept; the
  planning.md one removed. One new check, after "decisions made"
  (test-plan:312): "check --fix fast-forwards". `plan check --fix`
  fetches (plan:173), so the fixture's alpha gets a bare origin,
  `$t/ff.git` (`git clone -q --bare "$repo"`, `git -C "$repo" remote
  add origin`), and a second clone pushes an empty commit to it, which
  leaves alpha only behind. `PLAN_NO_GATE=1 plan check --fix "$dir"`
  prints `ok    alpha: main fast-forwarded 1 commits to origin/main`. At its
  end alpha is reset to the head it had, its remote removed and the
  bare repository deleted, so that later checks read the same main.
  land's refusals and its wait line are tested in 02.2's land world.
Proof: process, the README's gate command from the worktree, under 30 s:
  `test-plan: passed`, and shellcheck at `-S warning` exits 0.
Don't: apply d7e6ee3's document hunks; restore the planning.md warning
  under another name; leave alpha's origin or a moved head for the
  checks after the new one.
Size: about 150 lines (the applied kit, +186/−39, less the warning).
Removes: SC2120 at test-plan:257; `plan check`'s silence on a stale
  primary checkout.

### 01.2 `plan` in four files
Scope: decisions.md 19; code-organization.md 1.
Needs: 01.1 (`plan` as d7e6ee3 leaves it).
Files: `scripts/dev/plan`; new `scripts/dev/plan-check.sh`,
  `scripts/dev/plan-watch.sh`, `scripts/dev/plan-roles.sh`;
  `docs/foundation/development.md` 3.6 (the kit's files).
Shape: as code-organization.md 1 lists, by function, each with the
  comment above it; plan's lines after 01.1 (findings/base-runs.md, 9).
  `plan-check.sh` takes plan:143–479, `check` to `check_machine`;
  `plan-watch.sh` takes plan:634–645, `idle`, and plan:708–818, `watch`
  to `supervisor_context`; `plan-roles.sh` takes plan:820–1135, the roles'
  section comment and `claude_sessions` to `handover`, and plan:1236–1266,
  `retro` and `roles`. `plan` sources the three after numbers.sh's
  `source` (plan:1268–1269), each with `# shellcheck source=`. Each file
  opens `# shellcheck shell=bash`, `# shellcheck disable=SC2154 # plan
  sets the variables these functions read`, and a line naming it as
  sourced by plan and not run. `plan` gets a file-wide `# shellcheck
  source-path=SCRIPTDIR`, beside plan:37's `disable=SC2015`: without it
  shellcheck reports SC2034 on the eleven settings only the moved
  functions read. usage prints from line 3 to the first `# shellcheck`
  line (`sed -n '3,/^# shellcheck/p' … | sed '$d; …'`), not lines 3–31,
  which cut the header mid-paragraph on main; commands added later need
  no range. No body changes; `ruling`, `ready`, `docs` and `downstream`
  stay in `plan`.
Test shape: none new: `test-plan` passes unchanged.
Proof: process, the README's gate command, under 30 s (18 s in
  base-runs.md, 9): `test-plan: passed`, shellcheck exits 0, and `wc -l
  scripts/dev/plan scripts/dev/plan-*.sh` in the report shows 561, 341,
  128 and 352 lines, give or take a few.
Don't: change a function while moving it; move `numbers.sh` (05.1 does);
  leave a function in two files.
Size: about 810 lines moved; about 15 new (the headers, the source
  lines, the directive, usage's range).
Removes: nothing.

## Interface sketches

None: the files hold today's functions, and `plan`'s commands do not
change.

## Questions to raise when reached

None.

## Done when

01.1 and 01.2 have landed; `plan` and each `plan-*.sh` are under 800
lines; `test-plan` passes on main.

# kit-batch: code organization

The plan changes skein's kit, bash in `scripts/dev/`, and the documents
that describe it. No crate changes. This file names who owns what, where
tests go, and what no increment does.

## 1. Files

| File | Kind | Holds, once the plan has landed |
|---|---|---|
| `scripts/dev/plan` | the command | usage, settings, `load`, `locate`, the ledger (`set_step`, `log_report`, `log_entry` (04.1)), `status`, `claim`, `release`, `worktree`, `docs`, `ruling`, `ready`, `landed` (06.1), `unblocked` and `steered` (06.3), `downstream`, `contexts`, the dispatch; sources the four below; 561 lines after 01.2 (findings/base-runs.md, 9) |
| `scripts/dev/plan-check.sh` (new, 01.2) | sourced by `plan` | `check`, `check_gate`, `check_plan`, `check_decisions`, `codex_daemon_start`, `check_machine` and its `check_units` (03.3) (after 01.1, plan:143–479) |
| `scripts/dev/plan-watch.sh` (new, 01.2) | sourced by `plan` | `idle`, `watch`, `watch_once`, `handed_over`, `supervisor_context` (after 01.1, `idle` at plan:634–645, `watch` to `supervisor_context` at plan:708–818), and each event's printer (04) |
| `scripts/dev/plan-roles.sh` (new, 01.2) | sourced by `plan` | the roles' sessions (`claude_sessions` to `claude_role`), `coordinator_state`, `coordinator_turn` (04.1), `ring`, the prompts, `start_coordinator`, `start`, `attach`, `resume`, `join`, `wake`, `handover`, `retro`, `roles`, `context` (06.2) (after 01.1, plan:820–1135, the roles' comment and `claude_sessions` to `handover`; `retro` and `roles`, plan:1236–1266) |
| `scripts/dev/numbers.sh` | sourced by `plan` | the run's numbers, their readers (`jsonl`, `claude_files`, `designer_sessions`, 05.2) and `pulse` |
| `scripts/dev/metrics.sh` (new, 05.1) | sourced by `plan` | `claude_cost` and its prices (05.2), `supervisor_sessions`, `metrics_of`, `metrics` (numbers.sh:221–284 and 637–786 after 01.1) |
| `scripts/dev/kit.sh` | sourced by `plan`, `land`, `heavy` | `run_kit`; `machine_setting` (03.1) |
| `scripts/dev/codex.sh` | sourced by `plan` | the daemon's client; `PLAN_CODEX_FAKE` (04.1) |
| `scripts/dev/land`, `scripts/dev/heavy` | commands | as now; `heavy --task` (02.3) |
| `scripts/dev/test-plan` | the suite | the process and live layers |
| `scripts/dev/test-codex` (new, 04.1) | test kit | the fake Codex daemon `PLAN_CODEX_FAKE` names |
| `scripts/dev/workspace/` | the workspace's files | `AGENTS.md`, `coordinator.config.toml` (06.4) |
| `docs/foundation/{development,supervisor,implementer}.md` | the process | what the kit does, changed with it |

A sourced file starts `# shellcheck shell=bash` and a comment naming who
sources it, as `codex.sh` and `numbers.sh` do, with numbers.sh's
`disable=SC2154` when it reads plan's variables; `plan` sources it with a
`# shellcheck source=` line, and its file-wide `source-path=SCRIPTDIR`
(01.2) lets shellcheck follow.

## 2. Names

- **Commands and options** are the ones decisions.md names: `plan landed
  STEP COMMIT`, `plan unblocked`, `plan steered`, `plan handover
  REPOSITORY NN CONTEXT`, `plan resume [CAUSE]`, `plan context
  [SESSION]`, `plan idle REASON [--tasks ID…]`, `heavy --task`.
- **log.md's headings** are development.md's words exactly, with
  `$(now)`'s UTC time: `## Coordinator resumed, <time>: <cause>`, `##
  Coordinator loaded, <time>: by <command>`, `## Blocked and unblocked,
  <time>`, `## Steered, <time>`, and the line `Handover, <time>:
  <repository> <NN> at <context>`.
- **The watch's words** end in a colon and start the line: `blocked:`,
  `answer:`, `check:`, `started:` (04.4), `hourly:` (04.5), `alert:`
  (04.5), `quiet:`, `quiet again:`, `paused:`, `done:`, `context:`,
  `handed over:`. Lines under a word are indented by two spaces.
- **Environment settings** start `PLAN_` or `HEAVY_`, are read once at
  the top of the file that uses them, with their default, and are named
  in its header comment.

## 3. Shared interfaces and their owners

| Interface | Owner | Users |
|---|---|---|
| `plan-check.sh`, `plan-watch.sh`, `plan-roles.sh`, as files | skein 01.2 | 03, 04, 05.2, 06 |
| `alerts.md`, and `plan status` printing it | skein 01.1 | 04.5 |
| `heavy --task`, and heavy.tsv's row written by the task | skein 02.3 | 02.4 |
| `machine_setting KEY DEFAULT` in kit.sh | skein 03.1 | `plan`'s checks, `heavy`, the pulse's disk rule (05.5) |
| `log_entry HEADING [BODY_FILE]`: appends to log.md under its lock | skein 04.1 | 06.3 |
| `coordinator_turn COMMAND STEER START`: loads the thread if needed, then steers or starts a turn | skein 04.1 | `ring`, `resume` (06.3), the watch (04.3) |
| `$t/stub/pueue`, the queue the watch reads in test-plan | skein 04.3 | 04.5 |
| `$t/stub/systemctl` | skein 03.3 | none |
| `PLAN_CODEX_FAKE` and `test-codex`'s state directory | skein 04.1 | 04.3, 06.3, 06.4 |
| `boot_unit enable\|disable` in plan-roles.sh | skein 03.3 | `start`, `release` |
| `metrics.sh` | skein 05.1 | 02.4, 05.2 to 05.5 |

## 4. Tests and fixtures

- **One suite, `scripts/dev/test-plan`.** Every check is an `expect
  "<name>" "<output>" "<wanted line>"…` (test-plan:26) on the workspace
  it builds in `$t`; its helpers, at their lines after 01.1, `commit`
  (:125), `assistant` (:193), `boundary` (:200), `watch` (:257) and
  `request` (:346), are reused, never copied. A check's name says what it shows.
- **Stubs go in `$t/stub`,** first on `PATH` only for the call that
  needs them (`PATH=$t/stub:$PATH "$plan" …`), so that `plan check`'s
  machine lines elsewhere stay the machine's. A private queue (base-runs.md, 6): a
  `pueued` started once, at the suite's top, with `PUEUE_CONFIG_PATH`
  exported as `$t/pueue/pueue.yml`, and shut down by the EXIT trap, made
  by 02.3; the fake Codex by 04.1.
  Never the machine's queue or daemon.
- **The live block** (`--live`, test-plan:355 after 01.1) gets one
  check for each live Proof, after its existing ones, on the same
  thread: it starts no second coordinator.
- **Time:** `test-plan` takes 12 to 13 s at the base. A Proof's "under N
  s" bounds its own checks; together the plan's checks may add about
  25 s, to about 40 s in all, the Pulse's focused budget; a `sleep` in
  a check is at most 1 s, and polls use `PLAN_POLL=0.1`. Each report's
  Evidence gives `test-plan`'s time before and after its increment, and
  a check over its Proof's bound is cut to it before it lands
  (ruling 104).

## 5. Don't

- No function beside an existing one for the same thing: change it in
  place, and its callers in the same increment.
- No check that reads the machine's own queue, Codex daemon, Claude
  sessions or `~/src/rust`: `test-plan` builds its own.
- No `eval` and no `source` of a file a run or the user writes (the
  machine file is parsed).
- No shellcheck `disable` without the reason on its line, as heavy:45
  has it.
- No new `PLAN_*` setting where a file of the run or the machine says it.
- The words the Pulse forbids: legacy, compat, hack, workaround.

## 6. Size

A file of at most 800 lines (the Pulse); a new function of at most about
60, and a function an increment changes grows by at most about 20 (the
base has longer ones: `check_machine`, 155 lines; `metrics_of`, 118).
`test-plan` is 409 lines after 01.1 and grows with every session: an
increment that would take it past 800 first moves its live block
(test-plan:355–403) into `scripts/dev/test-plan-live.sh`, sourced, as a
commit of its own: skein-05.s1 did, at 4d6df9e (ruling 105). A further
move this section calls for is a step of its own, `<session>.s<N>`:
the implementer names it in its report's Next before it starts, and the
coordinator adds it to the ledger, as for 05.s1.
An increment's size is its hand-written production lines, test-plan
and its sourced `test-plan-*.sh` (ruling 108), `test-codex` and
Markdown aside, counted from the worktree with:

```
git diff --numstat origin/main -- scripts/dev ':!scripts/dev/test-plan*' ':!scripts/dev/test-codex' ':!scripts/dev/workspace/*.md' | awk '{ a += $1 } END { print a + 0 }'
```

A pure move (01.2, 05.1) counts only its new lines (the files' headers
and `source` lines), not the lines it moved; its report gives `git diff
--stat` and names the functions moved.

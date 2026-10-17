# The adopted kit changes, against main

Read at skein `b30cc95` (origin/main, 2026-10-17), from each
retrospective's section 8 (the proposal), section 5 (its evidence) and
section 10 (its decision): temper's `forgejo-17` and `protocol`, jig's
`client-composition` and `skein-ui`, smith's `parity` and `wall-clock`.
"Kit" is `scripts/dev/` and `scripts/dev/workspace/`; the documents are
skein's `docs/foundation/`.

## 1. The rows on `kit-after-parity`

The branch holds one commit beyond its base `bc26df2`, 135 commits behind
main: `d7e6ee3` ("Decide test-budgets' and the reliability run's open
proposals"), 7 files, +443/−166. Its `scripts/dev/` part (land 66, plan
43, numbers.sh 81, test-plan 35 lines changed) applies to `b30cc95` with
`git apply --3way` and no conflict, and `test-plan` passes on it
(base-runs.md, 3). Its document part does not apply: main's
development.md, implementer.md and supervisor.md moved under it. A
sub-agent read each of its rules against main:

| Rule of d7e6ee3 | On main | Where it goes |
|---|---|---|
| 3. done.md opens "Recorded <UTC time>." (7.6, 3.5) | missing | 01.1, with the kit |
| 5. every alert also goes to `alerts.md`; `plan status` prints it first (3.5, 10.5; supervisor.md 2) | missing | 01.1 |
| 10. land's turn from rebase to push; refusal of unread Plan/Step/Proof lines and of the Pulse's words (9.1, 13 "Gating again"; implementer.md 8.4, 8.5) | missing | 01.1 |
| 11. 14.2's record bullet | missing | 01.1 |
| 12. 15.2's `wall_h`, `proofs` by each step's last commit, `lines` in four kinds | missing | 01.1 |
| 14b. 3.6's `plan check --fix` row: fast-forward a clean primary checkout only behind; hide `.codebase-memory/` | missing | 01.1 |
| 1. the README names the longest landing chain, in hours at `step_h` (5.2) | missing | 02.1 |
| 4. a find-the-cause step's file gives a budget of runs and minutes (10.4; implementer.md 8.7) | missing | 02.1 |
| 7. `nextest -E 'rdeps(crate)'` once before land (implementer.md 8.1, 8.3) | missing | 02.1 |
| 8. edits that follow the Shape need no question, listed under "Users outside Files:" (implementer.md 8.2, 8.5; 7.6) | missing | 02.1 |
| 9. diagnostics never committed (implementer.md 8.2) | missing | 02.1 |
| 13. Done when: a live probe's sentence (5.2) | missing | 02.1 |
| 14k. a failing Done-when command reaches the user as an alert (supervisor.md 8) | missing | 02.1 |
| 2. `findings/planning.md`, and `plan check`'s warning | superseded by parity 10: the outputs go in `findings/` | dropped (decisions.md, 2) |
| 6. README and code-organization read by cited sections; "Rules that bind it" repeats all | superseded: main's 5.2 says "the two or three" | dropped |
| 14d. at most eight increments a session | superseded by main's 5.3 | dropped |
| 14e. the implementer splits an outgrown increment | superseded by implementer.md 8.6's tolerance | dropped |
| 14a, 14c, 14f–14j, 14l–14o | on main already | nothing |

## 2. The later proposals

| Proposal | What the kit does | On main at b30cc95 | Increment |
|---|---|---|---|
| forgejo-17 1 | `plan check` refuses a plan with no base runs in `findings/`, or with design questions open | no check | 03.2 |
| forgejo-17 6 | `plan downstream` removes its worktrees when its check ends | worktrees stay | 03.4 |
| skein-ui 9 | `plan context` for a Claude session | no command | 06.2 |
| skein-ui 10 | `plan downstream`'s worktrees outside `wt/`, removed at the end | under `wt/` | 03.4 |
| protocol 4 | the coordinator compacts at 200,000 tokens | the profile says so (c0268a8), but `start_coordinator` sends no profile: the thread never gets it (section 3) | 06.4 |
| protocol 5 | no graph server in the coordinator; implementers query only their project | neither | 06.4 |
| protocol 6 | `plan landed` runs 7.3's checks; the coordinator's fixed headings written by the kit | by hand | 06.1, 06.3 |
| protocol 8 | ring's prefix; ring loads the thread; the boot unit; the unattended-upgrade warning | `plan resume` loads (7779af1); ring exits 3 | 04.1, 03.3, 06.3 (the resume's cause) |
| protocol 9 | `test-plan --live` checks the 300,000 ms wait | unchecked | 06.5 |
| protocol 11 (parity 15) | `plan metrics` as 15.2 says; the `split` state counted | partly, by d7e6ee3's numbers.sh | 05.2, 05.3, 05.4 |
| protocol 12 | land: a lost ref-lock race is exit 6; `plan set` refuses a step starting with `-` | exit 7; accepted | 02.2 |
| client-composition 4 (parity 5) | ring, resume and the watch load an unloaded thread; `plan idle` waits on queue tasks | resume only | 04.1, 04.3 |
| client-composition 5 | a new supervisor's first watch prints every open event | only new ones | 04.2 |
| client-composition 9 | `Designer-Session:` counted; a Proof's layer up to its first hyphen; hand loads as restarts | none | 05.2, 05.3, 05.4 |
| parity 6 | `PLAN_CONTEXT_LIMIT` 150,000 | 200,000 (plan:1304, numbers.sh:595) | 05.5 |
| parity 7 | the pulse's disk line: `kache gc` below 80 GB, at most six sessions below 60 GB | text only | 05.5 |
| parity 8 | pueued restarts with no start limit; the watch alerts when it is down | `Restart=on-failure` only | 03.3, 04.5 |
| parity 11 | a watch event when the last hourly check is over 65 minutes old; no cron | cron (supervisor.md 1) | 04.5 |
| parity 14 | "Starting things": never a sub-agent; metrics count a named sub-agent's sub-agents | the first in part | 06.4, 05.2 |
| wall-clock 7 | heavy writes its row inside its task; the pulse reads pueue's tasks | the client writes it | 02.3, 02.4 |
| wall-clock 8 | each message priced at its model's rates; `Designer-Session:` counted as design | Opus rates for all | 05.2 |
| director's candidate 7 | each watch event prints what handling it needs; supervisor.md says so | no | 04.4 |
| per-machine settings | `PLAN_MIN_MEM_GB` and the lanes from a machine file | env only | 03.1 |

## 3. The coordinator's profile never reaches its thread

`start_coordinator` (plan:924–940) sends `thread/start` with `cwd`,
`model`, `sandbox`, `approvalPolicy` and `config:
{model_reasoning_effort, agents: {max_threads}}`; the daemon starts as
`codex app-server daemon start`, with no profile. codex-cli 0.162.1's
`ThreadStartParams` has no profile field (its keys: approvalPolicy,
approvalsReviewer, baseInstructions, config, cwd, developerInstructions,
ephemeral, model, modelProvider, personality, sandbox, serviceName,
serviceTier, sessionStartSource, threadSource). So
`model_auto_compact_token_limit = 200000` in
`workspace/coordinator.config.toml` never reached a coordinator.
`config` takes any `config.toml` key: `codex -c
'mcp_servers.codebase-memory-mcp.enabled=false' mcp list` shows the
server disabled.

## 4. Not read

The director's `experiments.md` is not on this machine: candidate 7 is
taken from the brief's text (decisions.md, 1).

# kit-batch: later

2026-10-17. What the retrospectives or this plan's findings name that the
plan does not do, and when it should be done.

| Item | Design | When |
|---|---|---|
| skein's `AGENTS.md` names `scripts/check.sh` as the gate of every landing; this plan gates the kit with `test-plan` and shellcheck (decisions.md, 5). If the user keeps that, `AGENTS.md` and development.md 3.6 name the kit's gate for a change to `scripts/dev/` alone | development.md 3.6, 9.1; decisions.md "For the user" | the user's answer to "For the user"; then the process designer's next landing |
| shellcheck's info findings at the base (SC1091 ×9, SC2012, SC2013 ×2, SC2016, SC2119 ×6; findings/base-runs.md, 2): the gate runs at `-S warning` | none | the next kit change that opens each file, or never: none is a fault |
| Loading a thread the real daemon has unloaded is tested against the fake only (decisions.md, 22): no client call unloads one, and a restart would unload the run's coordinator | development.md 7.6; client-composition 4 | when Codex's app-server has an unload call, or on a machine with no run: one live check that restarts the daemon |
| The director's other candidates in `experiments.md`, not on this machine (decisions.md, 1) | the director's | the director's next brief |
| The fence ends with temper's client-api: a run claimed after it needs no such rule | decisions.md 3 | nothing to do: the README's command prints 0 |
| `running_h` counts the gaps between the coordinator's events, where 15.2 says pauses and the heartbeat: parity and wall-clock found the two not comparable, no adopted proposal changes it, and the heartbeat's history is not kept | development.md 15.2 | the next retrospective that proposes one definition |
| The layout of a sub-agent's own sub-agents is inferred (flat in the root's `subagents/`, `spawnDepth` 2, its `toolUseId` in its parent's transcript): none was found on hel8. If it differs, `claude_files` counts too little, never too much | none | the first nested transcript seen: check it against 05.2's fixture |
| The coordinator still types `Rang, <time>: <message>` (7.4) and `Steer <time>, point N: done` or `cannot` (10.3) by hand: no adopted proposal gives them a command | development.md 7.4, 10.3 | a retrospective that finds them misread |

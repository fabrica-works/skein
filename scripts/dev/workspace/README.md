# The workspace's operations

For the user and the director, not for agents at work: `AGENTS.md`
beside this file is what every agent reads.

## A new machine

Clone skein, smith, jig, hearth and temper from
`git@github.com:fabrig-dev/` into `~/src/rust/`, then run
`skein/scripts/dev/plan check --fix`. It links the tools (`heavy`,
`land`, `plan`) into `~/.local/bin`, links `AGENTS.md`, `CLAUDE.md`,
the Claude Code settings (`.claude/settings.json`) and the coordinator's
Codex profile into the workspace, sets up the queue, and names what is
still missing (pueue, kache, jq, cargo-nextest, and cargo's `[net]
git-fetch-with-cli = true` in `~/.cargo/config.toml`).

## New work

New work goes through owners: skein's `docs/foundation/owners.md`. The
director starts them; their notes are in `tmp/owners/<name>/`.

## Runs started before 2026-10-19

They finish under skein's `docs/foundation/development.md`, with `plan`:
- `plan status` shows this machine's run; `plan attach supervisor` and
  `plan attach coordinator` reach its roles; `plan resume` after a
  restart of the machine.
- A run's state is in `tmp/<repository>/<plan>/`, its `status.md` first,
  and `tmp/active-plan` names this machine's run.
- After a run, the supervisor starts its retrospective (`plan retro`),
  and the user decides each proposal in its section 10.

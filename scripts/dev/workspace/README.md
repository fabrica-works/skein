# The workspace's operations

For the user and the director, not for agents at work: `AGENTS.md`
beside this file is what every agent reads.

## A new machine

Clone skein, smith, jig, hearth and temper from
`git@github.com:fabrig-ai/` into `~/src/rust/`, then run
`skein/scripts/dev/plan check --fix`. It links the tools (`heavy`,
`land`, `plan`) into `~/.local/bin`, links `AGENTS.md`, `CLAUDE.md`,
the Claude Code settings (`.claude/settings.json`) and the coordinator's
Codex profile into the workspace, sets up the queue, and names what is
still missing (pueue, kache, jq, cargo-nextest, and cargo's `[net]
git-fetch-with-cli = true` in `~/.cargo/config.toml`).

## Work

Work goes through owners: skein's `docs/foundation/owners.md`. The
director starts them; their notes are in `tmp/owners/<name>/`.

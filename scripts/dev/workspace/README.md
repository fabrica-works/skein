# The workspace's operations

For the user and the director, not for agents at work: `AGENTS.md`
beside this file is what every agent reads.

## A new machine

Clone skein, smith, jig, hearth and temper from
`git@github.com:fabrig-ai/` into `~/src/rust/`, then run
`skein/scripts/dev/setup --fix`. It links the tools (`heavy`, `land`,
`setup`) into `~/.local/bin`, links `AGENTS.md`, `CLAUDE.md` and the
Claude Code settings (`.claude/settings.json`) into the workspace, sets
up the queue, and names what is still missing (pueue, kache, jq,
cargo-nextest, and cargo's `[net] git-fetch-with-cli = true` in
`~/.cargo/config.toml`).

## Work

Work goes through owners: skein's `docs/foundation/owners.md`. The
director starts them; their notes are in `tmp/owners/<name>/`.

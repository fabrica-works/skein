# Agent guidance for ~/src/rust

This directory holds the Rust projects, each a git repository with its own
`AGENTS.md`, and the machine's working files in `tmp/`. Read a
repository's `AGENTS.md` before touching its code.

This file is skein's `scripts/dev/workspace/AGENTS.md`, linked into
`~/src/rust/`: change it there, and land it in skein. It holds only what
every agent here needs.

## The projects

| Repository | What it is | Builds on | Its gate |
|---|---|---|---|
| `skein` | the io and protocol kit, and the foundation documents | — | `scripts/check.sh` |
| `smith` | the agent kit | skein | `scripts/check.sh` |
| `jig` | the kit for agentic applications | skein, smith | `scripts/check.sh` |
| `hearth` | the factory's build daemon | skein | `scripts/check.sh` |
| `temper` | the application | skein, smith, jig | `scripts/check.sh` |

Dependencies point upstream only: skein names neither smith nor temper,
smith never names temper, and jig never names temper.

On a machine that keeps its checkouts elsewhere (`~/src/` rather than
`~/src/rust/`), read that directory where this file says `~/src/rust`;
the tools find it by themselves.

## The code's rules

skein's `docs/foundation/programming-model.md` (how code is shaped) and
`testing-strategy.md` (how it is tested) are the rules for every
repository. Depart from them only with a reason, stated in the design or
the commit. Read each one's section 1, "In one page", first, and a
later section only when the work touches it. `notes.md` beside them
holds their reasons and open questions.

## Building and landing

- **Work on a branch in a worktree of your own,** never in a primary
  checkout.
- **Build and test through `heavy`:** `heavy cargo nextest run -p
  <crate>`. It queues every build on the machine, so that builds don't
  starve each other and gates run alone.
- **`main` moves only through `land`,** which rebases, runs the
  repository's gate and pushes; `land --docs` for Markdown alone. Don't
  run the gate yourself before it.

## The code graph

Use the codebase-memory-mcp tools to inspect the code. They index each
repository's primary checkout, `main`, as a project named by that
checkout's absolute path, without its leading `/` and with each `/` a
`-`: `realpath ~/src/rust/<repository> | sed 's|^/||; s|/|-|g'` prints
it. Pass that name to each graph call. The index covers `main`, never a
worktree, and may lag behind it: check each name the graph gives with
`rg` in your worktree before relying on it. Indexing is the director's
and the runs' supervisors': never call `index_repository` or
`delete_project` otherwise.

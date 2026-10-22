# skein: what an agent must know before touching its code

skein is the io and protocol kit, and holds the workspace's foundation
documents and its development tools. Read this, then the design sections
your task names.

## Where things are

- **The design is the contract:** `docs/design/`, cited from the code as
  "tls.md, 3.6". `docs/design/STATUS.md` says what is built.
  `docs/foundation/` holds the rules every crate follows:
  `programming-model.md` (step machines, states, what step code may
  use) and `testing-strategy.md` (tests, worlds, suites); a departure
  needs its reason, stated in the design or the commit. `notes.md`
  holds their reasons and open questions.
- **The layout** of crates and tests: `docs/design/testing.md`, section 6.
  Step crates are `crates/*`; their worlds are `tests/*`; shared test
  harnesses are `testing/*`.
- **The code graph** (codebase-memory-mcp) indexes the primary checkout,
  `main`, as the project named by that checkout's path, as the
  workspace's `AGENTS.md` says: `realpath ~/src/rust/skein | sed
  's|^/||; s|/|-|g'`. Use it for structure; check each name it gives with
  `rg` in your worktree before relying on it.

## The gate

- `scripts/check.sh`: formatting, clippy with warnings as errors,
  `cargo check -p skein-tls` alone, then the focused, fuzzy and browser
  suites. `land` runs it; do not run it yourself before `land`.
- The suites' budgets are in `.config/nextest.toml`: the focused suite
  within 30 seconds (15 seconds before the user's doubling of 10-22,
  provisional), the fuzzy one (binaries named `fuzzy_*`) within 120 seconds
  (60 seconds before the user's doubling of 10-22, provisional). A test past
  its slow period is flagged SLOW.
- Run what compiles and the tests of the crates you touch through
  `heavy`: `heavy cargo nextest run -p <crate>`.

## The code's rules

- **Step crates** follow `programming-model.md`, section 10: no `std`, no
  `unsafe` but where a module says why, every `match` lists every
  variant (no `_`, no `matches!`), no indexing, no unchecked arithmetic,
  no `as`, no `unwrap`. The workspace's `Cargo.toml` sets the lints, and
  `clippy.toml` bans what step code may not use (`include_bytes!`,
  `copy_from_slice`, `Vec` outside a scoped `#[expect]` with its reason).
- **Test crates** under `tests/` are ordinary Rust with
  `tests/clippy.toml`'s bans: one thread, one loop, a seed replays.
- **A lint exception** is a scoped `#[expect(…, reason = "…")]`; never
  `#[allow]`.
- **Comments** say what is true now, in the design's words; no history
  ("originally", "unchanged"), and no emphasis words ("actual",
  "genuine").
- **rustls's debug assertions are off** in every dev build
  (`[profile.dev.package.rustls]`): tls.md, section 6 says why.

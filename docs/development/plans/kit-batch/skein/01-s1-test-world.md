# Supplement skein-01.s1: test-plan's world keeps off the machine

Added by ruling 102, from the ready checks of skein-05.1 and 06.1
(2026-10-18). Read first: skein's `AGENTS.md`; this plan's README,
"Rules of this plan" (the gate; "none restarts pueued").

Scope: 01.1's test "check --fix fast-forwards" (`test-plan:404` at
`817b15c`) runs `plan check --fix` with the machine's `$HOME` and user
bus: `check_machine` (`plan-check.sh:195-204`) relinks
`~/.local/bin/{heavy,land,plan,test-plan}` to the tree test-plan runs
from, and its pueued branch (`:251-279`) reaches the machine's unit
through `systemctl --user`. Each ready check's run relinked the
machine's tools to its scratch worktree. And the world's queue races:
`pueue group add` (`:36`) runs before `pueued -d` (`:35`) has written
its secret; two ready checks' first runs failed there.
Needs: nothing (skein's `main`).
Files: `scripts/dev/test-plan`.
Shape:
- Default: at the top, save the machine's `HOME`, `XDG_RUNTIME_DIR` and
  `DBUS_SESSION_BUS_ADDRESS`; beside `CODEX_HOME` (`:39`) export
  `HOME=$t/home` and unset the two bus variables, so that
  `check_machine`'s links, `~/.cargo` reads and pueued branch stay in
  `$t`; the live block (`:452`, where it unsets `CODEX_HOME`) restores
  the three. If a git command then lacks an identity, write
  `$t/home/.gitconfig` with `test@example.com`. If the wide shape
  breaks a test, run only `plan check --fix` so (`env HOME=$t/home -u
  XDG_RUNTIME_DIR -u DBUS_SESSION_BUS_ADDRESS`), and say why in the
  commit body.
- After `pueued -d`, poll `pueue status` every 0.1 s, up to 10 s, before
  the first `pueue group add`.
Test shape: "the machine's links stay": the `readlink` of the machine's
`~/.local/bin/plan`, taken before the world is set up, equals its
`readlink` after the offline layer; "check --fix links in the world":
`$t/home/.local/bin/plan` links to `$kit/plan`.
Proof: process, `test-plan` twice in a row through `heavy`, from the
session's worktree, both `passed`, with `readlink ~/.local/bin/plan`
before and after printing the same.
Don't: change `plan-check.sh`; stop, restart or reconfigure the
machine's pueued; touch `~/.local/bin` in the commit.
Size: about 20 lines.

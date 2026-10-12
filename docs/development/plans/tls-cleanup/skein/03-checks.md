# Session 03: the kit and the gate

Two small changes under `scripts/`. `test-plan --live` cannot pass on any
machine today, and fails silently; it is fixed first, at once. Then,
once sessions 01 and 02 are done, the gate checks the plan's two rules
that matter most, so that they hold after the plan: none of the Pulse
words, and no file over 800 lines, in skein-tls and its world crate.

## Read first

- development.md, sections 3.6 (the kit) and 4.5 ("Check what matters,
  as well as saying it").
- `scripts/dev/test-plan` and `scripts/check.sh`.

## What the design asks

Nothing: these are the process's tools, not skein's design.

## Choices this plan makes

- The gate's check is a script of its own, `scripts/taste.sh`, which
  takes the directories to check, so that a later plan widens it by
  naming more.

## Rules that bind it

- **Only `scripts/**`** (README, "Rules").
- **3.2 lands after every increment of sessions 01 and 02** (README, "The
  sessions").

## Increments

### 3.1 test-plan --live runs where Claude Code trusts it
Scope: development.md, 3.6's `test-plan [--live]`.
Needs: nothing.
Files: `scripts/dev/test-plan`.
Shape:
  - The temporary workspace is made under the real workspace's `tmp/`,
    which `plan check` requires Claude Code to trust, instead of under
    `/tmp`: `t=$(mktemp -d "$workspace/tmp/test-plan.XXXXXX")`, with
    `workspace=${DEV_ROOT:-$(dirname "$(dirname "$(git -C "$kit"
    rev-parse --path-format=absolute --git-common-dir)")")}`: the
    directory holding skein's primary checkout, also when the script runs
    from a worktree. Today `claude --bg` refuses the `/tmp` workspace
    ("Workspace not trusted"), and `plan start` fails.
  - A failing command never ends the script silently: each `$(…)` whose
    command may fail in the `--live` part ends in `|| true`, so that its
    output reaches `expect`, which prints it; and a `trap … ERR` prints
    "test-plan: failed at line N" for anything else.
Test shape: none (a shell script; `test-plan` is its own test).
Proof: live, from `~/src`, `<worktree>/scripts/dev/test-plan`, then the
  same with `--live`, once each, with the cheap models the script sets (a
  few cents of each program's usage); each prints "test-plan: passed",
  within 5 minutes. If one fails, fix what it names and run that one
  once more; after a second failure, report its output as a question,
  and do not run it again.
Don't: change `plan`; change what `test-plan` expects.
Size: at most 30 lines.
Removes: nothing.

### 3.2 The gate checks the words and the file limit
Scope: development.md, 4.5.
Needs: the crate and the world crate free of the Pulse words and within
  800 lines a file: every increment of sessions 01 and 02 (1.1 to 1.5,
  2.1 to 2.4).
Files: new `scripts/taste.sh`; `scripts/check.sh`.
Shape:
  - `scripts/taste.sh DIR...` exits 1, printing each hit, when a file
    under a `DIR` holds a word of the list, case-insensitive, also inside
    a name joined by `_` (`rg -i
    '(^|[^[:alnum:]])(actual|…|truly)([^[:alnum:]]|$)'`, as the README's
    "Done when", item 2), or a `.rs` file under it is over 800 lines;
    otherwise it exits 0 and prints nothing.
  - Its word list is the README's Pulse `words`, written once in the
    script with a comment naming this plan.
  - `scripts/check.sh` runs `scripts/taste.sh crates/skein-tls tests/tls`
    after `cargo clippy`.
  - At most 30 lines, POSIX `sh` like `check.sh`.
Test shape: none.
Proof: process, `scripts/taste.sh crates/skein-tls tests/tls` exits 0;
  then, with a temporary copy of `crates/skein-tls` under
  `<run>/logs/skein-03/` holding one comment with "actually" and one file
  of 801 lines, it exits 1 naming both; once, seconds.
Don't: check other directories; change the gate's other steps.
Size: at most 40 lines.
Removes: nothing.

## Done when

- README's "Done when", items 3 and 8, hold.

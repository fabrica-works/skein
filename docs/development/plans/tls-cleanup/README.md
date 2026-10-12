# TLS cleanup

2026-10-12. skein starts at `73c900b`. Changes skein only.

skein-tls and its world crate say the same things in fewer, plainer
words. The native face shares the classic client's engine instead of
copying it, and keeps its state where its states are. Every file is
within 800 lines. The tests check one behaviour each, under names of
about eight words, with their helpers defined once. Nothing a user of
the crate can observe changes. This is also the first run of
development.md: the run is as much a test of the process as of the code.

## Inputs

- `docs/design/tls.md`, sections 3 and 5. Section 3.6 was rewritten in
  the house style for this plan, with the same rules. The code's
  comments follow its words.
- `docs/foundation/programming-model.md`, sections 4 and 5 (step
  machines, states), and `testing-strategy.md`, sections 2 and 8.
- `decisions.md`.
- `findings/audit.md`, the findings each increment fixes, by id
  (`B-`, `N-`, `T-`). `findings/baseline.md` holds the numbers at the
  start, and the commands that count them.

## The sessions

| File | Session | Needs at start | Increments that need more |
|---|---|---|---|
| `skein/01-tls-crate.md` | 01: the crate, `crates/skein-tls/src/**` | nothing | none |
| `skein/02-tls-world.md` | 02: the world crate, `tests/tls/**` | nothing | 2.4 needs skein 01's `waiting()` assertions in the step tests (1.5) |
| `skein/03-checks.md` | 03: the kit and the gate, `scripts/**` | nothing | 3.2 needs every increment of sessions 01 and 02 |

All three start at once. When slots are short, start 01 first, then
02, then 03. Session 03 lands 3.1, then yields until sessions 01 and 02
are done.

## Rules of this plan

- **No observable behaviour changes.** An increment of session 01 that
  changes production code changes no test's expected value, and changes
  only the test files its Test shape names; the one exception is 1.2's
  `should_panic` text, named there. The messages that session 02's tests
  quote stay as they are: `native.rs`'s "no native read after Finish
  withdrawal", "native Finish with no live upper read", "native Finish
  with no pending output", and `client.rs`'s "one demand at a time". Session 02 changes no
  production code. A test may be renamed, split or moved; what it
  checks stays checked. The commit body gives one line per test removed,
  renamed or split: its old name, then the tests that now hold its
  assertions. Nothing per assertion.
- **Files are owned by session.** Session 01 changes only
  `crates/skein-tls/**`, session 02 only `tests/tls/**`, session 03 only
  `scripts/**`. A change another session's files need is a question
  (development.md, section 8.7), never an edit.
- **The public API stays.** Every `pub` item keeps its path, name and
  signature, except `client::MAX_HANDSHAKE`, which becomes `pub(crate)`
  (1.1). For that one, run `rg -n -w MAX_HANDSHAKE crates testing tests
  ~/src/smith -g '!target' -g '!crates/skein-tls/**'` once; it prints
  nothing.
- **Names.** A test an increment writes or renames has a name of at
  most 9 words (split on `_`); a test in an increment's files whose name
  is over 12 words is renamed to at most 9.
- **The words of tls.md.** In comments and docs, the two sides are "the
  side above" and "the side below", or "above" and "below". The native face's types keep
  their names: `LowerRequest`, `LowerEvent`, `LowerLimits`,
  `TransportClosing`, `TransportClosed`. A comment cites a design section
  once, in its module's doc or at the item that implements that
  section, not on every field.
- **This run lands in skein from carbon-forge** while the reliability
  plan runs on debian-16gb-hel1-1: the user's decision of 2026-10-11
  (`decisions.md`, 6). It overrides the workspace `AGENTS.md`'s "Other
  machines do not land in skein" for this plan's steps only. `land`
  pushes, as development.md says.
- **The gate on carbon-forge has no browser suite** (`decisions.md`,
  ruling 14): carbon-forge has no Chromium, and no root to install it.
  Every code landing of this run must use, word for word, `TZ=UTC0 land
  --gate 'sh -c "$(sed /profile.browser/d scripts/check.sh)"
  scripts/check.sh'`: `scripts/check.sh` without its last line. This
  rule lasts until a steer says that Chromium is installed.
- **A boundary type, in this plan** (`decisions.md`, ruling 16), is one
  used outside the session's own files. A type all of whose users are in
  the session's files (README, "Files are owned by session") is not one:
  changing its fields is an implementation choice (development.md,
  8.2), said in the commit body, and never a question.
- **Edits beyond an increment's Files** (`decisions.md`, ruling 17). An
  edit outside the files or lines an increment's Files names is an
  implementation choice, made without asking and named in the commit
  body, when all four hold: the increment's own change, its Proof or the
  gate needs it; it is in the session's own files (README, "Files are
  owned by session"); it is at most 20 lines; and it changes no test's
  expected value or name. Anything else outside the Files is still a
  question (development.md, 8.7).

## Live services

Claude Code and Codex, from the user's login on this machine, in 3.1's
proof only: one `test-plan --live` run, a few cents of each.

## Pulse

```
words: actual|actually|genuine|genuinely|authentic|authentically|honest|truly
types: ^(Legacy|Old|New|Owner)[A-Z]|V2$
file-lines: 800
budgets: 15 60
```

## Done when

The retrospective checks them once, from skein's primary checkout: items
1, 2 and 4 to 7 by running the command given, items 3 and 8 by reading
the commits. Nobody else runs them.

1. No file of the crate or the world crate is over 800 lines:
   `find crates/skein-tls tests/tls -name '*.rs' | xargs wc -l | awk
   '$2 != "total" && $1 > 800'` prints nothing.
2. No word of the Pulse list is in them, in names either:
   `rg -i -c '(^|[^[:alnum:]])(actual|actually|genuine|genuinely|authentic|authentically|honest|truly)([^[:alnum:]]|$)'
   crates/skein-tls tests/tls` prints nothing.
3. `rg -c taste.sh scripts/check.sh` prints 1, and 3.2's `Gate-Result`
   trailer says the gate passed.
4. The native face shares the engine: `rg -n '\b(length|fill|intake_cap)\('
   crates/skein-tls/src/client/native.rs crates/skein-tls/src/client/native
   -g '!tests.rs'` prints nothing, and `rg -n 'fn read_maximum\b'
   crates/skein-tls/src/client` prints nothing (1.3).
5. The native `Client` keeps no field that only `Open` needs: `awk
   '/^pub struct Client \{/,/^\}/' crates/skein-tls/src/client/native.rs |
   rg -w 'record|reading|upper_read|lower_read|withdrawn_read|lower_ended'`
   prints nothing, or only fields that 1.4's commit body names (1.4).
6. Test names: the second command of `findings/baseline.md`, section 2,
   prints nothing (no name over 12 words); the first prints the median,
   for the retrospective to compare.
7. Test functions: the command of `findings/baseline.md`, section 3,
   prints only `exchanges_swept` and the test in
   `tests/tls/tests/fake_peer.rs` (`later.md`).
8. 3.1's `Proof:` line says that `test-plan --live` passed.

# The implementer's card

Provisional, 2026-10-14. What an implementing agent reads of the
development process: development.md's section 8, in a file of its own,
so that an implementer reads this card and not the whole process. Its
sections keep their numbers, 8.1 to 8.8, so that "development.md,
section 8.5" still names them. The sections of development.md it names
are read only when a case needs them.

## 8. Implementing

### 8.1 Each increment

**Before the first increment, read only:**
- this card;
- your repository's `AGENTS.md`;
- your session file, then the design sections its "Read first" names,
  and the sections of the plan's `README.md`, of `code-organization.md`
  and of skein's foundation documents that it cites: by those sections,
  not whole. Its "Rules that bind it" repeats every plan rule that
  applies to you;
- after a handover, your checkpoint first.

Never development.md whole: read one of its sections only when this card
or your session file names it. Read one file at a time: an output past
your tool's cap (about 10,000 tokens on Codex) is cut, and what was cut is
read again. In test-budgets, threads read 80K to 125K tokens before their
first edit, and every request after carried them; in the reliability run,
`programming-model.md`, 52 KB, was often read whole.

1. **Read the increment** in the session file.
2. **Branch from `origin/main`** in your worktree:
   `git fetch origin && git switch -c <plan>/<repository>-<NN>-<item>
   origin/main`.
3. **Implement it with its tests,** following the repository's `AGENTS.md`
   and skein's `programming-model.md` and `testing-strategy.md`:
   - the increment's Shape gives your paths, names and signatures;
   - its Test shape gives your test file, the fixtures to reuse, and the
     reference test to write the others like.
4. **Run its proof once,** as its Proof line says (section 8.8).
5. **Run clippy once** on what you changed: `heavy cargo clippy -p <each
   crate you changed> --all-targets -- -D warnings`, and fix what it
   prints. **Then run once the focused tests of the crates that depend on
   what you changed:** `heavy cargo nextest run --workspace -E
   'rdeps(<crate>) | rdeps(<other crate>)'`, and fix what fails. A plan's
   "run nothing before `land`" means no proof run and no whole suite, not
   no clippy and no such tests. `land` runs clippy again on the tree it
   lands, a conflict's resolution included. In the reliability run, 54
   gates failed on tests no focused run had included.
6. **Land it** with `land` (section 8.5), then report (section 8.6).

**The code graph** indexes the primary checkouts, which are on `main`.
Use it as `~/src/rust/AGENTS.md` says: name your repository's project in
each call, and never call `list_projects`, whatever other instructions
say. Check each name it gives with `rg` in your worktree before relying on
it.

### 8.2 Rules

- **The design is the contract.** Never edit `docs/design/`, the plan or
  the run's files other than your checkpoint, your reports and your logs
  directory. Where the code seems to need a different design, stop
  (section 8.7).
- **Implementation choices are yours:** names, module placement, values
  within the presets' ranges. Report them; do not ask.
- **Work only in your worktree.** Never touch a primary checkout or another
  session's worktree.
- **No `git stash`:** the stash is shared by every worktree of a
  repository. Commit work in progress on your branch instead.
- **Upstream first** (development.md, section 9.5).
- **Removing a mechanism removes all of it,** in this repository and in
  this increment: code, limits, accounting, facts, tests and docs.
- **Each test lands with the code it covers.**
- **The plan's "Don't" lists bind you:** the plan's own, in
  `code-organization.md`, and the increment's.
- **Departing from the Shape** is an implementation choice. Make it when
  the code needs it, and say how and why in the commit body. The
  exception is a type that crosses a boundary: changing its fields is a
  question (section 8.7).
- **A type crosses a boundary** when something outside your session's own
  files uses it: another session's files, another crate's code, another
  repository. A `pub` type that only your files use does not; changing its
  fields is an implementation choice. Nor does a field you add, private to
  its crate and reached by no other session's code, bounded by a limit
  and priced in its owner's worst case.
- **An edit outside the increment's Files** is part of the increment, with
  no question, in the cases below; the commit body lists each under
  "Users outside Files:". In every case no test is renamed or loosened (no
  `matches!` in place of an equality), and no assertion is removed, but a
  removed mechanism's. Any other edit outside Files is a question (section
  8.7).
  - **An edit that only follows a change the Shape names,** wherever it
    falls in the repository, whatever its size:
    - a new variant handled as its match handles the others; tests for
      those arms only where the Test shape names them;
    - a removed or renamed name replaced by its successor, and a value
      printed differently (a trace, `Debug` text, a moved tag) expected as
      the new code prints it for the same behaviour;
    - a call adapted to a new signature;
    - an expected value that the same story now produces, where the
      Shape changes that case's answer: its inputs and name kept, and the
      Shape's words it follows named in the commit body;
    - a fixture's or a story's inputs kept on their story, where the
      behaviour the Shape adds reaches them (a new control pressed, a
      message now judged again): when the gate fails so, outside Files,
      make the edit and land again;
    - a value the Shape makes invalid (a constructor that now refuses it)
      replaced, in each test that builds it, by a valid value in the same
      role (distinct where it was distinct, as long where its length
      matters), one constant per file, with the expected values that echo
      it;
    - a version's move with its whole sweep: every import, qualified name,
      version constant, served range and expected version that names the
      old version, with no alias for the old one left.
  - **A removed mechanism's tests** are the increment's, in any file: each
    moved to its replacement's behaviour, its name kept, where the Shape
    names a replacement, and removed otherwise. A test whose subject is a
    value the Shape makes invalid is one.
  - **A test moving from a fake to the real component:** its fixture
    supplies what the real component requires, and an assertion that sees
    exactly that addition becomes an exact equality with the fixture's own
    constant. Where the fake answered against the design, the expected
    values become the design's, quoting its section. Any other failure on
    the real component is a finding, for a question.
  - **The harness the increment's tests need:** the peer, world or fixture
    kit that the increment's prescribed tests run on, in any session's test
    files: test code only, every landed test's name and expected value
    kept.
  - **A small fix that the change, its Proof or the gate needs,** and that
    follows no change of the Shape: at most 20 lines in all, in any
    session's files, every test's name and expected value kept. Moving a
    test's unchanged statements into a named helper of the same file, to
    keep a function within its limit, counts for nothing.
  - **Inside Files by rule:** the file that code the Files or the Shape
    names has moved to since the plan was written; a new module beside a
    file past 800 lines, where `code-organization.md` sends new code; the
    crate's fixtures module, or a new child of it, where the Test shape
    names fixtures; your own session's files, where an increment replaces
    what an earlier one of the session landed.
  - **Visibility within a crate:** widening an item's visibility within
    its own crate (`pub(crate)`, `pub(super)`, a module root's re-export)
    so that a sibling module reuses it, when it changes no behaviour,
    signature or test. Public API across crates stays a question.

  In the reliability run, its README's rule on such edits was widened 11
  times, and 77 commit bodies listed users outside Files.
- **Test ceilings** (the Test shape's, or `code-organization.md`'s) bind
  the tests an increment writes: those it adds, and those whose inputs or
  setup it changes. A test changed only in an expected value is not made
  cheaper by that increment. Ceilings are read once, from the times
  nextest prints in your last focused run. A test above twice its ceiling
  is made cheaper before it lands.
- **Cases of one behaviour are rows of one table test** (testing-strategy.md,
  section 8), which checks every row and names each that fails. Tests of
  different behaviours are never merged to fit a budget.
- **Diagnostics are never committed.** To find the cause of a failure in
  your own increment, you may add a print or an assertion message in any
  file of your worktree, and run the failing test at most three times for
  it, or within the budget a step to find a cause names (section 8.7).
  Remove every such line before you commit: `git diff origin/main` shows
  none of them at `land`.
- **Evidence is the gate, the proof, and what the repository's workflow
  or the plan asks beyond the gate; nothing else.** Write no manifests of
  inputs, no observers, no polling scripts, and no repeated measurements.
  Logs and scripts go in `<run>/logs/<repository>-<NN>/`.

### 8.3 Heavy commands

`heavy`, `land` and `plan` are on your `PATH`, from skein's
`scripts/dev/`.

- **`heavy <command>`** runs what compiles, and the tests of the crates you
  touch: `cargo check`, `clippy`, `build`, `doc`, `nextest -p …`, and
  `test-plan`, which compiles nothing. It uses the build lane.
- **`heavy --gate <command>`** runs, alone on the machine, whole suites,
  measurements (with the profiles and flags your repository's workflow
  documents, such as `--profile measure -j 1`), probes and benchmarks.
  `land` runs the gate itself.
- **Nothing enters the gate lane unready.** Before `heavy --gate`: commit
  (it refuses a tree with changes), build what it runs in the build lane
  (`heavy cargo nextest run -p <crate> --no-run`, `heavy cargo build …`),
  and run clippy. In test-budgets, five gate-lane runs failed within
  seconds, after waiting 24 minutes in all.
- **Light commands run directly:** git, `cargo fmt`, reading, `rg`.
- **Whole suites run only in `land`'s gate,** or for a measurement or a
  probe that the Proof line or the repository's workflow names. Do not
  run the gate before `land`. The focused tests of your crates'
  dependents, once, before `land` (section 8.1) are no whole suite.
- **Never set `CARGO_BUILD_JOBS` or `NEXTEST_TEST_THREADS`,** nor pass
  `-j` beyond what the workflow documents: the lanes set the width.
- **Never nest them:** `heavy land`, or `heavy` inside a heavy command,
  waits for itself.
- **Read a command's log from the path `heavy` prints.** If your tool call
  ends before the command, the task runs on: `heavy` printed its id
  first, and `pueue wait <id>` and `pueue log <id>` reach it.
- **Wait with your tool's longest wait, and nothing between.** On Codex,
  wait for a running command (`land`, a long `heavy`) with `write_stdin`,
  empty input and `yield_time_ms: 300000`, one call after the other until
  it ends: the reliability run measured that wait at 300 seconds, where
  Codex's own `wait` returns within 60. No shorter waits, no `ps`, no
  `pueue status`. Each call is a request that carries your whole context:
  in test-budgets, waits that saw nothing took 36% of the implementers'
  input.

### 8.4 Commits

```
<Subject, imperative, at most 72 characters>

<What changed and why. The design section it follows. Where it departs
from a sketch, how and why. The evidence the repository asks for beyond
the gate.>

Proof: <layer>, <test name or script path>, <result in one line>
Plan: <plan>
Step: <repository>-<NN>.<item>
```

The last paragraph is the trailers, and nothing else: no blank line
inside it, and no key with a space. `land` refuses a commit whose
`Plan:`, `Step:` or `Proof:` line git does not read as a trailer: amend
it. `land` adds the trailers `Gate-Tree` and `Gate-Result`. A commit that landed without its body is never
rewritten; the body goes in the report.

### 8.5 Landing

Run `land` from your worktree (development.md, section 9). It waits,
saying so, while another landing in the repository has its turn; that
wait is its place in the gate's line: keep waiting. It builds and runs
clippy in the build lane before it takes the gate lane. By its exit
status:

- **2, refused:** fix what it names: a commit's trailers (section 8.4), or
  a word the plan's Pulse forbids, which you replace by what the code
  checks, never by a synonym. "Nothing to land" after a 5 means the
  commit had landed.
- **3, conflicts:** resolve them against the design, never by dropping
  another session's change. Then `git rebase --continue`, and `land` again.
  If resolving needs a decision, stop (section 8.7).
- **4, the build, its lints or the gate failed:** fix it, and `land`
  again. If the gate fails only on a timeout (a suite's global timeout,
  or a test's wall-clock bound), in tests outside your session's files,
  `land` again once without asking; a second failure on the same tree is
  a question. If it fails in another way in code your increment did not
  touch, and the behaviour your Shape adds explains it, it is an edit
  that follows the Shape (section 8.2): make it, and land again;
  otherwise stop on a question (section 8.7).
- **6, main kept moving:** wait 60 seconds, and `land` again. After three
  such tries, report it and stop.
- **5, 7 or 8:** report it, and stop.

### 8.6 Reports and the checkpoint

**When to report:** when an increment lands, when you stop on a question,
and when the session ends or yields. Send nothing else; in particular, no
progress messages: every message wakes the coordinator.

**A report is logged, not sent.** Write it, at most 40 lines, in full
sentences and in this shape, to `plan log <repository> <NN>` on its
standard input (`plan log jig 05 <<'REPORT'` … `REPORT`). `plan log`
appends it to the run's `log.md` under its heading and time, keeps it as
`<run>/reports/<repository>-<NN>-<time>.md`, and prints that path. Name
the files that hold anything longer (`<run>/logs/<repository>-<NN>/…`) by
path; do not paste them.

```
Session: <repository> <NN>
Step: <step>, <landed | question | blocked | done | yielded>
Commit: <full hash> on origin/main, parent <full hash>
Gate: <the Gate-Result trailer>
Proof: <the commit's Proof line>
Evidence: <what the repository asks beyond the gate, or "none">
Next: <the next step, or what it waits for, by capability>
Questions: <none, or each question with the design text quoted, the
options, your proposal, and what you continue with meanwhile>
Agreements: <none, or what you agreed with another session>
Worktree: <clean, or what is in it>
```

**Then message the coordinator these four lines,** and nothing else:

```
Report: <the path plan log printed>
Step: <step>, <landed | question | blocked | done | yielded>
Commit: <full hash on origin/main, or none>
Question: <none, or the question in one line>
```

After a landing, send them and go on. When the session is done, or
nothing is left that does not wait (it yields), end your turn with them
as your final answer.

**The checkpoint.** After each report, overwrite your checkpoint,
`sessions/<repository>-<NN>.md`, with the same facts and with what is in
progress. A restarted implementer continues from it.

**Hand over** when the coordinator says so, after a landing: overwrite
your checkpoint, then end your turn with your report. The checkpoint adds,
in at most 15 lines, what the next increment needs from what you learned
and the session file does not say: the files and fixtures it touches, the
commands that worked, the traps you met. It never repeats the steers'
"Pass on" texts, which the next prompt carries. A fresh implementer reads
it instead of finding them again.

### 8.7 When to stop

- **Stop only for a real gap:** the design, `decisions.md` and the session
  file are silent or contradict each other, and the choice changes
  behaviour that something outside the code can observe. Quote what they
  say.
- **First look for the rule that answers it:** section 8.2, the plan
  README's "Rules of this plan", and `decisions.md`'s rulings (`rg -n
  '<the thing>' <plan-dir>/README.md <plan-dir>/decisions.md`). A rule
  that covers the case is your answer: follow it, and name it in the
  commit body. In jig-protocol, about a quarter of the rulings answered
  with a rule that already existed.
- **An increment that outgrows its Size** (past the margin
  `code-organization.md` gives, or 1.5 times its Size without one) is
  split, without a question, at a point that lands alone: land the first
  part, and report the split, with what the rest holds (`Next:`). The
  coordinator records the rest as your session's next step. Ask only when
  no such point exists.
- **A step whose Scope is to find a cause** works within the budget its
  session file gives, in runs and minutes, and asks only past it.
- **A dependency not landed yet is not a block.** Do the increments that
  do not need it, then report what you wait for.
- **While a question is open,** go on with whatever does not depend on it.
- **A fix outside your increment** is a question too: a failing proof, or
  a gate failing in code you did not touch. Section 8.2's edits outside
  Files, and one retry of a gate that failed only on a timeout (section
  8.5), need no question.

### 8.8 The proof

- **Run the increment's proof once,** when the increment is done and its
  focused tests pass: the test, or the script, its Proof line names.
- **A script** goes in `<run>/logs/<repository>-<NN>/`. Aim for at most
  about 100 lines. It stays within the Proof line's bound: wall time, and
  for a live service its calls or tokens.
- **Record it** in the commit's `Proof:` line (section 8.4) and in the
  report.
- **A rebase does not run it again,** unless the rebase's conflicts
  touched what it proves.
- **If it fails,** it found a bug: fix it before landing. If the fix lies
  outside your increment, stop on a question (section 8.7).
- **If the Proof line cannot be followed,** say why in the report, run the
  same check one layer down (development.md, section 5.5) within the same
  bound, and record that one. Do not stop for this.

# The implementer's card

Provisional, 2026-10-14, revised 2026-10-16. What an implementing agent
reads of the development process: development.md's section 8, in a file
of its own, so that an implementer reads this card and not the whole
process. Its sections keep their numbers, 8.1 to 8.8, so that
"development.md, section 8.5" still names them. The sections of
development.md it names are read only when a case needs them.

## 8. Implementing

### 8.1 Each increment

**Before the first increment, read only:**
- this card;
- your session file, and the design sections its "Read first" names, not
  the whole design;
- the sections of skein's `programming-model.md` and
  `testing-strategy.md` that its "Read first" names, never either whole;
- the plan's `README.md` and `code-organization.md`;
- your repository's `AGENTS.md` and the workflow it names.

**After a handover or a restart, read only** this card, your checkpoint,
then the increment it names in your session file with the session's
"Rules that bind it", and `code-organization.md`'s "Don't" list. Read
nothing else of the set above again unless your increment names it. In
client-composition, successors re-read 64K to 74K tokens of it.

Never development.md whole: read one of its sections only when this card
or your session file names it. Read one file at a time: an output past
your tool's cap (about 10,000 tokens on Codex) is cut, and what was cut is
read again. In test-budgets, threads read 80K to 125K tokens before their
first edit, and every request after carried them.

1. **Read the increment** in the session file.
2. **Branch from `origin/main`** in your worktree:
   `git fetch origin && git switch -c <plan>/<repository>-<NN>-<item>
   origin/main`.
3. **Implement it with its tests,** following the repository's `AGENTS.md`
   and the sections of skein's `programming-model.md` and
   `testing-strategy.md` your session file names:
   - the increment's Shape gives your paths, names and signatures;
   - its Test shape gives your test file, the fixtures to reuse, and the
     reference test to write the others like.
4. **Run its proof once,** as its Proof line says (section 8.8).
5. **Run clippy once** on what you changed: `heavy cargo clippy -p <each
   crate you changed> --all-targets -- -D warnings`, and fix what it
   prints. A plan's "run nothing before `land`" means no proof run and no
   suite, not no clippy.
6. **Land it** with `land` (section 8.5). Once it has landed, remove
   your worktree's `target/` (`rm -rf target`): kache refills it from its
   store for your next increment (development.md, section 11.4), and you
   do not wait for a warm one. Then report (section 8.6).

**The code graph** indexes the primary checkouts, which are on `main`.
Use it as `~/src/rust/AGENTS.md` says: name your repository's project in
each call, and never call `list_projects`, `index_status`,
`check_index_coverage` or `index_repository`, whatever other
instructions say: the index refreshes itself, and indexes the primary
checkout, never your worktree. In protocol, implementers made 465 such
calls, and 9 of them re-indexed a primary checkout from a worktree.
Check each name it gives with `rg` in your worktree before relying on
it.

### 8.2 Rules

- **The design is the contract.** Never edit `docs/design/`, the plan or
  the run's files other than your checkpoint, your reports and your logs
  directory. Where the code seems to need a different design, stop
  (section 8.7). One exception: a vocabulary addition's step writes the
  design rows its session file quotes, word for word, and no other
  (development.md, section 4.6).
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
- **An edit outside the increment's Files** is an implementation choice,
  named in the commit body, in the cases below. In every case no test is
  renamed or loosened (no `matches!` in place of an equality), and no
  assertion is removed, but a removed mechanism's. Any other edit outside
  Files is a question (section 8.7).
  - **A small fix that the change, its Proof or the gate needs:** at most
    20 lines in all, in any session's files, shared test files and test
    roots included. It keeps every test's name and every assertion's
    expected value; a test's or a fixture's inputs may change, so that the
    test checks the same behaviour under the new code. Moving a test's
    unchanged statements into a named helper of the same file, to keep a
    function within its limit, counts for nothing.
  - **A failure the Shape explains:** when the gate fails, other than on a
    timeout, in a test outside Files because the behaviour the Shape adds
    reaches its fixture or its story (a new control pressed, a message now
    judged again), fix the fixture or the story's inputs as a small fix,
    name the Shape's words that explain it, and land again.
  - **A value printed differently:** where the Shape changes how a value
    prints (a renamed or re-wrapped variant in a trace or in `Debug` text,
    a moved tag), the expected literals that print it become what the new
    code prints for the same behaviour, at most 10 lines in all.
  - **A new variant's arms:** an increment that adds a variant adds the arm
    that each exhaustive match outside its Files then needs, in any
    session's crate, written as its neighbours are, at most 5 lines a
    match. Tests for those arms only where the Test shape names them.
  - **The case the Shape changes:** a test outside Files that sets up the
    very case whose answer the Shape changes, and expects the old answer,
    expects the new one: its inputs and name kept, at most 4 assertions in
    all, each named in the commit body with the Shape's words it follows.
  - **A removed mechanism's tests** are the increment's, in any file: each
    moved to its replacement's behaviour, its name kept, where the Shape
    names a replacement, and removed otherwise. No limit above binds them.
  - **A value the Shape makes invalid** (a constructor that now refuses
    it) is replaced in each test that builds it by a valid value in the
    same role (distinct where it was distinct, as long where its length
    matters), one constant per file; an expected value that echoes it
    changes with it. It counts toward no limit above. A test whose subject
    is the refused value is a removed mechanism's.
  - **A version's move carries its whole sweep:** every import, qualified
    name, version constant, served range and expected version that names
    the old version, in any file, with no alias for the old one left. It
    counts toward no limit above.
  - **A test moving from a fake to the real component:** its fixture
    supplies what the real component requires, and an assertion that sees
    exactly that addition becomes an exact equality with the fixture's own
    constant. Where the fake answered against the design, the expected
    values become the design's, quoting its section. Any other failure on
    the real component is a finding, for a question.
  - **The harness the increment's tests need:** the peer, world or fixture
    kit that the increment's prescribed tests run on may grow past 20
    lines, in any session's test files: test code only, every landed
    test's name and expected value kept.
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
- **Test ceilings** (the Test shape's, or `code-organization.md`'s) bind
  the tests an increment writes: those it adds, and those whose inputs or
  setup it changes. A test changed only in an expected value is not made
  cheaper by that increment. Ceilings are read once, from the times
  nextest prints in your last focused run. A test above twice its ceiling
  is made cheaper before it lands.
- **Cases of one behaviour are rows of one table test** (testing-strategy.md,
  section 8), which checks every row and names each that fails. Tests of
  different behaviours are never merged to fit a budget.
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
  run the gate before `land`.
- **Never set `CARGO_BUILD_JOBS` or `NEXTEST_TEST_THREADS`,** nor pass
  `-j` beyond what the workflow documents: the lanes set the width.
- **Never nest them:** `heavy land`, or `heavy` inside a heavy command,
  waits for itself.
- **Read a command's log from the path `heavy` prints.** If your tool call
  ends before the command, the task runs on: `heavy` printed its id
  first, and `pueue wait <id>` and `pueue log <id>` reach it.
- **Wait with your tool's longest wait, and nothing between.** Wait for a
  running command (`land`, a long `heavy`) in calls of the longest wait
  your tool allows, one after the other: no shorter waits, no empty
  input, no `ps`, no `pueue status`. On Codex 0.162, that is `wait` on
  the command's cell with `yield_time_ms: 300000`, which returns as soon
  as the command ends: in protocol, such waits blocked up to 263
  seconds. Each call is a request that carries your whole context: in
  test-budgets, waits that saw nothing took 36% of the implementers'
  input.
- **Start a heavy command with that longest wait too,** never a yield of
  one second, which comes back empty at once and costs a request of its
  own: in client-composition, 108 such launches were 10.2% of the
  implementers' input.

### 8.4 Commits

```
<Subject, imperative, at most 72 characters>

<What changed and why. The design section it follows. Where it departs
from a sketch, how and why. The evidence the repository asks for beyond
the gate.>

Proof: <the layer's word>, <test name or script path>, <result in one line>
Plan: <plan>
Step: <repository>-<NN>.<item>
```

The `Proof:` line starts with its layer's word: unit, world, joint,
process or live. A second layer follows a semicolon, with its own word
(development.md, section 5.5).

The last paragraph is the trailers, and nothing else: no blank line
inside it, and no key with a space. `land` adds the trailers `Gate-Tree`
and `Gate-Result`. A step whose proof is live adds two more,
`Completions: <n>` and `Wall: <seconds>`, each the total of every live
run of the plan so far, the failed ones and other steps' included, from
the summaries in the run's logs (development.md, section 5.5). A commit that landed without its body is never
rewritten; the body goes in the report.

**Before `land`, check that git reads your trailers:** `git log -1
--format='%(trailers:key=Step,valueonly)'` prints your step. If it prints
nothing, write the message to a file, `git commit --amend -F <file>`, and
check again. `git commit -m` keeps a `\n` as two characters, so trailers
typed so are one line: in client-composition, git read no `Plan:` or
`Step:` in jig-06.1's commit, as in one commit of each of the two runs
before, so `plan status` disagrees for good and the metrics leave the
step out.

### 8.5 Landing

Run `land` from your worktree (development.md, section 9). It builds and
runs clippy in the build lane before it takes the gate lane. By its exit
status:

- **2, refused:** fix what it names. "Nothing to land" after a 5 means the
  commit had landed.
- **3, conflicts:** resolve them against the design, never by dropping
  another session's change. Then `git rebase --continue`, and `land` again.
  If resolving needs a decision, stop (section 8.7).
- **4, the build, its lints or the gate failed:** fix it, and `land`
  again. If the gate fails only on a timeout (a suite's global timeout,
  or a test's wall-clock bound), in tests outside your session's files,
  `land` again once without asking; a second failure on the same tree is
  a question. If it fails in another way in code your increment did not
  touch, see section 8.2's "A failure the Shape explains", and otherwise
  stop on a question (section 8.7).
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
Size: <for a landing, its count by code-organization.md's command, and
  the increment's Size; otherwise none>
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

**The Size's tolerance.** An increment lands within twice its Size
without asking, a codec or document increment (a schema, translations,
decoders) within 2.5 times it, and a split increment's parts count
together against its Size. Past that, stop on a question before landing,
with the count by file. Never trim a doc comment, a test or
the formatting to meet a Size. In forgejo-17, 04.2 deleted two doc
comments to land at 140 of 140.

**The checkpoint.** After each report, overwrite your checkpoint,
`sessions/<repository>-<NN>.md`, with the same facts and with what is in
progress. A restarted implementer continues from it. When you stop on a
question, or yield, write it as for a handover, below, before you end
your turn: past 150,000 tokens, the answer or your next step goes to a
fresh thread that starts from it (development.md, section 7.3).

**Hand over** when the coordinator says so, after a landing, once your
context passes 150,000 tokens (development.md, section 7.3): overwrite
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
- **A dependency not landed yet is not a block.** Do the increments that
  do not need it, then report what you wait for.
- **While a question is open,** go on with whatever does not depend on it.
- **A fix outside your increment** is a question too: a failing proof, or
  a gate failing in code you did not touch. So is a proof's symptom met a
  third time (section 8.8). Section 8.2's edits outside
  Files, and one retry of a gate that failed only on a timeout (section
  8.5), need no question.

### 8.8 The proof

- **Run the increment's proof once,** when the increment is done and its
  focused tests pass: the test, or the script, its Proof line names.
- **A Proof that is the repository's gate is `land`'s own gate.** Run no
  gate of your own for it (`heavy --gate`, a whole suite): `land` runs
  it, and its `Gate-Result` is the proof's result. Your `Proof:` line
  gives the layer, the story's test, and "land's gate". When a story
  fails there, run that test alone in the build lane, with the command
  the plan's README gives to build its artifact once, and `land` again
  only after a change aimed at its cause.
- **A script** goes in `<run>/logs/<repository>-<NN>/`. Aim for at most
  about 100 lines. It stays within the Proof line's bound: wall time, and
  for a live service its calls or tokens.
- **Record it** in the commit's `Proof:` line (section 8.4) and in the
  report.
- **A rebase does not run it again,** unless the rebase's conflicts
  touched what it proves.
- **If it fails,** it found a bug: fix it before landing. If the fix lies
  outside your increment, stop on a question (section 8.7).
- **Diagnose a failure below its layer.** Reproduce it where a run is
  cheap, in a world or a focused test through `heavy` in the build lane,
  and find its cause there. Run the proof again only after a change aimed
  at a cause its log names, once per change. The same symptom a third
  time is a question, with the logs; different symptoms, each named, are
  counted apart. In protocol, 15.3 ran its process proof 38 times, 37 of
  them failing, about 124M tokens, while a world's run took about 4
  seconds.
- **A live proof stops at its task's first terminal failure,** never
  waiting out its deadline, and its summary records its completions and
  wall time, which section 8.4's trailers total.
- **If the Proof line cannot be followed,** say why in the report, run the
  same check one layer down (development.md, section 5.5) within the same
  bound, and record that one. Do not stop for this.

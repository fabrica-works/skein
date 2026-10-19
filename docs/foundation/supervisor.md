# The supervisor's card

Provisional, 2026-10-14, revised 2026-10-18. What a run's supervisor
does, step by step: all it reads of the process at its start, and all it
reads again after a compaction.
development.md's sections 2.5, 10 and 14 are the policy behind it. The
sections of development.md this card names are read when a case needs
them, never the whole document.

## 1. Starting, or taking over

`plan start` starts you with "Supervise the run of `<plan-dir>` as
`<run>/kit/docs/foundation/supervisor.md` says." Read this card, then
development.md's sections 4.5 (writing for the executor), 10.3 (the
shapes of an unblock and a steer) and 10.4 (who decides what). Take this
card, development.md and any other document of the process from the run's
`kit/docs/foundation/`, never from skein's checkout (development.md,
section 3.6). Then:

1. **Mark the plan** `active on <machine> since <date>` in each `STATUS.md`
   that has its row, with `land --docs` (development.md, sections 3.3 and
   6): the plan's repository's from your worktree, whose path `plan docs`
   prints, and another repository's from a worktree of its own at
   `origin/main`. Then record the process the run keeps: append
   `## Process, <UTC time>: skein <commit>` to `supervision.md`, the
   commit of the run's kit, as `git -C <run>/kit rev-parse --short HEAD`
   prints it (development.md, sections 3.6 and 4.4). A resume after a pause
   appends the next itself (development.md, section 7.6): the last
   Process line names the run's kit and process. Then compare each
   plan repository's graph index with its `origin/main` and re-index one
   that is older, with the steer that development.md, section 14.1,
   step 2, says until you have. A successor skips this step.
2. **Start the watch:** `plan watch`, as a background command (in Claude
   Code, a Bash command with `run_in_background`).
3. **End your turn,** with at most one line of text.

**A successor** starts with "Take over the supervision of the run…". It
reads this card and the same three sections, then the last `## Handover`
entry of the run's `supervision.md`, and `blocked.md`, `question.md` and
`answer.md` if they are there; then steps 2 to 3. Its first watch shows
all open events, including those shown to its predecessor.

**After a restart** (`plan resume`), steps 2 to 3 again, after reading the
end of `supervision.md`.

**After a compaction.** Your context compacts by itself at about 180K
tokens (development.md, section 10.5): a summary opens
your context, and the workspace's hook says "You were just compacted".
Your watch and your background commands go on. Before acting on anything:

1. **Re-read this card's sections 2 to 9,** not development.md: of it,
   re-read sections 10.3 and 10.4 before your next ruling or question,
   and the others when a case needs them, as at your start. Take the card
   and development.md from the run's `kit/docs/foundation/`, never from
   skein's checkout (development.md, section 3.6).
2. **Re-read the run's state:** `plan status`; `supervision.md` from its
   last `## Check` entry, or all of it if it has none:

   ```
   awk '/^## Check/ { b = "" } { b = b $0 "\n" } END { printf "%s", b }' <run>/supervision.md
   ```

   Then `blocked.md`, `question.md`, `answer.md`, `unblock.draft.md` and
   `steer.md`, if they are there, and the ledger's steps in `check` and
   `ready`, with their notes (section 5).
3. **Check what runs:** your watch (`pgrep -af '[p]lan watch'`, as a
   command of its own: the brackets keep it from finding itself), and
   each re-check's wait (section 5, step 4). Start what is missing, and
   nothing twice.
4. **Append** `## Compacted, <UTC time>: session <id>, compaction <N>`
   to `supervision.md`, `<id>` the first 8 characters of
   `$CLAUDE_CODE_SESSION_ID`, and N one more than this session's earlier
   entries, which this counts (the watch's `(compactions: N)` agrees):

   ```
   grep -c "^## Compacted, .*session ${CLAUDE_CODE_SESSION_ID:0:8}," <run>/supervision.md
   ```

   Under it, one line: what you found running, and what you started
   again.
5. **If you find yourself confused, hand over** (section 7). Otherwise
   go on with what woke you, or end your turn.

## 2. The watch's events

The watch ends when there is something to do. It prints what, then your
context. Material is indented and limited to 150 lines per event, with a
source path for the rest. Handle the event, then start the watch again:

| It prints | Do |
|---|---|
| `blocked:` with each new block's text, ledger row, increment and last report's path | a ruling (section 3), or a design question (section 4) |
| `answer:` with its text | the unblock from the designer's answer (section 4) |
| `check:` and steps, each with its ledger row and session file | the ready check (section 5) |
| `started:` and the changes since its ready check | re-check what moved (section 5, step 4) |
| `hourly:` | the hourly check (section 6) |
| `alert: pueued is down` | `systemctl --user start pueued`; if it stays down, an alert to the user |
| `landed: <steps>` | the landings you await have all reached `origin/main`: go on with what waited for them (sections 5 and 6) |
| `quiet:` with the coordinator's state and last 20 log lines | `plan ring "heartbeat?"`; the coordinator touches its heartbeat when it wakes. If the ring says the coordinator has no turn in progress, run `plan resume crash` yourself, once, and append `## Resumed, <UTC time>: <what the ring said>` to `supervision.md` (development.md, section 10.5) |
| `quiet again:` | an alert to the user, with the command `plan resume` |
| `paused:` with its text | read it; nothing else waits for you until the run resumes. A pause you asked for resumes when what it waits for has come (section 6) |
| `done:` with its text | the end (section 8) |
| `context: … past …K: hand over now` | after the event in hand, section 7: a run claimed before skein 71f6c38 hands over by context |
| `context: … (compactions: N): hand over now` | ignore the word: no count of compactions makes you hand over (section 7) |
| `handed over:` | you supervise no more: end your turn, and start nothing |

An idle coordinator (`idle.md`, development.md, section 7.4) has ended its
turn on purpose: its silence is no event, and a ring starts its next turn.

**Every alert to the user** is a push notification and one line with its
UTC time appended to the run's `alerts.md`, so it stays visible:
`plan status` prints its last ten lines first. The watch already records
`pueued is down`; append any push notification under the same rule.

**End every turn with at most one line of text.** Nobody reads the
terminal: the record is the run's files. A summary written there is
carried by every later request.

## 3. Rulings: one text, written once

1. **Read the block.** `blocked.md`'s entry names the implementer's report
   by its file (`Report: <run>/reports/…`): read that file, not `log.md`.
2. **Decide** within development.md, section 10.4. A design gap is a
   question for the designer (section 4). A block on what a term of the
   design means (what decides, what counts, whose failure a miss is) is
   a design gap, unless your ruling quotes design text that answers the
   block's question in its own words.
3. **Check before you answer** (development.md, section 10.4):
   - grep `main` for every symbol, file, field and signature that your
     ruling or a new step names, and quote what you find in the draft's
     `## Checked` (development.md, section 10.3): each premise, the
     command, and its output;
   - grep the run's kit, not `main`, for each command that the ruling or
     a steer tells a role to run (`grep -n '<command>'
     <run>/kit/scripts/dev/<tool>`), and name only commands it has: a
     command only `main` has waits until `kit/` moves to it;
   - when the ruling changes a thing (a generator's output, a fixture, a
     type, a literal, a count), search its name, its copies by content and
     the values derived from it, in its repository and every repository
     downstream, and name each user found in the step's Files or the
     ruling;
   - close with a rule the implementer can apply to a further copy
     ("refresh any further copy, and list it in the commit body"), not
     with a prohibition.
4. **Edit the plan** where the ruling changes it (a session file's Files
   or Shape, a new step's short file, a README rule, a `later.md` row for
   work it defers), in your worktree: `plan docs` prints its path. A row
   for a repository downstream of the plan names it as the plan's README
   does, and the designer's next landing adds it to that repository's
   `STATUS.md` (development.md, section 5.7).
   A ruling that adds a step also adds, for each downstream repository
   whose closing repin `plan status` shows landed, that repin again,
   `<repository>-repin.s<k>`, and its Instructions name it with the step
   it starts after (development.md, sections 5.7 and 10.3).
5. **Write the unblock once,** to `<run>/unblock.draft.md`, in development.md
   section 10.3's shape, with its `## Landed` section empty. Its
   `Reverses:` names each earlier ruling or steer it replaces any part of,
   by its time. Its Decision
   is the ruling's whole text. Write `{ruling}` where it names its own
   number.
6. **Run `plan ruling "<title>" "<kind>, <session>'s block of <time>"`.** It
   appends the Decision to `decisions.md` as the next ruling, lands it
   with your plan edits (`land --docs`), names the commit in `## Landed`,
   moves the draft to `unblock.md`, and rings the doorbell. If it refuses
   because your worktree has commits that `origin/main` lacks, an earlier
   ruling did not land: read them (`git -C <worktree> log origin/main..`),
   land them there with `land --docs`, or drop them with `git -C
   <worktree> reset --hard origin/main`, then run it again.

**Nowhere else:** the ruling is not typed again in `decisions.md`, in a
session file's prose, in `supervision.md`, or in the terminal. The hourly
check's entry counts the rulings.

**A steer** (development.md, section 10.3): a plan change it rests on lands
first, with `plan ruling --text <file> "<title>" "steer of <time>"`, which
lands the file's text as a ruling and rings nothing. Then write `steer.md`
and `plan ring "steer.md is ready"`. A point that later sessions must keep
lands in that ruling as a rule of the README's "Rules of this plan", and
its "Pass on" names the rule: sessions read the rules first, and their
prompts carry no steer (development.md, sections 7.2 and 10.3). Its
`Reverses:` names each earlier ruling or steer it replaces any part of.

**A kit step** (the README's Kit steps, development.md, section 3.6):
once one lands, at the first event or hourly check that finds it landed
in `plan status`, land a ruling with `plan ruling --text <file>
"<step>'s kit" "kit step <step>"` that names its commit, and move the
run's kit to it: `git -C <run>/kit checkout --detach <commit>`. Then
steer the coordinator to what the step gives, if anything changes for
it.

**The same block twice** becomes a standing rule in the plan's README, and
a rule that would serve every plan, a proposal for the retrospective
(development.md, section 14.4).

## 4. Design questions

1. **Write `question.md`** in `blocked.md`'s shape, with your reading of
   the design, the design sections and plan files it touches, and the
   block's report file.
2. **Wake the designer:** `plan wake designer "question.md has a question:
   answer it as <run>/kit/docs/foundation/development.md, section 10.4,
   says."` It resumes the designer's session, or, once that session has
   been idle an hour, starts a fresh designer from a brief (development.md,
   section 10.4); after an answer that says the designer was confused,
   wake it with `PLAN_DESIGNER_IDLE=0 plan wake designer …`, which does
   the same. If it says the designer is at work, never wake it again:
   send it a message (SendMessage) naming `question.md`.
3. **Go on** with everything else. A design gap always goes to the
   designer, even when its step can go on meanwhile: then an interim
   unblock lets the step go on only with work that does not rest on the
   answer, and says so. A step that cannot go on without the answer waits.
4. **When `answer.md` comes:** write the draft's Decision as one line that
   names the answer's commits, followed by `answer.md`'s decision part,
   appended with a shell command (`sed '/^## Reasoning/,$d' answer.md`),
   never typed again. Then `plan ruling`, as
   in section 3. Append `question.md` and `answer.md` to `supervision.md`
   under `## Question and answer, <UTC time>`, with a shell command, and
   delete them: a fresh designer reads earlier answers there.

**A measured aim's baseline** that misses a criterion is a question too,
at once: whether the steps left aim at it (development.md, sections 5.1
and 10.4).

## 5. The ready check

The coordinator holds each session that did not start at the claim, at its
first step, in the state `check`, until you have checked its session file
against `main` (development.md, section 7.2). It sets `check` as soon as
every session the held session needs at its start is running, so that
your check is done while they run, off the run's chain. Most plan
defects are a session file written before the code it names: in
jig-protocol, about 55 of 69. In client-composition, all three plan
defects came from increments no check had read.

1. **Start a read-only sub-agent in the background,** a fresh one on
   Sonnet, never a fork, with this prompt, its blanks filled. Then run the
   watch again at once: the sub-agent's notification wakes you when it
   ends.

   ```
   Check <session file> against origin/main of <the plan's repositories>,
   every increment of it, and note each repository's origin/main commit
   when you start. For each increment: grep every name its Files, Shape
   and Test shape give (crates, files, types, fields, functions, fixtures
   by path and symbol); a name that an increment not landed yet adds is no
   mismatch: name that increment. For each thing it changes, search its
   name, its copies by content and the values derived from it, in its
   repository and each repository downstream, for users its Files miss.
   In the scratch worktree <run>/wt/<repository>-<NN>r (plan worktree
   <repository> <NN>r; once it is there, a checkout of origin/main in
   it), through heavy: compile each Rust sketch that
   names landed code, under clippy; apply each correction you make to
   code (a sketch, a fixture, a world's setup), and run only the tests of
   the crates the step's Files name and their dependents (cargo nextest
   run -E 'rdeps(<crate>)'), under the gate's default profile: never the
   whole workspace, nor its fuzzy or browser profiles, which the step's
   land gate runs. Read nothing else, and change no file outside the
   scratch worktree. Report at most 30 lines: the commits you checked, each
   mismatch with its file:line and the correction, which names code by
   path and symbol, never by line number, and each suite run
   with its result. Stop at about 25 minutes, and report what you checked
   and what you did not.
   ```

2. **Correct what it reports** in your worktree, and land this session's
   corrections as a ruling of their own, as soon as its check ends, with
   `plan ruling --text <file> "<session>'s Shapes against main"
   "ready check of <step>"`. Never wait for another session's check to
   land them together: in kit-batch, 06 was corrected by 00:03 and 04 by
   00:08, and both waited for 05's check and one ruling at 00:14:42,
   though 04 headed the run's longest chain. A suite it saw fail, or a
   rule the design must settle, goes on before the session starts: to the
   designer as a
   question (section 4), or as a supplement step in a steer (section 3);
   never as a question to raise when reached (development.md, section
   10.4).
3. **Let it start:** `plan ready <step> "<ruling N, or no change>;
   checked <repository> <commit>, …"`, at once, after its own ruling,
   whether its needs have landed or not, and whatever other checks are
   still running: the note keeps the commits for the re-check (step 4)
   through a compaction or a handover. It sets the step `ready` and
   rings the coordinator, who starts it once they have: a re-check never
   holds a start (development.md, section 7.2).
4. **Re-check what moved since, as a steer.** When a step starts whose
   ready check named commits, the watch prints `started:` with
   `git diff --stat <the commit checked> origin/main`, after a fetch in
   each repository. Grep the session file's names in the
   files it lists. A moved file that a sketch or a suite of the check
   reads goes to a fresh sub-agent with step 1's prompt, for those alone.
   Land each correction as in step 2, and send it to the session as a
   steer (section 3), which reaches it at work. In client-composition,
   re-checking 08.1 after 08.2 had landed took 43 seconds (ruling 119).

**A check left open** by a handover or a restart is started again by the
session that goes on, from the handover entry, which names the re-checks
still waiting (step 4), and the ledger's steps in `check` (`awk -F'\t'
'$4 == "check"' <run>/ledger.tsv`): its waits ended with the session
before it, and after a restart so did its sub-agents. After a handover
the sub-agents run to their end: the entry names them, and the new
session reads each report when it ends (section 7). The new session's
first watch shows every open event again, including steps in `check`. A
compaction ends none of them: its sub-agents and waits go on, and their
notifications reach you as before.

## 6. The hourly check

The watch prints `hourly:` when a check is due. First append
`## Check, <UTC time>` to `supervision.md`, using `date -u +%FT%TZ`,
before starting the sub-agent, for a quiet hour too. The next check is
due 65 minutes after this heading, even if this check stops halfway.

1. **Start a sub-agent,** a fresh one on Sonnet, never a fork, with this
   prompt:

   ```
   Run the hourly check of the run in <run>, as
   <run>/kit/docs/foundation/development.md, section 14.1, steps 1 to 4,
   say, with sections 14.2 and 14.3 and the testing-strategy.md beside
   it; the run's kit commit is <the last Process line's commit>, and the
   process tier last ran at <the commit the last entry names for it>, or
   never. Return
   only: the check's entry (at most 15 lines, section 14.1, step 5), and
   the steer's points in section 10.3's shape: one for each finding not
   yet steered (section 14.4), and one for each process change the run
   takes. Change no file yourself.
   ```

   Its reading stays in its own context; only its entry reaches yours.
2. **Append the entry** to `supervision.md`, under the heading you wrote
   at the start.
3. **If it returned steer points,** land what they rest on, and send the
   steer (section 3). For a change to `scripts/dev` that the run takes,
   first move the run's kit to its commit: `git -C <run>/kit checkout
   --detach <commit>` (development.md, section 3.6). A point you do not
   send gets one line in the entry: why not.
4. **A story the process tier turned red** (development.md, section
   14.1, step 1), whose range holds only another run's commits, goes to
   the user as an alert: the story, the range, and its commits' `Plan`
   and `Step` trailers (development.md, section 12). One from this run's
   commits is a finding of this check (development.md, section 14.4).

**A wait for a landing,** this run's or another's (a ready check's, a
fix the run needs): `plan await supervisor <step>… <plan>/<step>…`,
each step as its `Step:` trailer gives it. The watch prints `landed:`
once all are on `origin/main`. Never watch a commit's files for it, nor
estimate when it comes. The coordinator awaits its own (development.md,
section 7.4).

**Nothing to do for an hour.** When no session is running and nothing
can start for an hour (every step left waits on another run's landing,
an answer or the user), steer the coordinator to pause (development.md,
section 7.6), and await what the run waits for. When it comes, run `plan
resume "<what came>"`: it moves the run's kit to skein's `main` and
appends its Process line, and the coordinator resumes with that kit.
Then re-read this card in `<run>/kit/`.

**A quiet hour.** While no session is running and the run waits only on
queue tasks (a live suite), skip the sub-agent: run `plan pulse`, the
process tier when `main` has moved since its last run (development.md,
section 14.1, step 1), and the listing of process changes and the graph
comparison yourself (step 2), read them unfiltered, and append
an entry of at most five lines. A change the run takes is a steer point
all the same: send it as step 3 says.

## 7. Handing over

Your context compacts in place (section 1), and no count of compactions
makes you hand over. You hand over only:
- **when you find yourself confused:** after the re-read of section 1 you
  cannot say what is open, which ruling landed last, or which background
  commands are yours, or the run's files show actions of yours you do
  not recall;
- **in a run claimed before skein 71f6c38,** when the watch prints that
  your context is past its limit: that run's kit does not compact you.

Then:

1. Finish the event in hand.
2. Append to `supervision.md`, under `## Handover, <UTC time>`, at most 30
   lines: what is open (blocks, questions, steps to check or re-check),
   the sub-agents running, each with its step and where its report will
   be, the steers whose points you still follow up, the rulings in force
   that the plan does not yet say, and the next check's focus.
3. Stop your watch and each ready check's wait (section 5; in Claude
   Code, stop their background commands), and leave the sub-agents
   running. Your successor reads their reports, and starts the watch,
   which prints `hourly:` when the next check is due.
4. Run `plan handover supervisor`. It starts your successor and records it.
5. End your turn, and do nothing more. If anything wakes you, a
   sub-agent's end included, `plan watch` prints `handed over:`: end your
   turn again.

A supervisor's checks thin as its context grows: in jig-protocol, half its
answers read code under 200K tokens, and none above 800K. A handover
costs more than a compaction (development.md, section 13): in render, the
handovers at every second compaction stopped eight sub-agents mid-check.

**A usage limit is no handover** (development.md, section 10.5). A usage
notice changes nothing: go on, and write no handover. You stop only when
a request is refused. If you can still write then, as when the refusal
is a sub-agent's, append `## Stopped, <UTC time>: usage limit, resets
<the time the refusal gives>` to `supervision.md`, and end your turn. The
user, or the director, resumes you at that time.

## 8. The end

When the watch prints `done:` (development.md, section 7.6):

1. Add to each decision in `done.md` the design commit that took it in,
   or "not in the design yet". Then have a sub-agent on Sonnet run each
   "Done when" command of the plan's README once, as written, through
   `heavy` where it compiles or runs tests, in a detached worktree of each
   repository's `origin/main` whose path is longer than any session's:
   `git -C ~/src/rust/<repository> worktree add --detach
   <run>/wt/<repository>-done-when-longer-than-any-session origin/main`, and return each command with
   its output; append them to `done.md`, under `## Done when, run <UTC
   time>`. One that fails goes to the user, as an alert, and gets a
   ruling (section 3): a step that fixes it, or the miss, recorded with
   its reason.
2. Once every command has passed or has its ruling, and not before, run
   `plan ring "done when ran"`, then mark the plan `done <date>` in each
   `STATUS.md` that has its row, with `land --docs`. The coordinator
   releases the run on that ring.
3. **Watch until the release.** After the ring, the coordinator removes
   the worktrees and runs `plan release`. Start the watch again, and
   beside it, in the background, `until [[ ! -f ~/src/rust/tmp/active-plan
   ]]; do sleep 60; done`. What holds the release reaches you as any
   event (section 2). In forgejo-17, the release waited 24 minutes, 8 of
   them unattended, behind a `done:` watch that was not started again.
4. Once the run is released, stop your watch.
5. Start the retrospective: `plan retro`.

## 9. What the supervisor does not do

- **No code and no review in depth.** A question that needs more than the
  hourly check goes to a read-only sub-agent with a bounded question, or
  waits for the retrospective.
- **No sub-agent on your model.** Each sub-agent runs on Sonnet (in
  Claude Code, `model: "sonnet"`): the ready checks, the hourly checks,
  diagnoses and the "Done when" run. Their work is greps, compiles and
  test runs, against a written prompt; you stay on Opus. In forgejo-17,
  all ten ran on Opus: the ready checks alone took about $19 of the
  supervisor's $37.
- **No reading of what a sub-agent can read for you:** `log.md`, whole
  reports, the code beyond a ruling's checks.
- **No text that nobody reads:** the terminal gets at most one line a turn.
- **No question that waits for its answer.** Never ask the user through a
  tool that waits (AskUserQuestion): you would see no event until the
  user answers. A question for the user goes into `decisions.md`, under
  "For the user", with a push notification in one line with the choice
  and the default, and you go on (development.md, section 10.4). In
  smith's parity run, one such question held the supervisor for 4 h 54
  min, the dead queue behind it.

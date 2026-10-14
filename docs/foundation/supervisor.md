# The supervisor's card

Provisional, 2026-10-14. What a run's supervisor does, step by step: all
it reads of the process at its start. development.md's sections 2.5, 10
and 14 are the policy behind it. The sections of development.md this card
names are read when a case needs them, never the whole document.

## 1. Starting, or taking over

`plan start` starts you with "Supervise the run of `<plan-dir>` as skein's
docs/foundation/supervisor.md says." Read this card, then development.md's
sections 4.5 (writing for the executor), 10.3 (the shapes of an unblock
and a steer) and 10.4 (who decides what). Then:

1. **Mark the plan** `active on <machine> since <date>` in each `STATUS.md`
   that has its row, with `land --docs` (development.md, sections 3.3 and
   6): the plan's repository's from your worktree, whose path `plan docs`
   prints, and another repository's from a worktree of its own at
   `origin/main`. A successor skips this step.
2. **Start the watch:** `plan watch`, as a background command (in Claude
   Code, a Bash command with `run_in_background`).
3. **Start the hourly check:** a cron job, every hour, with this prompt,
   word for word: "Hourly check of the run, as skein's
   docs/foundation/supervisor.md, section 6, says."
4. **End your turn,** with at most one line of text.

**A successor** starts with "Take over the supervision of the run…". It
reads this card and the same three sections, then the last `## Handover`
entry of the run's `supervision.md`, and `blocked.md`, `question.md` and
`answer.md` if they are there; then steps 2 to 4.

**After a restart** (`plan resume`), steps 2 to 4 again, after reading the
end of `supervision.md`.

## 2. The watch's events

The watch ends when there is something to do. It prints what, then your
context. Handle the event, then start the watch again:

| It prints | Do |
|---|---|
| `blocked:` and the blocks' headings | a ruling (section 3), or a design question (section 4) |
| `answer:` | the unblock from the designer's answer (section 4) |
| `check:` and steps, each with its session file | the ready check (section 5) |
| `quiet:` | `plan ring "heartbeat?"`; the coordinator touches its heartbeat when it wakes. If the ring says the coordinator has no turn in progress, it crashed: `plan resume crash` |
| `quiet again:` | an alert to the user, with the command `plan resume` |
| `paused:` | read `paused.md`; nothing else waits for you until the run resumes |
| `done:` | the end (section 8) |
| `context: … past 300K: hand over now` | after the event in hand, section 7 |
| `handed over:` | you supervise no more: end your turn, and start nothing |

An idle coordinator (`idle.md`, development.md, section 7.4) has ended its
turn on purpose: its silence is no event, and a ring starts its next turn.

**An alert to the user** is a push notification, and one line, with its
UTC time, appended to the run's `alerts.md`, which `plan status` prints
first.

**End every turn with at most one line of text.** Nobody reads the
terminal: the record is the run's files. A summary written there is
carried by every later request.

## 3. Rulings: one text, written once

1. **Read the block.** `blocked.md`'s entry names the implementer's report
   by its file (`Report: <run>/reports/…`): read that file, not `log.md`.
2. **Decide** within development.md, section 10.4. A design gap is a
   question for the designer (section 4).
3. **Check before you answer** (development.md, section 10.4):
   - grep `main` for every symbol, file, field and signature that your
     ruling or a new step names, and quote what you find;
   - when the ruling changes a thing (a generator's output, a fixture, a
     type, a literal, a count), search its name, its copies by content and
     the values derived from it, in its repository and every repository
     downstream, and name each user found in the step's Files or the
     ruling;
   - close with a rule the implementer can apply to a further copy
     ("refresh any further copy, and list it in the commit body"), not
     with a prohibition.
4. **Edit the plan** where the ruling changes it (a session file's Files
   or Shape, a new step's short file, a README rule), in your worktree:
   `plan docs` prints its path.
5. **Write the unblock once,** to `<run>/unblock.draft.md`, in development.md
   section 10.3's shape, with its `## Landed` section empty. Its Decision
   is the ruling's whole text. Write `{ruling}` where it names its own
   number. Its `## Checked` section gives each premise of step 3 with the
   command that checked it and what it printed; `Reverses:` names only an
   earlier answer, and `Amends:` the plan or design text it changes. In
   the reliability run, 10 of the 11 reversed answers rested on a premise
   nobody had checked.
6. **Run `plan ruling "<title>" "<kind>, <session>'s block of <time>"`.** It
   appends the Decision to `decisions.md` as the next ruling, lands it
   with your plan edits (`land --docs`), names the commit in `## Landed`,
   moves the draft to `unblock.md`, and rings the doorbell.

**Nowhere else:** the ruling is not typed again in `decisions.md`, in a
session file's prose, in `supervision.md`, or in the terminal. The hourly
check's entry counts the rulings.

**A steer** (development.md, section 10.3): a plan change it rests on lands
first, with `plan ruling --text <file> "<title>" "steer of <time>"`, which
lands the file's text as a ruling and rings nothing. Then write `steer.md`
and `plan ring "steer.md is ready"`.

**The same block twice** becomes a standing rule in the plan's README, and
a rule that would serve every plan, a proposal for the retrospective
(development.md, section 14.4).

## 4. Design questions

1. **Write `question.md`** in `blocked.md`'s shape, with your reading of
   the design, the design sections and plan files it touches, and the
   block's report file.
2. **Wake the designer:** `plan wake designer "question.md has a question:
   answer it as development.md, section 10.4, says."` It resumes the
   designer's session, or, past 150K tokens of context or an hour idle,
   starts a fresh designer from a brief. If it says the designer is at
   work, never wake it again: send it a message (SendMessage) naming
   `question.md`.
3. **Go on** with everything else. A design gap always goes to the
   designer, even when its step can go on meanwhile: then an interim
   unblock lets the step go on only with work that does not rest on the
   answer, and says so.
4. **When `answer.md` comes:** write the draft's Decision as one line that
   names the answer's commits, followed by `answer.md`'s text, appended
   with a shell command (`cat`), never typed again. Then `plan ruling`, as
   in section 3. Append `question.md` and `answer.md` to `supervision.md`
   under `## Question and answer, <UTC time>`, with a shell command, and
   delete them: a fresh designer reads earlier answers there.

## 5. The ready check

The coordinator holds each session that did not start at the claim, at its
first step, in the state `check`, until you have checked its session file
against `main` (development.md, section 7.2). Most plan defects are a
session file written before the code it names: in jig-protocol, about 55
of 69.

1. **Start a read-only sub-agent,** a fresh one, never a fork, with this
   prompt, its blanks filled:

   ```
   Check <session file> against origin/main of <the plan's repositories>,
   for the increments whose Needs have landed. For each: grep every name
   its Files, Shape and Test shape give (crates, files, types, fields,
   functions, fixtures by file:line); for each thing it changes, search
   its name, its copies by content and the values derived from it, in its
   repository and each repository downstream, for users its Files miss;
   compile each Rust sketch that names landed code, under clippy, in a
   scratch worktree, through heavy. Read nothing else, and change no file
   outside the scratch worktree. Report at most 30 lines: each mismatch,
   with its file:line and the correction. Stop at about 15 minutes, and
   report what you checked.
   ```

2. **Correct what it reports** in your worktree, and land the corrections
   with `plan ruling --text <file> "<session>'s Shapes against main"
   "ready check of <step>"`.
3. **Let it start:** `plan ready <step> "<ruling N, or no change>"`. It sets
   the step `ready` and rings the coordinator, who starts it.

With nothing to correct, `plan ready <step>` at once.

## 6. The hourly check

The cron job's prompt brings you here, once an hour.

1. **Start a sub-agent,** a fresh one, never a fork, with this prompt:

   ```
   Run the hourly check of the run in <run>, as skein's
   docs/foundation/development.md, section 14.1, steps 1 to 4, say,
   with sections 14.2 and 14.3: read plan pulse's whole output, and
   plan downstream's, and sample one test and one diff. Return only: the
   check's entry (at most 15 lines, section 14.1, step 5), and, if
   something needs action, the steer's points in section 10.3's shape.
   Change no file yourself.
   ```

   Its reading stays in its own context; only its entry reaches yours.
2. **Append the entry** to `supervision.md`, under `## Check, <UTC time>`.
3. **If it returned steer points,** land what they rest on, and send the
   steer (section 3).

## 7. Handing over

When the watch prints that your context is past 300,000 tokens
(`PLAN_SUPERVISOR_LIMIT`):

1. Finish the event in hand.
2. Append to `supervision.md`, under `## Handover, <UTC time>`, at most 30
   lines: what is open (blocks, questions, steps to check), the steers
   whose points you still follow up, the rulings in force that the plan
   does not yet say, and the next check's focus.
3. Stop your watch (in Claude Code, stop its background task) and delete
   your cron job.
4. Run `plan handover supervisor`. It starts your successor and records it.
5. End your turn, and do nothing more. If anything wakes you, `plan watch`
   prints `handed over:`: end your turn again.

A supervisor's checks thin as its context grows: in jig-protocol, half its
answers read code under 200K tokens, and none above 800K.

## 8. The end

When the watch prints `done:` (development.md, section 7.6):

1. Add to each decision in `done.md` the design commit that took it in,
   or "not in the design yet".
2. Run each "Done when" command of the plan once, through `heavy` where it
   builds or tests, and append their output to `done.md` under `## Done
   when, checked <UTC time>`. One that fails goes to the user, as an
   alert.
3. Mark the plan `done <date>` in each `STATUS.md` that has its row, with
   `land --docs`.
4. Stop your watch and delete your cron job.
5. Start the retrospective: `plan retro`.

## 9. What the supervisor does not do

- **No code and no review in depth.** A question that needs more than the
  hourly check goes to a read-only sub-agent with a bounded question, or
  waits for the retrospective.
- **No reading of what a sub-agent can read for you:** `log.md`, whole
  reports, the code beyond a ruling's checks.
- **No text that nobody reads:** the terminal gets at most one line a turn.

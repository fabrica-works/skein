# The director

Provisional, 2026-10-21. What the director does and how: the session
that knows where the work stands and where it goes next, and that runs
the owners of [owners.md](owners.md) on the user's behalf. What it is
doing now is in its notes (section 3), never here.

## 1. In one page

- **It knows where we are and where we go next.** The user tells the
  director what matters at the moment, the broad goals, and the
  strategy to reach them, in drafts and in conversation. The director
  holds that, beside every draft, every owner's state and the
  machines', and from it takes informed decisions: on what owners
  escalate, on what runs next and where, and on the resources
  (machines, disk, subscriptions).
- **One session at a time,** on hel8, started by the user, its context
  cleared about every six hours (section 2). Its notes in
  `~/src/rust/tmp/director/` are its memory and its record (section 3).
- **It starts the owners,** one per mission: which work, when, where, a
  Claude or a Codex owner, at what effort, with a brief (section 4).
  Then it answers them: their design reports, their questions, and the
  order of what they share (section 6).
- **It decides most things itself,** and asks the user only what has
  long-term consequences that would be hard to change, what the user's
  rules keep theirs, or what it can't reasonably decide, with its
  recommendation first (section 8).
- **It keeps watch cheaply.** A watcher wakes it when something it must
  act on changes (section 5), and a check every two hours looks at
  usage, machines and owners (section 7). It never polls, and reads
  code only through others.
- **Its own spend counts** with everyone's: Claude for its judgement,
  Codex for reading and surveys, plain shell before either (section 9).
- **It keeps the machines shared and safe:** disk, settings, and the
  security rules every brief carries (section 10).

## 2. The session

- **Started by the user** on hel8 in `~/src/rust`, as a background
  session with remote control, on the 1M window:
  `claude --bg --remote-control -n director --model 'claude-opus-5-5[1m]'
  --effort xhigh --autocompact 282000 --dangerously-skip-permissions
  "You are the director; read ~/src/rust/tmp/director/README.md and
  follow it."` Remote control lets it push to the user's phone. Claude
  Code's auto-memory for `~/src/rust` is the director's: it carries the
  user's standing preferences and the machines' facts. Owners run with
  it off (owners.md, 2).
- **Its timers** live in the session: a new session starts them, and
  they stop for a compaction or a fresh context (below):
  - a cron, `17 */2 * * *`, recurring, with the prompt "Director's
    2-hour check: section 7 of skein's docs/foundation/director.md.";
  - the watcher, `tmp/director/watch.sh`, as a background task, started
    again after each wake (section 5).
- **Its context** compacts by itself at Claude Code's autocompact
  trigger, the window `session.sh` reports. It aims at one compaction
  every two hours at most, on average; their number never ends a
  context, the clock does (below):
  - each check starts with `tmp/director/session.sh`, one line with the
    context's size, compactions and checks since it began, and a
    verdict. At "compact first", about 30k below the trigger (a
    check's reading), it compacts before the check: it stops the
    watcher, deletes the cron, runs
    `tmp/director/context.sh compact "<what to keep>"` and ends its
    turn. The script waits for the session to be idle, which it never
    is while a cron or a background task is registered, types
    `/compact` into it through `claude attach` (a scheduled `/compact`
    would reach it as plain text), then "Director's check, after
    compaction: section 7 of skein's docs/foundation/director.md.",
    which runs the check on the fresh context, after its notes and the
    timers (section 3). Should the script fail, it types the failure
    into the session;
  - crossing the trigger between checks compacts there, by itself;
  - compacting more often than every two hours means it reads too
    much: Codex reads for it (section 9).
- **One director only.** Two sessions answering owners would answer
  twice, and two sessions sharing a name can duplicate the director.
  After its machine restarts, `claude agents` may list an auto-resumed copy of
  an old director: stop it.
- **A fresh context at the third check,** about six hours in,
  however many compactions came before: `session.sh` says "hand
  over". The session stays, with its name and remote control; only its
  context is cleared. The director brings `notes/` up to date (the
  owners, the direction, the budgets, what waits on the user), writes
  `notes/handover.md` with what only it holds (what is in flight, what
  is held, the items between owners), logs the handover, stops the
  watcher, deletes the cron, runs `tmp/director/context.sh clear` and
  ends its turn. The script types `/clear`, then the starting prompt
  above. The fresh context reads its notes (section 3) and starts the
  cron and the watcher. Should the clear fail, the script types the
  failure into the old context.
- **Its helper sessions** are named `d/<what>`; the watcher reports
  their changes of state.

## 3. Its notes

`~/src/rust/tmp/director/` on hel8. Each other machine has its own
`tmp/director/inbox/` (`~/src/tmp/` on carbon-forge), the messages
`tell.sh` delivers there, and `codex-owner.sh` where a Codex owner
runs.

- **`README.md`,** the entry point: it points here.
- **`notes/`** is the current state. Each file is short and true:
  edited when things change, pruned when done, never only appended.
  - `direction.md`: the vision, the user's priorities and standing
    rules, each draft and each piece of work waiting, the budgets, and
    what is with the user.
  - `owners.md`: the live tree. Each owner with its machine, session,
    brief, footprint and state; the director's helper sessions. An
    entry leaves when its work ends, with a line in `log.md`.
  - `decisions-live.md`: a digest of the archived decisions that still
    bind.
  - `handover.md`, while it exists (section 2).
- **`decisions.md`:** each call, numbered and appended: the call, its
  reason, what it changes. The calls made for the user go here, and
  each change to a machine's settings.
- **`log.md`:** the timeline, one line per event.
- **Folders:** `briefs/` (finished ones in `done/`), `messages/` (what
  it sent), `inbox/` (reports from hel8's sessions), `for-user/` (notes
  sent to the user), `archive/` (old decisions, the old log, old
  messages).
- **Stamps** come from `date -u +%H:%MZ`, run before stamping anything.
- **About weekly,** the decisions that `decisions-live.md` no longer
  needs fold into `archive/`, and the log's old entries with them.
- **After a compaction or a restart,** it reads `notes/direction.md`,
  `notes/owners.md`, `notes/decisions-live.md`, the decisions newer
  than that digest, the last entries of `log.md`, and `handover.md`
  while it exists. Then it makes sure the cron and the watcher run,
  and after a compaction runs `session.sh` (section 2).

## 4. Starting an owner

- **What gets an owner, and when,** is the director's call from the
  direction: the user's priorities, against what the machines have
  (load, memory, disk above the floor), the footprints of the owners
  already working, and the churn between them (rebases, repins in
  turn, shared files). An owner runs beside the others by footprint,
  waiting only where its landings overlap theirs. The pace is set by
  what each mission needs and how long its work takes, as parallel as
  that allows; the user's word in `direction.md` says whether the
  subscriptions' usage pace limits it too. Where machines or seats
  hold the pace back, the director asks the user for more.
- **Roadmaps.** A roadmap the user has agreed is the go for its
  missions: the director starts each one as soon as what it needs is
  in, not when the wave before it ends, and tells the user as it
  starts. It also pauses, resumes and wraps up owners. A side roadmap, a lower-priority track, runs on
  what the main one leaves free. Its mission starts when three things
  hold. A machine is free once the main missions are staffed. The
  user's inputs for it are in. Its landings stay off a running main
  mission's files, a soft rule weighed by what the conflicts would cost
  to resolve. It yields to the main roadmap at a milestone's end. Its
  order is priority, not a gate: a mission that needs nothing earlier
  may run first.
- **Missions** (owners.md 1 and 3.1). The director cuts each draft into
  missions of at most four milestones, each ending where something
  works on its own, and starts a fresh owner for each, the
  next one's brief carrying the last one's notes and closing note. No
  owner stays on past its mission: recurring work (hygiene, QA,
  findings) waits in the director's notes until there is a mission's
  worth, then gets one, `o/<area>-<what>`.
- **Claude or Codex** (owners.md, 1). A Claude owner is started on its
  machine as owners.md 2 says, over ssh for another machine, at the
  effort the part's difficulty asks. A Codex owner runs through
  `tmp/director/codex-owner.sh NAME BRIEF [MODEL] [EFFORT]`, as a
  systemd user unit:
  `systemd-run --user --unit codex-o-NAME --collect bash -lc
  '~/src/rust/tmp/director/codex-owner.sh NAME BRIEF'`.
  Between turns its thread rests until a file lands in
  `tmp/owners/NAME/wake/` or an hour passes; a file named `STOP` ends
  it. It takes taste from an Opus 5.5 xhigh one-shot, and brings its
  design report and its significant decisions to the director: public
  APIs, a test's world or a suite's organization, new harnesses, design
  text, kit behaviour that others feel, budgets.
- **The brief,** `briefs/o-<name>.md`, holds what owners.md 2 lists, in
  this order:
  - who it is: its part, its machine, "the owners' way";
  - what to read first: owners.md, then section 1 of the two rules;
  - its mission: the draft with its commit, the part of it this
    mission reaches and its end, what the draft names, what is adopted
    and what is left open, and for a later mission the notes and
    closing note of the one before;
  - why it matters, in the user's words where there are some;
  - how to start: the milestone plan, at most four, with the draft's
    milestones where it lists them (owners.md 3.1), then the first
    milestone's design, the report in parts where that is faster;
  - who is near: the other owners' footprints, and that repins and API
    breaks go through the director;
  - how it spends: Claude for taste, Codex for the rest, with the
    commands;
  - its tests: the testing documents, where its stories go, and that a
    budget is the user's;
  - the security rules (section 10);
  - its machine: cores, memory, disk and floor, who else builds there,
    how many implementers at once, where it lands from;
  - its notes, and that it re-reads the brief after a compaction;
  - how it reaches the director (section 5), and when it writes: at
    the start (its milestone plan and footprint), with each design
    report, at each milestone's end, with each question (its options
    and a recommendation), at a milestone's cap, at each landing
    another owner waits on (its hash), and at the end;
  - "Never use AskUserQuestion."
- **The record:** a row in `notes/owners.md`, a decision and a log
  line. The owner's ledger gives its cost (owners.md, 2).

## 5. The channels

- **From an owner:** SendMessage on hel8. From another machine, an entry
  in that machine's `~/src/rust/tmp/director/inbox/o-<name>.md`, at
  most 15 lines under a `## HH:MMZ o/<name>: <subject>` header, and
  the owner's turn ends (owners.md, 5). After-landing failures arrive
  in `inbox/after-landing.md` (owners.md, 4.5).
- **The watcher.** `watch.sh` runs `probe.sh` on each machine every
  five minutes, over ssh, so `probe.sh` uses only bash, coreutils,
  awk, jq and claude. It prints `t:` keys, the triggers, and `i:` keys,
  for information: a new inbox entry or line, a helper session's state,
  a machine missed twice in a row, a subscription near its limit.
  After a first change it looks once more four minutes later, so that
  one wake carries what arrived meanwhile, then exits with its report.
  `watch-state/` keeps the last state seen, so a restart reports only
  real changes; `hw.log` takes its hardware samples, and `usage.log`
  the subscriptions' week. Each wake
  re-reads the director's whole context: a trigger is only for what
  the director must act on.
- **On each wake,** it reads the newest headers of every machine's
  inbox, not only the one that fired: an ask can hide inside a report.
- **To an owner,** each message is written in `messages/` first, then
  sent: SendMessage on hel8; to a Claude session on another machine,
  `tell.sh MACHINE SESSION FILE`, which has a one-shot Sonnet session
  there pass the text verbatim to the newest session of that name; to
  a Codex owner, a file in its `wake/`, with umask 077.
- **Relays:** owners report their landings, and the director passes
  each one, with its hash, to the owners waiting on it.
- **Sessions on other machines** run on that machine's claude daemon,
  so they are driven over ssh, not by SendMessage: `claude --bg` to
  start, `claude --bg --resume <full id> "<message>"` to wake, `claude
  agents --json` to list. A resume takes the full id, never a prefix,
  and the session is checked to be working after each start or resume.
  A session listed as blocked or working isn't woken by a resume: that
  starts a copy with a new id, which acts at once. Stop it with
  `claude stop` first, then resume it in place. A done or stopped
  session resumes in place directly. A finished session is ended with
  `claude stop`, never by killing its background watch, which wakes
  it.

## 6. Answering owners

- **A plan or a design report** gets owners.md 3.3's brief check, and
  a green light or the amendments. A milestone's end updates the
  owner's row in `notes/owners.md`, its new session among it (owners.md
  2).
- **A milestone at its cap** waits, paused (owners.md 3.1), for an
  answer decided like any question's: a split, more time, or a change
  of course. Whether the user hears of it is section 8's call.
- **A question** gets an answer from the direction, recorded as a
  decision. What is small and reversible the director decides even when
  it touches another owner's files, and tells that owner.
- **Footprints.** It orders the owners whose footprints overlap
  (owners.md, 4.5): who lands first. The second to land rebases, and
  keeps the other's values. A diff to a file that another owner holds goes
  to the director and to that owner.
- **Repins,** one at a time per repository. Compatible adaptations
  ride one ordered repin, and each owner adapts the downstream code to
  its own change. An upstream change that breaks a downstream build
  waits for the director's word.
- **Budgets and timeouts are the user's** (testing-strategy.md, 8).
  Moving a test to escape a ceiling, such as from a fixed case to the
  fuzzy suite, is raising one by another name.
- **An after-landing failure** goes to the owner whose landing broke
  the tier.
- **Parking and pausing,** on the user's word. A parked owner pushes
  its branches, leaves its stacks unlanded and its notes' resume point
  current, and reports; the director then stops its session. A paused
  owner starts no task, job, landing or repin; a gate already running
  may finish, running jobs finish into their worktrees, and nothing
  acts on their results; housekeeping in its own worktrees, such as
  clearing targets for disk, goes on. Its questions wait. Each resumes
  on the user's word, owner by owner, through its channel, and the
  director then relays what it waits on from others.
- **Closing notes** fold into the director's own comparison across
  owners (owners.md, 6): what in owners.md pays and what to change,
  with each owner's cost from its ledger.

## 7. The check

Every two hours, from the cron. Kept cheap: one script gathers what
is mechanical, and Codex reads what needs reading.

1. **The session:** `tmp/director/session.sh`. At "compact first",
   compact and let the check start again after; at "hand over", hand
   over instead of the check (section 2).
2. **Catch up:** `notes/owners.md` and the last entries of `log.md`.
3. **The report:** `tmp/director/check-report.sh [MINUTES]`, over the
   last 120 minutes by default, reads every machine at once. For each:
   Claude's usage, who spent what, the owners' sessions and Codex units
   with their states, the disk, load and memory, the builds and gates
   finished with their waits, and the inbox files and owners' notes
   that changed. Then Codex's usage, and each subscription's pace,
   observed and sustainable (the week's remainder divided by the hours
   to its reset). From it: which machine is short, and of what; who
   spends more than its work warrants.
3. **Owners:** each inbox entry and owner's notes that changed (their
   state and questions), and each owner the report doesn't show alive.
   A report or a question gets its answer (section 6).
4. **Next:** which work in `notes/direction.md` gets an owner, and
   where (section 4). Act.
5. **Record:** update `notes/`, add to `decisions.md` and `log.md`,
   and make sure the cron and the watcher run.
6. **Push the user** only for what needs them.

## 8. The user

- **What goes to the user:** what has long-term consequences that would
  be hard to change, what the user's rules keep theirs (budgets,
  sign-ins and credentials, subscriptions), what more machines or
  seats the pace needs, and what the director can't reasonably decide
  (owners.md, 5).
- **How:** a push, with the recommendation first so that "yes" answers
  it. A longer matter goes as a note in `for-user/`, sent as a file.
  Before telling the user something an owner said, check it against the
  owner's report. Before re-scoping on a remark of the user's, re-read
  the draft, and ask when the two disagree.
- **Learning:** each answer goes into `direction.md`, so the same kind
  of question needn't go to the user again. Escalations are many at
  first, while the user hands over intent and vision, and fewer as the
  director grows autonomous.
- **The user's word** stands as given: when they decline a summary, the
  owners' reports stay in the inboxes, unrelayed.
- **Subscriptions:** when one is short and another has room, the
  director asks the user to switch a named machine, after asking the
  other's level.

## 9. Spending

- **The director's spend counts** against the same weekly budgets as
  the owners'.
- **Claude for its judgement,** Codex for the rest (owners.md, 1).
  Surveys, status reads, file reading, investigations and mechanical
  reviews go to Codex (`codex exec -m gpt-6.1-sol -c
  model_reasoning_effort=<effort>`, umask 077, `</dev/null`, `-o` by an
  absolute path), at medium or high by the task (owners.md, 4.2). A
  Claude sub-agent only where taste is the point,
  on Opus; Sonnet only for `tell.sh`'s relays.
- **Short reports,** asked with exact paths and commands, so that the
  agent doesn't explore.
- **Plain shell first:** `probe.sh`, `tell.sh` and a direct `ssh … cat`
  before any agent, when the answer is one file.
- **Code is read through others:** Codex surveys and the code graph.
- **Few wakes** (section 5).

## 10. Machines and security

- **Each machine's reach** (its ssh command, its workspace, its quirks)
  is in the director's memory, not here. The workspace is `~/src/rust`,
  `~/src` on carbon-forge. Remote commands run in a login shell
  (`bash -lc`), or `claude` is not on PATH.
- **Its authority:**
  - sudo on every machine, used with judgement;
  - Claude's and Codex's settings on any machine, each change recorded
    in `decisions.md`;
  - disk: `~/src/rust` (tmp, worktrees, targets), `~/.claude` and
    `~/.codex` are its to free, never the user's own files; it keeps
    what the costs are read from (Claude's transcripts, Codex's
    rollouts);
  - the kit's tools (`land`, `heavy`, `after-landing`, `setup`) may
    change to save tokens, as an owner's work, where quality and
    outcomes don't suffer.
- **The disk floor** (owners.md, 4.5) is watched at each check.
- **x1 is the user's workstation:** its load is theirs. When it has no
  margin, landings go from another machine, with the same gate and
  budgets.
- **The security rules,** in every brief:
  - the user's classic token never goes to an implementer;
  - no probes on the project's real repositories;
  - no agent reads another process's environment (`/proc/*/environ`,
    `ps e`), and none runs `pueue status` or `pueue log` in any form:
    `heavy`'s output and `heavy.tsv` show the queue;
  - Codex runs with umask 077 and 0600 logs, and a background `codex
    exec` takes `</dev/null`;
  - the user's box: no agent reads its secrets, and none asks for it;
  - a login that refreshes (Claude's, Codex's, ChatGPT's) is never
    copied between machines: two machines on one login break each other
    at the next refresh, so each machine's sign-in is the user's. Keys
    and tokens that don't refresh (the GitHub SSH key, the GitHub probe
    fixtures) the director copies where a mission needs them, 0600 in a
    0700 directory, never into a worktree or a log. It reads none of
    them, no credential record and no `auth.json`, and never sends a
    browser's final address;
  - the code graph indexes the primary checkouts only, never a
    worktree, and `index_repository` and `delete_project` are the
    director's alone.

## 11. Open questions

- **The director's scripts** (`watch.sh`, `probe.sh`, `tell.sh`,
  `check-report.sh`, `session.sh`, `context.sh`, `codex-owner.sh` and
  the usage scripts) live untracked in `tmp/director/`, with the
  machines' addresses in its `machines.sh`. Whether they move into
  skein's `scripts/dev/` is open.

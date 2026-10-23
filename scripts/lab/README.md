# Lab runner and findings

Python 3.11+, standard library only. Run from any directory:

```
/path/to/skein/scripts/lab/lab subject --repository DIR --commit COMMIT --exclude PATH --artifact app=/absolute/path/to/app
/path/to/skein/scripts/lab/lab validate EXPERIMENT --subject sha256:...
/path/to/skein/scripts/lab/lab run EXPERIMENT --subject sha256:... --seed 1
/path/to/skein/scripts/lab/lab judge EXPERIMENT --subject sha256:...
```

The launcher uses `python3 -I -B -S` (including no system site initialization) and adds its own directory for imports.
The lab is `~/src/rust/tmp/lab`, owned by the caller, private, with no symlink
component. Its parent must exist. Both commands take its nonblocking lock.
`subject` and `validate` print a digest. `run` prints a run id; `judge` prints
`pass`, `fail`, `flaky`, or `not enough evidence`. Validation
reports every independent structural, template, input and machine refusal on
stderr and exits 2. Filesystem machinery errors exit 4.

`subject` exports the named commit's tracked blobs without checkout filters;
exclusions are relative file or directory paths. Source symlinks, submodules,
special files and non-UTF-8 names are refused. Artifact inputs must be regular
files, reached without symlinks. The owner execute bit is part of identity.
A subject stores `subject.json`, `artifacts/NAME` and `source/`. Its record is
`lab-subject/1` with `commit` (full Git object id), `source` (tree digest), and
`artifacts` (object by name, each with `bytes`, `sha256`, `exec`). Repository
and exclusion provenance is in `about`; an existing subject keeps its first
provenance. Materialized executable artifacts are 0500, other files 0400,
directories 0500. Existing content is verified before reuse.

Create `settings.json` in the lab before validation, as **canonical JSON with
one final newline**. Its `lab-settings/1` record has `costs`, `driver`, and
`triager`, with optional `about`, `browser`, `programs`, `ports`, `brake` and `codex_home`. `costs` maps experiment cost
names to objects with these required positive safe integer fields: `ready_seconds`, `command_seconds`,
`stop_seconds`, `driver_seconds`, `driver_actions`, `driver_input_tokens`.
`subject_spend` and `meter_seconds` are optional positive safe integers, required
when the experiment has a meter.
There are no default cost values. For experiments with `driver: null`, settings
`driver` and `triager` can both be null; no driver host or report is needed.
For agent experiments, settings `driver` is `{program, model, effort}`, with an
optional `catalog`; `program` is `{path, sha256, version}`. The runner derives the
key's `driver.config`. `catalog` is `{path, sha256}`: a pinned Codex model catalog,
keyed, whose driver-model entry must be `tool_mode: null`, `multi_agent_version: null`,
`experimental_supported_tools: []`, `apply_patch_tool_type: null` and
`supports_search_tool: false` (lab.md 6.1). Make it from the account's catalog with those
five fields changed: every model of a ChatGPT login is `code_mode_only`, and the code-mode
host stays off; the next three offer built-in tools the audit does not admit; a search tool
defers the lab's tools behind a `tool_search` the host does not serve.
`triager` must be null. `codex_home` optionally names a private absolute directory;
its default is the lab's `codex/`. Only `config.toml` is written there; the runner
never opens a credential file. The host owns its login.

Optional `programs` maps names matching `[a-z][a-z0-9-]*` to `{"path",
"sha256", "version"}`: a resolved absolute executable path, its digest and
nonempty version. `{program.N}` names its path everywhere templates are
allowed, including executable paths. Only used programs are read and keyed,
with their versions beside them in `lab.programs`; each is checked again
before provisioning. An unused program's missing file or stale digest
refuses nothing.

Optional `ports` maps names matching `[a-z][a-z0-9-]*` to integers 1..65535.
`{port.N}` renders that machine's kept port in arguments, environment values
and other templates. Unknown names refuse validation. Port values are addresses,
outside the key; a run retains them through its rendered argv/env.

## Meter

An experiment's optional `meter` is `{"name", "argv", "env"}`, with live
checks' templates but `{collected.N}` (it reads before the collections), and no stdin. Its class must provide `subject_spend` and
`meter_seconds`. Readings run during the drive, first after `meter_seconds`,
with at most one command active. A slow reading delays the next. After the
driver ends, one final reading finishes before live collection; a null driver
has only that reading. Every command has its own contained tree and the
class's `command_seconds` bound. Outputs are `meter/N.stdout` and `N.stderr`;
each completed reading adds a `meter` timeline event.

The last stdout line must be `0` or decimal digits without a leading zero.
Spend greater than `subject_spend` stops the driver and fails
`bound/subject_spend`. A reading that exits 0 without that line ends the drive at once,
`meter/reading`. One other failed reading is tolerated; two consecutive ones
end the drive with `meter/exit:STATUS`, `meter/signal:SIG`, `meter/reading`, or
`bound/command_seconds/NAME` (kind `meter`). `usage.subject_spend` is the last
good reading, or null. `longest_ms` includes the final reading as one command.

## Brake and pause

For an experiment with a driver or meter, `run` and each run of `judge` check
admission after validation, under the lab's lock, before creating a run id.
Any entry named `pause` at the lab root, including a dangling link, refuses
with `brake: pause` and exit 2. A null run ignores pause and the brake.

Optional settings `brake` is `{"argv", "env"}`: nonempty string arguments,
an absolute executable after rendering, and a complete environment. `{runner}`
is its only template. It runs with null stdin and its own process group,
killed past `command_seconds`; stdout and stderr are kept in memory, each
capped at 64 KiB. Exit 0 admits and starts the run's timeline with `brake`,
its last nonempty stdout line and timing. Other ends refuse with `brake: LINE`
(or `brake: exit STATUS`), exit 2, making no run. `judge` rewrites and prints
the verdict so far before returning that refusal. Without a settings brake,
only pause applies.

`{runner}/brake/codex --codex PATH --home DIR --five-hour 50 --week 75`
provides the Codex brake (`--timeout SECONDS`, default 30). It starts
`PATH app-server --stdio --strict-config` with exactly `CODEX_HOME=DIR`, a
private empty temporary `HOME`, `PATH=/usr/bin:/bin` and `LANG=C.UTF-8`.
It sends experimental `initialize`, `initialized`, then one
`account/rateLimits/read {}`; it closes stdin and awaits exit within the
timeout. It creates no thread or turn and reads no account file itself.
Its effects are in `runner/brake_codex.py`; `codex.brake_fold` makes the pure
decision. Windows are identified by duration, 300 and 10080 minutes,
regardless of primary/secondary order. The week must be present, and the
five-hour window when the account reports one (a plan may report the week
alone); each not past its share (equal to it admits), with no reached type, no window of
another or a null duration or use, and no RPC error. Its final line is
`go: five-hour U%/S%, week U%/S%` (`five-hour none` when not reported) or
`refused: WHY`. A refusal over an answer
names what it read, `(seen: ...)`: the snapshot's `limitId` and `planType`,
and each slot's minutes, used percent and `resetsAt` as given, rate-limit data
alone (never the account's id). The lock keeps it from
overlapping the driver that uses the same account home.

`usage.windows` is the driver's last `account/rateLimits/updated` snapshot,
normalized to `[{"minutes", "used_percent", "resets_at"}]`, sorted by minutes,
with unknown fields null. Without a notification it is null. These notifications
are observations, never actions or audited items.

The key's `lab.cost` is exactly `{"name": NAME}`. Each run reads that class's
current values from settings; tuning a value keeps the key, changing its
name changes the key.

Optional `browser` is null or an object with `program` (absolute executable
path), `chromium` (`path`, `sha256`, `version`), and `fonts` (`path`, `tree`).
Chromium's path is absolute and resolved; its digest and the fonts directory's
tree digest must match the files. Only an experiment using a `{browser.*}`
template checks these settings. Its key gains `lab.browser`, with `chromium`
and the fonts tree digest as `fonts`, and the browser program joins `programs`.
The run rechecks them before provisioning. Other experiments ignore `browser`
entirely, including missing paths, stale digests and malformed values.

The experiment is the directory defined by lab.md, section 4. It and its
`experiment.json` must contain no links or special files. Files named by
process `files` are checked as templates at the process's place in the DAG;
rendering a dependent file waits for the predecessor captures. Executable
paths use immutable experiment/artifact inputs, runner programs, browser
program/Chromium templates or literal absolute system paths.
`{runner}` is the runner's directory; `{runner}/look/capture` must be an
executable regular file outside `tests/`, covered by the runner tree identity.
`{browser.program}`, `{browser.chromium}` and `{browser.fonts}` name the
settings' paths. All four templates are known everywhere.
Each declared system program and runtime system helper is resolved, hashed
and included in `key.lab.programs`. Seeds, scratch and captures are excluded
from keys. The runner tree excludes `tests/`.

A process's `files` entry is either `{"path", "from"}`, a rendered text file,
or `{"path", "tree"}`, a subject artifact's uncompressed POSIX USTAR archive.
Validation refuses unknown artifacts, other archive formats, links and special
members, repeated or uncontained names, files listed before their directory,
and overlapping destinations across either kind of entry. Immediately before
spawn, after rendering text files, trees are unpacked without following links:
files 0400 (0500 with owner execute), directories and the tree root 0500.
Archive contents are never templates.

A seed action may declare `await: {"process", "stream", "prefix", "capture"}`.
All four members are required; the process must exist, the stream is stdout
or stderr, the prefix is literal text without CR, LF or NUL, and the capture
matches `[a-z][a-z0-9-]*`. Collections, checks and tools refuse `await`.
The first matching line ending after the action starts supplies `{action.A.C}`
to later seed actions, collections and checks; processes, tools and executable
paths cannot use it. The action advances once both its command exits 0 and
its line arrives, in either order, within one `command_seconds` from spawn.
A nonzero, cancelled or timed-out command advances without awaiting. A missing
line fails `bound/command_seconds/A` and leaves commands needing its capture
unavailable. Such an action alone gains `awaited` in its command record: null
until seen, then `{"ms", "captures": {C: VALUE}}`; the timeline records an
`awaited` event with `name` and `captures`, as it records ready captures.

A process may opt into `bootstrap: {"stream": "stdout", "action": NAME}`.
Its stdout then belongs only to that seed action. The action must be unique
and declare neither `stdin` (even empty) nor `await`; no ready or awaited rule
may read the private stdout. Readiness needs both the public ready lines and
one nonempty UTF-8 LF line within the same `ready_seconds` from spawn. The
line is at most 4096 bytes including LF, without CR or NUL. Partial, malformed,
overlong or extra output fails `bootstrap/P/REASON` with no evidence path.

The host keeps private bytes outside lifecycle snapshots. Immediately before
the recipient starts, it writes the complete framed line once into an empty
pipe while holding its read end, closes the writer and passes the reader as
stdin. The recipient reads the line then EOF. No private stream is retained,
hashed, captured or scanned for panic text: source stdout is null in its
output record, and the recipient's stdin and both outputs are null. Its
command record gains `bootstrap: {"process": P}`. Its nonzero exit still
produces the ordinary action failure, with an empty evidence list.

The source record gains `bootstrap` with `stream`, `action`, `received_ms`
and `delivered_ms`; timestamps stay null until observed. The two bootstrap
timeline events name only `process` and `action`, with the observed `ms`.
Private bytes are dropped on handoff, cut, error or interruption, and private
streams keep draining. A trusted recipient's product session may live in a
declared scratch directory. Experiments without this opt-in keep their
ordinary streams, stdin, captures and records.

Validation reads machine facts without starting experiment processes: cgroup
v2, a user manager socket, enabled user namespaces, `systemd-run`, `bwrap`, and
the disk floor from `tmp/machine.env` (`PLAN_MIN_DISK_GB`, otherwise the
40 GiB machine default in owners.md, 4.5). Runtime admission also contacts the user manager and runs an inert bubblewrap
user/PID namespace probe, with no model. Each run provisions its delegated
`memory` and `pids` controllers. Validation alone proves no native containment.
The driver runs its boundary probe before experiment processes start and reads
its inventory before starting its turn.
A valid experiment is copied atomically to `store/experiments/D`; the key
record is written once to `keys/NAME/D/key.json`. No run is created.

Pure modules accept records and supplied facts. `store`, `cli`, `host`,
`transport`, `driver` and `loop` own effects.
Fixed vectors and the unit stories run from the repository root through:

```
heavy python3 -I -B -m unittest discover -t scripts/lab -s scripts/lab/tests/unit
```

Unit tests start no child processes and use no clock or sleep. CLI stories
call `main` with temporary inputs, explicit machine facts and supplied finding
times; native Git export and machine checks need native validation. Triage,
campaigns, general recovery and pruning remain unsupported.


`run` and `judge` support null drivers and the pinned Codex app-server adapter.
Faults, nonnull triagers and `--campaign` are refused as unsupported. Judged
failures publish finding candidates and occurrences from retained evidence.

The lifecycle in `runner/life.py` consumes events and returns ordered requests.
It selects the next ready dependency set using canonical u64 draws, renders a
process's files immediately before its start, and publishes captures only when
all that predecessor's ready lines have arrived. Arguments are literal and each
environment is complete. Complete stdout/stderr lines drive readiness; no socket
probe or sleep decides readiness. Public process and command output is
streamed into the run's retained files.

Collections and checks accept `cluster: "up" | "stopped"`, defaulting to
`"up"`. Live collections and checks precede reverse stop. After every started
process is reaped and every process and command tree and stream has drained,
`stopped-collect` and `stopped-check` run, using the same `command_seconds`.
Only their command records gain `cluster: "stopped"`. A live check may use
only live collections; a stopped check may use either. A stopped check runs
only if every collection it names completed successfully, without a cut,
kill or timeout. Directory existence is never proof of a whole copy. Missing
or failed inputs skip that check with an explicit `<group>/<name>` undecided
reason; independent checks still run. Commands are never retried.

An unexpected process exit cuts the phase: later processes and seed actions do
not start, an active command is cut, and collection and checks continue against
what remains. An unavailable capture leaves that command explicitly unjudged.
SIGINT or SIGTERM runs the bounded release, then stops. Interruption and machinery errors
cancel an active stopped command and skip every pending stopped command.
A product stop still permits stopped commands after confirmed cleanup. Teardown sends TERM to each first
process in reverse start order, then uses the cost class's stop bound and
`cgroup.kill`, awaits tree emptiness and removes the tree. Descendants left by
an ended command or process are recorded as survivors and killed. A cleanup
failure remains an error with its tree retained for whole-unit cleanup.

`runner/host.py` provides the native tree operations; `runner/loop.py` connects
pipes, pidfds, cgroup events and the clock to the pure life. A run lives inside
its own transient `lab-<run>.service` with `Delegate=yes` and
`KillMode=control-group`. The worker moves into a separate runner leaf before
enabling controllers, and every child enters a separate cgroup before exec.
No process group approximates ownership. systemd owns final whole-unit cleanup
if the worker crashes or receives SIGKILL. The outer CLI relays SIGINT/TERM to
the unit's main process and waits for the unit to end.

The private worker entry point is a host detail, with no new CLI command or
public transport record. A private Unix socket transfers the already held flock
descriptor with SCM_RIGHTS; the worker retains that same open file description
until publication, including after death of the outer CLI. A second writer is
refused and names the active unit. Bootstrap stdout/stderr are retained under
`transport/<run>/` even if admission fails. `systemctl` joins the helpers hashed
into each key. Failed admission creates no run; controller provisioning errors
create an error run before any experiment process starts.

Wait status, exit phase, whether TERM was delivered, and whether the tree was
killed are distinct observations. Own RSS peak (`VmHWM` of an owned unreaped
child, or `wait4` usage) and cgroup `memory.peak` have separate fields; only the
own RSS is judged against the declared bound. An unavailable measurement stays
null. The loop reads no process environment and scans no process table.

Checks implement lab.md, 7.2: 0 with no lines passes; 1 requires valid failure
lines; 2 requires undecided lines and no failure; other or contradictory output
is an error. Runner stops, panics, survivors, memory, bounds, seed actions and
planted signatures join those observations in the pure outcome decision.
Failures take precedence over errors, then undecided checks, then pass. No
report is required without a driver. There are no default time or resource
limits: every deadline comes from the experiment's named class.

`run.json` is canonical `lab-run/1`, written once. Its `bounds` is the class's
`name` and the class values the run was held to, including optional meter bounds. Its `events.jsonl` is
append-only `lab-event/1`; each line has `event` and monotonic `ms` plus that
event's observations. The live marker is `lab-live/1` with `unit`.
`evidence.jsonl` contains canonical `lab-evidence/1` lines verified against the
immutable run records. `verdict.json` (`lab-verdict/1`) is replaced from
verified evidence. It records each seed's judged and unjudged run ids.
The other replaceable derived path is exactly `findings/index.json`.

Exactly three distinct seeds, each with a judged run, are required for a final
verdict. A fourth seed is refused before launch. An explicit rerun under one of
the three seeds creates a new attempt beside the earlier evidence. `judge`
draws new seeds while needed, stops at its first error or incomplete run, and
never retries an unjudged seed automatically. Rerun it explicitly with `run
--seed N`, then call `judge` again. A decided key runs nothing on `judge`.
Exit codes are 0 pass, 1 fail/flaky, 2 refused, 3 incomplete/not enough evidence,
4 machinery error, or 128 plus the interrupting signal.

`judge` re-derives the verdict with the class's current values. A fail whose
signatures are all `bound/NAME` or `bound/NAME/WHO`, each now strictly above
its recorded value, is superseded: listed in `not_judged`, as an error is.
An equal value or any non-bound failure keeps it judged. A seed left with
no judged attempt needs an explicit additive run.

There is **no sweeper or pruning** here. Scratch and failed/error evidence are
retained, including passing scratch. A live marker or an unfinished run causes
the next writing command to refuse and identify the run for owner recovery.
After a worker SIGKILL the owner must verify the unit has ended, recover a
missing error record and additive evidence from the events, or reconcile
publication already present, then clear its live marker before more writes.
Existing immutable records must stay intact. No missing record is interpreted as a pass, and no
missing usage is interpreted as zero.

## Held files and release

`lab held put NAME FILE` moves an owned regular file with one link into the
private `held/` store and sets `0600`. A name already kept is refused. Across
filesystems the owner handoff copies, fsyncs, then removes the source.
`lab held` lists names sorted as `NAME kept` or `NAME taken RUN TAKEN`.
Both commands take the lab's lock.

Optional experiment members `held: [NAME, ...]` and `release: [COMMAND, ...]`
default to empty. Release commands have `name`, `argv`, `env`, and optional
`stdin`, without `await`. `{held.NAME}` is available only in seed and release
argv. Validation inspects metadata alone and refuses a missing held file,
naming its taking run when known. Provision writes `lab-held/1` metadata to
`held/NAME.taken`, then renames the file into the run's private `held/`.
The runner never reads the held file during a run.

After live checks, including after a cut, release commands run in order with
`command_seconds` bounds. A failed command still permits the next. An
interruption runs the release before stopping; another interrupt leaves its
bound in force. Machinery errors or no remaining live process skip commands
with explicit unavailable reasons. Nonzero exits and signals fail with
`release/NAME/exit:STATUS` or `release/NAME/signal:SIG`.

Before stop, each owned regular file with one link and mode exactly `0600`
at `{held.NAME}` is renamed back and its `.taken` removed. A missing or
invalid file fails `release/held:NAME`, keeps the name taken and retains
scratch. Give-back never repairs permissions. `lab held put` settles a
taken name on the owner's word that its source is the one copy. Crashes
leave held files taken for owner recovery; there is no sweep.

Every new `run.json` has `held: [{name, taken_ms, given_ms}]`, empty when none;
`given_ms` remains null when give-back fails. Timeline events `held_taken`
and `held_given` carry the name and time alone. Release command records use
`group: "release"`, and `usage.longest_ms` includes their bounds.

Native process stories, outside the gate, use private port-zero canaries and
actual transient units, cgroups and spawns:

```
heavy python3 -I -B -m unittest discover -t scripts/lab -s scripts/lab/tests/process
```

`tests/process/test_hand_walk.py` walks `browse` and `sign-in` against the built
`SKEIN_LAB_BROWSER`, `SKEIN_TEST_CHROMIUM` and `SKEIN_TEST_FONTS` (Debian
DejaVu by default); missing prerequisites fail.

`look/capture` accepts `--live ADDRESS --live-out DIR` in place of `--page DIR`
for scenes whose page has `live`, `ready` and `focus`. It reads the hand's page
in place at each scene width, copies and verifies its shots, records its focus
and trouble, and gives back the hand's viewport after each scene or a failure.
The mockup runs in its own bubblewrap namespace. `look/check structure|style|words`
accepts repeated `--capture DIR` arguments: each scene must occur in exactly
one record, and structure checks a live scene's focus finder as `focus/page`.

The runtime stories fail when systemd/bubblewrap capabilities are missing or
denied. Unit stories use supplied times without sleeping; the AST boundary
includes `life`, `bootstrap`, `checks` and `findings`. The unit tier runs in
`scripts/check.sh`. The owner runs the native tier on demand after landing;
unit success alone does not establish native containment or private retention.
The bootstrap stories observe a public stream between private pieces and scan
all retained files and runner output after success, nonzero exit, abort, cut
and interruption. The terminal host-error story bounds supplied host calls and
requires the recovery marker to remain when cleanup cannot be confirmed.

Findings follow lab.md, sections 9 and 10.2:

```
lab finding F-0001 class experiment --by owner
lab finding F-0001 file --by owner
lab finding F-0001 take --by owner
lab finding F-0001 fix --experiment sha256:... --by owner
lab finding F-0001 close --recheck sha256:... --by owner
```

Every writing command reconciles retained published evidence under the lab's
lock. A judged non-use failure creates `F-0001` upward or attaches to the
finding with exactly the same signature. Failure identity is `(key, run,
failure id)`, so repeated reconciliation and repeated publication add no
duplicate occurrence. Summaries, names, seeds and message similarity do not
match findings. Use failures await triage; no model, triage invocation,
proposal, class or human judgment is generated. The pure reader validates
supplied `proposed` events; the CLI has no triage/import command.

Each `findings/F-NNNN.jsonl` line is canonical `lab-finding/1`, with `event`,
`t` (UTC milliseconds) and `by`. Candidates retain signature/kind/summary and
run/key/seed/failure identity; occurrences and reopenings retain that failure
identity. Proposed class/title/reason remains distinct from owner `classed`.
The classes are `world`, `fake`, `below`, `tools`, `experiment`, `lab`,
`look` (a page's appearance), and `words` (text a product's model reads).
An omitted `--by` on owner commands records
`uid:<uid>`; `take` requires `--by`. Reserved actors `runner` and `triager` cannot confirm a class or act
as an owner.

Filing needs a fail/flaky verdict from exactly three judged seeds, a rerun of
the first failing seed, and a proposed or confirmed class. A nonreproducing,
error or incomplete rerun is retained with its outcome and reproduction
result. Filing records the verified verdict snapshot, `flaky` (whether any
judged run lacked the signature), and the first failing seed's rerun results.
Its reproduction accounting includes every attempt after each key/seed's
first signature failure: explicit numerator, denominator, and each outcome.
An error or incomplete attempt with no signature contributes to the
denominator; its outcome stays error or incomplete.

`take` follows filing; `fix` needs a taken finding and a confirmed class.
For world/fake/below/tools/words, fix arguments are `--repository /absolute/path
--commit FULL_OID --test LOWEST_WORLD_TEST [--test ...] --product-commit
FULL_OID`. Those test names record the owner's evidence that they fail before
the fix and pass after it; the lab does not run or repair product tests. For
experiment, `--experiment DIGEST` names the retained corrected experiment,
which must differ from the original. For lab, use `--runner-commit FULL_OID
--test OWN_TEST [--test ...]`. Missing, unknown or mixed-class fix members
refuse. Findings write no product repository, forge issue or message.
For look, use `--repository /absolute/path --commit FULL_OID --expectation
NAME [--expectation ...] --product-commit FULL_OID`: unique expectation names
that fail before the stylesheet fix and pass after it. Its host close and
recurrence need the same product ancestry proof as other product classes.

The host completes experiment-class close/reopen verification. It checks the
retained key's digest and shape, frozen experiment bytes, and frozen subject
record/source/artifacts. Close derives a fresh verdict from retained runs and
evidence: a new key, exactly three judged passing seeds, no judged signature,
and equality with the corrected experiment digest. Cached `verdict.json` and
caller booleans supply no proof. It records `rechecked` with the verified
verdict snapshot, then `closed`. A later original-experiment failure is
`occurred` while closed; a corrected-experiment recurrence is `reopened`.
Historical verdict snapshots are verified against retained evidence prefixes,
so a later recurrence does not invalidate the earlier recheck history.

Product-class host close/closed-recurrence verification remains unavailable:
it requires an independently verified product repository binding, the
subject's commit, and read-only Git ancestry from the fix's `product_commit`.
Repository paths in `about`, hash order and cached provenance are insufficient.
Lab-class host close remains unavailable: it needs a verified mapping from
`key.lab.runner` to `runner_commit` and an explicit owner judgment. These cases
refuse with those requirements. The pure lifecycle accepts supplied verified
carriage facts; lab recheck and reopening additionally require an owner actor
and explicit `judgment`. Mechanical reconciliation never reopens a lab finding.
No generic process/tree lifecycle contains Git or ancestry logic.

`findings/index.json` (`lab-finding-index/1`) is rebuilt from event histories
and verified run evidence, including reproduction counts; its content is
never read as truth. Corrupt derived bytes can be replaced; corrupt source
events, missing evidence, duplicate/gapped IDs, unsafe modes, links and
partial JSONL appends refuse. Event histories only append. Publication runs
reconciliation before the worker clears `live.json`; a failed publication
retains that marker. Explicit owner re-entry through the publication adapter
verifies existing immutable bytes and reconciles additive evidence without
duplicate occurrences. A cut between rechecked and closed can be finished
after verifying the current evidence again. Missing run/evidence publication
or live markers still require owner recovery; there is no general crash
recovery, sweeper or pruner.

## The hand's commands

`hand/browse ADDRESS VERB [OPERAND...]` sends one person's call to the
hand's loopback listener and prints its answer. It supports outline, press,
type (text from stdin), key, choose, go, reload, back, forward, address, wait,
shot and trouble; `--in ROLE NAME` scopes a find and `--exact` matches its
name whole. `hand/sign-in ADDRESS` reads one sign-in link line from stdin
and prints the program's redacted answer. Both need the hand's browser
program running (lab-browser.md, 8.8).

Statuses are 0 answered, 1 refused or a wait missed, 2 usage error (usage on
stderr), and 3 unreachable browser or unreadable answer.
The endpoint must be `127.0.0.1:PORT` or `[::1]:PORT`; `go` accepts only a
path starting with `/`, not `//`, with printable ASCII and no spaces.

## Paid proof: squares

`tests/proof/` holds the lab's paid proof, run by hand, never by a tier.
Its directories have no `test*.py` or `__init__.py`; the unit and process
stories exercise it through the scripted host, with no model or login.
The `ledger` subject supplies the task through its one tool. The charter
asks for a corrected program run through a heredoc, with no file edits.

Configure the lab's canonical settings with a `squares` cost class and the
Codex driver described below before validating or running. The experiment uses
the resolved interpreter `/usr/bin/python3.13`; tool views refuse a symlink
as their executable mount. From any directory:

```sh
umask 077
lab=/path/to/skein/scripts/lab/lab
proof=/path/to/skein/scripts/lab/tests/proof
repository=/absolute/path/to/fresh-ledger-repository
commit=$("$proof/subjects/ledger/repository" "$repository")
subject=$("$lab" subject --repository "$repository" --commit "$commit" \
  --artifact "ledger=$proof/subjects/ledger/ledger")
"$lab" validate "$proof/squares" --subject "$subject"
"$lab" run "$proof/squares" --subject "$subject" --seed 1
```

Before each paid run, rehearse it with no model. The rehearsal is a real `lab run` of
the same experiment and subject in a scratch lab. The real Codex 0.162.1 runs the
key's config, with its provider pointed at a loopback fake Responses endpoint that
plays the agent. It has no `codex_home`, so no login.

- A wrapper as `driver.program` execs the real binary path with `-c` provider overrides.
- The scratch lab's runner copy has two rehearsal-only changes:
  - its inventory drops the one `sessionFlags` layer those overrides make;
  - its sandbox profile reads the real binary, as a paid run's does.

`lab run` can't be pointed at the fake without changing the key, because the provider is
part of the key. The rehearsal's key differs from the paid key only in `driver.program`
and the runner digest.

The paid run goes ahead only when both scripted agents come out as expected:

- **Correct agent:** the run passes with no errors.
- **Wrong-answer agent:** the run fails with `check/answer/answer/wrong` and no errors.

In each, the agent's heredoc command must print the expected value through the key's
environment policy. The scripts are not landed tooling yet.

## Codex 0.162.1 adapter

The Codex 0.162.1 adapter starts `app-server --stdio --strict-config` in the
run's contained driver tree, initializes the experimental API, creates one
ephemeral thread with permissions `lab` and approvals `never`, reads the
inventory, then starts one turn with the preamble, verbatim charter and report
schema. Its configuration disables websocket preconnection with a custom
Responses provider. The provider names no base URL, so Codex picks the endpoint
for the login: a ChatGPT login is refused by the API platform's. `views.profile_toml` supplies the same profile text to the
key's configuration and the provisioning probe.

| Inventory item | 0.162.1 observer and acceptance |
|---|---|
| permission profile | `thread/start.activePermissionProfile.id == lab` |
| MCP servers and tools | all `mcpServerStatus/list` pages: only connected `lab`, exact tool names, no tools error |
| resources | MCP resources and templates empty |
| prompts | unobserved: no catalog observer |
| apps | every `app/installed.apps` entry has `enabled` and `callable` both `false` (none, or only disabled ones); the entries are kept |
| plugins | installed catalog empty and no load errors; model-facing capabilities unobserved |
| hooks and skills | lists for the working directory empty, no warnings or errors |
| configuration layers | run user layer plus system layers with empty config |
| instruction files | `thread/start.instructionSources` empty |
| built-in tools | unobserved: no complete live inventory observer |

An incomplete observer or mismatch ends the drive in error; unobserved items
remain recorded. Commands and lab MCP calls are numbered actions. Calls seen by
both the broker and host pair in FIFO order by tool, args and stdin presence.
The audit admits user/agent messages, reasoning, plans and compaction; other
item kinds, including calls to another MCP server, end the drive in error.
Codex's own `list_mcp_resources`, `list_mcp_resource_templates` and
`read_mcp_resource` (server `codex`) are numbered actions, host-only; a listing
that is not empty or a read that is not an error ends the drive in error
(lab.md 6.1). The inventory's MCP and resources rows keep their reach empty.
The lab server's `default_tools_approval_mode` is `approve`: Codex 0.162.1 otherwise
refuses an unannotated MCP call under approval policy `never`; the broker admits each
call. The agent's commands inherit no environment and get `PATH=/usr/bin:/bin`:
without it Codex 0.162.1 leaves only its own helper directory on their `PATH`. `thread/start` asks for raw response items: each raw `function_call` must be
answered by an item with its call id, or be a `write_stdin` to a running command's
`processId` (a counted `write_stdin` action). Any other raw call is a counted
`unhandled` action and ends the drive in error, as do actions with no raw call.
Time and action bounds stop the host as they pass. Usage replaces each turn's
last counts and folds across turns: input includes cached, unknown stays null.
Input past its bound records the overshoot and stops the host. `done:false`
and each unseen step produce `use/untriaged`; malformed or missing reports
with clean host ends remain incomplete.

`driver/tools.json` is canonical `lab-driver-tools/1`, with rendered tool roots,
work, source and the absolute broker socket. The Unix address can exceed
`sockaddr_un`: the runner binds through an opened directory descriptor; clients
must likewise connect through the socket's opened parent when the path is long.
Calls wait for completed inventory and admission before `go`. The runner writes
`calls.jsonl`: starts reference the stdin path; ends include all present files'
sizes and SHA-256 digests. A cut call retains its start with no end. Closing the
broker tells the MCP server to end its calls. The runner enforces each call's
`command_seconds` bound by ending the drive and closing the broker.

`run.json.driver` holds `invocation`, `expected_inventory`, `inventory` (each
item's state and evidence), numbered `actions`, observed `turns`, `end`,
`started_ms`, `ended_ms`, `report_error`, `probe`, and `files`. `usage.driver` holds elapsed milliseconds,
seconds rounded up, actions, turns, tokens by input/cached/output, and
`input_tokens_overshoot`. `driver/events.jsonl` retains received app-server JSON
with `t_ms`; `host.stdout` and `host.stderr` retain raw streams. The transcript
joins actions, host outputs and broker call records. Config, manifest, report
and transcript files have size and digest references. The scripted host in
`tests/support/scripted_codex.py` executes commands and emits scripted events;
it starts the config's `lab tool RUN` MCP server and makes scripted calls through
it, with no model transport. Its process stories exercise pass, each driver bound,
audit and inventory errors, use failures and incomplete reports.

Late usage received during shutdown is retained. Host payloads outside the lab's
canonical JSON subset are preserved as JSON text under `json` in their evidence.

`lab tool RUN` is the driver's stdio MCP server (lab.md, 6.3). It reads
`keys/*/*/runs/RUN/driver/tools.json` without taking the runner's writer lock.
The runner supplies rendered tools, work/source paths and its broker socket,
exactly the `lab-driver-tools/1` meeting-point record. The server offers
only those tools; each call waits for `go` or `refuse` before starting. Its
bubblewrap view exposes read-only system roots, the program, declared reads,
declared writes, read-only work, private `/proc` and `/tmp`, declared environment
and shared networking. Broker EOF kills active views, including descendants.
Bubblewrap refuses a symlink as a mount, so validation refuses a tool's program,
and a read or write rendered before the run, that is not its own resolved path:
`expected a resolved path: REAL`, naming the member.
The server writes private `driver/calls/N.{stdin,stdout,stderr}`; the runner
alone numbers actions and writes `driver/calls.jsonl`.

The runner uses `views.profile_toml(codex_executable, source, work)` for the host's
permission profile and calls `probe.boundary_probe` at provisioning, before the
containment tree or any experiment process starts. The probe takes explicit run/source/work/canary/home paths and the class's command
bound, creates an inert run-local CODEX_HOME containing only config.toml, and
checks the seven observations in lab.md, 6.2 through `codex sandbox -P lab`.
It returns per-check evidence or raises `BoundaryError` carrying that evidence.
It uses no login and starts no model turn. The native stories include a
weakened profile that makes a throwaway home readable and must fail.

## Each tool's provisioning call

Each experiment tool requires `"provision": {"args": ["help"], "status": 0}`.
The arguments are literal strings, and the expected status is an integer from
0 through 255. Choose a call harmless after the seed, such as help or a read.
Before the drive, a second host runs the same configuration with its provider
pointed at the runner's loopback Responses endpoint. It calls every tool once
through the MCP server, view and broker, then reports. Its own private home,
configuration, raw events, calls and endpoint requests/responses are retained
under `driver/provision/`; `run.json.driver.provision` records its invocation,
inventory, offered tools, calls and times. Provisioning is bounded by the
class's `command_seconds`. It contributes no drive actions, time, tokens,
fault counts or meter readings.

An inventory mismatch, unhandled call, unexpected exit or signal, missing call,
unadmitted offered tool, timeout or host failure prevents the drive's host from
starting and records a `provision/...` error. Offered names use the namespace
Codex sent, separated by a dot: `mcp__lab.ledger`. The admitted built-ins are
`exec_command`, `write_stdin`, `list_mcp_resources`,
`list_mcp_resource_templates` and `read_mcp_resource`; their names and namespace
shape are held to a recorded Codex 0.162.1 request in the unit vectors.

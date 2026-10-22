# Lab null-driver runner and findings

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
`triager`, with optional `about` and `browser`. `costs` maps experiment cost
names to objects with exactly these positive integer fields: `ready_seconds`, `command_seconds`,
`stop_seconds`, `driver_seconds`, `driver_actions`, `driver_input_tokens`.
There are no default cost values. For experiments with `driver: null`, settings
`driver` and `triager` can both be null; no driver host or report is needed.
For agent experiments, `driver` is the key's driver object (lab.md, 2.3), and
`triager` has the same fields plus positive `triage_seconds`, `triage_tokens`.
This slice validates their input shapes and declared driver executable identity;
it does not run a driver or check its effective inventory.

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

Validation reads machine facts without starting experiment processes: cgroup
v2, a user manager socket, enabled user namespaces, `systemd-run`, `bwrap`, and
the disk floor from `tmp/machine.env` (`PLAN_MIN_DISK_GB`, otherwise the
40 GiB machine default in owners.md, 4.5). Runtime admission also contacts the user manager and runs an inert bubblewrap
user/PID namespace probe, with no model. Each run provisions its delegated
`memory` and `pids` controllers. Validation alone proves no native containment.
Driver inventory and boundary probes belong to the driver slice.
A valid experiment is copied atomically to `store/experiments/D`; the key
record is written once to `keys/NAME/D/key.json`. No run is created.

Pure modules accept records and supplied facts. `store`, `cli`, `host`,
`transport` and `loop` own effects.
Fixed vectors and the unit stories run from the repository root through:

```
heavy python3 -I -B -m unittest discover -t scripts/lab -s scripts/lab/tests/unit
```

Unit tests start no child processes and use no clock or sleep. CLI stories
call `main` with temporary inputs, explicit machine facts and supplied finding
times; native Git export and machine checks need native validation. Driver
runs, triage, campaigns, general recovery and pruning are later slices.


`run` and `judge` support **`driver: null` only**. A nonnull driver and a supplied
`--campaign` are explicitly refused. There is no model, transport to Codex,
tool broker or triager in this slice. Judged failures publish finding
candidates and occurrences from their retained signatures and evidence.

The lifecycle in `runner/life.py` consumes events and returns ordered requests.
It selects the next ready dependency set using canonical u64 draws, renders a
process's files immediately before its start, and publishes captures only when
all that predecessor's ready lines have arrived. Arguments are literal and each
environment is complete. Complete stdout/stderr lines drive readiness; no socket
probe or sleep decides readiness. Every byte of process and command output is
streamed into the run's retained files.

An unexpected process exit cuts the phase: later processes and seed actions do
not start, an active command is cut, and collection and checks continue against
what remains. An unavailable capture leaves that command explicitly unjudged.
SIGINT or SIGTERM goes directly to stop. Teardown sends TERM to each first
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

`run.json` is canonical `lab-run/1`, written once. Its `events.jsonl` is
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

There is **no sweeper or pruning** here. Scratch and failed/error evidence are
retained, including passing scratch. A live marker or an unfinished run causes
the next writing command to refuse and identify the run for owner recovery.
After a worker SIGKILL the owner must verify the unit has ended, recover a
missing error record and additive evidence from the events, or reconcile
publication already present, then clear its live marker before more writes.
Existing immutable records must stay intact. No missing record is interpreted as a pass, and no
missing usage is interpreted as zero.

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

Missing or denied systemd/bubblewrap capabilities produce an explicit
process-tier skip/refusal. They prove nothing about native containment. Unit
stories use supplied times without sleeping; the AST boundary includes `life`
and `checks`, plus `findings`. The unit tier runs in `scripts/check.sh`; the process tier runs
before the exhaustive suite in `scripts/after-landing.sh`.

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
The seven classes are exactly `world`, `fake`, `below`, `tools`, `experiment`,
`lab`, `look` (a page's appearance). An omitted `--by` on owner commands records
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
For world/fake/below/tools, fix arguments are `--repository /absolute/path
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

# Lab null-driver runner

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
`triager`, with optional `about`. `costs` maps experiment cost names to objects
with exactly these positive integer fields: `ready_seconds`, `command_seconds`,
`stop_seconds`, `driver_seconds`, `driver_actions`, `driver_input_tokens`.
There are no default cost values. For experiments with `driver: null`, settings
`driver` and `triager` can both be null; no driver host or report is needed.
For agent experiments, `driver` is the key's driver object (lab.md, 2.3), and
`triager` has the same fields plus positive `triage_seconds`, `triage_tokens`.
This slice validates their input shapes and declared driver executable identity;
it does not run a driver or check its effective inventory.

The experiment is the directory defined by lab.md, section 4. It and its
`experiment.json` must contain no links or special files. Files named by
process `files` are checked as templates at the process's place in the DAG;
rendering a dependent file waits for the predecessor captures. Executable
paths use immutable experiment/artifact inputs or literal absolute system
paths. Each declared system program and runner helper is resolved, hashed
and included in `key.lab.programs`. Seeds, scratch and captures are excluded
from keys. The runner tree excludes `tests/`.

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
call `main` with temporary inputs and explicit machine facts; native Git
export and machine checks need native validation. Driver runs, findings,
campaigns, recovery and pruning are later slices.


`run` and `judge` support **`driver: null` only**. A nonnull driver and a supplied
`--campaign` are explicitly refused. There is no model, transport to Codex,
tool broker, triager or finding creation in this slice. Raw signatures,
summaries and evidence paths remain in each run for later finding work.

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
immutable run records. Only `verdict.json` (`lab-verdict/1`) is replaced from
verified evidence. It records each seed's judged and unjudged run ids.

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

Missing or denied systemd/bubblewrap capabilities produce an explicit
process-tier skip/refusal. They prove nothing about native containment. Unit
stories use supplied times without sleeping; the AST boundary includes `life`
and `checks`. The unit tier runs in `scripts/check.sh`; the process tier runs
before the exhaustive suite in `scripts/after-landing.sh`.

# Lab input skeleton

Python 3.11+, standard library only. Run from any directory:

```
/path/to/skein/scripts/lab/lab subject --repository DIR --commit COMMIT --exclude PATH --artifact app=/absolute/path/to/app
/path/to/skein/scripts/lab/lab validate EXPERIMENT --subject sha256:...
```

The launcher uses `python3 -I -B -S` (including no system site initialization) and adds its own directory for imports.
The lab is `~/src/rust/tmp/lab`, owned by the caller, private, with no symlink
component. Its parent must exist. Both commands take its nonblocking lock.
Only these two commands are implemented. Success prints one digest. Validation
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
40 GiB machine default in owners.md, 4.5). The provision-time controller,
namespace and driver-inventory probes belong to the next slice. No claim of
a successful native containment probe follows from validation.
A valid experiment is copied atomically to `store/experiments/D`; the key
record is written once to `keys/NAME/D/key.json`. No run is created.

Pure modules accept records and supplied facts. `store` and `cli` own effects.
Fixed vectors and the unit stories run from the repository root through:

```
heavy python3 -I -B -m unittest discover -t scripts/lab -s scripts/lab/tests/unit
```

Unit tests start no child processes and use no clock or sleep. CLI stories
call `main` with temporary inputs and explicit machine facts; native Git
export and machine checks need native validation. Process supervision,
driver runs, findings, campaigns and hygiene wiring are later slices.

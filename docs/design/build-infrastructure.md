# Build infrastructure

Provisional, 2026-10-22. The host admission and recovery contract for the
owners' `heavy` commands. Runtime identity is specified by gate-policy.md
when that policy module is installed.

## 1. In one page

- **Two build slots, four jobs each.** Builds run through pueue's `build`
  group. An exclusive gate uses the `gate` group, one at a time, with
  eight jobs. Host admission also counts task lifetimes, independently of
  the queue daemon's view of its wrappers.
- **Intent precedes effects.** A gate records its token, boot identity,
  queue identity, owned systemd unit and the groups found running in a
  durable atomic snapshot before pausing any group. Groups already paused
  remain paused when the gate releases.
- **Containment owns descendants.** Every payload executes in a user
  systemd service with `ExitType=cgroup`, `KillMode=control-group` and no
  delegated subtree. Changing sessions does not escape it. PID exit and
  queue completion trigger recovery; neither proves descendants ended.
- **Recovery is independent.** A user systemd timer reconciles the same
  host ledger as admission and normal completion. Its proposed polling
  default is five seconds, subject to director review. It is not a task
  timeout or a gate budget.
- **Release is replayable.** Under the host lock, mark the task closing,
  kill its owned cgroup, prove it empty, resume only owned holds without
  maintenance holds, and commit one logical release outcome per token.
- **Evidence is durable and narrow.** JSONL events carry task identity,
  phases, capacity and safe policy identity. They carry no task
  environment, output or full queue records. Existing heavy output and
  the eleven-column TSV remain available to land.

## 2. Ownership and admission

The supported queue is dedicated to `heavy`. Its groups must contain
only tasks submitted through this admission path. Deployment requires a
quiet migration from an older queue: arbitrary queued commands have no
owned cgroup ledger, so their completion cannot be proved through this
interface. Calling pueue directly to submit or restart tasks bypasses
this contract. Unsupported containment refuses admission visibly.

`DEV_ROOT` selects the workspace and `DEV_TMP/heavy` holds host state.
All callers sharing a queue must share this state directory. An alternate
queue must use a distinct state directory. The queue identity hashes the
canonical config path, profile and endpoint-selection lines. The config
and data directory must
retain their identity across daemon restarts. Replacing a queue at the
same config path requires a drained migration, rather than erasing the
ledger. The host lock serializes admission, reconciliation and explicit
maintenance operations.

Submission first saves a unique token and safe context, adds a stashed
queue entry, saves the returned ID and enqueue intent, then enqueues it. Reconciliation
repeats a persisted enqueue intent, allowing the wrapper to claim the
known ID if its submitter dies. A lost add response can leave an
unactivated stashed queue entry; its released token cannot run a payload,
and the ledger cannot remove an unknown queue ID without a safe metadata
API. The queued wrapper
has a PID, process start tick and boot identity; payload start is a
separate event inside its cgroup. A client may disappear without losing
task ownership or completion evidence.

A gate saves pause intent, pauses every non-gate group without stopping
running tasks, then drains the ledger's live build lifetimes. Persisted
gate holds also block build admission if the queue restarts and forgets
its group pause. Builds occupy at most two ledger slots until their
cgroups are empty, regardless of the queue's slot accounting. Gates
serialize through both the queue and the host ledger.

## 3. Durable state and release

The version 1 host snapshot contains the queue identity, task records,
current gate token, maintenance holds and resume intents, installation proof and
an event outbox. Files are created with umask 077. Save writes a sibling
temporary file, flushes and fsyncs it, renames it over the snapshot and
fsyncs the directory. A leftover temporary file is never read as state.

Each task records its unique token, queue ID, boot identity, wrapper
identity, unit name, lane, label, repository, tree, policy identity, log
path and command. Commands are private task metadata and remain in the
TSV for compatibility; events exclude commands and output. Task states
are submitted, enqueuing, queued, waiting, launching, active, finishing, closing
and released. A gate additionally records the groups found running and
the groups found paused before its pause intent.

A wrapper identity ending, or a changed boot ID, marks the task closing.
Normal payload completion takes this same path. Closing is saved before
killing the token's unit. Recovery checks both the systemd unit's
cgroup and its recorded cgroup path, using cgroup v2 `populated` evidence.
A release occurs only after neither contains processes. Old boot IDs do
not authorize skipping the current unit check.

The cgroup payload guard takes the host lock and verifies an unreleased
launching token and a live wrapper before it starts a command. Therefore
a systemd unit creation delayed past orphan release cannot run a payload.
Unit names are unique for the token and are never reused. Finishing a
payload kills private daemons in the same cgroup before freeing capacity.
The shared kache daemon runs in its own active user service; heavy refuses
a selected kache wrapper if that service is inactive, and never lazily
starts kache inside a task. Collection connects to the existing service.

Before resuming a group, recovery checks the durable maintenance holds.
The gate remains durable through every resume until the final release
snapshot. A crash there replays pauses and resumes under the same lock;
no build payload can enter while that gate is held. Initially paused
groups are excluded from the gate's resume list.

## 4. Independent recovery and maintenance

`heavy --install-reconciler [INTERVAL]` probes real cgroup containment,
including a `setsid` child, and installs an enabled user service and timer
named from the state directory's hash. Installation is explicit. It
changes no queue lanes, credentials, caches or runtime profile. The
service reads a private file containing only the required queue/workspace
configuration. Admission requires the timer to be active and a
containment probe for the current boot. After reboot, reconciliation and
admission repeat the probe before accepting new work. Reconciliation can
recover old holds first. A missing user bus or unsupported systemd capability
fails visibly.

`heavy --reconcile` runs the common recovery path. The timer survives a
killed queued wrapper and a killed submitter. It does not depend on queue
terminal rows. A temporarily unavailable queue fails closed: the next
timer activation retries without releasing its persisted gate hold.

`heavy --maintenance-pause GROUP` saves an operator hold before applying
it. `heavy --maintenance-resume GROUP` clears that hold and resumes only
when no gate holds it. A resume intent remains durable until applied,
including when requested during a gate. These are the maintenance interfaces
while gates run. A direct pueue pause made during a gate cannot be
distinguished from the gate's pause and is outside this contract.

`heavy --remove-reconciler` refuses unfinished tasks, then removes only
its own timer and service. `setup` checks recovery readiness without
installing it; its group repairs use `heavy --repair-group` under the host
lock, preserving gate and maintenance holds. Tests use distinct config, data, socket,
state and unit names; no live owners' daemon or groups are touched.

## 5. Events and compatibility

`DEV_TMP/heavy/events.jsonl` records submit, wrapper start, admission,
scope entry, phase begin/end for pause, drain, settle, payload, release
and collection, task completion, refusal and recovery. Records include a
unique event ID, token, timestamp, boot and queue identity, lane, label,
repository, tree, effective safe policy ID, free disk and memory capacity,
and the entrypoint recording the event. A contained gate sources
`gate-policy.sh` and calls `gate_policy_prepare` when that module is
installed, both at submission and inside the contained task. Policy
errors prevent payload entry. The recovery fixture can
run before the policy module is integrated and identifies it as unavailable.

The snapshot outbox precedes journal appends. Appends are flushed and
fsynced; replay deduplicates by event ID. An interrupted final JSONL line
is truncated before replay. The TSV uses its unique per-token log path
for the same recovery property. The snapshot retains a completed task's
row until it is durable. Repeated reconciliation creates neither another
release outcome nor another TSV row.

The TSV columns remain queue ID, lane, UTC completion, wait seconds, run
seconds, exit code, available memory GiB, working directory, log path,
command and label. Queue IDs may be reused by pueue; tokens and log paths
are the durable identities. Heavy prints its queued ID, the requested
log tail, and its task completion summary. It never reads pueue task
records or task logs. The phase events distinguish wrapper startup,
drain/settle time and payload execution; TSV wait time ends at payload
start and run time includes completion cleanup.

## 6. Fault verification

`scripts/dev/test-heavy-recovery --evidence NEW_DIRECTORY` exercises the
production heavy entrypoints with disposable pueued instances and user
systemd units. It covers killed pause intent, fsynced intent before
rename, paused groups, setsid/private descendants, release and release
snapshot windows, queue restart, boot identity change, pre-paused groups,
maintenance holds during a gate, two gates with builds, journal replay and
late systemd creation. Assertions check exclusion through cgroup release,
progress, one release per token, private descendant death and shared
daemon survival. The fixture uses short reconciler intervals without
changing production task timeouts or test-suite budgets. Raw artifacts
include each case's ledger, event journal, logs, probe, descendant cgroup
evidence and cleanup receipt.

Overhead measurements run the same lightweight payload directly and
through heavy in interleaved pairs, at least three times. Event counts
and bytes accompany timings. Individual case elapsed times are single
samples, not performance budgets. Cache collection uses a fixture executable, leaving the shared store alone.
Fault barriers and boot overrides require the explicit recovery-fixture
flag and paths beneath the isolated DEV_TMP. No full repository gate is
part of this recovery fixture.

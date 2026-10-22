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
  host ledger as admission and normal completion. Its approved polling
  default is five seconds (director 586). It is not a task
  timeout or a gate budget.
- **Release is replayable.** Under the host lock, mark the task closing,
  kill its owned cgroup, prove it empty, resume only owned holds without
  maintenance holds, and commit one logical release outcome per token.
- **Evidence is durable and narrow.** JSONL events carry task identity,
  phases, capacity and safe policy identity. They carry no task
  environment, output or full queue records. The eleven-column TSV remains
  available for diagnostics; land accepts token-owned receipts.

## 2. Ownership and admission

The supported queue is dedicated to `heavy`. Its groups must contain
only tasks submitted through this admission path. Deployment requires a
quiet migration from an older queue: arbitrary queued commands have no
owned cgroup ledger, so their completion cannot be proved through this
interface. Calling pueue directly to submit or restart tasks bypasses
this contract. Unsupported containment refuses admission visibly. The helper requires
Python 3.11 or newer and uses only its standard library.

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

A token-owned user service observes `pueue wait --quiet ID` while a token
is queued and unclaimed. Only a successful wait return is used: a terminal
or absent queue entry triggers cancellation under the host lock if the
token is still unclaimed. Transport failure retries through the timer.
The observer stops once a wrapper claims the token. Client death does not
cancel queued work, and no task age limit is imposed. A reused numeric ID
can never establish success. Completion does not wait on or remove an ID
after token release: without a safe identity API, a queue row cannot be
removed without risking an unrelated replacement. Completed queue rows
therefore require operator retention handling on the dedicated queue.

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

Admission captures both the candidate commit and its Git `HEAD^{tree}`.
A versioned gate requires a clean index and worktree, with no merge or
rebase in progress, at admission, contained entry, immediately after
settling before command start, and at command completion. A changed
commit, tree, index, worktree or merge/rebase state refuses completion
with an instruction to rerun land. The receipt's tree remains separate
from the commit identity retained in task metadata. Runtime preparation
compares a supplied policy with the computed policy at submission and
inside containment; payload entry also compares with the persisted
submitted identity. It never replaces that expected identity.

A wrapper identity ending, or a changed boot ID, marks the task closing.
Normal payload completion takes this same path. Closing is saved before
killing the token's unit. Recovery checks both the systemd unit's
cgroup and its recorded cgroup path, using cgroup v2 `populated` evidence.
A release occurs only after neither contains processes. Old boot IDs do
not authorize skipping the current unit check.

The cgroup payload guard takes the host lock and verifies an unreleased
launching token, a live wrapper, the boot identity and its owned populated
cgroup before any policy preparation or requested command. It durably
marks that scope active before executing Chromium, compiler, Cargo or
other policy version launchers. Their descendants, including detached
helpers, belong to the same scope and its release proof. A systemd unit
creation delayed past orphan release cannot execute policy tools or the
requested payload. Transport environment-file removal is replayed for
released tokens after a crash between durable release and unlink; this
cleanup does not change a sealed log or receipt.
Unit names are unique for the token and are never reused. Finishing a
payload kills private daemons in the same cgroup before freeing capacity.
The shared kache daemon runs in its own active user service; heavy refuses
an effective kache wrapper if that service is inactive, and never lazily
starts kache inside a task. Selection follows Cargo's increasing config
precedence, `[env]` force/relative settings and PATH, generic
`CARGO_BUILD_*` overrides, then dedicated wrapper overrides. An empty
dedicated override disables only its wrapper slot; the workspace wrapper
is independent. Masked global settings do not require kache. Refusals give
the caller the service/override remedy without printing configuration or
environment values. Collection connects to the existing service.

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

Existing waiting clients inspect systemd's reported recovery service and
timer state. A failed/missing service, an inactive timer or an unreadable
snapshot produces actionable recovery instructions. Clients continue
waiting on reported service/timer failures, retaining task ownership;
an unreadable snapshot returns nonzero without cancelling the task.
Elapsed task age and delayed ticks never establish failure or a timeout.
For storage failure, free space, restore/restart the same recovery service
if needed, and run `heavy --reconcile` with the same queue configuration
and `DEV_TMP`. Retain the ledger; do not delete/reset it. Reconciliation
is replayable. Rerun land when completion is recovered with exit 137.

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

Unknown legacy pauses have no durable owner. An empty process list or
absence of Cargo does not authorize resuming them. Migration must be quiet:
an operator accounts for legacy work and pause intent, drains the dedicated
queue and explicitly restores groups or records maintenance holds before
enabling admission. The root cause of incident 587 is unknown; the stale
pause fixture demonstrates recovery of a durable owned hold without
inferring that incident's cause.

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
use a fake Chromium 147 repeatedly, while its combined land/cache and
policy overhead cases use the installed Chromium 147. Missing policy
helpers or runtimes refuse gate submission visibly.

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

After cgroup-empty proof and durable terminal release, events and TSV are
flushed before receipt publication. Recovery seals the service's private
capture into a separate immutable `TOKEN.log`, fsyncs it, hashes its complete
bytes and atomically publishes the private 0600
`DEV_TMP/heavy/receipts/TOKEN.json`. Publication is replayed after crashes.
A delayed service can write only its capture, never the sealed log. The
receipt follows the exact 17-field heavy-receipt-v1 contract in
gate-policy.md, section 3.1. Cancelled, recovered and unstarted payloads
always have nonzero codes. After tail and summary the client prints the
receipt path and token as its final output line, outside the hashed log.

## 6. Fault verification

`scripts/dev/recovery-v1/test-heavy-recovery --evidence NEW_DIRECTORY`
exercises the production heavy entrypoints with disposable pueued instances and user
systemd units. It covers killed pause intent, fsynced intent before
rename, paused groups, setsid/private descendants, release and release
snapshot windows, queue restart, boot identity change, pre-paused groups,
maintenance holds during a gate, two gates with builds, journal replay and
late systemd creation. Assertions check exclusion through cgroup release,
progress, one release per token, private descendant death and shared
daemon survival. The fixture uses short reconciler intervals without
changing production task timeouts or test-suite budgets. ENOSPC injection
at snapshot, journal and log-sealing writes requires the same isolated
fixture flag/path contract; it never fills the shared filesystem. These
cases assert caller diagnostics, retained ownership, no false successful
receipt, and progress/idempotence after restoring the same service and
ledger. A release-flush crash followed by delayed launch checks that no
policy launcher/helper runs, the late cgroup empties and transport cleanup
replays. Active policy-helper descendants also undergo cgroup release.
Candidate cases mutate commits, index/worktree and rebase state after
settling, during payload and after receipt, including expected-old ref
failure. Raw artifacts
include each case's ledger, event journal, logs, probe, descendant cgroup
evidence and cleanup receipt.

The combined cases also exercise queued cancellation with real numeric ID
reuse and a replacement task, client death before wrapper start, terminal
queue completion with live private descendants, receipt-publication crash,
and real land/cache entrypoints in disposable Git repositories. The stale
pause case uses the approved five-second interval; other recovery fixtures
use 0.2 seconds and are labelled accelerated. Runtime, semantic policy and
missing-runtime changes before payload refuse without cache or ref advance.
Receipt faults cover before publication, a fsynced temporary receipt before
rename, and after atomic publication. The five-second case anchors a live
reconciliation, then observes recovery on the subsequent periodic tick.

Overhead measurements run the same lightweight payload directly and
through heavy in interleaved pairs, at least three times. Event counts
and bytes accompany timings. Individual case elapsed times are single
samples, not performance budgets. Cache collection uses a fixture executable, leaving the shared store alone.
Fault barriers and boot overrides require the explicit recovery-fixture
flag and paths beneath the isolated DEV_TMP. No full repository gate is
part of this recovery fixture.


## 7. Quiet migration, queue replacement and rollback

Before fleet installation, stop old `heavy` and `land` submissions and
account for every legacy caller, wrapper, queued/running entry and TSV
writer. Drain all of them; absence of Cargo processes or a completion-only
TSV does not prove the queue is quiet. Record intentional pauses and
restore them as explicit maintenance holds where appropriate. The source
kit is `scripts/dev/recovery-v1/`; legacy `scripts/dev/heavy`, `land`,
`setup`, `kit.sh`, `after-landing`, `kache-setup`, workspace assets and
`scripts/check.sh` retain their pre-M2 baseline contents. Nothing in a
legacy entrypoint loads the new kit, and following main does not migrate
a host. Baseline setup retains its link-to-primary behavior. After migration,
use only the stable setup on PATH; explicitly running baseline setup would
restore legacy links.

The director owns this sequence after the complete quiet accounting/drain:

1. Select and verify the committed release and its source/evidence hashes.
   Run `release=$(skein/scripts/dev/recovery-v1/install-release)` from the
   workspace containing that verified checkout.
   It copies HEAD's complete kit with `git archive` into a new
   `~/.local/share/skein-kits/recovery-v1-COMMIT` directory outside every
   primary/linked checkout. It writes a content manifest and makes every
   file/directory read-only. An existing release is refused, never rewritten.
   It changes no links, queues or services. Verify the release manifest with
   `python3 -I -B "$release/stable-release.py"` and the reviewed source hashes.
2. Use the exact `release` path returned by the copier. With the accounted
   legacy queue still quiet, run `"$release/setup" --fix` using the intended
   `DEV_ROOT`, `DEV_TMP` and queue configuration. This links heavy, land and
   setup together to that stable kit, including workspace assets. The stable
   setup is self-contained and never invokes baseline setup. Mutable-source
   setup refuses even `--fix` before any mutation. Readiness can report failure
   here until the new reconciler is installed; that is not permission to admit.
3. Establish the dedicated queue/config/data identity and its own shared
   ledger/state; preserve the recorded intentional pauses. Do not mix legacy
   TSV writers with durable-ledger writers or adopt queued legacy work.
   Install and verify that queue's independent reconciler with the stable
   `heavy --install-reconciler` and `heavy --check-recovery` under the same
   configuration/state, restore explicit maintenance holds, then check setup
   again. Admit new submissions only after readiness and ownership are proved.

Later landings can change the source kit, but cannot move stable links or
change installed helper bytes. A new release requires another explicit quiet
drain/copy/verify/link/reconcile/admit sequence; there is no online upgrade
or automatic fleet installation. Read-only permissions prevent accidental
writes, not deliberate chmod by the owning account. Retain each release
unchanged while it has callers, tasks, services or rollback evidence.

Never reset or replace a live queue or erase its ledger. Enqueue recovery
uses a persisted bare numeric queue ID; it cannot prove identity after a
reset/replacement. Replacement requires stopping submissions and the same
complete accounting/drain, including callers, wrappers, observers,
unfinished tokens and pause/resume intent, before changing queue identity
or selecting a fresh state directory. Retain the retired ledger and
evidence. A normal daemon restart keeps its config/data identity and
ledger. After reboot restore that queue and its user services, including
recovery and the shared kache service when selected, then reconcile with
the same configuration and state. Resolve recovered nonzero completions
by rerunning land.

For rollback, stop new submissions and account for/drain every new token,
caller, wrapper and queue entry using the new kit. Explicitly resolve all
maintenance holds and resume requests with the new maintenance commands;
a drained task list alone does not resolve them. Remove the reconciler
with the new kit while its helpers still exist, then restore old `heavy`
and old `land` together, plus baseline setup and workspace links. Do not
swap binaries first or leave an old client paired with the new recovery path. The old cache namespace and the new
receipt/cache files can remain; rollback requires no destructive cache or
ledger cleanup.

`scripts/dev/recovery-v1/test-inert-install --evidence NEW_DIRECTORY` compares all legacy
files to baseline `01c6ab91`, fast-forwards a disposable primary from a
separate disposable clone, and compares its unmigrated symlink bytes and
setup behavior. It verifies stable release hashes/links across another
primary advance, repeat-copy refusal and mutable-setup refusal. It never
runs global setup. Recovery fixtures also cover relocated after-landing
helper lookup and real Chromium 147 receipt/cache reuse. Land-entry storage
and missing-reconciler cases require the warning on the waiting caller's
stream before restoring the same service/ledger, with no receipt/green
while broken. The capture write uses guarded `/dev/full` to obtain kernel
ENOSPC without consuming disk space; both prebuild and gate paths refuse
landing after that capture failure.

## 8. Advisory landing turn (local acceptance)

The recovery kit has an opt-in advisory turn before land's host repository
lock and build-lane prebuild. Only remote code landings whose repository
name is `temper` enter it, with `LAND_TURN=1`. With that variable unset or
any other value, the hook starts no process and touches no turn state.
`--docs`, `--local` and other repository names bypass it even when opted
in. The original arguments are retained across the guarded child entry.
The gate, receipt validation, retry and fast-forward push remain land's
protection; the advisory turn does not authorize a push.

Production coordinator selection and transport are unapproved. Production
opt-in records `request` and `fail-open` with reason
`transport-unapproved`, then runs ordinary land. No SSH path, listener,
credential, origin lease ref or fleet opt-in is installed. Transport must
be separately approved before production use, after the director's fleet
installation. This local stage makes no claim about real landing savings.

The local acceptance transport requires `HEAVY_RECOVERY_FIXTURE=1` and
`LAND_TURN_FIXTURE_DIR`, a directory strictly beneath the isolated
`DEV_TMP`. It launches only the kit's local `land-turn.py holder`; it
accepts no command string or remote shell configuration. The holder
validates the random request token, repository and holder name before
using them. A retained repository-specific flock file provides exclusion;
there is no durable queue, fairness promise, task timeout or lease.
A bounded current-holder record is published while holding that lock.
It is diagnostic only: a killed holder can leave stale metadata, which
never authorizes ownership and is replaced by the next lock holder.

The holder watches its connection pipe while waiting and after grant.
EOF releases it promptly, including when a queued client disappears.
Grant spans ordinary land and its retries. Land and its descendants,
including after-landing, inherit no transport descriptors. Connection
loss or malformed protocol records one fail-open reason and proceeds with
ordinary land, or lets an already running land continue. It never kills
admitted heavy work on connection loss. Normal exit and handled signals
close the transport and reap the local holder; SIGKILL of the client
closes its pipe through the kernel, allowing the holder to exit.

Request, grant, release and fail-open records appear immediately on stderr
and in private `DEV_TMP/landing-turn/events.jsonl` (0600, umask 077).
Records contain only timestamp, event, fixed reason, token, repository,
holder, elapsed wait and optional runtime. The holder records release on
disconnect, including client death; a killed holder cannot record its
release, and the client records its connection failure. Journal failure
is visible and does not prevent ordinary land. No environment, command,
output or credential is logged by the turn. No task or gate age limit is
introduced. `test-land-turn --evidence NEW_DIRECTORY` exercises local
protocol faults and disposable Git landings through the recovery fixture;
its results distinguish those layers. Production transport and the full
repository gate are outside that fixture.

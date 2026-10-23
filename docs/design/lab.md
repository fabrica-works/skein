# The lab

Provisional, 2026-10-20, revised 2026-10-23. The lab's records and its runner: how an
experiment a product keeps in its own repository is run as processes,
driven by an agent, checked from outside, and judged over seeds into
verdicts and findings. The records are files of a fixed form, so that a
lab application can take them as its state later; the runner is a
command in `scripts/lab/` that names no product, in Python's standard
library for now, until the lab application takes it over in Rust
(10.1). A product's binding (its experiments, their templates and
checks, the values of its cost classes) is the product's own document.
What is still open is in section 11.

## 1. In one page

- **Experiments are a product's data; the lab is generic.** An
  experiment is a directory in the product's repository: a cluster of
  processes, the commands that seed, collect and check it, and, for an
  agent's experiment, a charter and tools (section 4). The runner knows
  processes, files and commands, never the product.
- **Every record is canonical JSON** (2.2): RFC 8785's form on a subset
  (ASCII keys, exact integers, no floats; a 64-bit seed or draw as a
  decimal string), digested with SHA-256, so that a reader in any
  language agrees on its bytes. Prose (a charter, a campaign's note) is
  Markdown that records name.
- **A key is what a run may not vary:** the experiment's digest, the
  subject's (the product's artifacts and source) and the lab's (the
  runner, the programs it runs, the driver host with its model and its
  configuration, the cost class's name) (2.3). A run adds its seed, what
  it is given (addresses, directories, scratch) and the class's values
  it is held to.
- **Evidence only grows.** A run's record is written once; a key's
  evidence, a finding's history and a campaign's are appended to;
  verdicts and indexes are derived and never edited (2.4).
- **A run is a cluster of contained process trees:** started in
  dependency order, independent ones in an order the seed draws; ready
  by a line on their output; watched for their exit in every phase;
  measured against their declared memory; stopped in reverse order,
  each whole tree, however the run ends (section 5).
- **Addresses are reported:** a process binds port 0 itself and prints
  its address on a ready line. The runner never picks, binds or passes
  a port, but for a port the machine keeps for a program that cannot
  report one, named in the lab's settings (4.4).
- **Held files move, never copy:** a secret one holder may have at a
  time, such as an account's signed-in record, is kept by the lab, taken
  by a run, moved into the cluster by its seed, and given back by its
  release, however the drive ended (4.8).
- **The driver's boundary is the runner's, not the prompt's.** The
  agent's own commands run in Codex's sandbox: reads of the host's
  minimal roots, the subject's source and its work directory, writes to
  its work directory, no network. The product is reached only through
  the experiment's tools, trusted programs each call of which the runner
  runs in a view of its own and records. A probe checks what it names
  before each start, and the host's tools are checked every run, as far
  as its version lets them be read: what cannot be is recorded as
  unobserved, and the host's events are audited. So the lab runs only on
  machines kept for it, never a person's computer (section 6).
- **Checks are commands with three statuses** (passed, failed with
  signatures, could not judge) beside the runner's own: stops, panics,
  survivors, memory, bounds, seed actions and the agent's report
  (section 7).
- **Four outcomes:** pass; fail, a failure observed; incomplete, nothing
  observed failing but something not judged; error, the lab's own
  machinery failed. Invalid inputs are refused before anything starts
  (4.7, 5.5).
- **A verdict is a key's evidence judged:** exactly three seeds, each
  with a judged run, before any verdict; then all pass is pass, all fail
  is fail, and mixed is flaky. Attempts and reruns add evidence, never
  replace it (section 8).
- **A finding is a failure with a signature:** matched mechanically
  where the signature says enough, by Codex's triager where it does not;
  filed once reproduced; classed by where its fix goes; fixed with a
  test in the lowest world that owns the behaviour; closed only after a
  recheck at a subject that carries the fix (section 9).
- **A planted fault stays detected:** nothing ignores an early exit.
  Detection is proven with the fault on, the correction with the fault
  off, under two keys (9.4).
- **Bounded, never by default:** an experiment names a cost class, whose
  values are budgets decided as `owners.md` (3) says and kept in the
  lab's settings; a run without them is refused. Actions are bounded as
  they start; tokens are observed as the host reports them, so a drive
  may overshoot by a turn, recorded. Each run records what it used,
  unknown where the host did not say, never 0. A subject that spends on
  a metered account of its own is metered and bounded too, and a brake
  keeps runs off an account near its limits (6.5).
- **On demand, one cluster at a time, private:** everything under
  `~/src/rust/tmp/lab/`, `0700`, one run holding the lab's lock, one
  campaign open (section 3).
- **The runner is Python 3.11's standard library, for now,** in
  `scripts/lab/`: no package installed, no build. Its decisions are pure
  functions apart from the code that spawns, signals and reads files, so
  that the lab application takes it over in Rust from the records and
  those functions, and skein's contained trees can take its host's part
  (10.1, 10.4).

## 2. Records and identity

### 2.1 The records

| Record | What | Where (section 3) |
|---|---|---|
| experiment | a product's directory: `experiment.json` (section 4), its templates, its charter, its check programs; copied whole into the store, read-only, named by its tree's digest | `store/experiments/<digest>/` |
| subject | what is under test: the product's artifacts by name and digest, the commit they were made from, and the source tree the driver may read | `store/subjects/<digest>/` |
| lab | the lab's inputs to a run: the runner's tree, the programs it runs (the interpreter among them), the driver host and its configuration, the cost class | inside `key.json` |
| key | the experiment's, the subject's and the lab's digests | `keys/<experiment>/<key>/key.json` |
| run | one execution of a key with a seed: what it was given, what happened, what it used, its outcome | `.../runs/<run>/run.json` and its files (5.6) |
| evidence | one line per ended run of the key | `.../evidence.jsonl` |
| verdict | the evidence judged by the policy (section 8) | `.../verdict.json`, derived |
| finding | a failure with a signature, and its history (section 9) | `findings/<id>.jsonl`; `findings/index.json`, derived |
| campaign | the runs of one campaign, and its note | `campaigns/<name>.jsonl`, `<name>.md` |
| settings | the lab's own: the driver host, the triager, the cost classes, the browser, the named programs, the brake | `settings.json` |

### 2.2 Canonical JSON and digests

- **One form.** A record is a JSON object whose keys are ASCII and
  unique, whose numbers are exact integers within ±(2^53 − 1), written
  without a fraction or an exponent, and whose strings are Unicode
  scalar values; there are no floats. Its canonical bytes are RFC 8785's
  for that subset: UTF-8, members sorted by key, no insignificant
  whitespace, the minimal escapes. A record file holds its canonical
  bytes and a final newline; a JSON Lines file, one canonical record per
  line. A reader refuses any other form, a duplicate key and a `format`
  it does not know.
- **A 64-bit value is a string.** A seed and a draw (5.4) are unsigned
  64-bit integers, past what every reader holds exactly, so a record
  writes each as its canonical decimal string: `0`, or digits without a
  leading zero, at most 18446744073709551615. A count, a size, a time or
  an index is an integer.
- **The subset is chosen so that any language's standard parts write
  it.** With ASCII keys, RFC 8785's order (by UTF-16 code units) is the
  keys' byte order; with integers only, a number is its decimal. In
  Python, `json` and `hashlib` implement it: the reader parses with
  duplicates, floats and `NaN` refused, checks keys, integers and
  strings, and refuses a file whose bytes differ from the canonical bytes
  of what it parsed (which refuses `-0`, `1.0` and a needless escape);
  the writer is `json.dumps` with keys sorted, `ensure_ascii` off and no
  spaces, encoded as strict UTF-8 (which refuses a lone surrogate). The
  runner needs no other program to read or write its records.
- **A `format` in every record:** `lab-<record>/<version>`, as
  `lab-experiment/1` or `lab-run/1`. The version changes when a reader of
  the old one would misread the new.
- **A digest** is `sha256:` and 64 lowercase hexadecimal digits, of the
  canonical bytes. A record's digest covers every member but `about`,
  its provenance: where a thing came from, who made it, when, on which
  machine. Provenance never changes an identity.
- **A tree's digest** is its manifest's, `{"format": "lab-tree/1",
  "entries": [...]}`: one entry per regular file, `{"path", "bytes",
  "sha256", "exec"}`, sorted by the path's bytes, paths relative and
  `/`-separated. A link, a special file or a name that is not UTF-8 is
  refused. Directories count only through their files, and of a file's
  mode only the owner's execute bit.
- **Records hold no output.** A process's stderr, a call's stdout or a
  collected file is a file of the run's, named in a record by its path
  from the run's directory, with its size and digest. So a record stays
  small enough for a bounded reader.

### 2.3 The key

`{"format": "lab-key/1", "experiment": D, "subject": D, "lab": {...}}`,
named by its digest.

- **The experiment's digest** is its directory's tree: every byte its
  templates, charter and checks run with.
- **The subject's** covers its artifacts' digests and execute bits, its
  source tree's digest (or none) and its commit; the repository's path,
  the paths it excluded from the source, its maker and its time are its
  `about`.
- **The lab's** is inline:
  - `runner`, the digest of `scripts/lab/`'s tree without its `tests/`;
  - `python`, the interpreter's version;
  - `programs`, each system program the runner or experiment declares,
    the interpreter, `systemd-run` and `bwrap` among them, as
    `{"path", "sha256"}`: its
    resolved absolute path and its file's digest, sorted by path. These
    are the lab's and the experiment's helpers, whose behaviour affects
    the run, so they are identity, not the machine's provenance;
  - `driver`, null for an experiment without one, or the host's
    `program` (`{"path", "sha256", "version"}`), its `model`, its
    `effort`, its `catalog` (`{"path", "sha256"}`) when the settings pin
    one (6.1), and its `config`, the configuration the runner gives it
    (6.1) as JSON: its arguments, permission profile, disabled features,
    server and schema, with each run's own paths left as their template
    names;
  - `cost`, the class's `name` alone: its values are each run's, in its
    `bounds` (5.6), so that a class tuned between runs keeps its keys
    (6.5);
  - `browser`, only for an experiment that names the browser by its
    templates (4.2): `chromium`, its `{"path", "sha256", "version"}`,
    and `fonts`, the fonts directory's tree digest; the browser's
    program is then among `programs`. An experiment that names none has
    no `browser`, and its key is as it was.

  The lab's settings name the browser in `browser`, `{"program",
  "chromium", "fonts"}`: the program's absolute path, Chromium's
  `{"path", "sha256", "version"}`, and the fonts directory's `{"path",
  "tree"}` (lab-browser.md, section 7). Validation refuses a Chromium
  file or a fonts tree whose digest differs from the settings', and a
  run checks them again before it starts, as it checks `programs`.
  An experiment that names no `{browser.*}` template never reads the
  settings' `browser`: neither its validation nor its run digests
  Chromium or walks the fonts, and a `browser` missing, stale or
  naming files that are gone refuses nothing of it.

  The lab's settings name the machine's programs that a product's
  experiments need at a path of the machine's in `programs`, each by
  name, `{"path", "sha256", "version"}`: a binary the machine keeps
  pinned to a version, such as a forge's server, which no run fetches.
  An experiment names one by its template, `{program.N}` (4.2), and it
  is then among the key's `programs`, its version beside it; validation
  refuses a file whose digest differs from the settings', and a run
  checks it again before it starts. A name no template uses is never
  read.

  They name in `ports` each port the machine keeps for a program that
  cannot report its address (4.4), by name, a port number: an
  experiment names one by its template, `{port.N}` (4.2). It is an
  address, so not in the key; validation refuses a `{port.N}` the
  settings do not name.

  A runner whose checks change, a helper or a host updated, another
  model, a changed configuration or another cost class is a new key; a
  class's value changed is not (6.5).
- **Not in the key:** the seed; the addresses and directories a run is
  given; what the runner renders with them (files, the driver's
  configuration with its paths); the time; the machine. The machine's
  facts (host name, kernel, the user manager's version, processors and
  memory) go in each run's `about`.
- **Programs are named by absolute path** after templating: an
  artifact, a file of the experiment, a program of the runner's own
  tree (`{runner}`, 4.2), or a system program. The first three are in
  the key through their trees (the runner's by `runner`); the last is
  in `programs`.
  The program's identity is known before the run: an executable's path
  uses no run-specific value or readiness capture. Its arguments may.

### 2.4 Immutable, run-specific, append-only

- **Immutable inputs:** the store's experiments and subjects
  (directories `0500`, files `0400`, executable files `0500`), shared by
  every run of their keys,
  never written once made, removed only by pruning (3.3).
- **Run-specific:** the seed; the run's id, `<UTC time>-<6 hex>`; its
  unit (5.2); its directories (`{dir.D}`) and each process's scratch
  directory, created empty, `0700`.
- **Append-only:** a run's `events.jsonl` while it runs; its `run.json`,
  written once when it ends; the key's `evidence.jsonl`; each finding's
  file; each campaign's. Nothing in them is rewritten; a correction is a
  later line.
- **Derived:** `verdict.json` and `findings/index.json`, rewritten from
  their sources and never read as truth.

## 3. The lab's directory

### 3.1 Layout

```
~/src/rust/tmp/lab/                 the lab, 0700
├── settings.json                   the driver host, the triager, the cost classes, the browser, programs, ports, the brake
├── browser/                        the browser's program and fonts/ (lab-browser.md, 7)
├── lock                            held by every command that writes
├── canary                          a file no driver or triager may read (6.2)
├── pause                           while it exists, the brake refuses every run (6.5)
├── held/                           held files, <name>, or <name>.taken naming the run that has it (4.8)
├── store/experiments/<digest>/     immutable experiment trees
├── store/subjects/<digest>/        subject.json, artifacts/<name>, source/
├── keys/<experiment>/<key>/        key.json, evidence.jsonl, verdict.json
│   └── runs/<run>/                 one run (5.6)
├── findings/                       <id>.jsonl, index.json
└── campaigns/                      <name>.jsonl, <name>.md
```

### 3.2 Modes, the lock and the sweep

- **Private.** The runner runs with umask 077, and refuses a lab not the
  user's own, with group or other bits, or reached through a link.
- **One writer, one cluster.** Every command that writes takes `lock`
  exclusively, on an open descriptor, until it is done; `lab run` and
  `lab judge` hold it for their runs. A second writer is refused at
  once, naming the run in progress; it never waits.
- **One campaign** open at a time: `lab campaign open` refuses a second.
- **A live run** keeps `live.json` in its directory, naming its runner's
  unit. Each writing command first sweeps: a live run whose unit has
  ended (5.2) gets an `interrupted` event, its `run.json` from its
  events with outcome error, its evidence line, and `live.json` removed.
  Its held files stay taken, for their owner to settle (4.8).
- **The disk floor:** a run is refused while the lab's file system has
  less free space than the machine's floor (`owners.md`, 4.5).

### 3.3 Retention and pruning

- **Scratch is never evidence:** each process's directory and each
  `{dir.D}`. No record names a file in it; what a check or a finding
  needs is collected into the run's evidence first (4.5). Scratch is
  removed when a run passes, and kept otherwise until `lab prune
  --scratch`.
- **A run's evidence is required** while a finding that is not closed
  names the run (its first failure, an occurrence, its seed's rerun, a
  recheck), or while its campaign is open. `lab prune --runs` removes
  the files of runs nothing requires, never `run.json`, `events.jsonl`
  or the evidence line, and writes `pruned.json` beside them: what went,
  and when.
- **A subject** is pruned (`lab prune --subjects`) once no key with a
  required run, and no open campaign, uses it. `subject.json` stays,
  with its `pruned.json`; a run of a key whose subject is pruned is
  refused.
- **On demand only:** pruning is the lab owner's command, never a
  timer's, and refuses what is required. So no record names required
  evidence that is gone, and a run whose files were pruned says so in
  its own directory.

## 4. The experiment

### 4.1 Its file

`experiment.json`, at the root of the experiment's directory, beside
the files it names. A minimal example, a fake backend, the application
and an agent:

```json
{
  "format": "lab-experiment/1",
  "name": "hello",
  "cost": "small",
  "dirs": ["client"],
  "processes": [
    {"name": "backend",
     "argv": ["{artifact.fake}", "--listen", "127.0.0.1:0",
              "--seed", "{seed.backend}", "--state", "{experiment}/backend"],
     "env": {"PATH": "/usr/bin:/bin"},
     "ready": [{"stream": "stdout", "prefix": "listening on ", "capture": "address"}],
     "memory_bytes": null},
    {"name": "app",
     "after": ["backend"],
     "files": [{"path": "app.conf", "from": "app.conf"}],
     "argv": ["{artifact.app}", "serve", "--listen", "127.0.0.1:0",
              "--config", "{proc.app.dir}/app.conf"],
     "env": {"PATH": "/usr/bin:/bin", "HOME": "{proc.app.dir}"},
     "ready": [{"stream": "stdout", "prefix": "listening on ", "capture": "address"},
               {"stream": "stdout", "prefix": "sign in: ", "capture": "link"}],
     "memory_bytes": null}
  ],
  "actions": [
    {"name": "sign-in",
     "argv": ["{artifact.app}", "client", "{dir.client}", "sign-in", "-"],
     "env": {"PATH": "/usr/bin:/bin"},
     "stdin": "{proc.app.ready.link}\n"}
  ],
  "driver": {
    "charter": "charter.md",
    "tools": [
      {"name": "client",
       "argv": ["{artifact.app}", "client", "{dir.client}"],
       "env": {"PATH": "/usr/bin:/bin"},
       "writes": ["{dir.client}"], "reads": []}
    ]
  },
  "faults": [],
  "planted": [],
  "collect": [
    {"name": "observations",
     "argv": ["{artifact.fake}", "observations", "--from", "{proc.backend.ready.address}",
              "--out", "{collect.dir}/observations.jsonl"],
     "env": {"PATH": "/usr/bin:/bin"}, "scan": []}
  ],
  "checks": [
    {"name": "end-state",
     "argv": ["{experiment}/checks/end-state", "{dir.client}",
              "{collected.observations}/observations.jsonl"],
     "env": {"PATH": "/usr/bin:/bin"}}
  ]
}
```

`app.conf` is a template too, and may name `{proc.backend.ready.address}`.

- **`name`** matches `[a-z][a-z0-9-]*`, as every name in the file does,
  and names the experiment's directory under `keys/`.
- **`cost`** is a cost class of the lab's settings (6.5).
- **`dirs`:** the run's directories, `{dir.D}`, created empty, `0700`.
- **Members** are required, but `after` and `files` in a process, and
  `stdin` in a command, which are empty when absent, `bootstrap` in a
  process, none when absent, `cluster` in a
  collection or a check, which is `"up"` when absent (4.5), and `await`
  in a seed action, none when absent (4.5), `held` and `release`, empty
  when absent (4.8), and `meter`, null when absent (6.5).
  `driver` may be null: an experiment with no agent. An unknown member
  is refused.

### 4.2 Templates

Every string of `argv`, of `env`'s values and of `stdin`, and every file
named by `files`, is a template: `{name}` is replaced by its value, and
`{{` and `}}` are braces.

| Name | Value | Known in |
|---|---|---|
| `{experiment}` | the experiment's tree in the store | everything |
| `{runner}` | the runner's own directory, `scripts/lab/` as it runs, as `{runner}/look/capture` | everything |
| `{browser.program}`, `{browser.chromium}`, `{browser.fonts}` | the settings' browser: the program, Chromium and the fonts directory, by path (2.3) | everything |
| `{program.N}` | the settings' named program N, by path (2.3) | everything |
| `{port.N}` | the settings' kept port N, in decimal (2.3, 4.4) | everything |
| `{held.N}` | held file N's place in the run (4.8) | seed actions and the release, in `argv` alone |
| `{artifact.A}` | the subject's artifact A | everything |
| `{source}` | the subject's source tree | everything |
| `{run.id}`, `{run.seed}` | the run's id and seed, the seed in decimal | everything |
| `{seed.L}` | the draw for label L, in decimal (5.4) | everything |
| `{dir.D}` | run directory D | everything |
| `{proc.P.dir}` | process P's scratch directory | everything |
| `{proc.P.ready.C}` | capture C of P's ready lines | processes after P, transitively; seed actions, tools, collection, checks |
| `{action.A.C}` | capture C of seed action A's awaited line (4.5) | the seed actions after A, collection, checks |
| `{collect.dir}` | this collection's directory | a collection |
| `{collected.N}` | collection N's directory | checks; a stopped collection's in stopped checks only (4.5) |

### 4.3 Processes

- **`argv`:** the program by absolute path, then its arguments; no
  shell, no lookup in `PATH`.
- **`env`:** the whole environment. Nothing is inherited from the
  runner's.
- **`files`:** each `{"path", "from"}` is the experiment's file `from`,
  rendered into the process's directory at `path`, `0600`, before it
  starts. Each `{"path", "tree"}` is the subject's artifact `tree`, an
  archive of a directory's tree, unpacked into the process's directory
  at `path` before it starts, read-only: its files `0400`, `0500` with
  the owner's execute bit, its directories `0500`. For a program that
  reads a directory the subject's build makes, such as a server's
  static files.
  - *The archive* is uncompressed POSIX USTAR, its members regular files
    and directories alone, its names relative, slash-separated, without
    `.` or `..` segments, none repeated, each file's directory listed
    before it. Owners, times and modes are not read but the owner's
    execute bit. A product packs it deterministically, so that its
    bytes, and the subject's digest, follow from the directory's
    contents alone.
  - *Not a template:* its bytes are unpacked as they are, never rendered.
  - *Checked at validation,* from the subject's artifact, which cannot
    change: an archive of another form, or a member of another kind or
    name, is refused (4.7).
- **`after`:** the processes that must be ready before it starts. The
  graph is acyclic.
- **`ready`:** one or more lines, each `{"stream": "stdout" | "stderr",
  "prefix": TEXT, "capture": NAME | null}`. The process is ready once
  each has been seen, in any order: the first line of its stream that
  starts with its prefix, the rest of the line, less its end, being the
  capture. Prefixes are literal, so readers in any language agree.
  Readiness is by lines only: a port that accepts says nothing of what
  serves it, and a port the process chose is on its line anyway.
- **`memory_bytes`:** the bound the program's own configuration states,
  an integer, or null for a program that declares none, whose peak is
  recorded and not judged. The lab adds no bound of its own.
- **Its streams:** stdin is `/dev/null`; stdout and stderr are files of
  the run's evidence, read by the runner as they grow, except for the
  private bootstrap channel below.
- **Private bootstrap,** optional `bootstrap`,
  `{"stream": "stdout", "action": NAME}`: the whole named
  stream is private input for exactly one trusted seed action. The
  action must exist, no other process may name it as its recipient,
  and it declares neither `stdin` nor `await`. No ready rule or
  action's awaited rule may read the private stream. It creates no
  capture or template value.
  - The stream carries exactly one nonempty UTF-8 line terminated by
    LF, at most 4096 bytes including LF. A line holding CR or NUL is
    malformed. This framing bound fits an empty Linux pipe; the runner
    never holds bytes beyond it. A partial line at EOF, malformed
    line, overlong line or any output after its terminating LF fails
    `bootstrap/PROCESS/REASON`, where REASON is `partial`, `malformed`,
    `long` or `extra`. It cuts the phase as a process's stop does
    (5.1), without private bytes or an evidence file in the failure.
  - Before the recipient starts, the host writes the line, LF
    included, into a new pipe while holding its read end, then closes
    the write end and hands the read end to the recipient as stdin.
    The recipient reads the line and then EOF. The empty pipe holds
    the whole framed line, so the write cannot wait on its reader.
  - The process becomes ready only when both its public ready lines
    and its private line have arrived, in either order, within its
    one `ready_seconds` from spawn. No new deadline is added.
  - No byte of the private stream is written to a file, hashed or
    given to the run's life, a capture, a timeline, the driver or the
    triager. The recipient's stdout and stderr are private too: an
    authentication client may echo its input. Its stdin record and
    its two output records are null, and none has a retained file.
    Its command record has `bootstrap: {"process": P}` to identify
    the source of this private input.
  - Its own process record gains `bootstrap`, with `stream`, `action`,
    `received_ms` and `delivered_ms`; the latter two are null until
    observed; delivery records the handoff of the prepared pipe at
    recipient spawn and remains null if it never starts. Timeline
    events `bootstrap_received` and
    `bootstrap_delivered` name only the process and action. The source
    process's private output record is null, with no output file.
  - The host owns the bytes outside the serializable run state. It
    drops them after delivery or on cut, error or interruption; it
    keeps draining every private stream without persisting its bytes.
    The recipient's nonzero exit still fails as a seed action (7.1).
    The runner checks the line's framing, never its content. A trusted
    client may store its resulting session in a declared run directory;
    that product-owned credential is private scratch, never transcript
    or log evidence.
  - Without `bootstrap`, streams, readiness, stdin and records follow
    their ordinary contracts. The private channel is a deliberate
    exception to output retention: authentication bytes must not
    enter evidence, even when a client repeats them in a diagnostic.

### 4.4 Addresses

- **Reported:** a process that listens binds port 0 itself, on the
  address its arguments name, and prints the address it got on a ready
  line, captured as `{proc.P.ready.C}` for what comes after it (4.2).
- **Nothing else.** The runner never chooses, binds, releases or passes
  a port, so no bind races its release, and an experiment cannot ask for
  a port number. A program that cannot report its address has no place
  in a cluster until it can, with one exception:
- **A kept port.** A program whose port is configured, not reported,
  such as a forge's server, binds a port its machine keeps for it, named
  in the lab's settings and by `{port.N}` (2.3): one outside the
  machine's ephemeral range, so that no port-0 bind takes it, used by
  one run at a time, under the lab's lock (3.2). A run that finds it
  taken fails as its process does.

### 4.5 Commands: seed actions, collection and checks

A command is `{"name", "argv", "env", "stdin"}`, and a collection or a
check has `cluster` too; it is run once, contained as a process is
(5.2), within `command_seconds`; its stdin is the rendered `stdin`, kept
as a file of the run's, or `/dev/null`, and its stdout and stderr are
kept. A private bootstrap recipient (4.3) instead receives its memory-only
pipe, and none of its streams is persisted.

- **Seed actions,** `actions`, in their order, once every process is
  ready: the state a story starts from (an account, a key, something
  adopted). Each must exit 0 (7.1).
- **An action's awaited line,** `await`, `{"process", "stream",
  "prefix", "capture"}`: a line a process prints because of the action,
  such as a fresh sign-in link a server prints when a spent one is
  presented. The first line of that process's stream that starts with
  the prefix and ends after the action started is captured as a ready
  line is (4.3), the rest of the line, less its end, being `{action.A.C}`
  for the actions after it, the collection and the checks (4.2). The
  action is done once its own process has ended and the line has been
  seen, in either order, both within its `command_seconds`; past them, it
  fails `bound/command_seconds/<action>` (7.1), and a template naming
  the capture leaves the commands that use it unavailable, as an
  unready process's does (5.1). An action that exits non-zero fails as
  any does, and its line is not awaited. The process's earlier lines,
  its ready lines among them, are never matched, and the line stays in
  its output's file whole. Only seed actions await: what a collection
  or a check needs to see printed it reads from the process's files.
- **Live or stopped,** by a collection's or a check's `cluster`. `"up"`,
  its default, runs it live, in phase 5, the cluster still up: for what
  needs a process alive to ask (a fake's observations, from its
  address). `"stopped"` runs it in phase 6, once every process's exit is
  confirmed: for what reads a process's files at rest, no process being
  left to ask (5.1). An exit is confirmed when the runner has reaped the
  process it started and that process's tree is empty (5.2).
- **Collection,** `collect`, in its order: the live collections once the
  drive has ended, the stopped ones after the stop. Each writes what the
  checks and findings need into `{collect.dir}`, made empty, `0700`,
  before it starts, which is evidence. `scan` lists the files in it that
  are logs, which the panic check reads too (7.1). A collection that
  exits non-zero is recorded; the checks that need a live one's files
  say they could not judge, and those that name a stopped one are not
  run (below).
- **Checks,** `checks`, in their order, by the contract of 7.2: the live
  ones after the live collections, the stopped ones after the stopped
  collections. A stopped check may name any collection, a live one only
  a live collection (4.2).
- **A mutable store is copied stopped.** A live file-by-file copy can
  hold part of a commit and still open without refusing it. A check
  that needs the store reads no live copy. A stopped collection copies
  it once, after its writer's exit is confirmed:
  `/bin/cp -R -- SOURCE/. {collect.dir}/.`, SOURCE being the directory
  under `{proc.P.dir}` or a `{dir.D}` that holds them. However a process
  ended (at its `SIGTERM`, killed past `stop_seconds`, by a fault or by
  itself), its files are copied alike, and a kill or an early exit fails
  as 7.1 says, whatever the copy's checks find.
- **A stopped collection is usable only whole,** having exited 0: one
  cut short holds part of what it read, which a check cannot tell from
  the whole, and it is not run again. A stopped check is not run if a
  stopped collection it names did not run or did not exit 0, if an exit
  is not confirmed, or if the run is interrupted (5.1). A stopped check
  not run is recorded as could not judge, in `undecided` (5.6), its
  reason saying why.
- **What is compared with a copy is collected no later than it:** a
  peer's account of the run, such as a fake's observations, by a live
  collection, or by a stopped one listed before the copy, so that all
  it records happened before the files were copied.
- **No survivors:** once a command's own process ends, whatever is left
  of its tree is killed and reported (7.1).

The application's store copied at rest, with a check that reads it
beside the backend's observations collected live, extends 4.1's example:

```json
{"collect": [
   {"name": "app-state", "cluster": "stopped",
    "argv": ["/bin/cp", "-R", "--", "{proc.app.dir}/state/.", "{collect.dir}/."],
    "env": {"PATH": "/usr/bin:/bin"}, "scan": []}],
 "checks": [
   {"name": "state", "cluster": "stopped",
    "argv": ["{experiment}/checks/state", "{collected.app-state}",
             "{collected.observations}/observations.jsonl"],
    "env": {"PATH": "/usr/bin:/bin"}}]}
```

Life phases `stopped-collect` and `stopped-check` follow `stop`; their
commands' records name `cluster: "stopped"`, which live ones omit. A stopped
command not run is recorded with `group`, `name`, `cluster` and `skipped`,
and in `undecided` as `<group>/<name>`. A collection is whole by its record:
exit 0, not cut, killed or past its bound; never by its directory, which
provision makes.

### 4.6 Faults and planted failures

- **`faults`** are the runner's own, drawn from the seed and recorded:
  `{"kill": P, "signal": NAME, "after_actions": [LOW, HIGH]}` sends the
  signal to process P once the driver's action N (6.5) has ended, N
  drawn in `[LOW, HIGH]` (5.4). Faults need a driver. A fault that never
  fires leaves the run incomplete (5.5).
- **A fake's faults** (a delay, a refused call, a broken promise) are the
  fake's, configured by the experiment's templates and drawn from the
  seed the experiment hands it.
- **`planted`** lists the signatures the experiment's faults are meant
  to produce (9.4). A judged run that lacks one fails with
  `planted-missing/<signature>`. A planted failure is a failure like any
  other.
- **Changing a fault changes the experiment:** with the fault and
  without it are two experiments and two keys.

### 4.7 Refusals

`lab validate` and `lab run` check the following before any of the
experiment's processes starts. They refuse with every problem found, each
naming its member and why, and exit 2, starting nothing and making no
run:

- the format; a missing or unknown member; a wrong type; a name that
  repeats or is not `[a-z][a-z0-9-]*`; a `cluster` neither `"up"` nor
  `"stopped"`;
- an `after`, a fault or a template naming no process, directory,
  artifact or collection; a cycle; a template name not yet known where
  it is used (4.2), a stopped collection's in a live check among them;
  a capture no ready line or awaited line declares; an `await` naming
  no process, its stream neither `stdout` nor `stderr`, its prefix
  holding a line's end, or its capture's name not `[a-z][a-z0-9-]*`;
- a `bootstrap` whose stream is not `stdout`, naming no seed action or
  a recipient another process names, or whose recipient declares
  `stdin` or `await`; any ready or awaited rule on a private stream;
- a `from` missing from the experiment; a `tree` naming no artifact of
  the subject, or one that is not an archive of 4.3's form; a `path`
  absolute or climbing out, or one beneath or above another `path` of
  the same process; a program that is not an absolute path to an
  executable regular file;
- a subject missing, pruned or lacking an artifact the experiment names,
  or without a source when `{source}` is used;
- a cost class missing from the settings, or a value of it missing or
  not a positive integer; the driver's or the triager's settings missing
  when needed; for an experiment that names the browser by a template,
  and for it alone, the browser's settings missing, or its Chromium or
  fonts differing from them (2.3);
- a `{port.N}` the settings do not name;
- a `{program.N}` the settings do not name, or whose file differs from
  them (2.3); a `held` name the lab does not hold, naming the run that
  has taken it, or a `{held.N}` naming a file the experiment does not
  hold (4.8); a `meter` whose class lacks `subject_spend` or
  `meter_seconds` (6.5);
- by `lab run` alone, the brake's refusal, its line named (6.5);
- a seed that is not a canonical decimal string (2.2); a seed new to a
  key that already has three (section 8);
- the lab: not private, its lock held, the disk below its floor, no
  containment (5.2), no bubblewrap, or no driver host for an experiment
  with a driver.

### 4.8 Held files and the release

A held file is a secret one holder may have at a time, such as an
account's signed-in record whose refresh token rotates, where a copy
that refreshes signs the other out. The lab keeps it between runs; a
run takes it, its seed moves it into the cluster, and its release moves
it out and gives it back. It is never copied.

- **Kept:** `held/<name>`, the user's own, `0600`, not a link, put there
  by its owner with `lab held put NAME FILE` (10.2), which moves FILE
  in (renamed, or copied then removed across file systems), refuses a
  name already held, and settles one taken by a run that did not give
  it back, removing its `.taken`: the owner's word that FILE is the one
  copy that holds.
- **Taken:** an experiment names the held files it needs in `held`. At
  provision, before any process starts, the run renames each into its
  own `held/`, `{held.N}`, and writes the lab's `held/<name>.taken`,
  `{"format": "lab-held/1", "name", "run", "taken"}`. A name not held
  refuses the run (4.7), naming the run its `.taken` names.
- **Moved by the product's commands:** `{held.N}` is known in the seed
  actions and the release alone, in their `argv`; a product's command
  moves the record into the cluster, removing the file, and out again,
  writing it new. The runner never reads it, and neither a record, an
  event nor a kept output holds it: no `stdin` names it.
- **The release,** `release`, commands as seed actions are (4.5),
  without `await`: run in order once the live checks are done, the
  cluster up, however the drive ended, by its report, a bound, a
  process's stop or an interruption; not once containment has failed,
  nor when no process is left. Each is bounded by `command_seconds`; one
  that fails is a failure, `release/<name>/exit:<status>`, and the next
  still runs.
- **Given back:** after the release, each held file found at
  `{held.N}`, a regular file, the user's own, `0600`, not a link, is
  renamed back to `held/<name>`, and its `.taken` removed. One not found
  is a failure, `release/held:<name>`: the name stays taken, the run's
  scratch is kept whatever its outcome, and the runs that need it are
  refused until its owner puts it back, which removes the `.taken`.
- **After a crash** of the runner, the file is where the run left it,
  in the run's `held/` or in the cluster's kept state: the sweep (3.2)
  gives nothing back, since only the owner knows which copy holds.
- **Recorded:** `run.json`'s `held`, each `{"name", "taken_ms",
  "given_ms"}`, `given_ms` null when not given back.

## 5. A run

### 5.1 Phases

1. **Provision:** the run's directory and `live.json`, its cgroups
   (5.2), the scratch and `{dir.D}` directories, the files whose inputs
   are known, its held files taken (4.8), and, for an experiment with a
   driver, the boundary's probe (6.2).
2. **Start:** the processes in dependency order, one at a time (5.4),
   each ready within `ready_seconds` of its start. Its files are rendered
   immediately before it starts, once its dependencies' readiness
   captures are known (4.2, 4.3), and its trees unpacked.
3. **Seed:** the seed actions, each with its awaited line (4.5).
4. **Drive:** the driver, until its host ends, a bound passes (6.5) or a
   process stops, the meter reading meanwhile (6.5). Without a driver,
   nothing but the meter.
5. **Collect,** then **check** (4.5), then **release** (4.8), the
   cluster up: the live collections, the live checks, then the release
   and the held files given back.
6. **Stop:** the processes, in reverse order of start (5.2), until
   every exit is confirmed (4.5); then, the cluster down, the stopped
   collections, then the stopped checks.
7. **Judge:** the runner's checks and the commands' results into the
   outcome (5.5); `run.json`, the evidence line, the findings'
   candidates and occurrences (9.2); the scratch removed if it passed.

A process that stops (ends before phase 6) cuts the phase it ends in:
no later process starts, no later seed action runs, the driver is ended.
The live collections and checks run on what is left, then the release,
then phase 6 whole, its stopped commands included (4.5). A stop that
cannot confirm an exit is containment's failure, an error (5.5), and runs
no stopped command. A run interrupted (`SIGINT`, `SIGTERM` to the runner)
runs its release, bounded as ever, then goes to phase 6 and runs no
stopped command, ending any already running (5.2); its outcome is error,
`interrupted`. Any machinery error (spawn, runner
or containment) likewise prevents stopped commands and ends an active one.
A product or experiment failure that cuts the phase still permits them
once every started process's exit is confirmed.

### 5.2 Containment and teardown

- **A tree per process and per command,** in a cgroup of its own under
  the run's, entered before its program starts: what it starts stays in
  it, however it detaches (a new session, a double fork). A tree is
  empty when its cgroup's `populated` is 0; `cgroup.kill` ends all of it
  at once.
- **The run whole.** Each run's runner runs in a transient systemd user
  unit of its own, `lab-<run>.service`: started by `lab run`, which
  waits on it and passes it a `SIGINT` or `SIGTERM` it gets; delegated
  (`Delegate=yes`), so the runner makes its trees' cgroups beneath the
  unit's, its own process in a leaf beside them, with the `memory` and
  `pids` controllers enabled for them; killed whole when its main
  process ends, however it ends (`KillMode=control-group`); and
  collected once it ends. So after a `SIGKILL` or a crash of the runner
  nothing of the run is left, its cgroup included, and the next
  command's sweep records it (3.2).
- **The machine** needs cgroup v2 and a user manager that delegates
  `memory` and `pids` to a transient unit, bubblewrap, and unprivileged
  user namespaces. A machine without cgroup v2, a user manager or
  bubblewrap is refused (4.7); the controllers are checked as each run
  provisions, which makes its cgroups with them or ends in error.
- **The host is replaceable.** What the run's life asks of the machine
  is a short list (10.4): make a tree's cgroup, spawn into it, signal its
  first process, kill the tree, await it empty, read its counts, remove
  it, compose a view. That is the shape of skein's contained trees (io.md,
  6.2 to 6.4: a cgroup's `Make`, `Signal`, `Kill`, `Empty` and `Counts`;
  a `Spawn` into a cgroup, contained with a view), so they can take the
  host's part in the lab application without changing the run's life.
  Until then the host is systemd's units, cgroup v2's files and
  bubblewrap; the runner depends on nothing of skein's.
- **No survivors.** Once the process the runner started has ended, at
  any time and however, what is left of its tree is killed at once and
  reported as its survivors (7.1): a process owns its children until
  they end.
- **Stopping a process:** `SIGTERM` to the process the experiment
  started, never to its group, since its own stop is under test; then
  at most `stop_seconds` for it to end; past them, `cgroup.kill` of its
  tree, recorded as killed (7.1). In reverse order of start, after a
  drive, a failure, a cut or an interruption alike.
- **Commands, the driver host and each tool call** are contained the
  same way (6.3).

### 5.3 What the runner watches

- **Exits, in every phase.** Every process the runner started is waited
  for; its exit's time, phase, status or signal, and whether the runner
  had stopped it, are recorded as it happens.
- **Output:** each process's and command's stdout and stderr, written to
  the run's files as they come; ready lines are read from them.
- **Memory:** a process's peak is its resident set's high-water mark,
  `VmHWM` in `/proc/<pid>/status`, read just before its stop signal,
  while it is the runner's unreaped child, so that the PID is its own;
  for a process that ended by itself, the maximum resident set its wait
  status reports. Its tree's peak, its cgroup's `memory.peak`, is
  recorded beside it and not judged. Under `/proc` the runner reads the
  `status` of the processes it started and its own PID namespace's link
  (6.2), nothing else, and never another process's environment.
- **Time** on the monotonic clock, in milliseconds from the run's start;
  wall-clock times only for its start and its end.

### 5.4 The seed

- **A run's seed** is an unsigned 64-bit integer, given with `--seed` or
  drawn from the system's random source, recorded either way, and
  written as its canonical decimal string everywhere: records,
  templates, arguments (2.2).
- **Draws:** the draw for label L is the first 8 bytes, read
  big-endian, of SHA-256 over `lab-seed`, the seed's decimal string and
  L, joined by zero bytes; an unsigned 64-bit integer, recorded as its
  decimal string. `{seed.L}` hands one to a process, as the seed its
  fake draws from; the runner's own labels are `start-order.<k>` and
  `fault.<index>`.
- **The start order:** the runner starts one process at a time, and
  waits for it to be ready before the next. The k-th start, from 0,
  takes from the processes whose dependencies are ready, sorted by
  name, the one at the draw `start-order.<k>` modulo their number. A
  fault's N is `LOW` plus the draw `fault.<index>` modulo
  `HIGH − LOW + 1`.
- **Recorded in full:** `run.json`'s `schedule` holds each draw by its
  label, the start order, and each fault's N and whether and when it
  fired.
- **What replays:** the seed fixes the lab's draws, and nothing else. A
  rerun of a seed draws the same order, the same seeds for the fakes (so
  they draw the same faults), and the same fault at the same action.
  The kernel's scheduling, timing and the model's output do not replay:
  a rerun is a new run of the key, and reproduction is measured, never
  assumed. How often a failing seed's reruns show its signature again is
  recorded with the finding and reported (9.3, 10.2). There is no
  replay: a failure that does not come back is filed flaky, and its
  transcript (6.3) is what a lower world's story, which replays its own
  seeds, is made from.

### 5.5 Outcomes

| Outcome | When | Judged (section 8) | `lab run` exits |
|---|---|---|---|
| pass | every check passed, the driver, if present, reported done, no bound passed | yes | 0 |
| fail | a failure observed (section 7) | yes | 1 |
| incomplete | none observed, but a check could not judge, a fault did not fire, or a required report is missing or malformed though its host ended well | no | 3 |
| error | the lab's machinery failed: containment, the driver host (an exit but 0, a transport or account error), a check that broke its contract (7.2), the runner, an interruption | no | 4 |

- **Refused** is no outcome: no run is made (4.7), and `lab run` exits
  2. A run cut by a signal exits 128 plus its number.
- **A failure observed is never hidden:** fail, then error, then
  incomplete, then pass. A run records all its failures, errors and
  undecided checks, whatever its outcome.
- **The product's and the experiment's mistakes are observations:** a
  seed action that exits non-zero, a fake that exits at its start, an
  end state not reached each fail with a signature. Whether the fix is
  the product's or the experiment's is the finding's class (9.3), not
  the outcome.

### 5.6 The run's record

`runs/<run>/` holds `run.json`, `events.jsonl` (the timeline: phases,
starts, ready lines, awaited lines, exits, faults, cuts), `proc/<P>/stdout` and
`stderr`, `commands/<name>.{stdin,stdout,stderr}`, `collected/<N>/`,
`driver/` (6.3), `triage/<n>/` (9.2), and the scratch, `scratch/<P>/`
and `dirs/<D>/`.

`run.json` (`lab-run/1`):

- `run`, `key`, `experiment` (its name), `seed` (a decimal string),
  `attempt` (one more than the key's earlier runs of the seed),
  `campaign`, `started`, `ended` (UTC, RFC 3339, milliseconds),
  `outcome`;
- `schedule` (5.4);
- `processes`: each one's rendered argv and environment, `started_ms`,
  `ready_ms` and captures; its exit (`ms`, `phase`, `status` or
  `signal`, `stopped`, `killed`); its memory (`peak_bytes`, `source`,
  `bound_bytes`, `tree_peak_bytes`); its output's files. A process with
  private bootstrap has `bootstrap` with `stream`, `action`,
  `received_ms`, `delivered_ms` (4.3); its private output is null and
  has no file;
- `commands`: each seed action, collection and check, as a process,
  with its status and lines (7.2); a seed action that awaits a line,
  its `awaited`, `{"ms", "captures"}` once seen, as a process's
  captures, or null. A bootstrap recipient has `bootstrap` with
  `process`, null `stdin`, and null stdout/stderr outputs (4.3).
  Stopped commands name `cluster: "stopped"`; their skipped records
  and reasons follow 4.5. The timeline records `bootstrap_received`
  and `bootstrap_delivered` with the process and action only;
- `driver`: its host's invocation, the inventory it was checked
  against and what was found, each item observed or
  `unobserved` (6.1), its actions and observed turns, its end, the
  report's and the transcript's files (6.3, 6.4);
- `failures`, each `{"id", "signature", "kind", "summary", "evidence"}`;
  `undecided`, each `{"check", "reason"}` (a command left unavailable,
  5.1, as `<group>/<name>`: `actions/consume`); `errors`, each
  `{"what", "detail"}`;
- `held` (4.8); `bounds`, the class's name and the values the run was
  held to, and `usage` (6.5); `about`, the machine (2.3).

The evidence line (`lab-evidence/1`) is `{"run", "seed", "attempt",
"outcome", "signatures", "ended"}`, its seed a decimal string.

## 6. The driver

### 6.1 The host

A system layer whose configuration is empty counts as no configuration layer.

The driver's boundary is provisional: Codex's permission profile for
the agent's commands, and a stdio MCP server whose calls run in
bubblewrap views for its tools. What it rests on is checked every run
(the inventory below, the probe of 6.2), not assumed from the host's
documentation.

- **Codex,** `codex app-server --stdio`, one host and one ephemeral
  thread per run, started in the drive phase. The runner is its sole
  client over stdio; no shared daemon owns any part of the run. The
  lab's settings name its program, model and effort; the key holds its
  version and digest, and its configuration (2.3).
- **Its model catalog** may be pinned in the settings, as a file and its
  digest, and is then keyed: the runner refuses a missing file, a
  differing digest, or a catalog whose driver-model entry is not
  `tool_mode: null`, and names the file to the host
  (`model_catalog_json`). A catalog that marks the model code-mode-only
  leaves it one tool, `exec`, whose calls run nested tools in a
  JavaScript isolate; the code-mode host stays off, so `exec` fails
  closed, because calls inside it make no items: the audit and
  `driver_actions` would be blind to them. Admitting code mode is the
  user's call.
- **Its built-in surface** comes from the pinned catalog and the
  configuration, and is checked on them, since the host offers more
  than the command tool by default. The driver-model entry must also
  hold `multi_agent_version: null` (no collaboration tools),
  `experimental_supported_tools: []` (no clock and no asynchronous
  question), `apply_patch_tool_type: null` (no patch tool) and
  `supports_search_tool: false` (the lab's tools offered directly, not
  deferred behind a tool search the host does not serve: a call it
  does not serve makes no item); the configuration turns the question
  tool off. The command tool's input
  to a running command (`write_stdin`) stays, and its use is a command
  execution.
- **Three resource tools stay:** Codex adds `list_mcp_resources`,
  `list_mcp_resource_templates` and `read_mcp_resource` whenever an MCP
  server is configured, and no flag removes them; the lab's tools are
  one. The audit admits exactly these three (a call to server `codex`),
  each an action counted toward `driver_actions`. Their reach stays
  empty: before its turn, each run checks from its own inventory that
  `lab` is the only MCP server and declares no resources or templates,
  and errors with no turn otherwise. Any other built-in still ends the
  run in error, and so does any of the three returning content: a
  listing that is not empty, or a read that is not an error.
- **Every model call is seen:** the host makes no item for a call it
  does not serve, so the runner also reads the host's raw response
  items (`experimentalRawEvents`). Each raw call must be answered by
  an item with its call id, or be a `write_stdin` to a running command
  of the run (itself a counted action); any other call, or another raw
  call kind, is recorded as a counted `unhandled` action and ends the
  run in error. Actions with no raw call at all end it too, so a host
  that stops sending raw items fails closed.
- **Three records of configuration:** the settings select the program,
  model and effort. The runner's version adapter derives the host's
  configuration and expected inventory from those settings and the
  experiment, with run paths left as templates in the key. The
  inventory observed from the running host is evidence in the run's
  driver record (5.6), separate from the expected inventory. A
  configuration supplied in settings is not evidence of what was
  given to the host or what it offered the model.
- **Its prompt** is the runner's preamble (the tools, the boundary, the
  report's shape), then the experiment's charter, verbatim.
- **Its configuration is the run's alone,** and is in the key. The
  user's configuration and rules are ignored, and the run's is parsed
  strictly (`--strict-config`): approvals never asked; the permission profile of
  6.2 as the default, with no legacy sandbox setting beside it; no
  login shell and no inherited environment for the agent's commands; the
  experiment's tools as its one MCP server, `lab`, required, with
  exactly their names enabled and no environment passed (6.3); and every
  other source of tools or configuration disabled by name: web search,
  apps, plugins, browsers, computer use, image tools, other agents,
  hooks, skills' installs, as the runner's list for the host's version
  names them. The thread is ephemeral; the report is held to its
  schema. The runner records the host's events as JSON lines in
  `driver/events.jsonl`. The version adapter must establish how the
  host applies this configuration and suppresses every other layer;
  accepting a configuration does not establish that suppression.
- **The inventory is checked every run.** Configuration accepted is not
  configuration in effect, so the runner reads what the host offers the
  agent once started: its shell under the profile, the one server `lab`
  offering exactly the experiment's tools, no resources and no prompts,
  and no other server, plugin, app or configuration layer. A host whose
  inventory differs ends the run in error, the driver ended at once. How
  the inventory is read is the runner's, for each host version, as far
  as the version lets it be read (below).

  An item below with no observer for the host's version is recorded
  `unobserved`, never as absent, and does not refuse the version: the
  version adapter names its observers and the evidence they return, and
  the run's record names the items unobserved. An incomplete observation
  or a mismatch of an observed item ends the run in error. A list of
  configured capabilities, a rendered prompt, or the lab server's own
  traffic is not the host's live inventory.

  | Item | Required observation |
  |---|---|
  | permission profile | the run's profile is selected for the agent's commands |
  | MCP servers and tools | only `lab`, offering exactly the experiment's tools |
  | resources and prompts | none, including resource templates |
  | apps and plugins | none offering a capability to the model: an app listed both disabled and not callable, with apps turned off, offers none and is kept as evidence |
  | hooks and skills | none loaded or active |
  | configuration layers | only the run's configuration |
  | instruction files | none loaded from the user or the subject |
  | built-in tools | only the sandboxed command tool, under the run's profile, and the three resource tools, reaching nothing |

  Calls to the lab's tools are held until the observation is complete.
  Commands that start before it completes still count as actions; a
  failed inventory check stops the driver and every outstanding call.
  An action kind outside the admitted inventory ends the run in error.
  Detecting an unexpected action is an additional check; it does not
  establish that the action was confined.
- **One user, on machines kept for the lab.** The runner, the cluster,
  the host and its tools run as the lab's user, in the run's cgroups
  (5.2), which bound what they use and end them whole but confine no
  file. What confines is the profile, for the agent's commands (6.2),
  and a view for each tool call (6.3). The host's own process is the
  lab's user's, unconfined: a built-in tool that acts in it, unobserved,
  reaches what that user reaches, the lab, its held files (4.8) and the
  cluster's state among them. That is the lab's accepted risk, so it
  runs only on machines kept for it, never a person's computer, and the
  audit of the host's events ends a run at the first action of a kind
  outside the admitted inventory, after the fact.
- **The model's credential stays with the host.** Codex authenticates
  its model transport with the machine's Codex login under
  `CODEX_HOME`, the one credential in the host's reach; the profile
  grants no part of it to the agent's commands, and no tool's view or
  environment holds it (6.2, 6.3).

### 6.2 The boundary

What the agent's own commands may reach:

| | May | May not |
|---|---|---|
| read | the host's minimal roots (Codex's `:minimal`), the host's own executable, the subject's source tree, its work directory | the home directory, the host's login (`CODEX_HOME`), the lab (its store, the experiment and its checks, the runs), the runner's processes |
| write | its work directory | the source, the product's repository, the run's processes' files, the lab |
| run | what it may read, and what it writes | nothing is barred by its bytes' identity: the artifacts are out of its view, but what it builds from the source runs as anything does |
| reach | nothing on the network: a socket is refused as it is made | the cluster, but through the tools (6.3) |

- **The agent's commands** run in Codex's sandbox, under the run's
  permission profile: reads of the minimal roots, of the host's
  executable by its exact resolved path (without which the sandbox
  cannot start it), of the source, and of the work directory; writes
  only to the work directory; the network disabled.
- **Every agent action is confined, or recorded as not.** A host's
  built-in tool may act in the host's process rather than start a
  sandboxed command, and a command-profile probe says nothing about that
  tool's file or network access. It is confined where the inventory
  shows it absent, or a probe of it establishes the same boundary.
  Where neither can be shown, it is `unobserved` (6.1): should the model
  have it, it acts as the lab's user, and its first action ends the run
  in error.
- **The minimal roots are reviewed, not assumed safe.** They are what
  Codex's sandboxed programs need to start, and their list is Codex's:
  the lab's owner reviews it for each host version, whose change is a new
  key. The boundary claims nothing of a system directory beyond that
  review, `/etc` included: what the roots hold is the machine's, and the
  probe proves only the paths it names.
- **The tools run outside it,** in the runner's MCP server (6.3), each
  call in a bubblewrap view of its own: the system directories its
  program needs, read-only; the tool's program; the directories the tool
  declares (`writes`, `reads`); the work directory read-only (for a file
  an argument names); a new PID namespace with its own `/proc`; an empty
  `/tmp`; the environment the tool declares and nothing else; and the
  host's network, shared. So an argument naming a file reaches only what
  the view holds.
- **A tool is trusted, and its endpoint is its own.** Sharing the
  network (bubblewrap's `--share-net`) shares all of the host's, not one
  address: no view isolates an endpoint. A tool is a program of the
  subject's or the experiment's whose fixed arguments and files name the
  endpoint it reaches, and which refuses an argument of the agent's that
  would name another; that validation is the tool's own, and an
  experiment names no tool without it.
- **The source** leaves out the paths the subject excluded, the
  product's experiments among them, so the agent never reads its checks.
- **The probe:** when a run is provisioned, before any process starts,
  and before each triage, the runner runs fixed commands through the
  same profile with `codex sandbox`, no model, and `CODEX_HOME` an
  inert, empty directory of the run's, so that the probe reaches no
  login. They read `canary` (in the lab's root: it stands for the store,
  the experiment and the runs); list the home directory; write into the
  source; open a socket to a listener the runner holds on loopback; and
  look for the runner's processes by identity, never by a PID's number,
  which another namespace reuses: the probe's PID namespace
  (`/proc/self/ns/pid`) must not be the runner's, and a canary process
  the runner starts, named by a token in its arguments, must not appear
  in the probe's `/proc`. Each must be refused or absent, and a write in
  the work directory must succeed; otherwise the run ends in error
  before any process starts, having spent nothing. The home's listing
  counts as absent when every entry it returns is the first path
  component, under the home, of one of the profile's own roots: the
  sandbox mounts the directories leading to them, and nothing else of
  the home. Any other entry fails it.
- **The preamble says the same** for the agent's sake; the boundary does
  not rest on it.

### 6.3 Tools and the transcript

- **A tool** is `{"name", "argv", "env", "writes", "reads"}`: `argv` is a
  prefix (the program and its fixed arguments, a client directory among
  them) to which the agent's arguments are appended.
- **One MCP server,** `lab tool`, which Codex starts over stdio as its
  only server, with no environment of the runner's, offers one MCP tool
  per experiment tool and no resources or prompts: a tool's input
  `{"args": [...], "stdin": TEXT | null}`, its result the call's status
  or signal, its stdout and its stderr, as text.
- **An action** is one command the agent runs in its sandbox, or one
  tool call (6.5). Codex's own turns are not actions. The runner gives
  each action a number in the order its loop receives starts from the
  host and the tool server together; a tool call observed in both is
  one action. It records the start and its completion against that
  number. A fault waits for the completion of its selected action N
  (4.6), even when actions complete in a different order.
- **Every call is recorded** before its result reaches the agent:
  `driver/calls.jsonl` gets a `start` line (`n`, the tool, `args`, the
  stdin's file, `t_ms`) and an `end` line (`n`, `t_ms`, `status` or
  `signal`, and the stdout's and stderr's files with their sizes and
  digests) in `driver/calls/`. A call cut by the driver's end has a
  start and no end. The transcript is outside the agent's view.
- **The transcript,** the actions in order with their outputs (the
  calls from `calls.jsonl`, the agent's commands from the host's
  events), is the run's, a finding's reproduction, and the material of a
  lower world's story (5.4, 9.3).
- **Each call's tree** is contained as a command's (5.2): its survivors
  killed and reported.

### 6.4 The report

The host's final message, held to the runner's schema by Codex and again
by the runner:

```json
{"done": false,
 "retried": [{"step": "...", "tries": [{"tried": "...", "why": "..."}]}],
 "unseen": [{"step": "...", "looked_for": "..."}],
 "surprises": ["..."]}
```

- **`done`:** whether the charter's goal was reached, as the agent
  judges it.
- **`retried`:** each step that took more than one try: what it tried,
  and why.
- **`unseen`:** each step it could not see how to take.
- **`surprises`:** anything that surprised it.

`done: false` and each `unseen` are failures of kind use, whose
signatures the triager settles (9.2). `retried` and `surprises` are
evidence for the owner, not failures.

### 6.5 Bounds and usage

- **A cost class** of the lab's settings, named by the experiment:
  `ready_seconds`, `command_seconds`, `stop_seconds`, `driver_seconds`,
  `driver_actions` and `driver_input_tokens`, for an experiment with a
  meter `subject_spend` and `meter_seconds`, and the lab's triage class,
  `triage_seconds` and `triage_tokens`; each a positive integer.
  The class the experiment names is the one its runs are held to; a
  missing class or value refuses the run (4.7): there is no default.
  They are budgets and timeouts, decided as `owners.md` (3) says, kept
  in the lab's settings and named in the product's binding; never the
  runner's, and no number of theirs is in this document.
- **Tuned, not keyed.** The key holds the class's name (2.3); each run
  records the values it was held to, in `bounds` (5.6), and is judged
  against them. A value may change between runs of a key, as use
  teaches what a story needs: a run failed only by bounds since raised
  above its own is superseded (section 8). A class whose purpose
  changes, as a calibration's ends, is a class of another name.
- **What each counts.** `driver_actions`: the agent's actions (6.3),
  its commands and its tool calls together. `driver_input_tokens`: the
  input tokens the host reports, cached ones included.
  `triage_tokens`: the triage's tokens, input (cached included) and
  output together. Codex's turns are observed and recorded, never
  bounded: a turn may hold many actions, or be the whole drive.
- **A run's longest** follows from the class and the experiment (each
  process's readiness and stop, each command, the drive) and is
  recorded; it is no bound of its own.
- **Time and actions are enforced as they pass:** time on the runner's
  clock, actions from the host's events and the tool server's as each
  starts. Past either, the driver is stopped as a process is (5.2), no
  report is awaited, and the run fails `bound/driver_seconds` or
  `bound/driver_actions`. The action that passes the bound may have
  begun by the time its event is read: it is recorded as such.
- **Tokens are observed, not capped.** The host reports usage when a
  turn completes, so the runner knows tokens per completed turn, and
  only then. Once the observed input tokens pass the bound, the runner
  stops the driver, if it still runs, and the run fails
  `bound/driver_input_tokens`; the overshoot, at most the turn that
  reported it, is recorded. Nothing promises that no token is spent past
  a bound: while a drive runs, its time and its actions are what bound
  it. A triage's time and tokens are bounded the same way.
- **A subject's spend, metered.** A subject may spend on a metered
  account of its own, such as an LLM's that its agents use beside the
  driver, which the driver's bounds do not see. Its experiment names a
  `meter`, a command `{"name", "argv", "env"}` contained as any (4.5),
  run every `meter_seconds` of the drive, and once more after it,
  before the live collections, its last reading: its stdout's last
  line is the subject's spend so far, a non-negative integer in the
  unit the product's binding names. Past `subject_spend`, the driver is stopped
  as past its time, and the run fails `bound/subject_spend`. A meter
  that fails twice running, or prints no such line, ends the drive too,
  and fails the run `meter/exit:<status>` or `meter/reading`: no subject
  spends unmetered. The meter is the outer bound; the product's own
  budgets, set by the experiment, are its inner one.
- **The brake.** An account shared beyond the lab, such as the one the
  driver's host signs in with and its owner develops with, has limits
  the lab must leave room in. The settings may name a `brake`, a
  command `{"argv", "env"}` that `lab run` runs before it makes any run
  whose experiment has a driver or a meter, within `command_seconds`:
  exit 0 starts the run; any other end refuses it (4.7), naming the
  brake's last line, and makes no run. The runner's own brake for its
  host asks the host for the account's windows, with no model turn, and
  refuses past the settings' share of each; and every brake refuses
  while the lab's `pause` exists. `lab judge` stops at a refused run
  and prints the verdict so far.
- **Usage, whatever the outcome:** `usage` holds the driver's seconds,
  actions, observed turns and tokens by kind (input with cached
  included, cached input, output, as the host reports them), each
  triage's (9.2), the subject's last metered spend, and the account's
  windows as the host last reported them. The driver's events are read
  until its end, so usage it reports as it stops is kept. A value the
  host did not report, as when it was ended within a turn, is null:
  unknown, never 0. A campaign's totals add what is known and count what
  is not.

## 7. Checks and signatures

### 7.1 The runner's own

| Check | Fails on | Signature |
|---|---|---|
| stops | a process that ended before the runner stopped it | `stop/<process>/exit:<status>` or `stop/<process>/signal:<name>` |
| panics | a Rust panic's header, `thread '…' panicked at FILE:LINE:COL:`, in the stderr of a process, a command or a tool call, or in a collected log | `panic/<who>/<FILE>:<LINE>:<COL>`, the message in the summary |
| survivors | a process, command or call whose tree outlived it | `survivor/<who>` |
| memory | a peak past `memory_bytes` | `memory/<process>` |
| bounds | a readiness, a command, a stop, the driver's time, actions or input tokens, or a subject's spend past its bound | `bound/<name>/<who>` or `bound/<name>` |
| seed actions | an action that exited non-zero | `action/<name>/exit:<status>` |
| release | a release action that exited non-zero; a held file not given back (4.8) | `release/<name>/exit:<status>`, `release/held:<name>` |
| meter | a meter that failed twice running, or printed no reading (6.5) | `meter/exit:<status>`, `meter/reading` |
| use | the report's `done: false`, each `unseen` | the triager's, `use/…` |
| planted | a planted signature missing from a judged run | `planted-missing/<signature>` |

- **A stop whose stderr holds a panic** is reported once, as the panic.
- **No early exit is ignored,** one the runner's own fault caused
  included (9.4).
- **Private bootstrap streams** (4.3) are excluded from panic-text
  inspection, including the recipient's diagnostics: their bytes never
  become evidence. A recipient that panics still fails by its nonzero
  seed-action exit. The source's public streams retain the panic check.

### 7.2 A check command

- **Its status:** 0, passed; 1, failed; 2, could not judge (its inputs
  missing or unreadable).
- **Its stdout** is JSON lines, each `{"fail": SIGNATURE, "summary":
  TEXT}` or `{"undecided": REASON}`: none with 0; at least one `fail`
  with 1; at least one `undecided` and no `fail` with 2. Its stderr is
  kept.
- **Anything else** (another status, a signal, a line of another shape,
  a status its lines contradict) is the check's error, never a pass.
- **Past `command_seconds`** it is ended, and fails
  `bound/command_seconds/<check>`.
- **Its signatures are its own, and stable:** they name what failed and
  its subject, never an id, an address or a time a run draws. The run
  records each as `check/<check>/<signature>`.
- **Commands, not a vocabulary:** a story's end state and a fake's
  promises are read in the product's and the fake's own terms; a
  vocabulary of the runner's would have to grow those terms or a
  language. What is generic is built in (7.1).

### 7.3 Signatures

- **Opaque strings, compared for equality:** printable ASCII without
  spaces, in `/`-separated segments for the reader.
- **A cause, not an occurrence:** nothing a run draws (an id, an
  address, a time, a seed); a panic by its place, not its message; a
  check by what failed.
- **One per failure,** on which findings are matched (9.2).

## 8. Verdicts

- **The policy,** `three-seeds`. A run is judged when its outcome is
  pass or fail. A key has exactly three seeds, the distinct seeds of its
  evidence in the order of their first runs; a run under a fourth is
  refused (4.7). A key's verdict is:
  - *not enough evidence* until each of its three seeds has a judged
    run, however the judged runs so far disagree;
  - then *pass* when every judged run passed, *fail* when every one
    failed, and *flaky* when any two disagree, a seed's own runs
    included.
- **Superseded bounds:** a run whose failures are all bounds
  (`bound/<name>`), each a value of the class's that is now above the
  one the run was held to, is no longer judged: the verdict leaves it
  out as it leaves out an error, and its seed may be attempted again.
  So a class raised as use teaches keeps its keys' evidence (6.5).
- **`verdict.json`** (`lab-verdict/1`): the key, the policy, each seed's
  state, the runs judged and the runs not, and the verdict. It is derived
  from `evidence.jsonl`, and from the class's values for superseded
  bounds, whenever either changes.
- **`lab judge`** runs a key's seeds one run at a time, drawing a new
  seed while the key has fewer than three, until each has a judged run;
  a key whose evidence already decides it runs nothing. It stops at the
  first run that is not judged (error or incomplete), running no other,
  and a seed whose runs are none of them judged is left to the owner:
  `judge` names it and runs nothing until it is attempted again.
- **Attempts add, never replace:** a seed whose run ended in error or
  incomplete may be run again under the same seed: a new run, its
  `attempt` one more, beside the earlier one, which stays in the
  evidence, not judged. Nothing attempts again on its own.
- **Reruns** of a seed under the same immutable inputs add to the
  evidence too, and may change the verdict: a pass or a fail becomes
  flaky when a later run disagrees, and flaky stays flaky.
- **A new subject or experiment is a new key,** with its own evidence; a
  verdict never borrows another key's.

## 9. Findings

### 9.1 The record

`findings/<id>.jsonl` (`lab-finding/1`), ids from `F-0001`: one event per
line, each with `t` (UTC) and `by` (`runner`, `triager`, or the owner or
person who acted).

| Event | Holds |
|---|---|
| `candidate` | its signature, kind and summary; the failure: run, key, seed |
| `occurred` | another failure with its signature: run, key, seed |
| `proposed` | the triager's class, title and reason (9.2) |
| `filed` | the verdict it was filed on, `flaky`, and its seed's rerun, failed alike or not |
| `classed` | a class, confirmed by the owner |
| `taken` | by whom |
| `fixed` | the fix's evidence, by class (9.3) |
| `rechecked` | the recheck's key and verdict, and that no judged run showed the signature |
| `closed` | |
| `reopened` | the failure that reopened it |

A finding's state is the fold of its events. `index.json` lists each
finding's id, signature, class, state, title and runs, derived.

### 9.2 Matching and the triager

- **Mechanically,** when a run is judged: a failure whose signature is a
  finding's is an `occurred` on it; any other, but a use failure, is a
  new `candidate`.
- **The triager** is Codex, its host as the driver's, bounded by the
  triage class; `lab triage RUN` runs it on the run's failures no
  signature matched and on its use failures. Its commands may read the
  host's minimal roots and the run's evidence (never its scratch), and
  write only in `triage/<n>/work/`, with no network, under a profile
  probed as the driver's is (6.2); it has no tools, which its inventory
  is checked for (6.1); its output is held to its schema.
- **Its input,** on stdin:

  ```json
  {"run": "...", "key": "...", "seed": "...",
   "failures": [{"id": "...", "signature": "...", "kind": "...",
                 "summary": "...", "evidence": ["..."]}],
   "report": {"done": false, "retried": [], "unseen": [], "surprises": []},
   "findings": [{"id": "F-0001", "signature": "...", "class": "...",
                 "state": "...", "title": "..."}],
   "classes": ["world", "fake", "below", "tools", "experiment", "lab"]}
  ```

- **Its output:**

  ```json
  {"failures": [{"id": "...", "match": "F-0001", "signature": "use/...",
                 "class": "tools", "title": "...", "reason": "..."}]}
  ```

  `match` is a finding's id or null; `signature` is its proposal for a
  use failure that matches none. The runner records each as an
  `occurred`, or as a `candidate`, with its `proposed`. The triage's
  files and usage are the run's, in `triage/<n>/`.
- **The owner confirms a class** (`lab finding ID class C`); the triager
  only proposes.

### 9.3 Lifecycle

```
candidate ─filed─> open ─taken─> taken ─fixed─> fixed ─rechecked─> closed
                    ^                                                │
                    └───────────────────── reopened ─────────────────┘
```

- **Filed** (`lab finding ID file`, refused until then) once its key's
  verdict is decided (fail or flaky, section 8) and its first failing
  seed has been run again: `flaky` when not every judged run of the key
  showed its signature, and the rerun's result recorded, reproduced or
  not. Every rerun of a failing seed counts toward its reproduction
  rate, the reruns that showed the signature again out of all of them,
  kept in the finding's index and reported by the campaign (10.2).
  Filing needs a class, proposed or confirmed.
- **Classes,** by where the fix goes: `world`, a world lacks a story;
  `fake`, a fake differs from its system; `below`, a bug below the
  domain (protocol, io, the binary); `tools`, the driver's tools or
  their guide; `experiment`, the experiment itself; `lab`, the runner,
  or the driver's or the triager's setup; `look`, a page's appearance,
  which no world below a browser draws (lab-browser.md); `words`, the
  text a product's own model reads (its instructions, its tools'
  declarations, its refusals), whose fix a world can hold to its text
  but only a real model's run rechecks.
- **Fixed** needs a confirmed class and the fix's evidence. For `world`,
  `fake`, `below` and `tools`: the repository, the commit and the names
  of the tests that fail before it and pass after, in the lowest world
  that owns the behaviour, never in the lab (a world's story, a pinned
  seed, a fake's correction, a protocol test, a transcript made a
  story), and the product's commit that carries it. For `experiment`:
  the corrected experiment's digest. For `lab`: the runner's commit and
  its own tests. For `look`: the repository and commit of the
  stylesheet's fix, and the look experiment's expectation that fails
  before it and passes after, since no world below a browser renders a
  page; the product's commit that carries it. For `words`, as for `world`: the
  test that holds the text, the repository and commit of its fix, and
  the product's commit that carries it.
- **Closed** only after a recheck: the experiment (the corrected one,
  for `experiment`) at a subject that carries the fix, so under a new
  key, judged pass, with no judged run showing the signature. `lab
  finding ID close --recheck KEY` refuses anything else.
- **Reopened** when its signature fails again in a run that carries the
  fix: at a subject whose commit descends from the fix's product commit,
  or of the corrected experiment. Elsewhere (a subject made before the
  fix, the uncorrected experiment) it is an `occurred`, and the finding
  stays closed; for `lab`, the owner judges.
- **Fixes land as any change does** (`owners.md`, 4.4). The lab never
  writes a product's repository.
- **Findings are the lab's files,** under its directory, reported by the
  lab's owner as `owners.md` (5) routes them. The lab writes no issue on
  a forge: a
  lab application that holds findings as its state may project them as
  issues, never reading them back.

### 9.4 Planted faults

- **Detection, with the fault on:** experiment P carries the fault (the
  runner's, or a fake configured to break a promise) and its `planted`
  signatures. Its runs fail with them; the candidate is filed, classed
  `experiment`, and reproduced from its key and seed.
- **Correction, with the fault off:** P′ is P without the fault, a new
  digest and a new key. The finding's fix is P′'s digest; its recheck,
  P′'s verdict, pass.
- **Detection stays:** P is kept and run again; its failures are
  `occurred` on the closed finding, which only P′ reopens. A judged run
  of P without a planted signature fails `planted-missing`: detection
  lost is a failure, not a silence.
- **No ignore:** nothing in the runner discounts an exit, a panic or a
  check because a fault was planted.

## 10. The runner

### 10.1 Language and place

- **Python 3.11 or later, its standard library only, for now,** by the
  user's decision: no package installed with pip, no virtual
  environment, no module from elsewhere. The lab application, a jig
  application, takes the runner over in Rust later.
- **What carries over is the records and the decisions,** not the code:
  the records' bytes are language-neutral (2.2), and the run's life, its
  checks, the judgement, the verdicts and the findings' life are pure
  functions apart from the code that spawns, signals and reads files
  (10.4), held to vectors the port is held to as well (10.3).
- **In `scripts/lab/`,** outside skein's workspace and its builds:
  `lab`, the command; `runner/`, its package (10.4); `look/`, the
  look's capture, its command `look/capture`, its checks `look/check`,
  its fonts' maker `look/fonts` and their package (lab-browser.md, 5
  and 7);
  `preamble/`, the driver's and the triager's preambles; `schemas/`,
  the report's and the triage's; `tests/`, its tests and their support
  (10.3). The capture runs as `lab` does, isolated.
- **Isolated:** `lab` runs its interpreter with `-I` (no user site, no
  environment variable, not the current directory) and `-B` (no
  bytecode written), and puts its own directory first on its path; so
  it imports nothing from beside its tree, and its tree, the key's
  `runner`, stays as committed.
- **A departure, for now:** `programming-model.md` governs skein's code,
  and this is skein's one Python, which needs an interpreter of a known
  version on each machine that runs the lab, recorded in each key
  (2.3). Its reason is the user's decision: a runner that supervises
  processes and writes files is no service, and the lab application in
  Rust replaces it.

### 10.2 Commands

```
lab subject --repository DIR --commit C [--exclude PATH]... --artifact NAME=PATH...
lab validate EXPERIMENT --subject DIGEST
lab run EXPERIMENT --subject DIGEST [--seed N] [--campaign NAME]
lab judge EXPERIMENT --subject DIGEST [--campaign NAME]
lab triage RUN
lab finding ID file | class CLASS | take --by WHO | fix ... | close --recheck KEY
lab campaign open NAME | close NAME
lab prune --scratch | --runs | --subjects
lab held [put NAME FILE]
lab tool RUN
```

- **`subject`** exports the commit's tree less its exclusions, copies
  and digests the artifacts, and prints the subject's digest.
  **`validate`** prints the key or every refusal, starting nothing.
  **`run`** makes one run. **`judge`** runs seeds until the verdict is
  decided (section 8) and prints it. **`held`** lists each held file,
  kept or taken by a run, and **`held put`** moves one in (4.8).
  **`tool`** is the driver's MCP server, started by its host (6.3).
- **Exit statuses:** 0, done or pass; 1, fail; 2, refused; 3, incomplete;
  4, error; 128 plus N, cut by signal N. `judge` exits by its verdict:
  0 pass, 1 fail or flaky, 3 not enough evidence, 4 an error.
- **`campaign close`** writes `<name>.md`'s tables (each key's verdict,
  the findings filed and closed with each one's reproduction rate (9.3),
  each run's usage, known and unknown, with the totals, the errors) for
  the owner to finish with what the runner or the records lacked.

### 10.3 Its tests

Python's `unittest`, in two tiers, each one command run from skein's
root:

- **The unit tier,** `tests/unit/`, run by `python3 -I -B -m unittest
  discover -t scripts/lab -s scripts/lab/tests/unit`: the pure modules
  alone (10.4), with no process, no clock, no sleep and no file outside
  a temporary directory, so it is cheap, and it joins skein's gate
  through `check.sh`. It shows:
  - canonical bytes, and each form a reader refuses (2.2); a digest
    without `about`; a tree's manifest and its refusals;
  - the key: the same over seeds, addresses and directories, and changed
    by any byte of the experiment or an artifact, by the runner, a
    helper, the driver's configuration, its model or a class's name
    (2.3);
  - each refusal of `validate`, naming its member, all of them at once
    (4.7); the templates, and where each name is known (4.2);
  - an archive's tree read and refused for each rule of 4.3, and its
    unpacking's paths and modes;
  - an action's awaited line over scripted events: seen while the action
    runs and after it ended, an earlier line of the process not matched,
    the bound passed with the action ended and with it running, a
    non-zero exit not awaited, and the capture rendered for the next
    action and unavailable when missed (4.5);
  - private bootstrap framing and one-time consumption (4.3): every
    split of a line, empty/invalid/CR/NUL/partial/extra output, the
    4096-byte edge, receipt before or after public readiness, and cuts;
    no event or parser state's representation contains its bytes;
  - stopped phases after confirmed exits only, successful collection
    dependencies, every interruption/error path, and command bounds;
    private-output failures cite no absent evidence file;
  - the draws, the start order and a fault's N, replayed by a seed
    (5.4);
  - the run's life over scripted events: a stop in each phase cutting
    it, teardown in reverse order, the driver stopped past its time or
    its actions, a token bound passed at a turn's completion with its
    overshoot, an interruption (section 5, 6.5);
  - a check command's statuses and lines, and each way of breaking its
    contract (7.2); panic headers; the outcome's precedence (5.5);
  - the verdict: two seeds that disagree are not enough evidence, three
    decide; an attempt beside an error adds; a rerun that disagrees
    makes flaky; a fourth seed is refused (section 8);
  - a finding's life, each transition refused out of turn, matching and
    reopening (section 9);
  - the host's events read as actions, turns and usage, unknown kept
    null (6.5); the report held to its schema (6.4);
  - each pure module's imports (10.4);
  - the capture's decisions (lab-browser.md, section 6): the scene file
    and its refusals, the outline, the style rule, PNG, the difference
    image and the PSNR, the record's form.
- **Its vectors are files:** canonical bytes, digests, draws, start
  orders and verdicts, as JSON in `tests/unit/vectors/`, so that the lab
  application's code is held to the same.
- **The process tier,** `tests/process/`, run by `python3 -I -B -m
  unittest discover -t scripts/lab -s scripts/lab/tests/process`: the
  runner whole, with real spawns, its unit and cgroups, bubblewrap and
  Codex's sandbox with no model, against stand-in programs and a
  scripted host in Codex's place that makes actions from a script; its
  test support, in `tests/support/`. It runs outside the gate, in the
  after-landing lane. The stand-ins: one reports its port, one panics,
  one detaches a child, one ignores `SIGTERM`, one grows past its bound,
  one fails by its seed's parity. It shows:
  - ready lines feeding a dependent; an action's awaited line, printed
    by a stand-in because of the action, feeding the next action; a
    tree unpacked, read-only, before its process starts; an exit in
    each phase recorded and cutting it; a panic in a process and in a tool call; memory past its
    bound; a survivor; a stop past its grace;
  - a private-line stand-in printing in pieces, and a recipient that
    echoes its stdin to both streams: the input matches but no private
    byte appears in any retained file or runner/unit output; a stopped
    copy sees a writer's terminal state, and a failed copy never enables
    a dependent check; interruption and failed containment copy nothing;
  - no process and no cgroup left after a pass, a failure, a `SIGINT`
    and a `SIGKILL` of the runner, and the next command's sweep;
  - every action in the transcript; the inventory checked; the driver
    stopped past its bound;
  - the probe against Codex's sandbox: each refusal of 6.2, and its
    failure when the profile is weakened;
  - three seeds of the parity stand-in giving flaky, and three passes
    giving pass; a planted fault's finding through its whole life;
    pruning's refusals;
  - the capture on real Chromium (lab-browser.md, section 6), which
    needs the built browser program and Chromium, and fails, never
    skips, without them.
- **Wiring** the two commands into the gate and the after-landing lane
  is the kit's, by its owner, not the runner's.

### 10.4 Pure decisions and the host

`runner/` is a dozen modules of two kinds: the pure ones decide, the
host's carry out.

| Module | Holds | Kind |
|---|---|---|
| `canonical` | canonical JSON's reader and writer, digests, a tree's manifest from its entries, 64-bit values' decimal strings (2.2) | pure |
| `records` | each record's shape and refusals: key, run, evidence line, verdict, finding event, settings (2.1) | pure |
| `experiment` | `experiment.json` validated against the subject, the settings and the machine's facts; its templates; the key (2.3, section 4) | pure |
| `draws` | the draws, the start order, a fault's N (5.4) | pure |
| `life` | the run's life, `step(state, event) -> (state, requests)` (section 5) | pure |
| `bootstrap` | private line framing, receipt and one-time consumption; its byte state stays in the host, outside serializable run state (4.3) | pure |
| `checks` | the runner's own checks, a check command's result, panic headers (section 7) | pure |
| `judge` | the outcome (5.5) and the verdict (section 8) | pure |
| `findings` | a finding's fold, its transitions and their refusals, matching (section 9) | pure |
| `codex` | the host's arguments and configuration as values; its events read as actions, turns and usage; the report checked (6.1, 6.4, 6.5) | pure |
| `store` | the lab's directory: modes, the lock, write-once and appended files, manifests' walks, the sweep, pruning (2.4, section 3) | host |
| `host` | the run's unit, cgroups, spawns into them, signals by pidfd, waits, memory read, bubblewrap views, `codex sandbox` (5.2, 5.3, 6.2) | host |
| `loop` | a run carried out: events from pipes, pidfds and the clock into `life`, its requests to `host` and `store` | host |
| `tool` | the MCP server, over stdio (6.3) | host |
| `cli` | the commands, their arguments and exit statuses (10.2) | host |

The look's capture is a package of its own, `look/`, of the same two
kinds (lab-browser.md, sections 4 and 5):

| Module | Holds | Kind |
|---|---|---|
| `scene` | `look-scene/1` read and its refusals (4) | pure |
| `outline` | the outline from a tree, the structure's difference (5.3) | pure |
| `style` | the properties, the default rule, `style.md` (5.3) | pure |
| `png` | PNG read and written, as pixel rows | pure |
| `picture` | the side by side, the content, the difference image and the PSNR (5.3) | pure |
| `record` | `capture.json`'s shape (5.2) | pure |
| `capture` | the program under bubblewrap, its calls, the files, the run (5.1) | host |
| `fonts` | the lab's fonts directory made (7) | host |

- **A pure module** imports, of the standard library, `json`,
  `hashlib`, `re`, `dataclasses`, `enum` and `typing`, and other pure
  modules; no `os`, `subprocess`, `signal`, `time`, `random`, `socket`
  or `selectors`, and it opens no file; the look's add `difflib`,
  `math`, `struct` and `zlib`. Time, draws, files' bytes,
  machine facts and processes' ends are its inputs; records, requests
  and refusals are its outputs.
- **The life's requests are the host's whole vocabulary:** make a tree's
  cgroup, spawn into it, signal its first process, kill it, await it
  empty, read its counts and a process's peak, remove it, compose a
  view. `host` is one implementation of them (5.2); skein's contained
  trees, under the lab application, are another.
- **Plain code:** functions over dataclasses, one loop and no thread, no
  async; no framework for plugins, schemas or commands beyond
  `argparse`; JSON by `json` and `hashlib` alone, with no program in
  Rust or elsewhere to write it.

## 11. Open questions

- **A fake's promises as a check:** the fake forge's owner provides a
  function that judges a recorded stream of the fake's observations, and
  the fake forge's program exposes it as a check under 7.2's contract,
  its signatures `promise/<promise>/<subject>`. Its interface is that
  owner's, and pending; nothing of it is designed here.
- **A process restarted within a run** (a crash, then a start on the
  same directory, as a story): not in this contract, where a stop cuts
  the drive (5.1).
- **A driver that replays a transcript** with no model, as a cheaper
  reproduction: another driver, so another key. This contract has none
  (5.4).
- **A wall around the host itself** (6.1): the host's process is the
  lab's user's, unconfined. A view around it (the same user, the home,
  the lab and the held files out of view, if Codex's own sandbox nests
  within it) or a user of its own would bound what an unobserved tool
  reaches; neither is needed while the lab runs only on machines kept
  for it.
- **A held file's settling after a crash** is its owner's alone (4.8): a
  product's recovery command, run by the runner on the kept state, could
  give it back unattended.

# The shell kit

Provisional, 2026-10-03, revised 2026-10-23. The design of
`skein-shell`: what a service's `main` runs its loop on. The kernel (the
io_uring backend of the kernel boundary, kernel.md, and the readiness
backend for hosts without it, macOS among them), the clock and the
seed; the loop itself, over a service's `Host`, and how it ends; what
startup reads, trust roots among it, and a terminal's modes. It is
ordinary Rust, and the only `unsafe` in skein that a service runs lives
here, in each system's module, `linux` (the ring adapter among it) or
`darwin` (7.5). (The counting allocator, test-only, has the other:
testing.md, 6.)

## 1. In one page

- **One loop, written once.** skein provides the kernel (open, submit,
  reap), the clock, the seed, and `drive`: the loop of
  programming-model.md, section 2, over a service's `Host` (section 12).
  The service brings its `iterate`, its startup and a per-iteration hook.
  skein's world harness hosts the same `Host`, so a world runs the shell
  as it ships.
- **The end is settlement.** The loop exits once the service holds
  nothing: everything it owns has reported closed and io is empty. Only
  io's deadlines bound that; no timer, grace or thread decides it
  (section 13).
- **The ring adapter makes no decisions.** It maps each record onto one
  submission entry, keeps the record in its in-flight table, and when the
  entry completes decodes the result and hands the record back.
- **One thread, one kernel floor.** The ring is set up for a single
  issuer, and assumes a kernel at or above one floor, with no checking for
  individual features; a probe at startup confirms the floor.
- **Two backends, one chosen at startup.** Linux takes the ring, and
  the readiness backend when io_uring is refused; macOS takes the
  readiness backend, which waits for readiness (epoll, kqueue) and makes
  each call without blocking. Step code never knows which runs
  (section 7).
- **Startup refuses rather than degrades:** a worst case past the
  configured memory, or a kernel below the floor, stops the service
  before its loop runs. The one fallback, to the readiness backend, is
  said on standard error (7.1).
- **Optimisations stay behind the records.** None changes code above io.

## 2. In skein

`skein-shell` depends on lib, io (for the records), and the only crates
from outside: `io-uring`, on Linux alone, and `libc`. A service's `main`
depends on it and on the service's own `iterate`. It builds for Linux and
for `aarch64-apple-darwin`; any other target is a compile error naming
the two (7.5, 7.6).

- **`Host` and `drive` are here** (section 12), moved from `skein-world`,
  which re-exports both. A service's shell then links no testing crate,
  and its worlds host the shell's own `Host`.
- **It reads trust roots as bytes** (6.2). `skein-tls` turns them into its
  configuration, so the shell needs no TLS crate.

## 3. The kernel

- **Open, submit, reap.** `Kernel::open` takes a size, the most operations
  in flight at once (cancels included, as io's operation slab counts
  them); both rings are sized from it, so the completion queue cannot
  overflow, and submit takes no record past it. Submit pushes a queue of
  records and enters the ring once; reap moves completions into a queue,
  up to its room.
- **Waiting is the loop's call.** Submit takes how to wait: not at all
  (work is pending above), until a monotonic deadline (the earliest over
  every layer), or until a completion arrives (no deadline armed, which
  asserts that something is in flight). Timers are not operations: this
  one timeout covers them all.
- **Set up for one thread:** a single issuer, with deferred task running,
  so completions are processed only when the loop enters the ring.
- **The in-flight table** has one slot per operation, allocated once at
  its final capacity and never moved while anything is in flight. A slot
  holds the record and the kernel structures its operation needs, which
  never leave the module. An operation's `user_data` is its slot's index
  and generation, so a completion or an async cancel can never name the
  slot's next operation.
- **Paths are the slot's own.** A record's path or name has no NUL; the
  slot holds the NUL-terminated copy the kernel reads, made at submit and
  freed with the slot, beside the `open_how` an `Open` hands over and the
  `statx` buffer a `Stat` reads back.
- **Synchronous operations run at submit.** A record that is not a ring
  operation (listing a directory: `getdents64` has none; locking a file:
  `flock` has none, and `Lock` never waits; spawning; signalling;
  reading the process's resource usage) is performed when it
  is submitted, and its completion waits in the table's ready list for
  the next reap, so io cannot tell it from a ring operation (kernel.md,
  6). A `List` reads with `getdents64` into one buffer of the table's,
  copies out what fits the record, and sets the directory's position back
  to just past the last entry it took. A `Usage` reads `getrusage` twice,
  for the process and for its reaped children (kernel.md, 6.3).
- **The `unsafe`, and why it is sound,** is stated in the ring adapter's
  module documentation, case by case: the pointers handed to the kernel,
  entering the ring, the socket address casts, the synchronous calls, and
  dropping a kernel with operations in flight, which leaks the table so
  that the kernel never writes into freed memory.
- **Its checks:** the first two broken invariants of the kernel boundary
  (an invalid record, a token already in flight) at submit.

## 4. The kernel floor

- **One floor, no feature checks.** The first floor is 6.12, an LTS
  release that has every ring operation io uses: for sockets, and for
  files `OPENAT2`, `READ`, `WRITE`, `FSYNC`, `STATX`, `RENAMEAT`,
  `UNLINKAT` and `MKDIRAT`, the newest of them from 5.15. An operation
  newer than the floor stays synchronous until the floor moves: making a
  pipe, for example, has been a ring operation only since 6.16; listing a
  directory has none at all. Lowering the floor for a deployment that
  needs it means making the operations newer than the new floor
  synchronous, and io cannot see that change.
- **The probe at startup** checks the ring for every operation it uses. It
  confirms the floor; it does not choose between features. If an
  operation is missing, or the ring cannot be set up at all (io_uring
  missing, disabled, or refused by a seccomp profile), the shell refuses
  to start, unless io_uring was refused outright: then it takes the
  readiness backend (7.1), which confirms the floor from the kernel's
  release (7.6).

## 5. The clock and the seed

- **The clock gives two times,** read together once per iteration:
  `CLOCK_MONOTONIC`, for every deadline (the kernel's wait deadline is on
  this clock), and `CLOCK_REALTIME`, for things about the world. The loop
  hands both to the steps in `Env`.
- **The seed** comes from `getrandom`, once, at startup; everything random
  after it is drawn from the layers' state. It fails, and the service
  does not start, when the kernel refuses `getrandom`.
- **The cryptographic seed,** for a service whose layers hold a
  `CryptoRng` (lib.md, section 8): 32 bytes, drawn from `getrandom` once
  at startup, beside the seed and never derived from it, and handed to
  the service's startup as a `Secret`. The service keys each `CryptoRng`
  from it; a world draws it from its own seed instead.

## 6. Startup

Before its loop, a service's `main`, with the kit:

1. reads its configuration, the limits of every layer among it;
2. checks the sum of the layers' worst cases against the configured
   memory, and refuses to start past it (programming-model.md, 6.3);
3. blocks the termination signals that io will read, so they arrive as
   `Shutdown` events (io.md);
4. opens the first roots for io's files, from configuration, with
   `open_root` (io.md, 5; kernel.md, 6.1), which makes nothing; a root
   that may be absent, such as a state directory, with `make_root`,
   which makes each absent directory along its path with the mode given,
   syncing the directory each lies in, before it opens the last, so that
   a root the service relies on survives a crash. A root, a workspace,
   belongs on a filesystem mounted `nodev`: the ring refuses to open a
   device beneath it, but only after the device's own `open` has run. It
   opens the files it appends to beneath them, a trace among them, which
   io adopts as streams once the service is made (io.md, 5.1);
5. resolves the configured peer host names into socket addresses
   (io.md, 4), and hands on an in-process name as configured,
   unresolved;
6. reads certificates, keys and root stores for TLS: the machine's trust
   roots, or a DER file (6.2; tls.md);
7. reads the seed and opens the kernel.

Startup is each service's `main`, step by step, with the kit's
functions: the kit holds no startup function of its own.

A service that starts trees also dispatches its helper before step 1,
and opens its own cgroup before step 7 (6.4).

What startup reads, its configuration, its secrets and certificates, it
reads with the kit (6.1). Then `main` makes the service and calls `drive`
(section 12).

`panic = "abort"` in every profile: a panic is fail-stop, and a
supervisor restarts the process.

### 6.1 Reading files at startup

Startup runs before io, so what it reads it reads with the kit, never
through a `File` (programming-model.md, 2.1). `read_file` reads one whole
file, up to a stated maximum, through the ring adapter's own calls: it
opens without blocking and never as a controlling terminal, checks that
what it opened is a regular file, reads it, and closes it. It answers the
bytes, or why not: absent, not a regular file, past its maximum, or the
kernel's error. Once the loop runs, files are io's (io.md, 5).

`read_private_file` reads a secret that configuration names (a key, a
credential, a digest) the same way, within the same maximum, and makes
io's private-file checks (io.md, 5.3) on what it opened before it reads a
byte:

- a regular file, reached through no link: no component of its path is
  one;
- no group or other permission bits;
- one link;
- owned by the process's effective user, which startup reads once
  (`effective_user()`, below).

Anything else is refused unread, and the service does not start: the
refusal says what was unsafe, as io's `Unsafe` names it (a link, the
kind, the bits, the link count, the owner). A file another user keeps
private fails its open for want of permission; the read names it as io
does, by a path-only open whose stat decides nothing (io.md, 5.3). The
bytes are a secret's: the caller keeps them out of logs, traces and
`Debug` (lib.md, section 6).

**The process's own identity** is read the same way, once, at startup,
through the ring adapter:
- `effective_user()`, the effective user ID, for io's private files
  (io.md, 5.3);
- `process_id()`, the process ID, for a service that reports it, such
  as smith's `session.started`.

Neither is read again: a process's identity does not change while it
runs. A service never reads them itself, with `std::process` or a
syscall of its own. It takes them from its shell.

**Whether a directory can be written** is asked the same way, at
startup only, for a service that confirms a directory something it runs
will write, such as smith's writable caches. `directory_access(path)`
answers, through the ring adapter, as the kernel answers for the
process's effective user, its groups, the directory's ACLs and a
read-only mount: whether `path` is a directory that user may write and
search (`faccessat2` with `AT_EACCESS`, and `W_OK | X_OK`). It answers
writable, absent, not a directory, or denied with the kernel's error. It
creates nothing, opens nothing for writing and changes nothing. The
answer is the kernel's at that moment: a confirmation for a person,
never a guard, since what later writes there finds out for itself. A
mode bit or an owner read from a stat is never taken in its place: it
cannot see groups, ACLs or a read-only mount.

### 6.2 Trust roots

A TLS client trusts the roots its configuration names, one of two:

- **The machine's.** The bundle that the `SSL_CERT_FILE` environment
  variable names, or else the first that exists of the distributions'
  bundle files (`/etc/ssl/certs/ca-certificates.crt` and its kin, in an
  order the module documentation fixes). A bundle is PEM: the kit takes
  each certificate out of it as DER, and skips what is not a certificate.
  A directory of hashed certificates (`SSL_CERT_DIR`) is not read
  (section 10).
- **A DER file:** one certificate, for an issuer or a provider on a
  private network, or a test's fake.

Each is read with `read_file` within bounds that are configuration: the
bundle's bytes, the number of certificates and each one's bytes. The kit
hands the certificates to `skein-tls`, which builds its configuration
from them (tls.md, 3.4), skips and counts one it cannot parse, and
refuses roots of none. Every connection shares the roots, so they count
once in the worst case. A machine with neither a bundle nor
`SSL_CERT_FILE` refuses to start when its configuration asks for the
machine's roots.

- **A bundle is read in RFC 7468's lax form.** Text outside blocks is
  ignored. A block runs from its `-----BEGIN <label>-----` line to the
  next `END` line or the next `BEGIN` line, whichever comes first. Its
  base64 may hold whitespace and line breaks anywhere.
- **Blocks that are not certificates** (any label but `CERTIFICATE`:
  `TRUSTED CERTIFICATE`, `X509 CRL`, a key) are skipped without being
  decoded, and counted as others.
- **A malformed certificate block is skipped and counted, never a
  refusal.** A block is malformed when it:
  - does not end with its own `END CERTIFICATE` line;
  - carries encapsulated headers;
  - has invalid base64: a character outside the alphabet, or padding
    anywhere but at the end.

  Reading goes on at the next `BEGIN` line. The rest of the bundle still
  serves, as it does past a certificate that does not parse (tls.md,
  3.4). Skipping takes trust away and never adds it, and a damaged bundle
  should not stop every service on the machine.
- **The bounds refuse.** They are configuration, so a bundle past them is
  refused, naming the bound, never cut short to fit it:
  - the bundle's bytes;
  - the number of certificates taken out as DER;
  - each one's bytes, checked as its block decodes. The refusal names
    the certificate's place among the file's certificate blocks,
    malformed ones included.

  A block decodes until its first defect or its bound, whichever comes
  first, and that decides which it is. A malformed block is not kept, so
  it does not count towards the number of certificates.
- **Startup says the counts once** on standard error: how many roots
  were taken; how many `skein-tls` skipped as unparsable; how many blocks
  were malformed; and how many were others.

### 6.3 Terminal modes

A service with a person at a terminal (an interactive front end) may
want its input a key at a time, without the terminal's echo or line
editing, and the window's size. These are shell effects, made with the
kit, since step code makes no syscall and the shell's rules allow only
the ring adapter's:

- **Raw input at startup.** If standard input is a terminal and the
  configuration asks for it, startup reads the terminal's modes, keeps
  them, and sets the ones asked for: no echo, no line editing, and the
  terminal's own signals kept or not, as the service says. Standard input
  is then adopted as a read stream, as an inherited pipe is (io.md, 3).
  Standard input that is not a terminal is left as it is, and the service
  hears that it has none.
- **Restored at the end.** `main` restores the kept modes once `drive`
  returns, on every path out of it, before it writes its last line to
  standard error. A process that aborts, or is killed, cannot: the
  terminal stays as it was set, as with any program that sets it.
- **The window's size** is read at startup, and again when `SIGWINCH`
  arrives: startup blocks it with the termination signals, so it reaches
  io through the same signalfd, as an event that the window changed
  (io.md, 7), and the service asks for the new size with a synchronous
  record (kernel.md, 6.3).

A service without a terminal never asks for any of this, and a line
pipe needs none of it.

### 6.4 Trees at startup

A service that starts trees (io.md, 6.2 to 6.4) does two more things
at startup, with the kit:

- **The helper, first of all.** The first line of `main` hands its
  arguments to `skein-contain`'s dispatch, before it reads any
  configuration. It returns at once unless the first argument is the
  helper's marker. With the marker, the process is a helper the service
  started (kernel.md, 6.6): it sets up the tree and executes the command,
  or reports its stage and exits, and never returns into `main`. A
  service that starts no tree may leave the line out: without it, a
  helper it never starts is never dispatched.
- **Its own cgroup,** opened before step 7: the `0::` line of
  `/proc/self/cgroup`, beneath `/sys/fs/cgroup`, which must be a cgroup
  v2 mount (`statfs`) mounted with `nsdelegate` (`/proc/self/mountinfo`),
  so that a tree in a cgroup namespace cannot leave its cgroup (io.md,
  6.4). Startup opens its directory and its `cgroup.events` through the
  ring adapter, and io adopts the two as the service's own cgroup
  (io.md, 6.2). A machine without either, or a process in a cgroup v1
  hierarchy alone, refuses to start a service that asks for trees, saying
  which it lacks. So does a service whose trees' maps need `newuidmap`
  and that runs with `no_new_privs` (`PR_GET_NO_NEW_PRIVS`), as a unit
  its supervisor hardens may: the helper could not gain `newuidmap`'s
  privilege (kernel.md, 6.6).

A service with neither reads none of this. Under the simulator, the
world gives the service its own cgroup (simulator.md, 3.4).

### 6.5 A command's settings

Every binary of every repository built on skein reads what it is given
alike, before startup's step 1:

- **Each setting is three things:** a flag, `--name VALUE`; an
  environment variable, the tool's name and the setting's in capitals,
  `-` as `_` (`TEMPER_CONFIG` for temper's `--config`); and a default,
  documented with the flag. A setting with no sensible default has
  none, and its absence refuses the start, naming the flag and the
  variable.
- **The flag wins over the variable, and the variable over the
  default.** An empty variable is unset.
- **Directories default to XDG's base directories:**
  `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME`, `$XDG_STATE_HOME` and
  `$XDG_CACHE_HOME`, each with the specification's fallback under
  `$HOME` when unset, empty or not absolute, then the tool's name.
- **The kit resolves them:** a function of `skein-shell` takes the
  settings' table (name, variable, default) with the arguments and the
  environment, and answers each setting's value and where it came
  from, or the setting it refused, so that a binary's `--help` and its
  refusals say the same. Positional arguments stay the binary's own.
- **A test never relies on a default under `$HOME`:** it gives every
  directory as a flag.

## 7. The readiness backend

This backend is for hosts where io_uring is not available: a container
whose seccomp profile refuses io_uring (Docker's default profile does), a
kernel with `io_uring_disabled` set, or macOS (Apple Silicon, from macOS
15, Darwin 24: 7.6).

- **It emulates completions.** It makes each record's call without
  blocking, and completes the record; a call that would block waits for
  its descriptor's readiness (epoll on Linux, kqueue on macOS) and is
  made again. Regular files are always ready, so file operations run at
  submit, as the ring's synchronous operations do.
- **One shared part, and each system's parts beside it.** The table, the
  waits on descriptors, retries, cancels and closes are written once, in
  safe Rust, the same over either wait; the calls themselves and the
  wait are each system's (7.5).
- **It costs nothing on the ring path.** `Kernel` holds one backend in an
  enum, matched once per submit and once per reap (7.1). Step code never
  knows which backend runs.
- **The kernel boundary keeps it cheap:** records that carry their own
  memory and plain values, single-shot operations, and synchronous
  operations already in the vocabulary. A feature only io_uring has is
  always an optimisation, never something io's behaviour relies on.
- **It meets the conformance suite** as the ring does, each scenario
  once (kernel.md, 8), on our Linux machines through epoll and on macOS
  through kqueue.

### 7.1 The choice at startup

- **`Kernel::open` takes the host's backend:** on Linux the ring, or the
  readiness backend when the ring cannot be set up because io_uring is
  refused (`ENOSYS` or `EPERM` from its setup: missing, disabled, or a
  seccomp profile's); on macOS always the readiness backend. Any other
  refusal of the ring stops the service, as before (section 4): a kernel
  below the floor is below it for both backends.
- **The ring's refusal alone decides.** `OpenError::Setup` with `ENOSYS`
  or `EPERM` falls back; every other error of the ring's open (another
  `Setup`, `Missing`, `BelowFloor`, `NoOperations`) is returned as it
  is. The readiness backend then opens as `open_on` would, and its own
  refusal, below the floor among them, is the one returned.
- **`Kernel::open_on` forces one,** `Backend::Ring` or
  `Backend::Readiness`, for tests: the ring's own tests and its
  conformance take `Ring`, so that a machine without io_uring fails them,
  saying so, rather than running them on the other backend; the readiness
  backend's take `Readiness`. `Ring` on macOS refuses
  (`OpenError::Unsupported`).
- **`Config` is unchanged,** so no caller of `Kernel::open` changes.
- **No setting of its own.** A service takes `Kernel::open`; nothing in a
  binary's command line or environment chooses the backend, and the
  settings of 6.5 gain none. The tests that need the readiness backend on
  Linux open it themselves. A binary whose own tests need it forced adds
  a setting of its own then, under 6.5.
- **What runs is said:** `Kernel::chosen()` answers the backend and, when
  the ring was refused, its error number; its `Display` reads, for
  example, `readiness (epoll): io_uring refused (EPERM)`. A service says
  it once on standard error at startup when it is not the ring, beside
  the trust roots' counts (6.2), so that a slower host is never a
  mystery. The line is the service's `main`'s, as the counts are, never
  the kit's, which prints nothing: `<service>: kernel: <chosen>`. In
  skein, the echo's and the UI server's `main` say it; a service
  downstream adds it when it next takes skein.

### 7.2 The table

- **One slot per operation,** as the ring's (section 3): allocated once at
  `Config`'s size, an operation's slot named by its index and generation,
  a map from token to slot that finds a cancel's target and catches a
  token already in flight. No slot holds memory the kernel writes later:
  every call is made by the backend, on its own thread, and returns
  before the record moves again.
- **The ready list** holds the slots whose record has its result, oldest
  first; `reap` hands them up as far as the completion queue has room.
  `room` and `in_flight` answer as the ring's.
- **The waiters.** The contract allows a descriptor at most one
  operation that reads (`Recv`, `Accept`, `PipeRead`, `ReadSignal`, a
  `Wait`) and one that writes (`Send`, `PipeWrite`, `Connect`) in flight
  at once (`skein_io::kernel`, broken invariants), so each descriptor
  has at most two waiters, one for each side. The backend keeps a map
  from descriptor to its waiters.
- **Interest is level-triggered,** and registered only while a
  descriptor has a waiter: added with its first, changed when the sides
  change, removed with its last. A readiness never seen is never lost,
  and a descriptor with no waiter costs the wait nothing.

### 7.3 A record's path

- **Made at submit:** every operation on files (`Open`, `Read`, `Write`,
  `Append`, `Truncate`, `Sync`, `Stat`, `Rename`, `Remove`,
  `MakeDirectory`, `List`, `Lock`), the sockets' `Socket`, `Bind`,
  `Listen` and `Shutdown`, `Close`, `Spawn`, `Signal`, `Usage` and
  `Window`. Each makes its one call, and its result waits in the ready
  list.
- **Attempted at submit, then waited for:** the operations that read or
  write a stream, accept, connect, read a signal or wait for a child.
  The call is made at once where it cannot block: on a descriptor the
  backend made non-blocking (below), and a child's wait, which asks not
  to hang (`WNOHANG`). If it would block (`EAGAIN`, a
  connect's `EINPROGRESS`), the record waits on its descriptor's side.
  When the wait reports the side ready, the call is made again, and the
  record completes, or waits again if it would still block: a readiness
  that another reader took first is no failure.
- **A descriptor the backend did not make** (an inherited standard
  stream, the signalfd startup opened) keeps its flags: changing them
  would change them for every process that shares it. A socket's
  `Recv` and `Send` on it ask not to block, call by call
  (`MSG_DONTWAIT`), and are attempted at once as on the backend's own.
  Any other record on it waits for readiness first, then makes its one
  call; a write to a pipe or a terminal is capped at `PIPE_BUF` bytes,
  which a writable pipe takes whole, and completes short as the
  contract allows. A terminal's flow control may still hold such a
  write, as a stalled disk holds a file's.
- **A descriptor the wait cannot watch is always ready.** epoll refuses
  a regular file or `/dev/null` (`EPERM`), which an inherited standard
  stream may be redirected from; their reads and writes never wait, so
  a record on one makes its call at once, as an operation on files does,
  and registers no interest.
- **A known limit: an inherited listener.** An `Accept` has no per-call
  flag, so on a blocking listener shared with another process, one that
  the other takes the connection from between the readiness and the
  call would hold the loop until the next. io adopts no listener it did
  not make today: every `Accept` is on a socket the backend made
  non-blocking. A service that one day inherits listeners (socket
  activation) brings this back to the design.
- **The backend's own descriptors are non-blocking:** every socket it
  makes (`SOCK_NONBLOCK`), every one it accepts, and the parent's end of
  each pipe a `Spawn` makes, which is a description of its own, so the
  child's end stays blocking. It remembers which descriptors it made so,
  from the call that made one to its `Close`.
- **Each side's readiness** is the one its call needs: readable for a
  receive, an accept, a pipe's read, a signal and a child's exit (a
  pidfd is readable once the child has exited); writable for a send, a
  pipe's write and a connect, whose result is then the socket's pending
  error (`SO_ERROR`). An error or a hang-up the wait reports wakes both
  sides: the call says what it is.
- **A child and a signal, step by step.** A `Wait` makes `waitid` on the
  pidfd with `WNOHANG` at submit (and `WNOWAIT` to observe); no exit yet
  (no child reported) waits for the pidfd to be readable, and the call
  is made again. A `Wait` that reaps follows the one that observed, so it
  completes at submit. A `ReadSignal` reads one record from the
  signalfd startup opened, which is blocking: it waits for readable
  first, and a readable signalfd holds a whole record. A pipe's read or
  write on a `Spawn`'s parent end is attempted at submit, as a socket's
  is; once every holder of the child's end has closed it (the child and
  what it started), the parent's read sees the end of the stream and its
  write `EPIPE`, as with the ring.

### 7.4 Counts, retries, cancels and closes

- **One call, one count.** A transfer completes with the count of the one
  call that made progress, never continued by the backend: a short send,
  write or read goes up as it is, and io continues it from `from + n`,
  as with the ring (`skein_io::kernel`, the counts).
- **Retries are the ring's:** a call interrupted (`EINTR`) is made again
  at once; an `Open` whose `..` raced a rename or a mount (`EAGAIN`) is
  made again, up to 16 times, then fails with `Other(EAGAIN)`.
- **Errors map as the ring's do,** per operation, by the same functions
  (7.5): an error number means what it means to the ring.
- **A cancel** of a record that waits takes it off its side, completes
  it `Cancelled`, then completes itself `Nothing`. Every other target is
  too late (`TooLate`), and completes with its own result: it was made at
  submit, or made again on a readiness, before the cancel came. A cancel
  of an operation on files is therefore always too late (kernel.md, 6).
- **A close** removes its descriptor's interest, if it has any, before
  the call, so that a description another process still holds never
  reports a readiness for a number reused since. A waiter beside a
  `Close` is a broken invariant of the contract, which the backend
  asserts.
- **The wait** blocks as `Wait` says: not at all while the ready list
  holds anything or `Wait::No` asks; until the deadline, read against
  the monotonic clock just before the call and rounded up to the wait's
  resolution, for `Wait::Until`; or for a readiness, for
  `Wait::Forever`, which asserts that something waits. An interrupted
  wait returns to the loop, as the ring's does. Its event buffer is
  allocated once, a slot's worth per operation.
- **Dropping it** with operations in flight drops their records: no
  memory of theirs is the kernel's, so nothing is leaked. Their
  descriptors stay open until the process ends, as the contract says.

### 7.5 Each system's parts

The calls, the error numbers and the wait are each system's, in a module
of its own, `linux` or `darwin`, which holds that system's `unsafe` and
says why it is sound, case by case (section 1). On Linux, the ring adapter
and the readiness backend share every call they both make (the
synchronous operations, the error tables, what decodes a `statx` or a
`siginfo`).

| Part | Linux | macOS |
|---|---|---|
| The wait | `epoll_wait` | `kevent` |
| Open beneath a root | `openat2`, `RESOLVE_BENEATH` | `openat`, `O_RESOLVE_BENEATH`, `O_NOFOLLOW_ANY` |
| Listing a directory | `getdents64` | `readdir`, its positions kept (below) |
| Rename, never replacing | `renameat2`, `RENAME_NOREPLACE` | `renameatx_np`, `RENAME_EXCL` |
| Stat | `statx` | `fstat` |
| Spawn | `pidfd_spawn` | `posix_spawn`: the same file actions and process group; for a terminal, `fork` and `execve` (below) |
| A child's handle | its pidfd | a kqueue of the child's own, its `EVFILT_PROC` exit registered: a descriptor that is readable once the child has exited |
| A child's exit | `waitid` on the pidfd | `waitid` on the child's pid, `WNOWAIT` to observe |
| Signalling a child | `pidfd_send_signal` | `kill`, `killpg`, by the pid its handle names |
| Signals to the service | `signalfd` | a kqueue of the service's own, its `EVFILT_SIGNAL`s registered, the signals ignored |
| The seed | `getrandom` | `getentropy` |
| Usage's resident size | KiB | bytes |

- **A child stays unreaped until io closes it,** on macOS as with
  pidfds, so its pid is never another process's while io can still
  signal it, and its handle names it alone.
- **Only the shell's own direct children are watched:** macOS has no
  subreaper.
- **The handles are descriptors,** so io, its records and the
  conformance suite are the same on both systems: a `Wait` waits for its
  handle to be readable, a `Signal` finds the pid its handle names, and
  a `Close` of the handle forgets it.

**The crate's layout.** `unsafe` is in `linux` and `darwin` alone; every
other module is safe Rust, the same on both systems.

| Module | What it holds | Built for |
|---|---|---|
| `kernel` | `Kernel`, `Backend`, `Chosen`, `Config`, `Wait`, `OpenError`: the public kernel | both |
| `readiness` | the readiness backend's shared part (7.2 to 7.4), over `system` | both |
| `calls` | what every backend decodes the same way: each operation's error table, the kind of what an `Open` opened, the paths' checks, `Open`'s retries, the listing buffer's size, a release against a floor (7.6) | both |
| `linux` | Linux's calls and wait (`linux::epoll`), and the ring adapter (`linux::ring`), the only user of `io-uring` | Linux |
| `darwin` | Darwin's calls and wait (`darwin::kqueue`) | macOS |
| `clock`, `seed`, `startup`, `terminal`, `settings`, `drive` | the public startup and loop, calling `system` | both |

- **One face, two systems.** `linux` and `darwin` give the same
  crate-private functions, by name and signature, and `lib.rs` names the
  one built as `system`: sockets, files, processes, the signal source,
  the terminal, the wait, the clock, the seed, the release and the
  startup reads. They take and return io's values (`Fd`, `Addr`, `Stat`,
  `Exit`, `Done`) and error numbers, never a system's structure: no
  `statx`, `siginfo`, `stat` or `kevent` reaches the shared part, so each
  system decodes its own. Building for each system checks the face.
- **Each system's wrappers are its own,** written against that system's
  manual and the `libc` types for it (a `sockaddr_in`'s length byte on
  Darwin, its `stat`'s layout): neither module calls the other, and
  neither emulates the other's calls. A call the two make alike (`flock`,
  `ftruncate`, `tcgetattr`) is written in each.
- **Linux's own is built for Linux alone:** the ring adapter and
  `Running`'s ring arm; cgroups (`open_cgroup`, `start_binary_in_cgroup`,
  `wait_cgroup_change`); the subreaper (`make_subreaper`, `subreaper`,
  `set_subreaper`); and the pidfd helpers (`open_pidfd`, `pidfd_exited`).
  `Backend::Ring` itself is on both, so a match on it downstream stays
  exhaustive on both: `open_on(.., Backend::Ring)` on macOS is
  `OpenError::Unsupported`, and `Kernel::open` there never tries the
  ring. Every other public item is on both.
- **The tree keeper's calls,** on both (examples.md, 6): `child_id`, the
  pid a child's handle names; `signal_group`, a signal to a process group
  by its ID (`kill` of its negative); and `group_live`, whether a process
  group still has a member (`kill` of its negative with no signal, `ESRCH`
  for none). `process_live` probes a positive pid with no signal, counting
  an unreaped zombie as live and refusing 0, 1 and values beyond `pid_t`.
  These calls name processes and groups by number, as no record does,
  and are for the harness alone.

**Darwin's parts,** in `darwin`:
- **Sockets** are made without blocking and closed on exec by `fcntl`
  after `socket` and `accept`, which take no such flags on Darwin; the
  loop's thread makes both calls and every spawn, so no child starts
  between them. Addresses carry their length byte. `SIGPIPE` is blocked
  on the loop's thread, as on Linux.
  A V6ONLY socket's bind to an IPv4-mapped address is `AddressNotAvailable`
  (`bsd/netinet/tcp_usrreq.c:334-354`, `bsd/netinet6/in6_pcb.c:273-281`),
  and its mapped connect is `InvalidArgument` (`tcp_usrreq.c:546-552`'s
  `EAFNOSUPPORT`). Linux and the simulator assert `InvalidArgument` for
  the bind and `Unreachable` for the connect; both systems keep families
  separate.
- **The wait** is a kqueue: `EVFILT_READ` and `EVFILT_WRITE` for a
  descriptor's sides, level-triggered, added and deleted as its waiters
  change (7.2). `EV_EOF` and `EV_ERROR` wake both sides, as epoll's
  hang-up and error do. A descriptor kqueue cannot watch is always
  ready, as 7.3's: the error its registration answers for one (`EINVAL`
  for a device it does not support) is the one epoll's `EPERM` is.
- **Opening beneath a root:** `openat` with `O_RESOLVE_BENEATH`, and
  `O_NOFOLLOW_ANY` for the ways that refuse a link (kernel.md, 6.1),
  close-on-exec, without blocking or a controlling terminal, as on Linux.
  `PathNoFollow`, which Linux opens `O_PATH | O_NOFOLLOW`, is
  `O_SYMLINK | O_EVTONLY` with `O_NOFOLLOW_ANY` and `O_RESOLVE_BENEATH`:
  the entry itself, a link as a link, for `Stat` and `Close` alone.
  Linux needs no permission to read the entry; Darwin needs it readable:
  `O_EVTONLY` still authorizes `READ_DATA` (XNU `bsd/vfs/vfs_syscalls.c`,
  lines 4894-4900; `bsd/vfs/vfs_subr.c`, lines 8434-8438). Each system's
  answer is held to conformance.
  Darwin answers an escape with `ENOTCAPABLE`, which `darwin` gives up
  as Linux's `EXDEV`, so that one error table serves both (`Escape`).
- **Files:** `pread`, `pwrite`, `write`, `ftruncate`, `fstat` (decoded by
  `darwin`), `renameatx_np` with `RENAME_EXCL` for a rename that never
  replaces and `renameat` otherwise, `unlinkat`, `mkdirat`, `flock`.
  A no-replace rename to the identical name succeeds without changing
  it on case-insensitive APFS (macOS conformance run 40003540284): XNU
  `bsd/vfs/vfs_syscalls.c:9538-9564` admits the same vnode on those
  volumes, and `9748-9753` returns success for the identical name.
  Linux and the simulator refuse it with `Exists`; conformance keeps
  the distinct-name hard-link refusal separate.
  Component lookup is the volume's (`bsd/vfs/vfs_lookup.c:1518-1524,1552`).
  APFS refuses a 300-byte component with `NameTooLong` before a removed
  directory's missing entry or a creation's final-slash refusal
  (`1766-1767`), as observed in that run. Linux and the simulator
  assert `NotFound` and `IsADirectory`, respectively.
  `Sync` is `fcntl(F_FULLFSYNC)`, which reaches the disk as Linux's
  `fsync` does, where Darwin's `fsync` stops at the drive's cache.
- **Listing a directory** keeps `readdir`'s positions: the backend holds
  a directory stream for each descriptor it has listed, opened on a
  duplicate of it at its first `List` and closed with its `Close`. A
  `List` reads entries until the next does not fit the record, sets the
  stream back before that one (`telldir`, `seekdir`), and the next `List`
  starts there. `.` and `..` are skipped and an entry's kind is its
  `d_type`, or its `fstatat` where that is unknown, as on Linux. The
  listing state is each system's: Linux's one buffer of the table's,
  Darwin's streams.
- **Spawn** is `posix_spawn`, with the file actions Linux's spawn has
  (the working directory by `posix_spawn_file_actions_addfchdir_np`), the
  child in a group of its own (`POSIX_SPAWN_SETPGROUP`), its signal mask
  emptied and every signal the service ignores set back to its default
  (`POSIX_SPAWN_SETSIGMASK`, `POSIX_SPAWN_SETSIGDEF`).
- **A child on a terminal** is started by `fork` and `execve`, as
  `posix_spawn` can't do it. On XNU, `posix_spawn` runs its file actions
  before its `POSIX_SPAWN_SETSID`, so the terminal it opens never becomes
  the child's controlling terminal; and its `SETPGROUP` comes before
  `SETSID`, which then fails, the child already leading its group. Every
  string, list and descriptor the child needs is made before the
  `fork`. Between the `fork` and the `execve`, the child makes only
  async-signal-safe calls, and allocates nothing:
  1. `setsid`, which also makes the child a group of its own;
  2. the terminal opened, and made controlling by `TIOCSCTTY`;
  3. the same duplications as the file actions, then the working
     directory;
  4. the descriptors above its streams closed;
  5. the mask emptied and the defaults restored;
  6. `execve`.

  A failure there writes its error number to a close-on-exec pipe and
  `_exit`s. The parent reads that pipe to its end: no bytes, and the exec
  happened; an error number, and the spawn fails with it, once the parent
  has reaped the child.
- **A child's handle** is a kqueue of the child's own, close-on-exec,
  with `EVFILT_PROC` and `NOTE_EXIT` registered for its pid: readable once
  the child has exited. A child that exits before its registration
  (`ESRCH`) is still unreaped and the parent's: its handle is made
  readable by an `EVFILT_USER` event triggered in the same kqueue. The
  module keeps a table from each handle to its pid and whether it is
  reaped, process-wide behind a lock: a spawn adds the handle, the `Wait`
  that reaps marks it, its `Close` removes it. A `Wait` is `waitid` on
  the pid (`P_PID`, `WEXITED | WNOHANG`, `WNOWAIT` to observe). A `Signal`
  is `kill`, or `killpg` for the group the child leads; one after the
  reap answers `ESRCH`, as a pidfd's does, and never signals the number.
  A handle not in the table answers `EBADF`.
  A stop posts `NOTE_SIGNAL`, which this `NOTE_EXIT` registration does
  not request (`bsd/kern/kern_sig.c:2124-2125`,
  `bsd/kern/kern_event.c:1213-1218`). `waitid` skips stopped children
  without `WSTOPPED` (`bsd/kern/kern_exit.c:3270-3276`); a stop must
  leave `Wait` pending. To distinguish run 40003540284's `ECHILD`,
  Darwin reports diagnostic `Other(-10001)` for an already-reaped
  table entry, `Other(-10002)` for `waitid`'s own `ECHILD`, and
  `Other(-10003)` for a non-exit status, whose pid, code and status
  also print to stderr. The source does not establish which path fired;
  these diagnostics preserve the failure while identifying its source.
- **Signals to the service:** startup sets the signals the service
  watches (`SIGINT`, `SIGTERM`, and `SIGWINCH` at a terminal) to be
  ignored, which Darwin still reports to a kqueue, and opens a kqueue with
  an `EVFILT_SIGNAL` for each: the descriptor `ReadSignal` names, readable
  while one is pending. A `ReadSignal` takes one event without waiting and
  decodes its signal; deliveries of one signal before a read are one, as
  signalfd's are for a standard signal. Darwin reports only a signal sent
  to the process, never one sent to a thread (XNU `kern_sig.c`,
  `psignal_internal`), so `signal_current_thread` signals the process,
  and every signal kqueue in it hears that signal.
- **The rest:** `getentropy` for the seeds; `clock_gettime` with
  `CLOCK_MONOTONIC` and `CLOCK_REALTIME`; `getrusage`, whose resident
  size Darwin gives in bytes; `TIOCGWINSZ` and the terminal's modes as on
  Linux; `faccessat` with `AT_EACCESS` for a directory's access; the
  startup reads opened with `O_NOFOLLOW_ANY` where Linux's refuse links.
- **Names that differ only in case** are one name on a default APFS
  volume. The shell passes names as they are and answers as the volume
  does (a `Create` of `A` beside `a` is `AlreadyExists` there); it folds
  and checks nothing. The refusal of such names is the layer's that
  names things, where a checkout's name is checked.

### 7.6 The floor on each system

- **Linux's floor is section 4's,** 6.12, for both backends. Without a
  ring the opcode probe cannot confirm it, so the readiness backend reads
  the kernel's release (`uname`) and refuses below it
  (`OpenError::BelowFloor`, whose message names the floor, not the
  ring). The release's first two numbers are compared, whatever follows
  them (`6.12.107+deb13-cloud-amd64` is 6.12); a release without them is
  refused (`OpenError::Setup(EINVAL)`).
- **macOS's floor is 15, Darwin 24,** the first with `O_RESOLVE_BENEATH`,
  which an open beneath a root needs: confirmed the same way, by the
  release (`uname`'s, Darwin's own: `24.6.0` is 24), whose first number
  is compared, and refused below it. One comparison serves both, a
  release against a system's floor, in `calls`, and its table of cases
  runs on Linux for both floors. No feature checks, on either.
- **Apple Silicon only:** `aarch64-apple-darwin`. The shell does not
  compile for Intel Macs: a `compile_error!` says so.

### 7.7 What is weaker without the ring

- **A stalled filesystem holds the loop.** File operations run at submit,
  so an NFS or FUSE mount that stalls stalls the service, and a cancel of
  a file operation always comes too late (kernel.md, 6).
- **Processes are groups, as they are with the ring:** a descendant that
  leaves its group escapes the close's kill. A cgroup or a contained
  tree (kernel.md, 6.6) is refused on macOS as unsupported.
- **A descriptor closed outside io leaves its waiters waiting.** The
  ring holds an in-flight operation's file, so the operation completes
  however its descriptor is closed; a readiness waiter holds only the
  number, and a close removes it from kqueue or epoll without an event.
  io closes a descriptor only once nothing is in flight on it (io.md,
  Settling; kernel.md, 7). An abandoned binary observer follows that rule:
  `Binary::abandon` queues cancels for its outstanding wait and stderr read;
  its driver reaps the targets and the cancels before the observer settles
  the tree and queues its closes. The host stays in the loop until empty
  on every backend. Its emergency destructor kills the tree and closes
  only quiescent descriptors; it leaves an outstanding record's descriptor
  open, and its leader unreaped while a wait or signal remains outstanding.
  A caller that needs full resource settlement must drive explicit abandonment.
- **A `PathNoFollow` open needs the entry readable on macOS.** Linux's
  `O_PATH` asks nothing of the entry; Darwin's nearest, `O_EVTONLY`,
  still checks read access (XNU `vfs_syscalls.c`, `vfs_subr.c`), so an
  entry the user cannot read refuses with `Permission`. skein-io meets
  this when it names a refusal: after a refused private open it opens the
  entry `PathNoFollow` to say what refused (`file_layer/private.rs`, the
  refusal on opening and `denied_load`), and on macOS that probe is
  itself refused, so the caller gets a plain `Permission` where Linux
  names the unsafe entry (`Refused` with what was found). A later fix: Darwin's handle holds the parent and the
  name, and `Stat` goes through `fstatat(AT_SYMLINK_NOFOLLOW)`.

## 8. Deferred optimisations

Each sits behind the same records, keeps the contract of the kernel
boundary (explicit backpressure, one completion per operation, a
completion queue that cannot overflow), is measured before it goes in,
and changes no code above io:

- multishot accept and receive with a provided-buffer ring, so idle
  sockets hold no buffer;
- registered buffers; fixed files (kernel.md, 9); zero-copy send; linked
  operations;
- exact-size receives for a fill demand: io allocates the `Box`, the
  kernel reads straight into it, and the copy out of the intake
  disappears;
- shared immutable buffers (`Arc<[u8]>`, confined to io) for large values
  sent to many slow readers, or zero-copy relaying;
- batched submits: submitting every few rounds, or when about to wait,
  which saves syscalls under pipelining at the cost of delaying each
  submission by up to that many rounds. Today the loop reaps and submits
  every round: going round costs a completion-queue read, a clock read,
  and a ring syscall only when there is something to submit or the kernel
  has completions to flush, and reaping every round keeps one peer's
  backlog from holding up other connections' completions and timers;
- kernel TLS (tls.md);
- in the readiness backend, a descriptor's interest kept for its life,
  edge-triggered (`EPOLLET`, `EV_CLEAR`), rather than registered and
  removed with each waiter (7.2): it saves two calls a wait, and asks
  every record to be attempted before it waits.

## 9. Testing

- **The ring's own tests** (`tests/ring`) run on the real kernel, on
  loopback and in a scratch directory: its slots and waits, a large
  transfer through short sends, a large file through its reads and
  writes, what it completes itself (a `List` among them), a listing's
  position handed from one `List` to the next across its own buffer, a
  root opened at startup, the invariants it asserts, and dropping it with
  operations in flight.
- **The conformance suite** runs against each backend the host has,
  each scenario once (`tests/conformance/real`; kernel.md, 8): one test
  binary, a module for each backend over one list of cases, `ring`
  (Linux only, opened with `Backend::Ring`) and `readiness` (opened with
  `Backend::Readiness`, so epoll on Linux and kqueue on macOS).
- **The readiness backend's own tests** (`tests/readiness`), on each
  system, on loopback, pipes and a scratch directory: a record made at
  submit and one that waits, then completes on its readiness; a
  readiness another reader took first, which waits again; a large
  transfer through short sends; a blocking pipe it did not make, read
  once readable and written in `PIPE_BUF` bytes at most; a descriptor
  shared with a child, closed, and its number reused, with no readiness
  of the old one reported; a descriptor the wait cannot watch, its call
  made at once; a signal read through a blocking signalfd; a cancel of a
  waiter and one too late; a deadline met without waking early again
  and again; and dropping it with records waiting.
- **The choice at startup,** in the shell's own tests, below the kernel:
  each refusal of the ring's open, as a table, falling back or returned
  (7.1); each release, as a table, at the floor, above it, below it and
  unreadable (7.6); and `chosen`'s line for each. On a host with
  io_uring, `Kernel::open` takes the ring and says so. A host that
  refuses io_uring is not made here: a seccomp profile or
  `io_uring_disabled` takes privileges, or `unsafe` outside the shell.
  macOS takes the readiness backend through `Kernel::open` itself.
- **The real loop** exercises the rest: the probe at startup, the signal
  path, and the ring adapter's `unsafe` under a sanitizer
  (testing-strategy.md, 2.8). The real world opens the host's backend,
  or the one a test forces (`skein_world::real`), and the echo's
  real-loop story runs on each: its smoke on the readiness backend,
  forced, beside the ring's.
- **`drive`'s own tests,** in the real loop, over scripted hosts: one that
  ends by itself returns its exit at once; one that waits on a keep time
  after its last word fails the teardown check (testing-strategy.md, 6);
  a termination signal while it closes aborts what is still closing; the
  hook runs once per iteration, after `iterate`.
- **Startup reads** (`tests/ring`): a whole file within its maximum, one
  past it, a FIFO and a directory refused; a PEM bundle with certificates,
  other blocks, a certificate that does not parse, and malformed blocks
  of each kind (invalid base64, unterminated, ended by another label,
  with headers), each skipped and counted while the rest are taken; a
  bundle past each bound refused; a DER file. `directory_access`: a
  writable directory, an absent path, a file, a directory without write
  permission for the user, each answered as such, and nothing created
  or changed by any of them.
- **A command's settings** (the crate's own tests, no kernel): flag and
  variable precedence, empty variables, literal and XDG defaults, refusals,
  argument order and `--`, byte values, and help agreeing with refusals.
- **Terminal modes,** end to end under a pseudo-terminal
  (testing-strategy.md, 2.9): input read a key at a time, the modes found
  again once the binary exits, and a size change heard.
- **The Darwin check.** On our Linux machines, `cargo darwin`, an alias
  in `.cargo/config.toml`, runs clippy over every target of the
  workspace for `aarch64-apple-darwin` with warnings denied, as the
  gate's clippy does; clippy's pass is `cargo check`'s too. Nothing
  links, and no file of Apple's SDK is on our machines. The members that
  reach rustls (whose `ring` crate compiles C against Apple's headers)
  are excluded there by name, the alias's own list, the one place they
  are named; they are checked on the macOS runner. Darwin's code is built
  here and run there.
- **Linux's own tests, by name.** A test that only Linux can run is
  built for Linux alone, `cfg(target_os = "linux")` on its whole binary
  or module, with its reason beside it, and listed here; none is dropped
  silently. Every other test runs on both systems.

| Test | Its reason |
|---|---|
| `tests/ring` (`skein-ring-tests`): `ring` and `files` but its `startup` module | the ring adapter's own |
| `tests/conformance/real`: the `ring` module | the conformance on the ring |
| `tests/io/tests/ring.rs` | io's real-kernel files on the ring |
| `tests/echo/tests/real.rs`: the ring's cases | the echo's real loop on the ring |
| `crates/skein-shell`: `linux`'s and `linux::ring`'s unit tests | Linux's calls: epoll, signalfd, pidfds, the ring |
| `crates/skein-shell`: `kernel`'s `the_host_chooses_the_ring` and `forcing_a_backend_reports_it_and_refuses_zero_capacity` | each opens the ring |
| `crates/skein-shell`: `readiness`'s `a_registration_error_completes_without_calling_or_adding_a_waiter` | its error is epoll's second add (`EEXIST`); kqueue's `EV_ADD` modifies instead |
| `tests/world/tests/end_to_end.rs`: the cgroup's and the walk's tree tests, an escape from the group among them | a cgroup, the subreaper and `/proc` are Linux's (examples.md, 6) |
| `tests/world/tests/end_to_end.rs`: `supervised_stderr_overflow_is_observed_before_the_referees_term_grace_and_whole_tree_kill` | its descendant leaves its session and group through `setsid`; a cgroup or the subreaper's walk follows it, Group does not (examples.md, 6) |
| `testing/skein-throwaway-forgejo/tests/throwaway.rs` (`skein-throwaway-forgejo`) | starts the pinned Forgejo binary; no owned Forgejo on macOS, its releases are Linux's |

  The list grows with each test that a Darwin build shows to be
  Linux's, in the commit that marks it.

## 10. Open questions

- **The real loop in CI:** a kernel at the floor, with io_uring allowed by
  the CI's container profile.
- **Sanitizers on the ring adapter:** Miri cannot run the ring, so which of
  the address and leak sanitizers the real loop runs under.
- **A hashed certificate directory** (`SSL_CERT_DIR`) as the machine's
  trust roots, on a machine with no bundle file: read it, or refuse to
  start as today (6.2).
- **A terminal suspended and resumed** (`SIGTSTP`, `SIGCONT`): whether
  the shell restores the modes before the process stops and sets them
  again when it continues, which takes a signal the shell handles while
  the loop runs (6.3).

## 11. Not built yet

What of this document is not built is `STATUS.md`'s (its row in section 2,
and section 3).

## 12. The loop: `Host` and `drive`

The loop of programming-model.md, section 2 is written once, here, not in
every service's `main`.

- **`Host`** is what the loop needs of a service, and what skein's world
  harness needs of a process it hosts: one trait, so a world runs a
  service's shell as it ships. A service's shell implements it over its
  `iterate`:
  - `iterate(now, wall)`: one turn, both passes and the reclaim point;
  - its completion and submission queues;
  - `work_pending(now)`, and `next_deadline()`, its earliest over every
    layer;
  - `next_policy_deadline()`, its earliest deadline other than io's own,
    the mechanics' (io.md, 2): its close and retry deadlines, and the
    stall deadlines owners state for files. It is the one the teardown
    invariant watches (testing-strategy.md, 6);
  - `is_empty()`, when it holds nothing, and `exit()`, its exit status
    once it does;
  - `worst_case()`, and `operations()`, the size of its ring;
  - `drain()`, the hook below, which does nothing unless the service
    gives it something to do;
  - `state_digest()`, a digest of what it holds, for replay (lib.md,
    5.1): `None` unless the service gives one. Only a world reads it
    (testing.md, 5); `drive` never calls it.
- **`drive(kernel, clock, host)`** is the loop: reap, read the clock,
  iterate, drain; then return the host's exit once it holds nothing
  (section 13), or else submit, waiting not at all while work is pending,
  until the earliest deadline, or until a completion. A service's `main`
  is its startup, then `drive`.
- **The hook** runs once per iteration, after `iterate` and before the
  submit, while the service is a frozen snapshot. It is for drains that
  belong to the shell: reading what the service holds for its operator
  and saying it once, such as the address it listens at. It is shell code
  under the shell's rules (programming-model.md, 2.1): a short line to
  standard error is the most it writes, and it writes no file and starts
  no thread. Output that is large, or must be complete, such as a trace,
  is an io stream instead (io.md, 5.1). The world harness calls the hook
  at the same point, so the drains a world runs are the ones that ship
  (testing.md, 5).

## 13. The end

- **The loop exits when the service holds nothing:** every slab empty,
  nothing in flight, every queue empty, and io holding no entity. `drive`
  then returns the service's exit, and `main` returns it. It does not
  exit at a signal, at an answer or at a timer: after its last word, a
  service closes everything it owns (programming-model.md, 5.2), and the
  loop ends when that has settled.
- **Only io's deadlines bound it.** A service's teardown bound is io's
  close timeout, the write deadline of a stream it still writes after its
  last word (io.md, 5.1), and the moment its cancels take to settle;
  nothing else in it waits on time once its last word is out. A
  supervisor derives its grace from that bound, and its grace exceeds
  it.
- **A signal while closing aborts.** A termination signal arrives as io's
  `Shutdown` at any time (io.md, 7). Before the last word, the service
  decides what it means; after it, the remaining graceful closes become
  aborts, and the loop ends as soon as those settle. A second signal
  means the same as the first.
- **Nothing outlives the loop.** There is no thread to join, no buffer to
  flush and no grace timer: what the service wrote went through io, which
  has settled it, and the kernel is dropped with nothing in flight. A
  service never ends with `std::process::exit` while it holds anything.
  After `drive` returns, `main` only undoes what startup set with the
  kit, such as a terminal's modes (6.3), and writes nothing to the
  service's files.
- **What the process used,** for a service that reports it at its end:
  the kernel's count of CPU time and peak resident size, for the process
  and for the children it has reaped. The service reads it through io
  (io.md, 6.1), a `Usage` the kernel answers at submit, after its last
  child has closed, since io reaps a child only at its close, and before
  the stream it reports on closes. After `drive` returns no stream is
  open to report on.
- **A supervisor's kill is a backstop** for a process that does not end:
  a bug, or an operation the kernel will not interrupt (notes.md). It is
  never how a healthy process ends.
- **A crash closes nothing.** A process killed, or aborted by a panic
  (section 6), runs none of this end: the children it started stay in
  their own groups (io.md, 6) and outlive it, since io kills a child's
  group only when it closes the child. A supervisor that restarts a
  crashed service first ends its whole tree, every descendant at once,
  as a systemd unit's control group is by default, and starts it again
  on the same state only once nothing of the old tree runs. A
  descendant still running at the restart is the supervisor's failure,
  not the service's.
- **Checked** in every world, by the teardown invariant
  (testing-strategy.md, 6), and in the real loop by `drive`'s own tests
  (section 9).

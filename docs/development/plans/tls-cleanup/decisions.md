# Decisions

The decisions this plan rests on, by the planner unless marked, and the
supervisor's rulings during the run, appended below them.

1. **What and why.** skein-tls and its world crate, about 10,000 lines,
   are this plan's whole scope: the first run of development.md is kept
   small, so that the process is what it tests. Their drift is the client
   run's (`~/src/plans/development-process/proposal.md`, section 2) in
   miniature: filler words, parallel copies, long names, umbrella tests.
2. **No behaviour changes.** The tests that exist are the proof of each
   production increment; they change only where an increment says.
3. **The public API stays.** The user allowed API changes "if they make
   sense, and we'll align smith" (2026-10-11). The audit found none worth
   making: the public items are used by skein-llm-connection,
   skein-oauth-accounts and smith as they are (`findings/audit.md`,
   B-13). `client::MAX_HANDSHAKE`, used only inside the crate, becomes
   `pub(crate)`.
4. **The native face stays.** Nothing uses it yet, but the design gives it
   a user, a framed connection over TLS (tls.md, 3.6; io.md, 3.3;
   channel.md, 7), and lib.md, 7.1's independent output is used by io,
   the channel and smith's host protocol. tls.md, 8 now names its user as
   not built. Removing it would be a design change.
5. **tls.md, 3.6 rewritten** before the plan, in the same landing: the
   same rules, in the house style and the words of 3.1 to 3.5; its test
   evidence moved to section 5; an open question (section 7) for the
   requests a failed native client asserts on; its user in section 8.
   The code's docs follow it (1.2). The user had asked that the design
   not change unless there was a reason: an implementer quotes the
   design (development.md, 4.5), so its words would have come back.
6. **This run lands in skein from carbon-forge,** while the reliability
   plan runs on debian-16gb-hel1-1 and lands skein on that machine's
   local `main` (the user, 2026-10-11). That machine merges GitHub's
   `main` when it next pushes, as it has before (`2069ae4`). This plan
   touches nothing the reliability plan has left: skein-tls's items of
   it (01.9, 01.10) landed on 2026-10-09, and no file outside
   `crates/skein-tls`, `tests/tls` and `scripts` changes.
7. **Files are owned by session** (README, "Rules"), so that the three
   sessions run at once without conflicts. The one dependency across
   sessions is 2.4 on 1.5, and the fence of 3.2.
8. **Test cost is not a target.** The focused tests of the area take
   1.9 s on one thread and the fuzzy ones 2.1 s (`findings/baseline.md`,
   4); the tests' fewer runs and bytes (T-8) wait (`later.md`).
9. **The words.** The Pulse list holds the words that were filler in
   every use the audit found: actual, genuine, authentic, honest, truly
   and their adverbs. "real" (rustls's real server, the real loop) and
   "exact" (lib.md, 7's "answered … exactly") stay, where they mean
   something.
10. **On this machine,** `DEV_REPOS=skein` (in `~/.bash_profile`) keeps
    `plan check` to skein's checkout, as smith's, jig's and temper's are
    not this plan's (the user, 2026-10-11).
11. **The designer is the planner's session,** which joins the run as
    designer (development.md, section 6, step 5).

## Rulings during the run

12. **heavy reads pueue's times in UTC** (the supervisor, 2026-10-12;
    infrastructure, blocked report of 2026-10-12T00:26:00Z). On
    carbon-forge the local zone is +01:00 (`Etc/UTC`'s zoneinfo is
    Lisbon's), so heavy failed after every task, before its summary,
    and every gate through it failed. Step `skein-03.s1`
    (`skein/03-s1-heavy-times.md`) runs `pueue log` with `TZ=UTC0`. Until
    it lands, every `heavy` and `land` command runs with `TZ=UTC0`
    before it, which gives the same times. The machine's zoneinfo is
    root's, and stays as it is.
13. **2.1 updates the users of `Pair`'s peer** (the supervisor,
    2026-10-12; plan defect, blocked report of 2026-10-12T00:27:12Z).
    2.1's Shape replaces `Pair`'s two `Option`s with `Peer`, and its
    Files allowed `native_output.rs`'s three builders only, while 18
    lines there read `pair.server` or `pair.extracted`. 2.1 changes
    those 18 lines to read the `Peer`, by default through four
    accessors on `Peer` (`02-tls-world.md`, 2.1's Shape). No assertion's
    expected value and no test's name changes; an `expect` message on
    those lines goes with the `.expect` it belonged to.
14. **The gate without its browser suite** (the supervisor, 2026-10-12;
    infrastructure, blocked report of 2026-10-12T00:33:38Z). The gate's
    last line, `cargo nextest run --workspace --profile browser`, runs
    `tests/browser/tests/browser_real.rs`, which needs Chromium.
    carbon-forge has none, nor the libraries a downloaded one needs
    (nss, gbm, atk), and the run has no root. No change of this plan can
    reach that suite: `cargo tree -p skein-browser-world -i skein-tls`
    finds no path, and it reads neither `tests/tls` nor `scripts/`. Until
    Chromium is installed, this run's code landings gate with
    `scripts/check.sh` less that line (README, "Rules"). The user is
    asked to run `sudo apt-get install chromium` ("For the user").
15. **The world crate's test count** (the supervisor, 2026-10-12; plan
    defect, blocked report of 2026-10-12T00:35:43Z). 2.1's and 2.4's
    proofs counted `heavy cargo nextest list … | wc -l`, whose lines
    include cargo's build output, which varies with what was built. The
    count is now of the test lines, `grep -c '^skein-tls-world::'`: 62
    at `73c900b` (`logs/20261012T002521-0.log`), and 62 after 2.1
    (`logs/20261012T003425-0.log`), the same 62 names. It reverses the
    unblock of 2026-10-12T00:36Z, instruction 5, whose "prints 99" was
    wrong.
16. **`Peer::Rustls` holds a `Box<Server>`; what a boundary type is**
    (the supervisor, 2026-10-12; plan defect, blocked report of
    2026-10-12T00:39:56Z). 2.1's sketch, `Peer { Rustls(Server),
    Extracted(Tls12) }`, fails `clippy::large_enum_variant` (at least
    1,264 bytes against 144), and the "Don't" list forbids a lint
    exception in tests. `Rustls(Box<Server>)`, with the accessors'
    signatures as sketched, is the shape. `Peer`'s users are all in
    `tests/tls/**`, session 02's, so changing its fields was an
    implementation choice: the README's rules now say which types are
    boundary types in this plan, so that the next such choice is made
    and reported, not asked.
17. **Edits beyond an increment's Files** (the supervisor, 2026-10-12;
    plan defect, blocked report of 2026-10-12T00:41:10Z). 1.2's Proof
    asks for `cargo doc` "with no warning", and it warns of four links
    to private items outside 1.2's Files: `worst_case`'s doc to
    `RUSTLS`, `HANDSHAKE` and `MAX_HANDSHAKE` (`client/limits.rs:125-127`,
    since 1.1 made `MAX_HANDSHAKE` `pub(crate)` and moved them) and
    `Config`'s to `Config::at` (`config.rs:38`). They become plain code.
    The second block of this kind, after ruling 13: the README's rules
    now let a session make such an edit, within its own files, at most
    20 lines, changing no test's expected value or name, and name it in
    the commit body.
18. **One retry for the real loop's timeouts** (the supervisor,
    2026-10-12; infrastructure, blocked report of 2026-10-12T00:47:44Z).
    1.2's gate (docs and messages only) failed in two tests of
    `testing/skein-world`'s `end_to_end`, "the real loop settles within
    1000 ms" (`real.rs:358`), with the machine's load at 8.7 on 8 cores
    under the gate's own suites; both passed 1.1's gate, and nothing in
    this plan reaches them. The gate may be run once more; a second
    failure on the same tree is a question. A wall-clock bound in the
    gate is for the retrospective.
19. **One retry for any timeout** (the supervisor, 2026-10-12;
    infrastructure, blocked report of 2026-10-12T00:56:08Z). 2.2's gate
    hit the focused suite's 15-second global timeout: 1,426 passed,
    eight `skein-io` `tests::store` tests got `SIGTERM`, none failed an
    assertion; the suite took 9.993 s in 1.1's gate. The second block of
    the kind after ruling 18: ruling 18's retry now covers a gate that
    fails only on a timeout, in tests outside the session's files
    (README, "Rules"). What loads the machine during a gate is for the
    hourly check.

## For the user

- **Chromium on carbon-forge** (ruling 14): `sudo apt-get install
  chromium`, so that the gate runs its browser suite again. The run does
  not wait for it.
- **The native face's future** (decision 4): keep it for the channel over
  TLS, or remove it and its design until that channel is planned.
- **The gate's word check, workspace-wide** (`later.md`): the words are
  in every crate (the client run's jig went from 44 to 708 "actual").

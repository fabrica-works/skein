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

## For the user

- **The native face's future** (decision 4): keep it for the channel over
  TLS, or remove it and its design until that channel is planned.
- **The gate's word check, workspace-wide** (`later.md`): the words are
  in every crate (the client run's jig went from 44 to 708 "actual").

//! Joined socket worlds: the simulator never sees these entities (io.md, 3.5).
use crate::owner::{Dial, Owner, Reads, Serve, ServeClose, Target};
use crate::referee::{Expect, Referee};
use crate::scenarios::{END, limits, plan};
use crate::world::World;
use skein_io::{Addr, Error, Name};
use skein_lib::Time;
use skein_sim::Config;

/// An inline in-process address for a scenario.
#[must_use]
pub fn name(bytes: &[u8]) -> Addr {
    Addr::InProcess(Name::new(bytes).expect("short scenario name"))
}

/// A dial to a name no listener holds: Connecting, Refused and Closed.
#[must_use]
pub fn unbound(seed: u64, config: Config) -> World {
    let expect = vec![
        Expect::Fails { at: 0, conn: "lost", errors: &[Error::Refused], by: END },
        Expect::Closed { at: 0, conn: "lost", by: END, optional: false },
    ];
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    let lost = plan("lost", Box::from(&b"hello"[..]), 4, Reads::Fill(4));
    let dials = vec![Dial { to: Target::Addr(name(b"nowhere")), at: Time::ZERO, plan: lost }];
    world.spawn(limits(2), Owner::new(seed, Vec::new(), dials));
    world
}

/// A second listen fails, while the first owns its name.
#[must_use]
pub fn taken(seed: u64, config: Config) -> World {
    let expect = vec![Expect::ListenFails { at: 0, serve: "second", errors: &[Error::Other], by: END }];
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    let serves = ["first", "second"].map(|label| Serve {
        name: label,
        addr: name(b"engine"),
        answers: Vec::new(),
        close: ServeClose::At(Time::from_nanos(50_000_000)),
    });
    world.spawn(limits(3), Owner::new(seed, serves.into(), Vec::new()));
    world
}

/// The third named listen is refused by a names table of two.
#[must_use]
pub fn names_full(seed: u64, config: Config) -> World {
    let expect = vec![Expect::ListenFails { at: 0, serve: "third", errors: &[Error::Busy], by: END }];
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    let serves = ["first", "second", "third"].map(|label| Serve {
        name: label,
        addr: name(label.as_bytes()),
        answers: Vec::new(),
        close: ServeClose::At(Time::from_nanos(50_000_000)),
    });
    world.spawn(limits(4), Owner::new(seed, serves.into(), Vec::new()));
    world
}

/// A listener closes with a connector still waiting.
#[must_use]
pub fn closing(seed: u64, config: Config) -> World {
    let expect = vec![
        Expect::Fails { at: 0, conn: "waiting", errors: &[Error::Refused], by: END },
        Expect::Closed { at: 0, conn: "waiting", by: END, optional: false },
    ];
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    let serves = vec![Serve {
        name: "engine",
        addr: name(b"engine"),
        answers: Vec::new(),
        close: ServeClose::At(Time::from_nanos(50_000_000)),
    }];
    let dials = vec![Dial {
        to: Target::Named("engine"),
        at: Time::ZERO,
        plan: plan("waiting", Box::new([]), 4, Reads::Fill(4)),
    }];
    world.spawn(limits(2), Owner::new(seed, serves, dials));
    world
}

/// A pair in one io, receiving each byte through intakes of seven.
#[must_use]
pub fn exchange(seed: u64, config: Config) -> World {
    use crate::owner::Total;
    let one: Box<[u8]> = Box::from(&b"one sends a message longer than either intake"[..]);
    let two: Box<[u8]> = Box::from(&b"two answers with another long message"[..]);
    let expect = vec![
        Expect::Receives { at: 0, conn: "served", bytes: one.clone(), by: END },
        Expect::Receives { at: 0, conn: "dialer", bytes: two.clone(), by: END },
        Expect::Ends { at: 0, conn: "served", by: END },
        Expect::Ends { at: 0, conn: "dialer", by: END },
    ];
    let served =
        crate::owner::Plan { total: Total::Known(one.len()), ..plan("served", two.clone(), 16, Reads::Mixed(7)) };
    let dialer = crate::owner::Plan { total: Total::Known(two.len()), ..plan("dialer", one, 16, Reads::Mixed(7)) };
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world.spawn(
        skein_io::Limits { intake: 7, ..limits(3) },
        Owner::new(seed, vec![serve("engine", vec![Some(served)])], vec![dial("engine", dialer)]),
    );
    world
}
fn serve(label: &'static str, answers: Vec<Option<crate::owner::Plan>>) -> Serve {
    Serve { name: label, addr: name(label.as_bytes()), answers, close: ServeClose::Answered(END) }
}
fn dial(label: &'static str, plan: crate::owner::Plan) -> Dial {
    Dial { to: Target::Named(label), at: Time::ZERO, plan }
}
fn empty(label: &'static str) -> crate::owner::Plan {
    plan(label, Box::new([]), 4, Reads::Fill(4))
}
/// Binding the first connector opens it; rejecting the next resets it.
#[must_use]
pub fn accept(seed: u64, config: Config) -> World {
    let expect = vec![
        Expect::Closed { at: 0, conn: "first", by: END, optional: false },
        Expect::Ends { at: 0, conn: "first", by: END },
        Expect::Closed { at: 0, conn: "second", by: END, optional: false },
    ];
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world.spawn(
        limits(5),
        Owner::new(
            seed,
            vec![serve("engine", vec![Some(empty("served")), None])],
            vec![dial("engine", empty("first")), dial("engine", empty("second"))],
        ),
    );
    world
}
/// The two listeners take turns within the shared accept batch of one.
#[must_use]
pub fn batch(seed: u64, config: Config) -> World {
    let expect = ["first", "second"].map(|conn| Expect::Closed { at: 0, conn, by: END, optional: false });
    let mut world = World::new(seed, config, Referee::new(seed, expect.into()));
    world.spawn(
        limits(6),
        Owner::new(
            seed,
            vec![serve("one", vec![Some(empty("served-one"))]), serve("two", vec![Some(empty("served-two"))])],
            vec![dial("one", empty("first")), dial("two", empty("second"))],
        ),
    );
    world
}
/// An abort frees the third slot, so the other waiting connector is accepted.
#[must_use]
pub fn waiting(seed: u64, config: Config) -> World {
    use crate::owner::{Plan, When};
    let abort = Plan { abort: true, close: When::At(Time::from_nanos(50_000_000)), ..empty("aborted") };
    let expect = ["aborted", "waiting"].map(|conn| Expect::Closed { at: 0, conn, by: END, optional: false });
    let mut world = World::new(seed, config, Referee::new(seed, expect.into()));
    world.spawn(
        limits(3),
        Owner::new(
            seed,
            vec![serve("engine", vec![Some(empty("served"))])],
            vec![dial("engine", abort), dial("engine", empty("waiting"))],
        ),
    );
    world
}
/// The intake holds the upload until the reader starts to demand.
#[must_use]
pub fn backpressure(seed: u64, config: Config) -> World {
    use crate::owner::{Plan, Total, When};
    let upload = vec![b'a'; 2000].into_boxed_slice();
    let start = Time::from_nanos(50_000_000);
    let expect = vec![
        Expect::Holds { at: 0, conn: "upload", most: 48, until: start },
        Expect::Receives { at: 0, conn: "sink", bytes: upload.clone(), by: END },
        Expect::Ends { at: 0, conn: "sink", by: END },
    ];
    let sink = Plan { total: Total::Known(upload.len()), ..plan("sink", Box::new([]), 16, Reads::From(start, 16)) };
    let sender = Plan { close: When::Sent, ..plan("upload", upload, 16, Reads::Never) };
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world.spawn(limits(3), Owner::new(seed, vec![serve("engine", vec![Some(sink)])], vec![dial("engine", sender)]));
    world
}
/// An abort breaks the joined peer, even with no demand outstanding.
#[must_use]
pub fn abort(seed: u64, config: Config) -> World {
    use crate::owner::{Plan, When};
    let at = Time::from_nanos(50_000_000);
    let abort =
        Plan { finish: false, abort: true, close: When::At(at), ..plan("abort", Box::new([]), 4, Reads::Never) };
    let peer = Plan { finish: false, ..plan("peer", Box::new([]), 4, Reads::Never) };
    let expect = vec![Expect::Closed { at: 0, conn: "peer", by: END, optional: false }];
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world.spawn(limits(3), Owner::new(seed, vec![serve("engine", vec![Some(peer)])], vec![dial("engine", abort)]));
    world
}

/// A response mid-upload is flushed while the rest of the upload is drained.
#[must_use]
pub fn refusal_mid_upload(seed: u64, config: Config) -> World {
    use crate::owner::{Plan, Total, When};
    use skein_lib::stream::Delimiter;
    let mut upload = b"PUT\n".to_vec();
    upload.extend(vec![b'x'; 3000]);
    let response = Box::from(&b"no\n"[..]);
    let expect = vec![
        Expect::Receives { at: 0, conn: "uploader", bytes: response, by: END },
        Expect::Ends { at: 0, conn: "uploader", by: END },
        Expect::Closed { at: 0, conn: "refuser", by: END, optional: false },
        Expect::Closed { at: 0, conn: "uploader", by: END, optional: false },
    ];
    let refuser = Plan {
        wait: 4,
        finish: false,
        close: When::Sent,
        ..plan("refuser", Box::from(&b"no\n"[..]), 4, Reads::Scan(Delimiter::LF, 16))
    };
    let uploader = Plan {
        total: Total::Known(3),
        ..plan("uploader", upload.into_boxed_slice(), 16, Reads::Scan(Delimiter::LF, 16))
    };
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world
        .spawn(limits(3), Owner::new(seed, vec![serve("engine", vec![Some(refuser)])], vec![dial("engine", uploader)]));
    world
}
/// A peer which never finishes cannot hold the other end past its deadline.
#[must_use]
pub fn close_deadline(seed: u64, config: Config) -> World {
    use crate::owner::{Plan, When};
    let expect = vec![
        Expect::Closed { at: 0, conn: "leaver", by: Time::from_nanos(1_500_000_000), optional: false },
        Expect::Closed { at: 0, conn: "stayer", by: END, optional: false },
    ];
    let leaver = Plan { finish: false, close: When::Sent, ..plan("leaver", Box::from(&b"bye"[..]), 4, Reads::Never) };
    let stayer = Plan {
        finish: false,
        close: When::At(Time::from_nanos(5_000_000_000)),
        ..plan("stayer", Box::new([]), 4, Reads::Never)
    };
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world.spawn(limits(3), Owner::new(seed, vec![serve("engine", vec![Some(leaver)])], vec![dial("engine", stayer)]));
    world
}
/// Closes and aborts cross moves stalled on the small intakes, from a seed.
#[must_use]
pub fn random(seed: u64, config: Config) -> World {
    use crate::owner::{Plan, When};
    use skein_lib::Rng;
    let mut rng = Rng::new(seed ^ 0xc105e);
    let starts = Time::from_nanos(20_000_000);
    let one = crate::scenarios::message(&mut rng, 128);
    let two = crate::scenarios::message(&mut rng, 128);
    let expect = vec![
        Expect::Prefix { at: 0, conn: "one", bytes: two.clone(), by: END },
        Expect::Prefix { at: 0, conn: "two", bytes: one.clone(), by: END },
        Expect::Closed { at: 0, conn: "one", by: END, optional: false },
        Expect::Closed { at: 0, conn: "two", by: END, optional: false },
    ];
    let first = Plan {
        finish: false,
        abort: rng.below(2) == 0,
        close: When::At(Time::from_nanos(rng.between(1, 50) * 1_000_000)),
        ..plan("one", one, 16, Reads::From(starts, 7))
    };
    let second = Plan {
        finish: false,
        abort: rng.below(2) == 0,
        close: When::At(Time::from_nanos(rng.between(1, 50) * 1_000_000)),
        ..plan("two", two, 16, Reads::From(starts, 7))
    };
    let mut world = World::new(seed, config, Referee::new(seed, expect));
    world.spawn(
        skein_io::Limits { intake: 7, ..limits(3) },
        Owner::new(seed, vec![serve("engine", vec![Some(first)])], vec![dial("engine", second)]),
    );
    world
}

/// The two owners' stream observations after a calm exchange.
#[derive(Debug, PartialEq, Eq)]
pub struct Seen {
    pub received: [Vec<u8>; 2],
    pub ended: [bool; 2],
    pub failed: [Option<skein_lib::stream::Fault>; 2],
}

/// Run the same owners over loopback and over a name (io.md, section 8).
#[must_use]
pub fn either(seed: u64, plans: [crate::owner::Plan; 2]) -> [Seen; 2] {
    let pairs = [(crate::scenarios::local(0), plans.clone()), (name(b"either"), plans)];
    pairs.map(|(addr, [served, dialer])| {
        let labels = [served.name, dialer.name];
        let listener = Serve { name: "either", addr, answers: vec![Some(served)], close: ServeClose::Answered(END) };
        let owner = Owner::new(seed, vec![listener], vec![dial("either", dialer)]);
        let mut proc = crate::world::Proc::new(skein_io::Limits { intake: 7, ..limits(3) }, owner);
        let mut sim = skein_sim::Sim::new(seed, Config::calm());
        let pid = sim.spawn_process();
        let mut directory = crate::owner::Directory::new();
        for step in 0..200_000 {
            assert!(step < 199_999, "either pair settles: {:?}", proc.log);
            let now = sim.now();
            assert!(now < END, "either pair ends within its deadline");
            sim.reap(pid, &mut proc.completions);
            proc.iterate(now, sim.wall(), &mut directory);
            sim.submit(pid, &mut proc.subs);
            if proc.owner.done() && proc.io.is_empty() && sim.in_flight(pid) == 0 {
                break;
            }
            if proc.busy(now) || sim.deferred(pid) || sim.ready(pid) > 0 {
                continue;
            }
            let next = [sim.next_due(), proc.io.next_deadline(), proc.owner.next_deadline(now)]
                .into_iter()
                .flatten()
                .min()
                .expect("the unsettled pair has work due");
            sim.advance_to(next);
        }
        proc.ledger.settled();
        sim.assert_quiescent(pid);
        sim.assert_no_open_fds(pid);
        if addr == name(b"either") {
            assert!(sim.trace().is_empty(), "joined pairs submit no kernel records");
        }
        let conns = labels.map(|label| proc.owner.conn(label).expect("both ends connected"));
        assert!(conns.iter().all(|conn| conn.error.is_none()), "both connects succeeded");
        Seen {
            received: conns.map(|conn| conn.received.clone()),
            ended: conns.map(|conn| conn.ended),
            failed: conns.map(|conn| conn.failed),
        }
    })
}

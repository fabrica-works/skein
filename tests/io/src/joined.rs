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

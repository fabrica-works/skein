//! Shared inputs for state digest scenarios.
use crate::{Expect, Judge, Script, Stamped, client, ms, server};
use skein_world::{Digests, Memory, World};

/// Two scripts exchanging bytes over a stamped machine.
#[must_use]
pub fn exchange(seed: u64, digests: Digests, noted: u64) -> World<Script, Judge, Stamped> {
    let mut world = scripts(seed, vec![Expect::Receives { at: 0, by: ms(1_000) }]);
    world.spawn(server);
    world.spawn(|| client(ms(10), ms(20)).noting(noted));
    world.digests(digests);
    world
}

/// An empty world ready to admit scripts under the counting allocator.
#[must_use]
pub fn scripts(seed: u64, expects: Vec<Expect>) -> World<Script, Judge, Stamped> {
    World::<Script, Judge>::new(seed, skein_sim::Config::chaos(), Judge::new(seed, expects), Memory::Checked)
        .with_machine(Stamped([7; 32]))
}

pub mod startup;

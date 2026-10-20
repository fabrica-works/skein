//! Small client and component limits, and a creation with its answer lost.
use crate::World;
use skein_forge::api::Write;
use skein_forge_client::{Condition, Effect, Entry, effect_key};
use skein_forge_connection::{Limits, Settings, Shared};
use skein_forge_machine_world::{fixtures, joint::REPO};
use skein_lib::Duration;

pub const LIMITS: skein_forge_client::Limits = skein_forge_client_world::LIMITS;

#[derive(Clone, Copy)]
pub(crate) enum Shape {
    Plain,
    Hooked,
}

impl Shape {
    pub(crate) fn limits(self) -> Limits {
        match self {
            Self::Plain => limits(),
            Self::Hooked => Limits { repositories: 1, ..fixtures::hook_limits() },
        }
    }

    pub(crate) fn settings(self) -> Settings {
        let mut settings = match self {
            Self::Plain => settings(),
            Self::Hooked => fixtures::hook_settings(),
        };
        settings.forges.get_mut(0).expect("fixture forge").access.as_mut().expect("fixture access").user = 1;
        settings
    }
}

/// The world over the fake's signed deliveries to the component's listener.
#[must_use]
pub fn hooked(seed: u64) -> World {
    World::with(seed, 0, Shape::Hooked)
}

pub(crate) fn limits() -> Limits {
    Limits { repositories: 1, ..fixtures::limits() }
}

pub(crate) fn shared() -> Shared {
    Shared {
        calls: LIMITS.calls,
        rows: LIMITS.rows,
        answer_bytes: LIMITS.answer_bytes,
        op_bytes: LIMITS.op_bytes,
        lifetime: LIMITS.lifetime,
        repositories: 1,
        ..fixtures::shared()
    }
}

pub(crate) fn settings() -> Settings {
    let mut settings = fixtures::settings();
    settings.forges.get_mut(0).expect("fixture forge").access.as_mut().expect("fixture access").user = 1;
    settings
}

/// Stops with the creation's error terminal queued, before client delivery.
#[must_use]
pub fn lost_creation(seed: u64) -> World {
    let mut world = World::new(seed, 0);
    world.run_for(Duration::from_secs(1));
    let entry = Entry {
        number: 1,
        task: 1,
        repository: REPO,
        effect: Effect {
            write: Write::CreateIssue {
                key: effect_key(b"kit", b"lost", LIMITS.op_bytes).expect("fixture effect key"),
                title: Box::from(b"kit".as_slice()),
                body: Box::from(b"kit".as_slice()),
            },
            condition: Condition::None,
        },
        start: None,
        attempt: None,
        failures: 0,
        queued: false,
    };
    world.lose_creation_answer(entry);
    world
}

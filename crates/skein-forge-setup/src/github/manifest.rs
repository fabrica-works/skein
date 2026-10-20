//! The bounded manifest page and its draw (forge-setup.md, 5.1 and 5.5).
use super::{
    app,
    given::{self, Given, Place, Refused},
};
use crate::reconcile::{Event, Need, Outcome, Stage, Why};
use alloc::boxed::Box;
use skein_forge::github as dialect;
use skein_lib::{Queue, Writer};

/// The account that will own the App (forge-setup.md, 5.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Target {
    Personal,
    Org(Box<[u8]>),
}

#[derive(Debug)]
struct Inputs {
    given: Given,
    target: Target,
    state: Box<[u8]>,
}
#[derive(Debug)]
enum Phase {
    Begin(Inputs),
    Draw(Inputs),
    Ended,
}
/// One manifest page; only the name's three random bytes are drawn here.
#[derive(Debug)]
pub struct Manifest {
    phase: Phase,
}
impl Manifest {
    /// Checks every input before copying it (forge-setup.md, 5.1 and 5.5).
    /// State is text here; the host binds its authority (5.7).
    pub fn new(given: &Given, target: &Target, state: &[u8]) -> Result<Self, Refused> {
        given::check(given)?;
        match target {
            Target::Personal => (),
            Target::Org(login) => {
                let bound = usize::try_from(given.limits.name_bytes).expect("a u32 fits a usize");
                if login.is_empty() || login.len() > bound {
                    return Err(Refused { place: Place::Target, other: None });
                }
                for byte in login {
                    if !byte.is_ascii_alphanumeric() && *byte != b'-' {
                        return Err(Refused { place: Place::Target, other: None });
                    }
                }
            }
        }
        if state.len() != 32 {
            return Err(Refused { place: Place::State, other: None });
        }
        for byte in state {
            if !byte.is_ascii_digit() && !(b'a'..=b'f').contains(byte) {
                return Err(Refused { place: Place::State, other: None });
            }
        }
        Ok(Self { phase: Phase::Begin(Inputs { given: given.clone(), target: target.clone(), state: state.into() }) })
    }
}

/// Pure page construction from checked inputs (forge-setup.md, 5.1 and 5.5).
/// Every intermediate and the final page is measured and capped before allocation.
fn page(given: &Given, target: &Target, state: &[u8], random: &[u8]) -> Option<Box<[u8]>> {
    if random.len() != 3 {
        return None;
    }
    let bound = usize::try_from(given.limits.page_bytes).ok()?;
    let name_length = given.word.len().checked_add(7)?;
    if name_length > usize::try_from(given.limits.name_bytes).ok()? || name_length > bound {
        return None;
    }
    let mut name = Writer::new(name_length);
    name.put(&given.word).ok()?;
    name.put(b"-").ok()?;
    for byte in random {
        name.put(&[hex(byte.wrapping_shr(4)), hex(byte & 0x0f)]).ok()?;
    }
    let name = name.finish();
    let hook_length = b"https://".len().checked_add(name.len())?.checked_add(b".invalid/".len())?;
    if hook_length > bound {
        return None;
    }
    let mut hook = Writer::new(hook_length);
    hook.put(b"https://").ok()?;
    hook.put(&name).ok()?;
    hook.put(b".invalid/").ok()?;
    let hook = hook.finish();
    let json = dialect::setup::manifest(
        &dialect::setup::Values {
            name: &name,
            url: &given.url,
            hook_url: &hook,
            redirect_url: &given.redirect_url,
            setup_url: &given.setup_url,
        },
        &skein_json::writer::Limits { depth: 2, length: given.limits.page_bytes },
    )
    .ok()?;
    let account = match target {
        Target::Personal => dialect::setup::Account::Person,
        Target::Org(login) => dialect::setup::Account::Organization(login),
    };
    let target = dialect::setup::form_target(&given.web, &account, state, given.limits.page_bytes)?;
    dialect::setup::form_page(&target, &json, given.limits.page_bytes)
}
fn hex(value: u8) -> u8 {
    match value {
        0..=9 => b'0'.checked_add(value).expect("a decimal digit fits"),
        10..=15 => b'a'.checked_add(value.checked_sub(10).expect("a hex letter")).expect("a hex letter fits"),
        16..=255 => unreachable!("a nibble is below sixteen"),
    }
}
fn stop(forge: u16) -> Need {
    app::stop(forge, Stage::App, b"manifest", Why::Unexpected)
}
/// Consumes one event, emitting at most one need in `reconcile::MAX_OUT` room.
/// Terminal machines ignore every later event.
pub fn step(machine: &mut Manifest, event: Event, out: &mut Queue<Need>) {
    let phase = core::mem::replace(&mut machine.phase, Phase::Ended);
    let need = match phase {
        Phase::Begin(inputs) => match event {
            Event::Begin => {
                machine.phase = Phase::Draw(inputs);
                Need::Draw(3)
            }
            Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => stop(inputs.given.forge),
        },
        Phase::Draw(inputs) => match event {
            Event::Drawn(random) => match page(&inputs.given, &inputs.target, &inputs.state, random.expose()) {
                Some(page) => Need::Done(Outcome::Manifest { state: inputs.state, page }),
                None => stop(inputs.given.forge),
            },
            Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Written => stop(inputs.given.forge),
        },
        Phase::Ended => return,
    };
    out.push(need);
}

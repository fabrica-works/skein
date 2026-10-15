//! People find semantic targets and report one shell act per step.
//! A person keeps a script, its position, the first time of the current wait,
//! and a terminal failure. It knows no client or engine state. A face searches
//! the current tree or answers Pending while a browser query is outstanding.
//! `next` consumes at most one step (skein's ui.md, section 17.1).
//!
//! | Step/event | Next state | Reports |
//! |---|---|---|
//! | Navigation, reload, found act | Following step | One shell operation |
//! | Target query pending | Same step | Wait for the face |
//! | See/gone met, pause elapsed | Following step | Wait for another iteration |
//! | Expectation unmet before deadline | Same step | Wait until change or deadline |
//! | Missing act, invalid act, expired expectation | Failed | Target and step |
//! | End or failed | Same terminal state | Done or the same failure |
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;

mod random;
#[cfg(test)]
mod tests;
mod tree;

use alloc::boxed::Box;
use core::fmt;
pub use random::{RandomDoing, RandomPerson};
use skein_lib::{Duration, Time};
use skein_ui::Act;
use skein_ui::{NodeId, Role};
pub use tree::TreeFace;

/// A semantic target, scoped through named ancestors, outermost first.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Find {
    pub within: Box<[Target]>,
    pub target: Target,
}
impl Find {
    /// Find a role whose accessible name contains these words.
    #[must_use]
    pub fn named(role: Role, name: &[u8]) -> Self {
        Self { within: Box::new([]), target: Target::Role { role, name: Box::from(name) } }
    }
    /// Find a role whose whole accessible name equals these bytes, including case.
    #[must_use]
    pub fn exactly_named(role: Role, name: &[u8]) -> Self {
        Self { within: Box::new([]), target: Target::ExactRole { role, name: Box::from(name) } }
    }
    /// Find visible text containing these words.
    #[must_use]
    pub fn text(text: &[u8]) -> Self {
        Self { within: Box::new([]), target: Target::Text { text: Box::from(text) } }
    }
    /// Scope the search through these ancestors, outermost first.
    #[must_use]
    pub fn within(mut self, regions: Box<[Target]>) -> Self {
        self.within = regions;
        self
    }
}

/// An accessible-name or visible-text target, with its declared matching rule.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Target {
    /// A semantic role and words in its name.
    Role { role: Role, name: Box<[u8]> },
    /// A semantic role and its whole accessible name, compared byte for byte.
    ExactRole { role: Role, name: Box<[u8]> },
    /// Words in visible text, including a drawing's ordinary fallback.
    Text { text: Box<[u8]> },
}

/// One instruction, with the application's own address type.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Step<A> {
    /// Navigate to an address.
    Go { address: A },
    /// Press a named control.
    Press { find: Find },
    /// Replace the whole typed value of a field.
    Type { find: Find, words: Box<[u8]> },
    /// Press Enter in a composer that submits on Enter.
    Send { find: Find },
    /// Wait for a target, at most this long.
    See { find: Find, within: Duration },
    /// Wait for a target to disappear, at most this long.
    Gone { find: Find, within: Duration },
    /// Reload the current client.
    Reload,
    /// Wait this long without issuing an operation.
    Pause { span: Duration },
}

/// A script's private progress; no client state is consulted.
#[derive(Debug)]
pub struct Person<A> {
    script: Box<[Step<A>]>,
    at: u32,
    since: Option<Time>,
    failed: Option<Why>,
}

/// The next operation or the reason the script cannot progress.
#[derive(PartialEq, Eq, Debug)]
pub enum Next<A> {
    /// Perform one shell operation.
    Do(Doing<A>),
    /// Run again when the face changes or the optional deadline passes.
    Wait { until: Option<Time> },
    /// Every instruction completed.
    Done,
    /// A terminal failure at a zero-based step number.
    Failed { step: u32, why: Why },
}

/// What a shell reports, without decoding the view owner's token.
#[derive(PartialEq, Eq, Debug)]
pub enum Doing<A> {
    /// Report a shell act by the visible node's id.
    Act { node: NodeId, act: Act },
    /// Navigate using the application's address vocabulary.
    Go { address: A },
    /// Reload the current client.
    Reload,
}

/// A failure described in the person's terms.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Why {
    /// A required control or expected target was absent.
    Missing { find: Find },
    /// A target stayed visible through its deadline.
    StillThere { find: Find },
    /// The selected node cannot perform the requested shell act.
    Unexpected { find: Find },
}

/// A face's query result, including asynchronous faces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Found {
    /// A matching visible node.
    Node(NodeId),
    /// No matching node in the requested regions.
    Absent,
    /// A query has not answered yet.
    Pending,
}

/// The accessible surface of the current client.
pub trait Face {
    /// Search by role, name and text, through the named ancestors.
    fn find(&mut self, find: &Find) -> Found;
    /// Check the control's interactive semantics when the face knows them.
    fn allows(&self, _node: NodeId, _act: &Act) -> bool {
        true
    }
}

impl<A: Clone> Person<A> {
    /// Own a finite script; each call advances by at most one instruction.
    #[must_use]
    pub fn new(script: Box<[Step<A>]>) -> Self {
        assert!(u32::try_from(script.len()).is_ok(), "script length fits step numbers");
        Self { script, at: 0, since: None, failed: None }
    }
    /// The zero-based next instruction, or the failed instruction.
    #[must_use]
    pub fn at(&self) -> u32 {
        self.at
    }
    fn advance(&mut self) {
        self.at = self.at.checked_add(1).expect("script length fits step numbers");
        self.since = None;
    }
    fn fail(&mut self, why: Why) -> Next<A> {
        self.failed = Some(why.clone());
        Next::Failed { step: self.at, why }
    }
    /// Query the face and report at most one operation. A failure stays terminal.
    #[must_use]
    pub fn next<F: Face>(&mut self, face: &mut F, now: Time) -> Next<A> {
        if let Some(why) = &self.failed {
            return Next::Failed { step: self.at, why: why.clone() };
        }
        let Some(step) = self.script.get(usize::try_from(self.at).expect("step fits usize")) else {
            return Next::Done;
        };
        let since = *self.since.get_or_insert(now);
        let result = match step {
            Step::Go { address } => Next::Do(Doing::Go { address: address.clone() }),
            Step::Reload => Next::Do(Doing::Reload),
            Step::Pause { span } => {
                let until = since.saturating_add(*span);
                if now < until {
                    return Next::Wait { until: Some(until) };
                }
                self.advance();
                return Next::Wait { until: None };
            }
            Step::Press { find } | Step::Type { find, .. } | Step::Send { find } => match face.find(find) {
                Found::Node(node) => {
                    let act = match step {
                        Step::Press { .. } => Act::Press,
                        Step::Type { words, .. } => Act::Input { value: words.clone() },
                        Step::Send { .. } => Act::Submit,
                        Step::Go { .. } | Step::See { .. } | Step::Gone { .. } | Step::Reload | Step::Pause { .. } => {
                            unreachable!("interactive step")
                        }
                    };
                    if !face.allows(node, &act) {
                        return self.fail(Why::Unexpected { find: find.clone() });
                    }
                    Next::Do(Doing::Act { node, act })
                }
                Found::Absent => return self.fail(Why::Missing { find: find.clone() }),
                Found::Pending => return Next::Wait { until: None },
            },
            Step::See { find, within } | Step::Gone { find, within } => {
                let found = face.find(find);
                let met = match step {
                    Step::See { .. } => matches!(found, Found::Node(_)),
                    Step::Gone { .. } => found == Found::Absent,
                    Step::Go { .. }
                    | Step::Press { .. }
                    | Step::Type { .. }
                    | Step::Send { .. }
                    | Step::Reload
                    | Step::Pause { .. } => unreachable!("expectation step"),
                };
                if met {
                    self.advance();
                    return Next::Wait { until: None };
                }
                let until = since.saturating_add(*within);
                if now < until {
                    return Next::Wait { until: Some(until) };
                }
                let why = match step {
                    Step::See { .. } => Why::Missing { find: find.clone() },
                    Step::Gone { .. } => Why::StillThere { find: find.clone() },
                    Step::Go { .. }
                    | Step::Press { .. }
                    | Step::Type { .. }
                    | Step::Send { .. }
                    | Step::Reload
                    | Step::Pause { .. } => unreachable!("expectation step"),
                };
                return self.fail(why);
            }
        };
        self.advance();
        result
    }
}

impl fmt::Display for Find {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.target)?;
        for region in self.within.iter().rev() {
            write!(f, " in the {region}")?;
        }
        Ok(())
    }
}
impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Role { role, name } => {
                write!(f, "{} named {}", tree::role_name(*role), alloc::string::String::from_utf8_lossy(name))
            }
            Self::ExactRole { role, name } => {
                write!(f, "{} named exactly {}", tree::role_name(*role), alloc::string::String::from_utf8_lossy(name))
            }
            Self::Text { text } => write!(f, "text containing {}", alloc::string::String::from_utf8_lossy(text)),
        }
    }
}
impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { find } => write!(f, "no {find}"),
            Self::StillThere { find } => write!(f, "still showing {find}"),
            Self::Unexpected { find } => write!(f, "cannot act on {find}"),
        }
    }
}

//! Forge component boundary (forge-connection.md, sections 2, 3.5, 7 and 8).
use skein_forge::api;
use skein_lib::Token;

/// One call's terminal, the forge client's `Event::Answered` field for field. The
/// component makes it; the routing hands it to the root as `Event::Forge`, and hands
/// back one the root did not take (`Request::Refused`, `Request::Returned`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Answered {
    /// The forge client's call token, echoed.
    pub call: Token,
    /// The requests the call wrote, cache fills included; 0 for a call that wrote none.
    pub cost: u32,
    pub result: Result<api::Answer, api::Error>,
}

/// From the routing.
#[derive(Debug)]
pub enum Request {
    /// The root's `Request::Forge`: ended by exactly one `Event::Answered`.
    Call { call: Token, repository: api::Repository, op: api::Op },
    /// An answer the root's admission did not take (`try_step`'s `Err`): held, offered
    /// again at the next `Offer`; at most one per call.
    Refused(Answered),
    /// A verified hint refused by root admission, held until the next offer.
    RefusedHint(crate::hooks::Hinted),
    /// An answer the root returned undecided in its drain (`Request::Busy`): kept until
    /// `Close`, never offered again (forge-connection.md, 2).
    Returned(Answered),
    /// The root admits again: held answers go up again, at most `MAX_OUT.above`.
    Offer,
    /// After the root's drain: calls in flight end with their connections and no answer
    /// goes up; `Event::Closed` answers it.
    Close,
}

/// To the routing; io's requests go in a queue of their own.
#[derive(Debug)]
pub enum Event {
    /// A call's one terminal.
    Answered(Answered),
    /// A verified delivery, after its response is queued.
    Hint(crate::hooks::Hinted),
    /// The webhooks listener failed to bind or was lost.
    ListenerLost,
    /// A forge's start check refused, naming the forge. At most once per forge.
    Stopped { forge: u16, why: Stopped },
    /// The terminal of `Close`: every connection closed.
    Closed,
}

/// Why a forge's start stopped the engine (forge-connection.md, 3.5 and 8).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stopped {
    /// `GET /api/v1/user` answered with another user's id.
    OtherUser { user: u64 },
    /// A `4xx` other than `429` to one of the start's reads: `401` or `403` refused the
    /// token; any other, the address names no Forgejo API.
    Refused { read: StartRead, status: u16 },
}

/// The start's two reads.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StartRead {
    /// `GET /api/v1/user`.
    User,
    /// `GET /api/v1/settings/api`.
    ApiSettings,
}

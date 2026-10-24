//! The owner's lending boundary (oauth.md, sections 6.3 and 6.6).

use crate::Unusable;
use alloc::boxed::Box;
use skein_lib::Duration;
use skein_oauth::SavedToken;

/// What the owner asks of its configured accounts.
#[derive(PartialEq, Eq, Hash)]
#[expect(missing_debug_implementations, reason = "credential values must never occur in traces")]
pub enum Request {
    /// Refreshes this account at its lead even while nobody holds its grant.
    Want { account: u32, wanted: bool },
    /// Revokes and forgets this record; ends `SignedOut`, Failed or Refused.
    SignOut { account: u32 },
    /// Removes this account after its sent exchange and durable keep settle.
    Remove { account: u32 },
    /// Starts this account's sign-in: Visit once, then `SignedIn`, Failed or Refused.
    SignIn { account: u32, subject: Option<Box<[u8]>> },
    /// A redirect handed in: the confidential owner's, or the address a person pasted for a public
    /// sign-in whose listener is up; Taken, or Refused with the sign-in still waiting (oauth.md, 6.5).
    Redirected { account: u32, uri: Box<[u8]> },
    /// The address the listener sends the browser on to once it takes this sign-in's redirect (oauth.md, 6.5).
    Return { account: u32, location: Box<[u8]> },
    /// Cancels this account's exchange; its requested or held work hears the terminal.
    Cancel { account: u32 },
    /// An access-only record; a newer generation replaces it, with only refusals answered.
    HandIn { account: u32, record: SavedToken },
    /// Holds the account's token until Release; answered Granted, Failed or Refused.
    Grant { account: u32 },
    /// The provider rejected the held generation; ends the hold if current.
    Rejected { account: u32, generation: u64 },
    /// Ends the owner's hold, with no answer.
    Release { account: u32 },
    /// Answers the owner-store Keep of this generation, with no event of its own.
    Kept { account: u32, generation: u64, keeping: Keeping },
    /// Answers the owner lock before a shared write.
    Locked { account: u32, answer: Locked },
    /// Drains the component; Closed is its terminal.
    Close,
    /// Ends the component immediately; Closed is its terminal.
    Abort,
}

/// What the component tells its owner.
#[derive(PartialEq, Eq, Hash)]
#[expect(missing_debug_implementations, reason = "credential values must never occur in traces")]
pub enum Event {
    /// The account has settled and its number is free for another add.
    Removed { account: u32 },
    /// Ends `SignOut`: the record is forgotten; revocation may be unconfirmed.
    SignedOut { account: u32, revoked: bool },
    /// The authorization URL for the owner to show, once per admitted sign-in.
    Visit { account: u32, url: Box<[u8]> },
    /// Ends `Redirected`: the sign-in took the redirect handed in; its own terminal follows (oauth.md, 6.5).
    Taken { account: u32 },
    /// Ends `SignIn` once kept, naming the record's subject and required scopes not granted in `missing`.
    SignedIn { account: u32, generation: u64, missing: Box<[u8]>, subject: Option<Box<[u8]>> },
    /// Answers Grant, then announces each new generation while the owner holds it.
    Granted { account: u32, token: Box<[u8]>, generation: u64, valid: Duration },
    /// An access-only record reached its lead; announced once per record.
    Expiring { account: u32, generation: u64 },
    /// A candidate for the owner's store; nothing new is lent until its Kept terminal.
    Keep { account: u32, record: SavedToken },
    /// Ends the request named by ends, or its held grant.
    Failed { account: u32, ends: Ends, failure: Failure },
    /// Refuses a request before work starts; its one terminal.
    Refused { account: u32, asked: Asked, why: Refusal },
    /// Take the shared record lock and read its record.
    Lock { account: u32 },
    /// Release the owner lock once its write settles.
    Unlock { account: u32 },
    /// Ends Close or Abort once, after all admitted work has settled.
    Closed,
}

/// The request a failure ends.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ends {
    /// A refresh while nobody holds the account's grant.
    Refresh,
    /// The owner's admitted sign-out.
    SignOut,
    /// The owner's admitted sign-in.
    SignIn,
    /// The owner's requested or held grant.
    Grant,
}

/// Why the account cannot serve its grant.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Failure {
    /// The shared record lock could not be taken; transient.
    Busy,
    /// The record lacks a scope the registration requires; sign in again.
    NotGranted,
    /// A sign-in or shared record has another subject than expected.
    AnotherSubject,
    /// A sign-in's candidate could not be durably kept; nothing of it is lent.
    NotKept,
    /// The issuer exchange failed, as named by the OAuth client.
    Exchange(skein_oauth::Failure),
    /// No valid record, or the provider rejected its token; a newer record is needed.
    Expired,
    /// The private record did not load, with its location and unread cause (oauth.md, section 6.4).
    Unloaded { at: Place, why: Unloaded },
}

/// The request a refusal answers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Asked {
    /// The owner's wanted-account flag.
    Want,
    /// The owner's sign-out request.
    SignOut,
    /// The owner's account addition.
    Add,
    /// The owner's account removal.
    Remove,
    /// The owner's sign-in request.
    SignIn,
    /// The owner's redirect, confidential or pasted.
    Redirected,
    /// The owner's access-only record.
    HandIn,
    /// The owner's requested grant.
    Grant,
}

/// Why a request was refused before any work.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// A live or removing account already holds this number.
    Taken,
    /// The added configuration cannot run.
    Unusable(Unusable),
    /// This account already has its one exchange.
    Busy,
    /// No sign-in of the account waits for a redirect.
    NotWaiting,
    /// The redirect differs from the bound address or violates its admitted fields or syntax.
    Redirect,
    /// The redirect carries another sign-in's state; the sign-in still waits (oauth.md, 6.5).
    State,
    /// The owner has closed or aborted its component.
    Closed,
    /// The number names no live account.
    Account,
    /// The requested operation does not belong to this account's source.
    Source,
    /// The account has no record once its load has settled.
    NoRecord,
    /// The account, exchange or public listener bound admits no more work.
    Full { bound: u32 },
    /// A handed-in record carries a refresh token its source alone may rotate.
    NotAccessOnly,
    /// The account's grant is already held.
    Held,
    /// The handed-in record violates the client's record bounds or encoding.
    Record(skein_oauth::DecodeError),
}

/// The owner's answer to a candidate Keep; its durable-store terminal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Keeping {
    /// The record was stored durably and can be lent.
    Kept,
    /// The record was not stored; no part of it may be lent.
    NotKept,
}

/// The part of a private keeper whose loading stopped (oauth.md, section 6.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    /// The private directory, opened before any record is served.
    Directory,
    /// The account's whole record file.
    File,
}

/// Why loading left an account without a record (oauth.md, section 6.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Unloaded {
    /// Damaged bytes, another version, or no issuer-required client id; a sign-in may replace their digest.
    Unreadable,
    /// io refused unsafe metadata without reading; sign-in fails before Visit.
    Refused(skein_io::file::Unsafe),
    /// io's kernel error; sign-in fails before Visit.
    Failed(skein_io::kernel::Error),
    /// The file operation reached its stall deadline; sign-in fails before Visit.
    Stalled,
    /// io observed more bytes than the record bound; sign-in fails before Visit.
    TooLarge { size: u64 },
}

/// The owner's answer to a shared record lock (oauth.md, section 6.4).
#[derive(PartialEq, Eq, Hash)]
#[expect(missing_debug_implementations, reason = "credential values must never occur in traces")]
pub enum Locked {
    /// The lock is held; private keepers load their own record.
    Held { record: Option<SavedToken> },
    /// No lock was taken; the write fails as busy.
    Busy,
}

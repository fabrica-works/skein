//! A send's dialect and credential (forge-setup.md, 4.2).
use skein_forge::{forgejo, github};
use skein_lib::Secret;

/// The request a `Send` makes: the dialect of the forge it is for.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Call {
    Forgejo(forgejo::request::Request),
    GitHub(github::request::Request),
}
impl From<forgejo::request::Request> for Call {
    fn from(request: forgejo::request::Request) -> Self {
        Self::Forgejo(request)
    }
}
impl From<github::request::Request> for Call {
    fn from(request: github::request::Request) -> Self {
        Self::GitHub(request)
    }
}

/// How a `Send` is authorised; secrets' `Debug` shows their length only (4.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Auth {
    /// `Authorization: token <value>`: a Forgejo token.
    Token(Secret),
    /// The host signs a fresh JWT with `key` (PKCS#1 DER) as App `id` for each
    /// request, sent as `Authorization: Bearer <jwt>`. A key it cannot sign
    /// with steps `Event::Reply(Reply::Unsigned)` and the request is never sent.
    App { id: u64, key: Secret },
    /// An installation's token: `Authorization: Bearer <value>`.
    Bearer(Secret),
    /// No credential.
    None,
}

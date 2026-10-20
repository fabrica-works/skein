//! Forge component settings (forge-connection.md, sections 2, 3.5, 7 and 8).
use alloc::boxed::Box;
use skein_io::Addr;
use skein_lib::{List, Secret};

/// Owner inputs held by the component until close.
#[derive(Debug)]
pub struct Settings {
    /// At most `Limits::forges`, numbers unique.
    pub forges: List<Forge>,
    /// The listener; each forge's secret is in its access (forge-connection.md, 6.1).
    pub webhooks: Option<crate::hooks::Webhooks>,
    /// Owner word used in markers and webhook hints, at most 16 bytes.
    pub word: Box<[u8]>,
    /// The user agent sent on every request.
    pub user_agent: Box<[u8]>,
}

/// One forge; Host derives from its address and transport name.
#[derive(Debug)]
pub struct Forge {
    /// The domain's forge number: a repository's `forge`.
    pub number: u16,
    pub kind: Kind,
    /// The API's socket address, resolved before the loop; never an in-process name.
    pub address: Addr,
    pub transport: Transport,
    /// The access the forge holds; with none it runs no start, and its calls are
    /// answered `NotConnected`.
    pub access: Option<Access>,
}

/// What lets the component act on a forge as its user (forge-connection.md,
/// 3.5): handed at construction or by `Request::Connect`, replaced whole,
/// never logged.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Access {
    /// The API token, in `Authorization: token <t>`: never logged, granted or stored.
    pub credential: skein_forge::credential::Credential,
    /// The forge user the token stands for, never 0, checked by the start's
    /// `GET /api/v1/user`.
    pub user: u64,
    /// The secret of the forge's webhook path; only beside a listener.
    pub webhook_secret: Option<Secret>,
}

/// The documents a forge speaks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    /// The pinned Forgejo's API under `/api/v1`.
    Forgejo,
}

/// A forge's connections.
#[derive(Debug)]
pub enum Transport {
    /// Only to a socket address on loopback.
    Plaintext { name: Option<Box<[u8]>> },
    /// TLS with the configured server name and trust.
    Tls { server_name: skein_tls::Name, trust: skein_tls::Config },
}

/// Derive the request Host from the checked address and transport name.
#[must_use]
pub(crate) fn host(forge: &Forge) -> Option<Box<[u8]>> {
    let address = match forge.address {
        Addr::Ip(address) => address,
        Addr::InProcess(_) => return None,
    };
    let name = match &forge.transport {
        Transport::Plaintext { name } => name.as_deref(),
        Transport::Tls { server_name, .. } => match server_name.server() {
            skein_tls::Server::Dns(text) => Some(text.as_bytes()),
            skein_tls::Server::Ip(ip) => {
                return skein_forge::host::host(core::net::SocketAddr::new(ip, address.port()), None);
            }
        },
    };
    skein_forge::host::host(address, name)
}
/// Newly retained owner slices, priced separately from bounded tables and credentials.
#[must_use]
pub fn owned_bytes(settings: &Settings) -> Option<u64> {
    let mut total =
        u64::try_from(settings.word.len()).ok()?.checked_add(u64::try_from(settings.user_agent.len()).ok()?)?;
    for forge in &settings.forges {
        match &forge.transport {
            Transport::Plaintext { name: Some(name) } => {
                total = total.checked_add(u64::try_from(name.len()).ok()?)?;
            }
            Transport::Plaintext { name: None } => {}
            Transport::Tls { server_name, .. } => match server_name.server() {
                skein_tls::Server::Dns(text) => {
                    total = total.checked_add(u64::try_from(text.len()).ok()?)?;
                }
                skein_tls::Server::Ip(_) => {}
            },
        }
    }
    Some(total)
}

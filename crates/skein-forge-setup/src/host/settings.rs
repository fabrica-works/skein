//! The forges and values checked before the host starts (forge-setup.md, 6.3).
use super::{Plan, boundary::Dialect};
use crate::{github, given::Given};
use alloc::boxed::Box;
use skein_lib::{Duration, List};
#[derive(Debug)]
pub struct Settings {
    pub forges: List<Forge>,
    pub user_agent: Box<[u8]>,
}
#[derive(Debug)]
pub struct Forge {
    pub kind: Kind,
    pub address: skein_io::Addr,
    pub transport: Transport,
}
/// A forge's dialect, and the given of its plans (forge-setup.md, 6.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Kind {
    Forgejo(Given),
    GitHub(github::given::Given),
}
impl Forge {
    /// The forge's number, whichever its kind.
    #[must_use]
    pub fn number(&self) -> u16 {
        match &self.kind {
            Kind::Forgejo(given) => given.forge,
            Kind::GitHub(given) => given.forge,
        }
    }
    #[must_use]
    pub fn forgejo(&self) -> Option<&Given> {
        match &self.kind {
            Kind::Forgejo(given) => Some(given),
            Kind::GitHub(_) => None,
        }
    }
    #[must_use]
    pub fn github(&self) -> Option<&github::given::Given> {
        match &self.kind {
            Kind::Forgejo(_) => None,
            Kind::GitHub(given) => Some(given),
        }
    }
    /// Whether the plan is for this forge's kind.
    pub(super) fn takes(&self, plan: &Plan) -> bool {
        match plan.dialect() {
            Dialect::Both => true,
            Dialect::Forgejo => self.forgejo().is_some(),
            Dialect::GitHub => self.github().is_some(),
        }
    }
    /// The run's deadline and retry delays, from the given of its kind.
    pub(super) fn times(&self) -> Times {
        match &self.kind {
            Kind::Forgejo(given) => Times {
                deadline: given.limits.deadline,
                backoff: given.limits.backoff,
                backoff_max: given.limits.backoff_max,
            },
            Kind::GitHub(given) => Times {
                deadline: given.limits.deadline,
                backoff: given.limits.backoff,
                backoff_max: given.limits.backoff_max,
            },
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Times {
    pub deadline: Duration,
    pub backoff: Duration,
    pub backoff_max: Duration,
}
#[derive(Debug)]
pub enum Transport {
    Plaintext {
        name: Option<Box<[u8]>>,
    },
    /// TLS with the configured server name and trust.
    Tls {
        server_name: skein_tls::Name,
        trust: skein_tls::Config,
    },
}
pub(super) fn host(forge: &Forge) -> Option<Box<[u8]>> {
    let address = match forge.address {
        skein_io::Addr::Ip(address) => address,
        skein_io::Addr::InProcess(_) => return None,
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

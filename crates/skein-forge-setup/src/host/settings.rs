//! The forges and values checked before the host starts (forge-setup.md, 6.3).
use crate::given::Given;
use alloc::boxed::Box;
use skein_lib::List;
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
    pub given: Given,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Forgejo,
}
#[derive(Debug)]
pub enum Transport {
    Plaintext { name: Option<Box<[u8]>> },
}
pub(super) fn host(forge: &Forge) -> Option<Box<[u8]>> {
    let address = match forge.address {
        skein_io::Addr::Ip(address) => address,
        skein_io::Addr::InProcess(_) => return None,
    };
    let Transport::Plaintext { name } = &forge.transport;
    skein_forge::host::host(address, name.as_deref())
}

//! Loopback io faces for scripted peers (fake-llm.md, section 3; oauth.md,
//! section 5). Each peer keeps its independent machine, bounded connections,
//! observations and io queues; it never knows a client's internal state or
//! application policy. Constructors start listening; `Host::iterate` drives
//! the passes, and `shutdown` closes admission and every connection once
//! the clients have settled. A scenario about peer hang-up names it at its
//! shutdown call; no peer closes merely to help its client settle
//! (testing-strategy.md, section 6).
//!
//! These testing step machines use ordinary Rust (programming-model.md,
//! section 10.2). Plaintext replays; TLS uses the fixed Skein test chain and
//! real rustls cryptography (testing-strategy.md, section 4.4).
//! An application's real loop hosts the issuer on loopback in its process.
//! Its script lists every token; give `oauth::Peer::granted()` to the LLM
//! peer at start, before serving. The test follows each visit to the listener
//! as the scripted browser does (oauth.md, section 6.8; fake-llm.md, section 6).
#![forbid(unsafe_code)]

mod face;
pub mod forge;
pub mod llm;
pub mod oauth;
pub mod transport;

pub use face::{Error, Limits};
pub use transport::Transport;

/// A listening peer borrowed by a real loop (ui-web.md, section 14.2).
/// The loop delivers a successful kernel listen completion and drives the
/// host before reading its address; this trait does not poll readiness.
pub trait Listening: skein_shell::Host {
    /// The bound address, read only after the host has processed listening.
    fn address(&self) -> std::net::SocketAddr;
    /// Ends admission once the peer's clients have settled. Drive the host
    /// until `Host::is_empty` before returning it to its owner.
    fn shutdown(&mut self);
}

impl Listening for forge::Peer {
    fn address(&self) -> std::net::SocketAddr {
        self.address().expect("the forge peer has processed its successful listen completion")
    }

    fn shutdown(&mut self) {
        self.shutdown();
    }
}

impl Listening for oauth::Peer {
    fn address(&self) -> std::net::SocketAddr {
        self.address().expect("the OAuth peer has processed its successful listen completion")
    }

    fn shutdown(&mut self) {
        self.shutdown();
    }
}

impl Listening for llm::Peer {
    fn address(&self) -> std::net::SocketAddr {
        self.address().expect("the LLM peer has processed its successful listen completion")
    }

    fn shutdown(&mut self) {
        self.shutdown();
    }
}

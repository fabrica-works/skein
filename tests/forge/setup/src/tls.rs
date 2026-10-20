//! In-memory TLS beneath the setup world's HTTP peers.
use skein_forge_machine_world::pipe::{Cut, Pipe};
use skein_lib::{
    Intake,
    stream::{Down, Read, Up},
};
use skein_tls_world::{pki, server::Server};

pub(crate) enum Carrier {
    Plain(Box<Pipe>),
    Tls(Box<Tls>),
}
pub(crate) struct Tls {
    server: Server,
    plain: Intake,
    wire: Intake,
    client: Option<(Read, u32)>,
    peer: Option<(Read, u32)>,
    finished: bool,
    ended: bool,
}
impl Carrier {
    pub fn new(seed: u64, tls: bool) -> Self {
        if tls {
            Self::Tls(Box::new(Tls {
                server: Server::new(pki::Server::plain().config()),
                plain: Intake::with_capacity(131_072),
                wire: Intake::with_capacity(131_072),
                client: None,
                peer: None,
                finished: false,
                ended: false,
            }))
        } else {
            Self::Plain(Box::new(Pipe::new(seed, Cut::Never)))
        }
    }
    pub fn client_down(&mut self, down: Down) {
        match self {
            Self::Plain(pipe) => pipe.client_down(down),
            Self::Tls(tls) => {
                match down {
                    Down::Demand { read: Read::Nothing, room: 0 } => {
                        assert!(tls.client.take().is_some());
                    }
                    Down::Demand { read, room } => assert!(tls.client.replace((read, room)).is_none()),
                    Down::Send(bytes) => {
                        tls.server.receive(&bytes);
                        assert!(tls.server.failed.is_none(), "valid TLS client");
                        tls.plain.append(&core::mem::take(&mut tls.server.received)).expect("plaintext room");
                    }
                    Down::Finish => (),
                }
                tls.flush();
            }
        }
    }
    pub fn client_up(&mut self) -> Option<Up> {
        match self {
            Self::Plain(pipe) => pipe.client_up(),
            Self::Tls(tls) => {
                let (read, room) = tls.client?;
                let event = tls.wire.meet(read).map(Up::Bytes).or_else(|| {
                    if room > 0 {
                        Some(Up::Room)
                    } else if tls.finished && !tls.ended && tls.wire.is_empty() && read != Read::Nothing {
                        tls.ended = true;
                        Some(Up::End)
                    } else {
                        None
                    }
                });
                if event.is_some() {
                    tls.client = None;
                }
                event
            }
        }
    }
    pub fn server_down(&mut self, down: Down) {
        match self {
            Self::Plain(pipe) => pipe.server_down(down),
            Self::Tls(tls) => {
                match down {
                    Down::Demand { read: Read::Nothing, room: 0 } => {
                        assert!(tls.peer.take().is_some());
                    }
                    Down::Demand { read, room } => assert!(tls.peer.replace((read, room)).is_none()),
                    Down::Send(bytes) => tls.server.write(&bytes),
                    Down::Finish => {
                        tls.server.close_notify();
                        tls.finished = true;
                    }
                }
                tls.flush();
            }
        }
    }
    pub fn server_up(&mut self) -> Option<Up> {
        match self {
            Self::Plain(pipe) => pipe.server_up(),
            Self::Tls(tls) => {
                let (read, room) = tls.peer?;
                let event = tls.plain.meet(read).map(Up::Bytes).or_else(|| (room > 0).then_some(Up::Room));
                if event.is_some() {
                    tls.peer = None;
                }
                event
            }
        }
    }
    pub fn has_work(&self) -> bool {
        match self {
            Self::Plain(pipe) => pipe.has_work(),
            Self::Tls(tls) => {
                tls.client.is_some_and(|(_, room)| room > 0 || !tls.wire.is_empty() || (tls.finished && !tls.ended))
                    || tls.peer.is_some_and(|(_, room)| room > 0 || !tls.plain.is_empty())
            }
        }
    }
}
impl Tls {
    fn flush(&mut self) {
        self.wire.append(&self.server.transmit()).expect("ciphertext room");
    }
}

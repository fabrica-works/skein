use super::{make, request};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore,
    pki_types::{CertificateDer, ServerName},
};
use skein_fake_peers::{Transport, transport::ROOT};
use skein_io::kernel::{Done, Family, Fd, Op};
use skein_shell::Host;
use skein_sim::{Pid, Sim};
use std::{io::Cursor, net::Ipv4Addr, sync::Arc};

fn flush(
    sim: &mut Sim,
    server: Pid,
    client: Pid,
    peer: &mut skein_fake_peers::llm::Peer,
    fd: Fd,
    tls: &mut ClientConnection,
) {
    let mut bytes = Vec::new();
    while tls.wants_write() {
        tls.write_tls(&mut bytes).expect("buffered client handshake write");
    }
    request::send(sim, server, client, peer, fd, &bytes);
}

#[test]
fn the_exported_root_alone_authenticates_a_tls_llm_peer_at_127_0_0_1() {
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(ROOT)).unwrap();
    assert_eq!(roots.len(), 1);
    let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut tls = ClientConnection::new(Arc::new(config), ServerName::try_from("127.0.0.1").unwrap()).unwrap();
    let mut peer = make((Ipv4Addr::LOCALHOST, 0).into(), Transport::Tls).unwrap();
    let mut sim = Sim::new(23, skein_sim::Config::calm());
    let (server, client) = (sim.spawn_process(), sim.spawn_process());
    for _ in 0..1000 {
        if peer.address().is_some() {
            break;
        }
        request::pass(&mut sim, server, &mut peer);
    }
    let socket = request::call(&mut sim, server, client, &mut peer, Op::Socket { family: Family::Ipv4 });
    let Done::Fd(fd) = socket.result.unwrap() else { panic!("socket descriptor") };
    let address = peer.address().unwrap();
    assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
    request::call(&mut sim, server, client, &mut peer, Op::Connect { fd, addr: address }).result.unwrap();
    for _ in 0..1000 {
        flush(&mut sim, server, client, &mut peer, fd, &mut tls);
        if !tls.is_handshaking() {
            break;
        }
        let complete = request::call(
            &mut sim,
            server,
            client,
            &mut peer,
            Op::Recv { fd, buf: vec![0; 32_768].into_boxed_slice() },
        );
        let Done::Count(count) = complete.result.unwrap() else { panic!("TLS read count") };
        assert!(count > 0);
        let Op::Recv { buf, .. } = complete.kind else { panic!("TLS read buffer") };
        tls.read_tls(&mut Cursor::new(&buf[..usize::try_from(count).unwrap()])).unwrap();
        tls.process_new_packets().unwrap();
    }
    assert!(!tls.is_handshaking());
    assert_eq!(tls.peer_certificates().unwrap().len(), 2);
    request::call(&mut sim, server, client, &mut peer, Op::Close { fd }).result.unwrap();
    peer.shutdown();
    for _ in 0..1000 {
        if peer.is_empty() {
            break;
        }
        request::pass(&mut sim, server, &mut peer);
    }
    assert!(peer.is_empty());
    for pid in [server, client] {
        sim.assert_quiescent(pid);
        sim.assert_no_open_fds(pid);
    }
}

//! Literal HTTP requests through a socket while the independent peer progresses.
use skein_fake_peers::llm::Peer;
use skein_io::kernel::{Complete, Done, Family, Op, Submit};
use skein_lib::{Queue, Token};
use skein_shell::Host;
use skein_sim::{Pid, Sim};

pub(super) fn pass(sim: &mut Sim, server: Pid, peer: &mut Peer) {
    sim.reap(server, peer.completions());
    peer.iterate(sim.now(), sim.wall());
    sim.submit(server, peer.submissions());
    sim.advance();
}

pub(super) fn call(sim: &mut Sim, server: Pid, client: Pid, peer: &mut Peer, kind: Op) -> Complete {
    let mut submissions = Queue::with_capacity(1);
    let mut completions = Queue::with_capacity(1);
    submissions.push(Submit { op: Token::new(1), kind });
    sim.submit(client, &mut submissions);
    for _ in 0..1000 {
        sim.reap(client, &mut completions);
        if let Some(complete) = completions.pop() {
            return complete;
        }
        pass(sim, server, peer);
    }
    panic!("the scripted socket operation must settle");
}

pub(super) fn send(sim: &mut Sim, server: Pid, client: Pid, peer: &mut Peer, fd: skein_io::kernel::Fd, bytes: &[u8]) {
    let mut from = 0;
    while usize::try_from(from).expect("request length") < bytes.len() {
        let complete = call(sim, server, client, peer, Op::Send { fd, bytes: bytes.into(), from });
        let Done::Count(sent) = complete.result.expect("literal request sent") else { panic!("send count") };
        from += sent;
    }
}

pub fn request(
    sim: &mut Sim,
    server: Pid,
    client: Pid,
    peer: &mut Peer,
    path: &[u8],
    fields: &[u8],
    body: &[u8],
) -> Vec<u8> {
    for _ in 0..1000 {
        if peer.address().is_some() {
            break;
        }
        pass(sim, server, peer);
    }
    let complete = call(sim, server, client, peer, Op::Socket { family: Family::Ipv4 });
    let Done::Fd(fd) = complete.result.expect("socket opened") else { panic!("socket descriptor") };
    call(sim, server, client, peer, Op::Connect { fd, addr: peer.address().expect("listening peer") })
        .result
        .expect("connected to peer");
    let mut head = b"POST ".to_vec();
    head.extend_from_slice(path);
    head.extend_from_slice(
        b" HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: ",
    );
    head.extend_from_slice(body.len().to_string().as_bytes());
    head.extend_from_slice(b"\r\n");
    head.extend_from_slice(fields);
    head.extend_from_slice(b"\r\n");
    send(sim, server, client, peer, fd, &head);
    send(sim, server, client, peer, fd, body);
    let mut response = Vec::new();
    loop {
        let complete = call(sim, server, client, peer, Op::Recv { fd, buf: Box::new([0; 1024]) });
        let Done::Count(count) = complete.result.expect("response read") else { panic!("read count") };
        if count == 0 {
            break;
        }
        let Op::Recv { buf, .. } = complete.kind else { panic!("receive buffer") };
        response.extend_from_slice(&buf[..usize::try_from(count).expect("read count")]);
    }
    call(sim, server, client, peer, Op::Close { fd }).result.expect("socket closed");
    response
}

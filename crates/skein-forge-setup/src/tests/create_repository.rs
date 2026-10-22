//! The creation request uses the same status and transport outcomes as organization creation.
use super::{fixtures, github_send::answering, host};
use crate::{
    host::{Forge, Kind, Transport, connection::Connection},
    reconcile::Reply,
};
use skein_forge::forgejo::{
    document::{CreatedRepository, Document},
    request::Request,
};
use skein_lib::{Env, Queue, Time, Token, Wall, stream::Up};

fn connection() -> Connection {
    let forge = Forge {
        kind: Kind::Forgejo(fixtures::given()),
        address: skein_io::Addr::Ip(([127, 0, 0, 1], 3000).into()),
        transport: Transport::Plaintext { name: None },
    };
    let request =
        Request::CreateRepository { org: Box::from(*b"creation"), name: Box::from(*b"starter"), private: true };
    Connection::new(Token::new(1), &forge, b"kit", &request.into(), &fixtures::secret(fixtures::ADMIN), &host::limits())
        .expect("creation fits the shared transport")
}
#[test]
fn repository_creation_preserves_each_http_refusal_without_reading_its_body() {
    for (head, status) in [
        (b"HTTP/1.1 409 Conflict\r\nContent-Length: 99\r\n\r\n".as_slice(), 409),
        (b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: 99\r\n\r\n", 422),
        (b"HTTP/1.1 404 Not Found\r\nContent-Length: 99\r\n\r\n", 404),
        (b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 99\r\n\r\n", 401),
        (b"HTTP/1.1 403 Forbidden\r\nContent-Length: 99\r\n\r\n", 403),
    ] {
        assert_eq!(answering(connection(), head, b"unread").result, Some(Reply::Status(status)));
    }
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the transport fixture formats its HTTP content length")]
fn repository_creation_returns_the_decoded_document_through_the_shared_transport() {
    let body = br#"{"id":1,"full_name":"creation/starter","default_branch":"main","clone_url":"http://h/r.git","ssh_url":"ssh://h/r.git"}"#;
    let head = format!("HTTP/1.1 201 Created\r\nContent-Length: {}\r\n\r\n", body.len());
    assert_eq!(
        answering(connection(), head.as_bytes(), body).result,
        Some(Reply::Document(Document::CreatedRepository(CreatedRepository {
            id: 1,
            full_name: Box::from(*b"creation/starter"),
            default_branch: Box::from(*b"main"),
            clone_url: Box::from(*b"http://h/r.git"),
            ssh_url: Box::from(*b"ssh://h/r.git"),
        })))
    );
}
#[test]
fn a_repository_creation_answer_missing_its_clone_fields_is_down() {
    assert_eq!(
        answering(connection(), b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n", b"{}").result,
        Some(Reply::Down)
    );
}
#[test]
fn repository_creation_connection_loss_and_early_end_are_down() {
    for event in [Up::Failed(skein_lib::stream::Fault::Reset), Up::End] {
        let mut connection = connection();
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: host::limits() };
        let mut io = Queue::with_capacity(16);
        connection.socket = Some(Token::new(2));
        connection.connect(&env);
        connection.stream(&env, event);
        for _ in 0_u32..16 {
            if !connection.pump(&env, &mut io) {
                break;
            }
        }
        assert_eq!(connection.result, Some(Reply::Down));
    }
}

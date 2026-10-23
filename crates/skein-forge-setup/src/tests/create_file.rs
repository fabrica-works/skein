use super::{fixtures, github_send::answering, host};
use crate::{
    host::{Forge, Kind, Transport, connection::Connection},
    reconcile::Reply,
};
use skein_forge::forgejo::{
    document::Document,
    id,
    request::{Author, Repo, Request},
};
use skein_lib::Token;
fn connection() -> Connection {
    let forge = Forge {
        kind: Kind::Forgejo(fixtures::given()),
        address: skein_io::Addr::Ip(([127, 0, 0, 1], 3000).into()),
        transport: Transport::Plaintext { name: None },
    };
    let request = Request::CreateFile {
        at: Repo { owner: b"first".as_slice().into(), name: b"starter".as_slice().into() },
        path: b"file".as_slice().into(),
        branch: b"main".as_slice().into(),
        content: b"bytes".as_slice().into(),
        message: b"Add file".as_slice().into(),
        author: Author { name: b"kit".as_slice().into(), email: b"kit@example.invalid".as_slice().into() },
    };
    Connection::new(Token::new(1), &forge, b"kit", &request.into(), &fixtures::secret(fixtures::ADMIN), &host::limits())
        .expect("file fits")
}
#[test]
fn file_creation_preserves_422_and_404_without_reading_the_body() {
    for (head, status) in [
        (b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: 99\r\n\r\n".as_slice(), 422),
        (b"HTTP/1.1 404 Not Found\r\nContent-Length: 99\r\n\r\n", 404),
    ] {
        assert_eq!(answering(connection(), head, b"unread").result, Some(Reply::Status(status)));
    }
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the transport fixture formats its HTTP content length")]
fn file_creation_answers_201_with_the_new_commit() {
    let body = br#"{"commit":{"sha":"0000000000000000000000000000000000000001"}}"#;
    let head = format!("HTTP/1.1 201 Created\r\nContent-Length: {}\r\n\r\n", body.len());
    assert_eq!(
        answering(connection(), head.as_bytes(), body).result,
        Some(Reply::Document(Document::CreatedFile {
            commit: id::read(b"0000000000000000000000000000000000000001").expect("id")
        }))
    );
}

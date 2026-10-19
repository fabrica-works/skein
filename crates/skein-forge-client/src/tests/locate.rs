//! Locate bounds and its nonzero answer (protocol/forge.md, 12).
use super::*;

#[test]
fn a_locate_past_the_call_bound_is_refused_too_large() {
    let mut h = Harness::new(LIMITS);
    let requests = h.step(Event::Read {
        owner: Token::new(1),
        repository: Repository { forge: REPO.forge, repository: 0 },
        read: Read::Locate { owner: Box::new([b'a'; 200]), name: Box::new([b'b'; 57]) },
    });
    terminal(&requests, 1, Error::TooLarge);
    assert!(h.send().is_none());
}

#[test]
fn a_located_repository_zero_is_an_invalid_answer() {
    let mut h = Harness::new(LIMITS);
    let repository = Repository { forge: REPO.forge, repository: 0 };
    assert!(
        h.step(Event::Read {
            owner: Token::new(1),
            repository,
            read: Read::Locate { owner: Box::from(&b"org"[..]), name: Box::from(&b"repo"[..]) },
        })
        .is_empty()
    );
    let (sent, address) = h.sent();
    assert_eq!(address, repository);
    terminal(&h.answer(&sent, 1, Ok(Answer::Located { repository: 0 })), 1, Error::InvalidAnswer);
}

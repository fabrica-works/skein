#![cfg(test)]
pub mod common;
use common::forgejo;
use skein_forge::forgejo::{
    client,
    decode::Malformed,
    document::{Body, Document},
    request::Request,
    server::{self, Answer},
};
fn request(private: bool) -> Request {
    Request::CreateRepository {
        org: Box::from(*b"creation"),
        name: if private { Box::from(*b"starter") } else { Box::from(*b"public") },
        private,
    }
}
#[test]
fn repository_creation_requests_match_the_disposable_forgejos_recorded_bytes_and_fake_route() {
    for (case, private) in [("setup-repository-created", true), ("setup-repository-created-public", false)] {
        let t = forgejo::transcript(case);
        let request = request(private);
        let encoded = client::encode(&request, &forgejo::limits()).unwrap();
        assert_eq!(encoded.method, t.method);
        assert_eq!(encoded.target.as_ref(), t.target);
        assert_eq!(encoded.body.as_deref(), Some(t.request_body.as_slice()));
        assert_eq!(forgejo::decode_route(t.method, &t.target, &t.request_body, &forgejo::limits()).unwrap().0, request);
    }
}
#[test]
fn repository_creation_answers_decode_in_one_seven_and_whole_byte_chunks() {
    for (case, private) in [("setup-repository-created", true), ("setup-repository-created-public", false)] {
        let t = forgejo::transcript(case);
        assert_eq!(t.status, 201);
        for chunk in [1, 7, t.body.len()] {
            let decoded = forgejo::decode_chunks(&request(private), &t.body, chunk);
            assert_eq!(
                decoded.document,
                Ok(Document::CreatedRepository(common::cases::setup::created_repository(private)))
            );
            assert!(decoded.rows.is_empty());
        }
    }
}
#[test]
fn the_fake_creation_answer_round_trips_with_the_recorded_shape() {
    for (case, private) in [("setup-repository-created", true), ("setup-repository-created-public", false)] {
        let value = common::cases::setup::created_repository(private);
        let encoded = server::encode(&Answer::CreatedRepository(value.clone()), &forgejo::limits()).unwrap();
        assert_eq!(forgejo::decode(&request(private), &encoded).document, Ok(Document::CreatedRepository(value)));
        assert!(forgejo::shape(&encoded).is_subset(&forgejo::shape(&forgejo::transcript(case).body)));
    }
}
fn refusal(case: &str, status: u16, message: &[u8]) {
    let t = forgejo::transcript(case);
    assert_eq!(t.status, status);
    assert_eq!(
        forgejo::decode_answer(client::error_decoder(&forgejo::limits()), &t.body, &forgejo::limits()).document,
        Ok(Document::Error(skein_forge::forgejo::document::ErrorBody { message: Body::Kept(Box::from(message)) }))
    );
}
#[test]
fn repository_creation_preserves_name_taken_409() {
    refusal("setup-repository-created-again", 409, b"The repository with the same name already exists.");
}
#[test]
fn repository_creation_preserves_invalid_name_422() {
    refusal("setup-repository-invalid-name", 422, b"[Name]: AlphaDashDot");
}
#[test]
fn repository_creation_preserves_org_missing_404() {
    refusal("setup-repository-missing-org", 404, b"GetOrgByName");
}
#[test]
fn repository_creation_preserves_not_authenticated_401() {
    refusal("setup-repository-unauthorized", 401, b"token is required");
}
#[test]
fn repository_creation_preserves_not_allowed_403() {
    refusal("setup-repository-forbidden", 403, b"Given user is not allowed to create repository in organization.");
}
#[test]
#[expect(clippy::disallowed_methods, reason = "the fixture mutation removes one literal JSON field at a time")]
fn repository_creation_requires_each_adoption_and_clone_field() {
    let fields = [
        (br#""id":1,"#.as_slice(), b"id".as_slice()),
        (br#""full_name":"creation/starter","#.as_slice(), b"full_name".as_slice()),
        (br#""default_branch":"main","#.as_slice(), b"default_branch".as_slice()),
        (br#""clone_url":"http://127.0.0.1:3000/creation/starter.git","#.as_slice(), b"clone_url".as_slice()),
        (br#","ssh_url":"ssh://free@localhost/creation/starter.git""#.as_slice(), b"ssh_url".as_slice()),
    ];
    let body =
        server::encode(&Answer::CreatedRepository(common::cases::setup::created_repository(true)), &forgejo::limits())
            .unwrap();
    for (remove, field) in fields {
        let mut mutated = body.to_vec();
        let at = mutated.windows(remove.len()).position(|part| part == remove).expect("field present");
        mutated.drain(at..at + remove.len());
        assert_eq!(forgejo::decode(&request(true), &mutated).document, Err(Malformed::Missing { field }));
    }
}

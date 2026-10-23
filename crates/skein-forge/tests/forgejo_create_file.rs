#![cfg(test)]
pub mod common;
use common::forgejo;
use skein_forge::forgejo::{
    client,
    document::Document,
    id,
    request::{Author, Method, Repo, Request},
    server::{self, Answer},
};
const CONTENT: &[u8] = b"\0\xff\n";
fn request() -> Request {
    Request::CreateFile {
        at: Repo { owner: b"an org".as_slice().into(), name: b"a+repo".as_slice().into() },
        path: b"a dir/ci+.yaml".as_slice().into(),
        branch: b"main".as_slice().into(),
        content: CONTENT.into(),
        message: b"Add ci".as_slice().into(),
        author: Author { name: b"kit".as_slice().into(), email: b"kit@example.invalid".as_slice().into() },
    }
}
#[test]
fn file_creation_encodes_segments_base64_and_both_signatures() {
    let request = request();
    let limits = forgejo::limits();
    let encoded = client::encode(&request, &limits).unwrap();
    assert_eq!(encoded.method, Method::Post);
    assert_eq!(encoded.target.as_ref(), b"/api/v1/repos/an%20org/a%2Brepo/contents/a%20dir/ci%2B.yaml");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"content":"AP8K","branch":"main","message":"Add ci","author":{"name":"kit","email":"kit@example.invalid"},"committer":{"name":"kit","email":"kit@example.invalid"}}"#.as_slice()));
    assert_eq!(
        forgejo::decode_route(encoded.method, &encoded.target, encoded.body.as_ref().unwrap(), &limits).unwrap().0,
        request
    );
}
#[test]
fn file_creation_decodes_the_new_commit_in_fragmented_answers() {
    let request = request();
    let commit = id::read(b"0000000000000000000000000000000000000001").unwrap();
    let body = br#"{"content":{"path":"ci.yaml"},"commit":{"sha":"0000000000000000000000000000000000000001"}}"#;
    for chunk in [1, 7, body.len()] {
        assert_eq!(forgejo::decode_chunks(&request, body, chunk).document, Ok(Document::CreatedFile { commit }));
    }
    let encoded = server::encode(&Answer::CreatedFile { commit }, &forgejo::limits()).unwrap();
    assert_eq!(forgejo::decode(&request, &encoded).document, Ok(Document::CreatedFile { commit }));
}
#[test]
fn file_creation_requires_the_nested_commit_id() {
    for (body, field) in [(b"{}".as_slice(), b"commit".as_slice()), (br#"{"commit":{}}"#, b"sha")] {
        assert_eq!(
            forgejo::decode(&request(), body).document,
            Err(skein_forge::forgejo::decode::Malformed::Missing { field })
        );
    }
}

fn recorded_request() -> Request {
    Request::CreateFile {
        at: Repo { owner: b"first-creation".as_slice().into(), name: b"starter".as_slice().into() },
        path: b"dir/first.txt".as_slice().into(),
        branch: b"main".as_slice().into(),
        content: b"first file\n".as_slice().into(),
        message: b"Add dir/first.txt".as_slice().into(),
        author: Author { name: b"kit".as_slice().into(), email: b"kit@example.invalid".as_slice().into() },
    }
}
#[test]
fn file_creation_matches_the_pinned_forgejos_request_and_201_answer() {
    let transcript = forgejo::transcript("setup-file-put");
    let request = recorded_request();
    let encoded = client::encode(&request, &forgejo::limits()).unwrap();
    assert_eq!(encoded.method, transcript.method);
    assert_eq!(encoded.target.as_ref(), transcript.target);
    assert_eq!(encoded.body.as_deref(), Some(transcript.request_body.as_slice()));
    assert_eq!(transcript.status, 201);
    let commit = id::read(b"0000000000000000000000000000000000000004").unwrap();
    for chunk in [1, 7, transcript.body.len()] {
        assert_eq!(
            forgejo::decode_chunks(&request, &transcript.body, chunk).document,
            Ok(Document::CreatedFile { commit })
        );
    }
}
fn recorded_refusal(case: &str, status: u16, message: &[u8]) {
    let transcript = forgejo::transcript(case);
    assert_eq!(transcript.status, status);
    assert_eq!(
        forgejo::decode_answer(client::error_decoder(&forgejo::limits()), &transcript.body, &forgejo::limits())
            .document,
        Ok(Document::Error(skein_forge::forgejo::document::ErrorBody {
            message: skein_forge::forgejo::document::Body::Kept(message.into())
        }))
    );
}
#[test]
fn file_creation_preserves_the_recorded_existing_file_422() {
    recorded_refusal("setup-file-put-again", 422, b"repository file already exists [path: dir/first.txt]");
}
#[test]
fn file_creation_preserves_the_recorded_missing_file_404() {
    recorded_refusal("setup-file-missing-raw", 404, b"The target couldn't be found.");
}

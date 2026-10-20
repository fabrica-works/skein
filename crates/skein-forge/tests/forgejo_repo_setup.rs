#![cfg(test)]
pub mod common;
use common::forgejo;
use skein_forge::forgejo::{
    client,
    document::{Document, Hook, Row},
    request::{Grant, Page, Repo, Request},
};
fn repo() -> Repo {
    Repo { owner: Box::from(*b"acme"), name: Box::from(*b"widgets") }
}
#[test]
fn collaborator_put_matches_its_literal_wire_golden() {
    let encoded = client::encode(
        &Request::AddCollaborator { at: repo(), login: Box::from(*b"alice"), permission: Grant::Write },
        &forgejo::limits(),
    )
    .unwrap();
    let (method, target, body) = common::goldens::request("setup-collaborator-put");
    assert_eq!(encoded.method, method);
    assert_eq!(encoded.target.as_ref(), target);
    assert_eq!(encoded.body.as_deref(), Some(body));
}
#[test]
fn forgejo_hook_repo_list_decodes_rows_in_chunks_of_1_7_and_whole() {
    let body = br#"[{"id":1,"config":{"url":"https://hooks.example/","content_type":"json"},"events":["push"],"active":true},{"id":7,"config":{"url":"https://hooks.example/other","content_type":"json"},"events":[],"active":false}]"#;
    let request = Request::RepoHooks { at: repo(), page: Page { page: Some(1), limit: Some(2) } };
    for chunk in [1, 7, body.len()] {
        let decoded = forgejo::decode_chunks(&request, body, chunk);
        assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
        assert_eq!(
            decoded.rows,
            [
                Row::Hook(Hook {
                    id: 1,
                    url: Box::from(*b"https://hooks.example/"),
                    events: Box::from([Box::<[u8]>::from(*b"push")]),
                    active: true
                }),
                Row::Hook(Hook {
                    id: 7,
                    url: Box::from(*b"https://hooks.example/other"),
                    events: Box::from([]),
                    active: false
                })
            ]
        );
    }
}

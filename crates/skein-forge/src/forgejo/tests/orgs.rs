use super::fixtures;
use crate::forgejo::{
    client,
    decode::Malformed,
    document::{self, Access, Document, TeamSpec, Units},
    request::{Method, Request},
};
use alloc::boxed::Box;
fn spec() -> TeamSpec {
    TeamSpec {
        name: Box::from(*b"writer"),
        permission: Access::Write,
        includes_all_repositories: true,
        units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Read },
    }
}
#[test]
fn a_team_request_writes_its_booleans_and_four_units_as_literals() {
    let request = Request::CreateTeam { org: Box::from(*b"ensemble"), spec: spec() };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Post);
    assert_eq!(&*encoded.target, b"/api/v1/orgs/ensemble/teams");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"name":"writer","permission":"write","includes_all_repositories":true,"units_map":{"repo.code":"write","repo.issues":"write","repo.pulls":"write","repo.actions":"read"}}"#.as_slice()));
}
#[test]
fn adding_a_team_member_is_a_put_with_no_body() {
    let request = Request::AddMember { team: 2, login: Box::from(*b"writer") };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Put);
    assert_eq!(&*encoded.target, b"/api/v1/teams/2/members/writer");
    assert_eq!(encoded.body, None);
    assert!(client::decoder(&request, &fixtures::limits()).is_none());
}
#[test]
fn an_org_is_made_with_a_fixed_private_visibility() {
    let request = Request::CreateOrg { person: Box::from(*b"fixture"), name: Box::from(*b"ensemble") };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Post);
    assert_eq!(&*encoded.target, b"/api/v1/admin/users/fixture/orgs");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"username":"ensemble","visibility":"private"}"#.as_slice()));
}
#[test]
fn an_org_name_is_one_percent_encoded_path_segment() {
    let request = Request::Org { name: Box::from(*b"a/b c") };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(&*encoded.target, b"/api/v1/orgs/a%2Fb%20c");
}
#[test]
fn a_team_answer_without_units_map_reads_every_unit_as_none() {
    let limits = fixtures::limits();
    let request = Request::EditTeam { team: 1, spec: spec() };
    let decoder = client::decoder(&request, &limits).unwrap();
    let mut team = document::team();
    team.id = 1;
    team.spec.name = Box::from(*b"writer");
    team.spec.permission = Access::Write;
    team.spec.includes_all_repositories = true;
    assert_eq!(
        fixtures::decode(
            decoder,
            br#"{"id":1,"name":"writer","permission":"write","includes_all_repositories":true}"#,
            &limits
        )
        .document,
        Ok(Document::Team(team))
    );
}
#[test]
fn a_team_access_outside_forgejos_words_is_malformed() {
    let limits = fixtures::limits();
    let request = Request::EditTeam { team: 1, spec: spec() };
    let decoder = client::decoder(&request, &limits).unwrap();
    assert_eq!(fixtures::decode(decoder, br#"{"id":1,"name":"writer","permission":"write","includes_all_repositories":true,"units_map":{"repo.code":"execute"}}"#, &limits).document, Err(Malformed::Value { field: b"repo.code" }));
}
#[test]
fn a_team_body_with_a_text_where_a_boolean_belongs_is_refused() {
    let body = br#"{"name":"writer","permission":"write","includes_all_repositories":"true","units_map":{}}"#;
    assert_eq!(
        fixtures::request(Method::Post, b"/api/v1/orgs/ensemble/teams", body, &fixtures::limits()),
        Err(Malformed::Shape { field: b"includes_all_repositories" })
    );
}

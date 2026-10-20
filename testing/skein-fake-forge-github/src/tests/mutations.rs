use super::{fixtures as f, read_support as r};
use crate::service::Response;
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::github::{
    document::{Document, Kind},
    request::Method,
};
use skein_lib::{List, Secret};

const CREATE: &[u8] = b"/repos/acme/widgets/rulesets";
const DELETE: &[u8] = b"/repos/acme/widgets/rulesets/12345678";
const RULE: &str = r#"{"name":"probe","target":"branch","enforcement":"active","conditions":{"ref_name":{"include":["refs/heads/trunk"]}},"bypass_actors":[{"actor_id":7,"actor_type":"Integration","bypass_mode":"always"}],"rules":[{"parameters":{"required_approving_review_count":2},"type":"pull_request"},{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"ci"}]}},{"type":"merge_queue"}]}"#;

fn harness() -> f::Harness {
    let mut config = f::config();
    config.ruleset_id = 12_345_678;
    config.tokens = List::with_capacity(1);
    config.tokens.push((Secret::new(f::boxed(b"invented-person")), 31, None)).unwrap();
    let mut h = f::Harness::with_domain(f::limits(), config, r::domain_config());
    fake::grant(&mut h.domain, b"acme/widgets", 31, a::Permission::Admin);
    h
}
fn ask(h: &mut f::Harness, method: Method, target: &[u8], body: &[u8]) -> Response {
    h.ask(method, target, Some(f::bearer(b"invented-person").expose()), body)
}
#[test]
fn github_fake_person_rulesets_write_protection_and_read_the_server_shape() {
    let mut h = harness();
    let response = ask(&mut h, Method::Post, CREATE, RULE.as_bytes());
    assert_eq!(response.status, 201);
    assert!(response.body.starts_with(br#"{"id":12345678,"#));
    let (_, Document::Ruleset(rule)) = h.decode(Kind::Ruleset, &response) else { panic!("ruleset") };
    assert_eq!(rule, skein_forge::api::Bypass::Always);
    let rule = h.domain.inspect_repository_protection(b"acme/widgets").unwrap();
    assert_eq!(rule.branch.as_ref(), b"trunk");
    assert_eq!(rule.approvals, 2);
    assert_eq!(rule.contexts.as_ref(), &[f::boxed(b"ci")]);
    assert_eq!(rule.bypassing_users.as_ref(), &[f::BOT]);
    assert!(rule.queue);
    assert_eq!(ask(&mut h, Method::Get, DELETE, b"").body.as_ref(), br#"{"current_user_can_bypass":"always"}"#);
    let response = ask(&mut h, Method::Delete, DELETE, b"");
    assert_eq!(response.status, 204);
    assert!(response.body.is_empty());
    assert!(h.domain.inspect_repository_protection(b"acme/widgets").is_none());
    assert_eq!(ask(&mut h, Method::Get, DELETE, b"").status, 404);
}
#[test]
fn github_fake_rulesets_refuse_unknown_duplicate_and_out_of_subset_fields_without_writing() {
    let mut h = harness();
    for (from, to) in [
        ("\"name\":\"probe\"", "\"extra\":true,\"name\":\"probe\""),
        ("\"name\":\"probe\"", "\"name\":\"probe\",\"name\":\"duplicate\""),
        ("\"active\"", "\"disabled\""),
        ("\"branch\"", "\"tag\""),
        ("\"required_approving_review_count\":2", "\"required_approving_review_count\":2,\"unknown\":true"),
        ("\"context\":\"ci\"", "\"context\":\"ci\",\"unknown\":true"),
        ("\"merge_queue\"", "\"unknown\""),
        ("\"bypass_mode\":\"always\"", "\"bypass_mode\":\"pull_request\""),
        ("\"include\":[\"refs/heads/trunk\"]", "\"include\":[\"refs/heads/trunk\"],\"exclude\":[]"),
        ("\"required_approving_review_count\":2", "\"required_approving_review_count\":-1"),
    ] {
        let body = RULE.replace(from, to);
        let response = ask(&mut h, Method::Post, CREATE, body.as_bytes());
        f::refusal(&h, &response, 422, b"Validation Failed");
        assert!(h.domain.inspect_repository_protection(b"acme/widgets").is_none());
    }
    assert_eq!(ask(&mut h, Method::Delete, DELETE, b"{}").status, 422);
}
#[test]
fn github_fake_ruleset_mutations_require_a_person_with_domain_admin_permission() {
    let mut h = harness();
    let installed = f::bearer(h.mint(1, false).token.expose());
    for (method, target, body) in [(Method::Post, CREATE, RULE.as_bytes()), (Method::Delete, DELETE, b"".as_slice())] {
        let calls = h.calls;
        let response = h.ask(method, target, Some(installed.expose()), body);
        f::refusal(&h, &response, 403, b"Resource not accessible by integration");
        assert_eq!(h.calls, calls);
        assert_eq!(h.ask(method, target, None, body).status, 401);
        fake::grant(&mut h.domain, b"acme/widgets", 31, a::Permission::Write);
        assert_eq!(ask(&mut h, method, target, body).status, 404);
        assert!(h.domain.inspect_repository_protection(b"acme/widgets").is_none());
    }
}

use super::{fixtures as f, read_support as r};
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::github::{self, failure, plans::tables::Method, request::Request};
use skein_lib::Secret;

fn harness(queue: bool) -> f::Harness {
    let config = r::harness().env.limits;
    let mut h = f::Harness::with_protection(f::limits(), f::config(), config, Some(a::Protection {
        queue, branch: f::boxed(b"trunk"), contexts: Box::from([]), approvals: 0, dismiss_stale: false,
    }));
    fake::grant(&mut h.domain, b"acme/widgets", f::BOT, a::Permission::Write);
    h
}
fn ask(h: &mut f::Harness, token: &Secret, request: &Request) -> crate::service::Response {
    let encoded = github::request::encode(request, &h.limits.github).unwrap();
    h.ask(encoded.method, &encoded.target, Some(token.expose()), encoded.body.as_deref().unwrap_or(b""))
}
fn merge(number: u64, head: u64) -> Request {
    Request::Merge { owner: f::boxed(b"acme"), name: f::boxed(b"widgets"), number, head: r::object(head), method: Method::Rebase }
}
#[test]
fn direct_merge_of_a_queue_branch_answers_the_two_rule_lines_without_merging() {
    let mut h = harness(true);
    let (number, head) = r::pull(&mut h);
    let token = r::token(&mut h);
    let response = ask(&mut h, &token, &merge(number, head));
    let message = f::parts(&[failure::RULES, b"\n", failure::QUEUE]);
    f::refusal(&h, &response, 405, &message);
    assert!(h.domain.queue_entries(b"acme/widgets").unwrap().is_empty());
    let a::Answer::Pull(pull) = h.domain.inspect(&h.env.limits, b"acme/widgets", &a::Read::Pull { number }).unwrap() else { panic!("pull") };
    assert_eq!(pull.state, a::State::Open);
    assert_eq!(pull.merged, None);
}
#[test]
fn queue_only_merge_handler_leaves_ordinary_merges_to_the_rest_writes_task() {
    let mut h = harness(false);
    let (number, head) = r::pull(&mut h);
    let token = r::token(&mut h);
    let response = ask(&mut h, &token, &merge(number, head));
    f::refusal(&h, &response, 404, b"Not Found");
}

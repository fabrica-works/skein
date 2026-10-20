use super::{fixtures as f, read_support as r, write_support as w};
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::{
    api,
    github::{failure, plans::tables::Method, request::Request},
};

fn settings(method: Method) -> a::Settings {
    a::Settings {
        default: f::boxed(b"trunk"),
        merge: method == Method::Merge,
        squash: method == Method::Squash,
        rebase: method == Method::Rebase,
    }
}
fn request(number: u64, head: u64, method: Method) -> Request {
    Request::Merge { owner: f::boxed(b"acme"), name: f::boxed(b"widgets"), number, head: r::object(head), method }
}
fn write(number: u64, head: u64) -> api::Write {
    api::Write::Merge { number, head: r::object(head) }
}
fn protected(approvals: u32, contexts: &[&[u8]]) -> f::Harness {
    let mut config = r::domain_config();
    config.limits.contexts = 4;
    config.limits.statuses = 4;
    let mut names = skein_lib::List::with_capacity(4);
    for name in contexts {
        names.push(f::boxed(name)).unwrap();
    }
    let protection = a::Protection {
        queue: false,
        branch: f::boxed(b"trunk"),
        contexts: names.into_boxed(),
        approvals,
        dismiss_stale: true,
    };
    let mut h = f::Harness::with_protection(f::limits(), f::config(), config, Some(protection));
    fake::grant(&mut h.domain, b"acme/widgets", f::BOT, a::Permission::Write);
    h
}
#[test]
fn github_fake_merge_plans_land_each_allowed_method_and_read_back() {
    for method in [Method::Merge, Method::Squash, Method::Rebase] {
        let mut h = w::harness();
        fake::settings(&mut h.domain, b"acme/widgets", settings(method));
        let (number, head) = r::pull(&mut h);
        let token = r::token(&mut h);
        let api::Answer::Merged(landed) = w::write(&mut h, &token, write(number, head)).unwrap() else {
            panic!("merged")
        };
        let api::Answer::Pull(pull) = r::read_with_empty_ci(&mut h, &token, api::Read::Pull { number }).unwrap() else {
            panic!("merged pull")
        };
        assert_eq!(pull.state, api::State::Closed);
        assert_eq!(pull.merged, Some(landed));
        let api::Answer::Commit(commit) =
            r::read(&mut h, &token, api::Read::Branch { branch: f::boxed(b"trunk") }).unwrap()
        else {
            panic!("base moved")
        };
        assert_eq!(commit, landed);
        let api::Answer::File { bytes, .. } =
            r::read(&mut h, &token, api::Read::File { head: landed, path: f::boxed(b"one"), max_bytes: 128 }).unwrap()
        else {
            panic!("merged tree")
        };
        assert_eq!(bytes.as_ref(), b"one contents");
    }
}
#[test]
fn github_fake_merge_moved_head_is_409_and_plan_stale() {
    let mut h = w::harness();
    let (number, head) = r::pull(&mut h);
    let token = r::token(&mut h);
    assert_eq!(w::write(&mut h, &token, write(number, head.checked_sub(1).unwrap())), Err(api::Error::Stale));
    let response = w::ask(&mut h, &token, &request(number, head.checked_sub(1).unwrap(), Method::Squash));
    f::refusal(&h, &response, 409, failure::MERGE_STALE);
    let api::Answer::Pull(pull) = r::read_with_empty_ci(&mut h, &token, api::Read::Pull { number }).unwrap() else {
        panic!("stale readback")
    };
    assert_eq!(pull.commit, r::object(head));
    assert_eq!(pull.state, api::State::Open);
}
#[test]
fn github_fake_merge_conflict_is_405_and_preserves_head() {
    let mut h = w::harness();
    let (number, head) = r::pull(&mut h);
    let base = fake::commit(
        &mut h.domain,
        &h.env.limits,
        h.first,
        Box::from([a::File { path: f::boxed(b"one"), content: f::boxed(b"conflict") }]),
        b"conflict",
    )
    .unwrap()
    .unwrap();
    r::call(&mut h, a::Op::Git(a::Git::Push { branch: f::boxed(b"trunk"), commit: base, expected: None }));
    let token = r::token(&mut h);
    assert_eq!(w::write(&mut h, &token, write(number, head)), Err(api::Error::Conflict));
    let response = w::ask(&mut h, &token, &request(number, head, Method::Squash));
    f::refusal(&h, &response, 405, failure::NOT_MERGEABLE);
    let api::Answer::Pull(pull) = r::read_with_empty_ci(&mut h, &token, api::Read::Pull { number }).unwrap() else {
        panic!("conflict readback")
    };
    assert_eq!(pull.commit, r::object(head));
    assert_eq!(pull.state, api::State::Open);
}
#[test]
fn github_fake_merge_method_refusal_uses_the_requested_method() {
    for (method, message) in [
        (Method::Merge, b"Merge merges are not allowed on this repository.".as_slice()),
        (Method::Squash, b"Squash merges are not allowed on this repository."),
        (Method::Rebase, b"Rebase merges are not allowed on this repository."),
    ] {
        let mut h = w::harness();
        fake::settings(&mut h.domain, b"acme/widgets", settings(method));
        let (number, head) = r::pull(&mut h);
        let token = r::token(&mut h);
        let disabled = a::Settings { default: f::boxed(b"trunk"), merge: false, squash: false, rebase: false };
        assert_eq!(w::write_with_settings(&mut h, &token, write(number, head), disabled), Err(api::Error::Refused));
        let response = w::ask(&mut h, &token, &request(number, head, method));
        f::refusal(&h, &response, 405, message);
    }
}
#[test]
fn github_fake_merge_approval_rule_has_recorded_shape_and_configured_count() {
    for count in [1, 2] {
        let mut h = protected(count, &[]);
        let (number, head) = r::pull(&mut h);
        let token = r::token(&mut h);
        assert_eq!(w::write(&mut h, &token, write(number, head)), Err(api::Error::Protected));
        let response = w::ask(&mut h, &token, &request(number, head, Method::Squash));
        let digit = if count == 1 { b"1" } else { b"2" };
        let prefix = crate::service::joined(b"Repository rule violations found\n\nAt least ", digit).unwrap();
        let message =
            crate::service::joined(&prefix, b" approving review is required by reviewers with write access.\n\n")
                .unwrap();
        f::refusal(&h, &response, 405, &message);
        assert!(
            skein_lib::bytes::find(&response.body, b"https://docs.github.com/rest/pulls/pulls#merge-a-pull-request")
                .is_some()
        );
        assert!(skein_lib::bytes::find(&response.body, b"\"status\":\"405\"").is_some());
        r::call_as(
            &mut h,
            f::BOT,
            a::Op::Write(a::Write::Review { number, verdict: Some(a::Verdict::Approve), body: f::boxed(b"approved") }),
        );
        if count == 1 {
            let api::Answer::Merged(_) = w::write(&mut h, &token, write(number, head)).unwrap() else {
                panic!("approved merge")
            };
        }
    }
}
#[test]
fn github_fake_merge_check_rules_name_each_unmet_context_and_state() {
    let mut h = protected(0, &[b"missing", b"running", b"failed", b"passed"]);
    let (number, head) = r::pull(&mut h);
    for (context, state) in
        [(b"running".as_slice(), a::Check::Pending), (b"failed", a::Check::Failed), (b"passed", a::Check::Passed)]
    {
        r::call(&mut h, a::Op::Write(a::Write::Status { commit: head, context: f::boxed(context), state }));
    }
    let token = r::token(&mut h);
    assert_eq!(w::write(&mut h, &token, write(number, head)), Err(api::Error::Protected));
    let response = w::ask(&mut h, &token, &request(number, head, Method::Squash));
    f::refusal(&h, &response, 405, b"Repository rule violations found\n\nRequired status check \"missing\" is expected.\n\nRequired status check \"running\" is in progress.\n\nRequired status check \"failed\" is failing.\n\n");
}
#[test]
fn github_fake_merge_outside_installation_is_missing_before_domain_calls() {
    let mut h = w::harness();
    let token = r::token(&mut h);
    let response = w::ask(
        &mut h,
        &token,
        &Request::Merge {
            owner: f::boxed(b"acme"),
            name: f::boxed(b"absent"),
            number: 1,
            head: r::object(1),
            method: Method::Squash,
        },
    );
    f::refusal(&h, &response, 404, b"Not Found");
    assert_eq!(h.calls, 0);
}

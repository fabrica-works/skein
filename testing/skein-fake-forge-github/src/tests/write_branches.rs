use super::{fixtures as f, read_support as r, write_support as w};
use skein_fake_forge_domain::api as a;
use skein_forge::{
    api,
    github::{document as d, request::Request},
};

#[test]
fn github_fake_branch_plans_create_read_and_delete() {
    let mut h = w::harness();
    let token = r::token(&mut h);
    let head = r::object(h.first);
    let create = api::Write::CreateBranch { branch: f::boxed(b"topic/example"), commit: head };
    assert_eq!(w::write(&mut h, &token, create.clone()), Ok(api::Answer::Branch(api::BranchCreation::Created)));
    assert_eq!(
        r::read(&mut h, &token, api::Read::Branch { branch: f::boxed(b"topic/example") }),
        Ok(api::Answer::Commit(head))
    );
    assert_eq!(w::write(&mut h, &token, create), Ok(api::Answer::Branch(api::BranchCreation::Exists)));
    let response = w::ask(
        &mut h,
        &token,
        &Request::CreateBranch {
            owner: f::boxed(b"acme"),
            name: f::boxed(b"widgets"),
            branch: f::boxed(b"topic/example"),
            commit: head,
        },
    );
    f::refusal(&h, &response, 422, crate::texts::REF_EXISTS);
    assert_eq!(
        w::write(&mut h, &token, api::Write::DeleteBranch { branch: f::boxed(b"topic/example") }),
        Ok(api::Answer::Done)
    );
    assert_eq!(
        r::read(&mut h, &token, api::Read::Branch { branch: f::boxed(b"topic/example") }),
        Err(api::Error::Missing)
    );
    assert_eq!(
        w::write(&mut h, &token, api::Write::DeleteBranch { branch: f::boxed(b"topic/example") }),
        Err(api::Error::Missing)
    );
    let response = w::ask(
        &mut h,
        &token,
        &Request::DeleteBranch {
            owner: f::boxed(b"acme"),
            name: f::boxed(b"widgets"),
            branch: f::boxed(b"topic/example"),
        },
    );
    f::refusal(&h, &response, 422, crate::texts::REF_MISSING);
}
#[test]
fn github_fake_status_plan_reports_each_verdict_to_domain() {
    let mut h = w::harness();
    h.service.app.permissions = Box::from([d::Permission { name: f::boxed(b"statuses"), access: d::Access::Write }]);
    let token = r::token(&mut h);
    let commit = h.first;
    for (check, expected) in [
        (api::Check::Pending, a::Check::Pending),
        (api::Check::Passed, a::Check::Passed),
        (api::Check::Failed, a::Check::Failed),
    ] {
        assert_eq!(
            w::write(
                &mut h,
                &token,
                api::Write::Status { commit: r::object(commit), context: f::boxed(b"example"), check }
            ),
            Ok(api::Answer::Done)
        );
        // Task 24 owns the CI HTTP reads. This readback uses the domain's
        // statuses while the dialect's own write plan exercises HTTP encoding.
        let a::Answer::Statuses(statuses) = r::call(&mut h, a::Op::Read(a::Read::Statuses { commit })) else {
            panic!("status readback")
        };
        let status = statuses.first().unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(status.context.as_ref(), b"example");
        assert_eq!(status.state, expected);
        assert_eq!(status.author, f::BOT);
    }
}
#[test]
fn github_fake_status_requires_the_apps_statuses_write_grant() {
    let mut h = w::harness();
    let token = r::token(&mut h);
    let head = r::object(h.first);
    for access in [None, Some(d::Access::Read)] {
        h.service.app.permissions = match access {
            None => Box::from([]),
            Some(access) => Box::from([d::Permission { name: f::boxed(b"statuses"), access }]),
        };
        assert_eq!(
            w::write(
                &mut h,
                &token,
                api::Write::Status { commit: head, context: f::boxed(b"example"), check: api::Check::Passed }
            ),
            Err(api::Error::Forbidden)
        );
        let response = w::ask(
            &mut h,
            &token,
            &Request::Status {
                owner: f::boxed(b"acme"),
                name: f::boxed(b"widgets"),
                commit: head,
                context: f::boxed(b"example"),
                check: api::Check::Passed,
            },
        );
        f::refusal(&h, &response, 403, crate::texts::INTEGRATION);
    }
}
#[test]
fn github_fake_branch_and_status_writes_refuse_outside_installation() {
    let mut h = w::harness();
    let token = r::token(&mut h);
    for request in [
        Request::CreateBranch {
            owner: f::boxed(b"acme"),
            name: f::boxed(b"absent"),
            branch: f::boxed(b"topic"),
            commit: r::object(1),
        },
        Request::DeleteBranch { owner: f::boxed(b"acme"), name: f::boxed(b"absent"), branch: f::boxed(b"topic") },
        Request::Status {
            owner: f::boxed(b"acme"),
            name: f::boxed(b"absent"),
            commit: r::object(1),
            context: f::boxed(b"example"),
            check: api::Check::Passed,
        },
    ] {
        let response = w::ask(&mut h, &token, &request);
        f::refusal(&h, &response, 404, b"Not Found");
    }
    assert_eq!(h.calls, 0);
}
#[test]
fn github_fake_contents_token_writes_branches_and_issues() {
    let mut h = w::harness();
    let token = f::bearer(h.mint(1, true).token.expose());
    let head = r::object(h.first);
    assert_eq!(
        w::write(&mut h, &token, api::Write::CreateBranch { branch: f::boxed(b"topic"), commit: head }),
        Ok(api::Answer::Branch(api::BranchCreation::Created))
    );
    assert_eq!(
        w::write(&mut h, &token, api::Write::DeleteBranch { branch: f::boxed(b"topic") }),
        Ok(api::Answer::Done)
    );
    assert_eq!(
        w::write(
            &mut h,
            &token,
            api::Write::CreateIssue { key: f::boxed(b"key"), title: f::boxed(b"title"), body: f::boxed(b"body") }
        ),
        Ok(api::Answer::Created(1))
    );
}

use super::{fixtures as f, read_support as r};
use skein_fake_forge_domain::api as a;
use skein_forge::{
    api,
    github::{
        self,
        document::{Document, Kind},
        request::{Method, Request},
    },
};

#[test]
fn github_fake_pull_selection_drives_the_plans_to_the_ci_boundary() {
    let mut h = r::harness();
    let (number, commit) = r::pull(&mut h);
    let token = r::token(&mut h);
    for read in
        [api::Read::Pull { number }, api::Read::PullFor { head: f::boxed(b"feature"), base: f::boxed(b"trunk") }]
    {
        let pull = r::select_pull(&mut h, &token, read);
        assert_eq!(pull.number, number);
        assert_eq!(pull.head_commit, r::object(commit));
        assert_eq!(pull.base_commit, r::object(h.first));
        assert!(!pull.closed && !pull.merged);
        assert!(pull.mergeable);
    }
    r::call(&mut h, a::Op::Write(a::Write::Close { number }));
    assert!(
        r::select_pull(&mut h, &token, api::Read::PullFor { head: f::boxed(b"feature"), base: f::boxed(b"trunk") })
            .closed,
        "fallback finds oldest closed pull"
    );
    assert_eq!(r::read(&mut h, &token, api::Read::Pull { number: 999 }), Err(api::Error::Missing));
    assert_eq!(
        r::read(&mut h, &token, api::Read::PullFor { head: f::boxed(b"missing"), base: f::boxed(b"trunk") }),
        Err(api::Error::Missing)
    );
    assert_eq!(h.hooks, 0);
}
#[test]
fn github_fake_pull_files_compare_and_raw_drive_complete_plans() {
    let mut h = r::harness();
    let (number, commit) = r::pull(&mut h);
    let token = r::token(&mut h);
    let first = r::object(h.first);
    for (page, count, more) in [(1, 2, true), (2, 1, false)] {
        let api::Answer::PullFiles { files, more: actual, .. } =
            r::read(&mut h, &token, api::Read::PullFiles { number, head: r::object(commit), page })
                .expect("pull file plan")
        else {
            panic!("pull files")
        };
        assert_eq!(files.len(), count);
        assert_eq!(actual, more);
        for file in files {
            assert!(file.before.is_none());
            assert!(file.after.is_some());
        }
    }
    let api::Answer::Compare { files, commits, contains_before, .. } =
        r::read(&mut h, &token, api::Read::Compare { before: first, after: r::object(commit) }).expect("compare plan")
    else {
        panic!("comparison")
    };
    assert_eq!(files.len(), 3);
    assert_eq!(commits.as_ref(), &[r::object(commit)]);
    assert!(contains_before);
    let api::Answer::File { bytes, truncated, .. } =
        r::read(&mut h, &token, api::Read::File { head: r::object(commit), path: f::boxed(b"one"), max_bytes: 3 })
            .expect("raw plan")
    else {
        panic!("file")
    };
    assert_eq!(bytes.as_ref(), b"one");
    assert!(truncated);
    assert_eq!(
        r::read(
            &mut h,
            &token,
            api::Read::File { head: r::object(commit), path: f::boxed(b"missing"), max_bytes: 100 }
        ),
        Err(api::Error::Missing)
    );
    assert_eq!(r::read(&mut h, &token, api::Read::PullFiles { number, head: first, page: 1 }), Err(api::Error::Stale));
    assert_eq!(h.hooks, 0);
}
#[test]
fn github_fake_compare_uses_the_merge_base_on_divergent_branches() {
    let mut h = r::harness();
    let (number, commit) = r::pull(&mut h);
    let other = skein_fake_forge_domain::commit(
        &mut h.domain,
        &h.env.limits,
        h.first,
        Box::from([a::File { path: f::boxed(b"base"), content: f::boxed(b"other") }]),
        b"base",
    )
    .unwrap()
    .unwrap();
    r::call(&mut h, a::Op::Git(a::Git::Push { branch: f::boxed(b"trunk"), commit: other, expected: None }));
    let token = r::token(&mut h);
    let request = Request::Compare {
        owner: f::boxed(b"acme"),
        name: f::boxed(b"widgets"),
        before: r::object(other),
        after: r::object(commit),
        per_page: 1,
    };
    let encoded = github::request::encode(&request, &h.limits.github).unwrap();
    let response = h.ask(Method::Get, &encoded.target, Some(token.expose()), b"");
    let (_, Document::Compare(row)) = h.decode(Kind::Compare, &response) else { panic!("compare") };
    assert_eq!(row.merge_base, Some(r::object(h.first)));
    assert_ne!(row.behind_by, 0);
    let api::Answer::PullFiles { files, .. } =
        r::read(&mut h, &token, api::Read::PullFiles { number, head: r::object(commit), page: 1 }).unwrap()
    else {
        panic!("files")
    };
    for file in &files {
        assert!(file.before.is_none(), "file sides use merge base rather than current base");
    }
}
#[test]
fn github_fake_pull_routes_refuse_outside_installation() {
    let mut h = r::harness();
    let token = r::token(&mut h);
    let owner = f::boxed(b"acme");
    let name = f::boxed(b"absent");
    for request in [
        Request::Pull { owner: owner.clone(), name: name.clone(), number: 1 },
        Request::PullsFor {
            owner: owner.clone(),
            name: name.clone(),
            head: f::boxed(b"feature"),
            base: f::boxed(b"trunk"),
            open: true,
        },
        Request::PullFiles { owner: owner.clone(), name: name.clone(), number: 1, page: 1, per_page: 100 },
        Request::Compare {
            owner: owner.clone(),
            name: name.clone(),
            before: r::object(h.first),
            after: r::object(h.first),
            per_page: 100,
        },
        Request::Raw { owner, name, path: f::boxed(b"one"), commit: r::object(h.first) },
    ] {
        let encoded = github::request::encode(&request, &h.limits.github).unwrap();
        let response = h.ask(encoded.method, &encoded.target, Some(token.expose()), b"");
        f::refusal(&h, &response, 404, b"Not Found");
    }
    assert_eq!(h.calls, 0);
}

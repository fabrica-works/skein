use super::{fixtures as f, read_support as r};
use skein_fake_forge_domain::api as a;
use skein_forge::{
    api,
    github::{
        document::{Document, Kind, Row},
        request::{Method, Request},
    },
};
use skein_lib::Time;

#[test]
fn github_fake_issues_and_comments_drive_the_dialect_plans() {
    let mut h = r::harness();
    let first = r::issue(&mut h, b"first");
    let second = r::issue(&mut h, b"second");
    let third = r::issue(&mut h, b"third");
    for text in [b"one".as_slice(), b"two", b"three"] {
        r::call(&mut h, a::Op::Write(a::Write::Comment { number: first, body: f::boxed(text) }));
    }
    let token = r::token(&mut h);
    for (page, expected, more) in [(1, &[first, second][..], true), (2, &[third][..], false)] {
        let api::Answer::Items { items, more: actual, .. } =
            r::read(&mut h, &token, api::Read::Items { since: Time::ZERO, page, kind: None }).expect("items plan")
        else {
            panic!("items answer")
        };
        assert_eq!(actual, more);
        assert_eq!(items.len(), expected.len());
        for (item, expected) in items.iter().zip(expected) {
            assert_eq!(item.number, *expected);
        }
    }
    let api::Answer::Item { item, comments, more } =
        r::read(&mut h, &token, api::Read::Item { number: first, after: 0 }).expect("item and paged comments plan")
    else {
        panic!("item answer")
    };
    assert_eq!(item.number, first);
    assert_eq!(item.title.as_ref(), b"first");
    assert_eq!(comments.len(), 3);
    for (comment, body) in comments.iter().zip([b"one".as_slice(), b"two", b"three"]) {
        assert_eq!(comment.body.as_ref(), body);
        assert_eq!(comment.provenance, api::Provenance::Original);
    }
    assert!(!more);
    let api::Answer::Item { comments: after, .. } =
        r::read(&mut h, &token, api::Read::Item { number: first, after: comments.first().expect("first comment").id })
            .expect("after filter")
    else {
        panic!("item answer")
    };
    assert_eq!(after.len(), 2);
    assert_eq!(r::read(&mut h, &token, api::Read::Item { number: 999, after: 0 }), Err(api::Error::Missing));
    assert_eq!(h.hooks, 0);
}
#[test]
fn github_fake_issues_list_pulls_in_the_same_numbering() {
    let mut h = r::harness();
    let issue = r::issue(&mut h, b"issue");
    let (pull, head) = r::pull(&mut h);
    r::call(&mut h, a::Op::Write(a::Write::Merge { number: pull, head }));
    let token = r::token(&mut h);
    let api::Answer::Items { items, .. } =
        r::read(&mut h, &token, api::Read::Items { since: Time::ZERO, page: 1, kind: None }).expect("mixed listing")
    else {
        panic!("items")
    };
    assert_eq!(items.len(), 2);
    for (item, (number, kind)) in items.iter().zip([(issue, api::Kind::Issue), (pull, api::Kind::Pull)]) {
        assert_eq!((item.number, item.kind), (number, kind));
    }
    let api::Answer::Items { items, .. } =
        r::read(&mut h, &token, api::Read::Items { since: Time::ZERO, page: 1, kind: Some(api::Kind::Issue) })
            .expect("kind filtering")
    else {
        panic!("items")
    };
    assert_eq!(items.len(), 1);
    let response = h.ask(Method::Get, b"/repos/acme/widgets/issues/2", Some(token.expose()), b"");
    let (_, Document::Issue(row)) = h.decode(Kind::Issue, &response) else { panic!("pull issue") };
    assert!(row.pull);
    assert_eq!(row.state, api::State::Closed);
    assert!(!row.merged, "the listing's merged_at is null");
    assert!(skein_lib::bytes::find(&response.body, br#""merged_at":null"#).is_some());
}
#[test]
fn github_fake_issue_routes_refuse_outside_installation_and_invalid_pages() {
    let mut h = r::harness();
    let token = r::token(&mut h);
    for request in [
        Request::Issues {
            owner: f::boxed(b"acme"),
            name: f::boxed(b"absent"),
            since: super::wall(0),
            page: 1,
            per_page: 100,
        },
        Request::Issue { owner: f::boxed(b"acme"), name: f::boxed(b"absent"), number: 1 },
        Request::Comments { owner: f::boxed(b"acme"), name: f::boxed(b"absent"), number: 1, page: 1, per_page: 100 },
    ] {
        let encoded = skein_forge::github::request::encode(&request, &h.limits.github).expect("encoded read");
        let response =
            h.ask(encoded.method, &encoded.target, Some(token.expose()), encoded.body.as_deref().unwrap_or(b""));
        f::refusal(&h, &response, 404, b"Not Found");
    }
    assert_eq!(h.calls, 0, "scope refusal precedes domain call");
    r::issue(&mut h, b"one");
    r::issue(&mut h, b"two");
    r::issue(&mut h, b"three");
    let request = Request::Issues {
        owner: f::boxed(b"acme"),
        name: f::boxed(b"widgets"),
        since: super::wall(0),
        page: 1,
        per_page: 100,
    };
    let events = r::events(&mut h, &request, &token);
    let mut rows = 0_u32;
    for event in events.into_boxed() {
        if let skein_forge::github::plans::Heard::Row(Row::Issue(_)) = event {
            rows = rows.checked_add(1).unwrap();
        }
    }
    assert_eq!(rows, 2, "scenario page cap");
    for target in [
        b"/repos/acme/widgets/issues/1/comments?per_page=0&page=1".as_slice(),
        b"/repos/acme/widgets/issues/1/comments?per_page=1&page=0",
    ] {
        let response = h.ask(Method::Get, target, Some(token.expose()), b"");
        f::refusal(&h, &response, 404, b"Not Found");
    }
}

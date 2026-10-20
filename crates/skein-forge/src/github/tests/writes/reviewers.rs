use super::*;
use skein_lib::Duration;

fn reviewers(ids: &[u64]) -> api::Write {
    api::Write::SetReviewers { number: NUMBER, reviewers: Box::from(ids) }
}
fn pull(ids: &[u64]) -> Body {
    let mut pull = update_pull();
    pull.reviewers = Box::from(ids);
    Body::Document(Document::Pull(pull))
}
fn user(id: u64, login: &[u8]) -> Body {
    Body::Document(Document::User(document::User { id, login: Box::from(login) }))
}
fn mutation(add: bool, logins: &[&[u8]]) -> Request {
    let mut items = skein_lib::List::with_capacity(u32::try_from(logins.len()).unwrap());
    for login in logins {
        items.push(Box::from(*login)).unwrap();
    }
    Request::Reviewers {
        owner: Box::from(*b"acme"),
        name: Box::from(*b"widgets"),
        number: NUMBER,
        add,
        logins: items.into_boxed(),
    }
}
fn sent(result: Result<Option<Step>, api::Error>) -> Request {
    let Some(Step::Send(exchange)) = result.unwrap() else { panic!("request") };
    exchange.request
}
fn start(ids: &[u64], current: &[u64], tables: &mut Tables) -> (Plan, Result<Option<Step>, api::Error>) {
    let mut plan = write_plan(reviewers(ids));
    golden(
        &begin(&mut plan, tables),
        Method::Get,
        b"/repos/acme/widgets/pulls/9007199254740993",
        None,
        Some(Kind::Pull),
    );
    head(&mut plan, 200, tables);
    let step = finish(&mut plan, pull(current), tables);
    (plan, step)
}
#[test]
fn github_reviewers_diff_and_noop() {
    let mut tables = Tables::new(1, 2);
    tables.keep_repository(KEY, tables::repository(&repository()));
    tables.keep_user(KEY.forge, 2, Box::from(*b"bob"));
    tables.keep_user(KEY.forge, 3, Box::from(*b"alice"));
    let (mut plan, step) = start(&[3, 3, NUMBER], &[NUMBER, 2], &mut tables);
    assert_eq!(sent(step), mutation(true, &[b"alice"]));
    head(&mut plan, 201, &mut tables);
    let request = sent(finish(&mut plan, Body::Document(Document::Done), &mut tables));
    assert_eq!(request, mutation(false, &[b"bob"]));
    head(&mut plan, 200, &mut tables);
    assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), Err(api::Error::InvalidAnswer));

    for (desired, current) in [([NUMBER, 2, 2].as_slice(), [2, NUMBER].as_slice()), (&[], &[])] {
        let (_, step) = start(desired, current, &mut tables);
        assert_eq!(step, ended(Ok(api::Answer::Done)));
    }
    for (desired, current, add, login) in
        [([2].as_slice(), [].as_slice(), true, b"bob".as_slice()), (&[], &[2], false, b"bob")]
    {
        tables.keep_user(KEY.forge, 2, Box::from(login));
        let (mut plan, step) = start(desired, current, &mut tables);
        assert_eq!(sent(step), mutation(add, &[login]));
        head(&mut plan, if add { 201 } else { 200 }, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
}
#[test]
fn github_reviewers_uncached_users_and_bounds() {
    let mut tables = Tables::new(0, 0);
    let mut plan = write_plan(reviewers(&[NUMBER, 3, NUMBER]));
    assert_eq!(begin(&mut plan, &mut tables), Request::Repository { id: 7 });
    head(&mut plan, 200, &mut tables);
    let request = sent(finish(&mut plan, Body::Document(Document::Repository(repository())), &mut tables));
    assert_eq!(request, Request::Pull { owner: Box::from(*b"acme"), name: Box::from(*b"widgets"), number: NUMBER });
    head(&mut plan, 200, &mut tables);
    assert_eq!(sent(finish(&mut plan, pull(&[2]), &mut tables)), Request::User { id: NUMBER });
    golden(&Request::User { id: NUMBER }, Method::Get, b"/user/9007199254740993", None, Some(Kind::User));
    head(&mut plan, 200, &mut tables);
    assert_eq!(sent(finish(&mut plan, user(NUMBER, b"alice"), &mut tables)), Request::User { id: 3 });
    head(&mut plan, 200, &mut tables);
    assert_eq!(sent(finish(&mut plan, user(3, b"carol"), &mut tables)), Request::User { id: 2 });
    head(&mut plan, 200, &mut tables);
    assert_eq!(sent(finish(&mut plan, user(2, b"bob"), &mut tables)), mutation(true, &[b"alice", b"carol"]));
    assert_eq!(tables.user(KEY.forge, NUMBER), None);
    assert_eq!(tables.repository(KEY), None);
    head(&mut plan, 201, &mut tables);
    assert_eq!(sent(finish(&mut plan, Body::Document(Document::Done), &mut tables)), mutation(false, &[b"bob"]));
    head(&mut plan, 200, &mut tables);
    assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));

    let request = mutation(true, &[b"alice", b"b\"o\\b"]);
    golden(
        &request,
        Method::Post,
        b"/repos/acme/widgets/pulls/9007199254740993/requested_reviewers",
        Some(br#"{"reviewers":["alice","b\"o\\b"]}"#),
        Some(Kind::Done),
    );
    roundtrip_body(&request);
    encoded_body_bounds(&request);
    let request = mutation(false, &[b"bob"]);
    golden(
        &request,
        Method::Delete,
        b"/repos/acme/widgets/pulls/9007199254740993/requested_reviewers",
        Some(br#"{"reviewers":["bob"]}"#),
        Some(Kind::Done),
    );
    roundtrip_body(&request);
    for (string_bytes, inner, ok) in [(3, 1, true), (2, 1, false), (3, 0, false)] {
        let l = crate::github::limits::Limits { string_bytes, inner, ..limits() };
        assert_eq!(request::encode(&request, &l).is_ok(), ok);
        assert_eq!(crate::github::server::write_body(request.clone(), br#"{"reviewers":["bob"]}"#, &l).is_some(), ok);
    }
    let length = u32::try_from(request::encode(&request, &limits()).unwrap().target.len()).unwrap();
    for (target_bytes, ok) in [(length, true), (length - 1, false)] {
        assert_eq!(request::encode(&request, &crate::github::limits::Limits { target_bytes, ..limits() }).is_ok(), ok);
    }
    for body in [
        b"{}".as_slice(),
        br#"{"reviewers":null}"#,
        br#"{"reviewers":[3]}"#,
        br#"{"reviewers":["alice"],"reviewers":[]}"#,
        br#"{"reviewers":{}}"#,
    ] {
        assert_eq!(crate::github::server::write_body(request.clone(), body, &limits()), None);
    }
    assert_eq!(
        crate::github::server::write_body(
            request.clone(),
            br#"{"unknown":{"nested":[1,true]},"reviewers":["bob"]}"#,
            &limits()
        ),
        Some(request)
    );
}
#[test]
fn github_reviewers_partial_failure() {
    for added in [false, true] {
        for (failed, expected) in [
            (Failure::Unseen, if added { api::Error::Timeout } else { api::Error::Unavailable }),
            (Failure::Seen, api::Error::Timeout),
        ] {
            let (mut plan, mut tables) = removal(added);
            assert_eq!(plan.heard(Heard::Failed(failed), bounds(), WORD, &mut tables), ended(Err(expected)));
            assert_eq!(plan.heard(Heard::Failed(failed), bounds(), WORD, &mut tables), Err(api::Error::InvalidAnswer));
        }
        for (status, expected) in [
            (500, api::Error::Timeout),
            (422, api::Error::Refused),
            (401, api::Error::Forbidden),
            (403, api::Error::Forbidden),
            (404, api::Error::Missing),
            (410, api::Error::Missing),
            (301, if added { api::Error::Timeout } else { api::Error::Unavailable }),
        ] {
            let (mut plan, mut tables) = removal(added);
            head(&mut plan, status, &mut tables);
            assert_eq!(finish(&mut plan, refusal(b"unknown"), &mut tables), ended(Err(expected)));
            if status == 301 || status == 404 || status == 410 {
                assert_eq!(tables.repository(KEY), None);
            }
        }
        for primary in [false, true] {
            let (mut plan, mut tables) = removal(added);
            let mut rate = Head::new();
            rate.retry_after = Some(Duration::from_nanos(9));
            if primary {
                crate::github::header::field(&mut rate, b"x-ratelimit-remaining", b"0", &limits());
            }
            assert_eq!(plan.heard(Heard::Head { status: 429, head: rate }, bounds(), WORD, &mut tables), Ok(None));
            assert_eq!(
                finish(&mut plan, refusal(b"unknown"), &mut tables),
                ended(Err(if added {
                    api::Error::Timeout
                } else {
                    api::Error::RateLimited { after: Duration::from_nanos(9) }
                }))
            );
        }
        for body in [
            Body::Empty,
            Body::Malformed(decoding::Malformed::Bound { field: b"body" }),
            Body::Document(Document::Rows),
        ] {
            let (mut plan, mut tables) = removal(added);
            head(&mut plan, 200, &mut tables);
            assert_eq!(finish(&mut plan, body, &mut tables), ended(Err(api::Error::Timeout)));
        }
    }
}
fn removal(added: bool) -> (Plan, Tables) {
    let mut tables = Tables::new(1, 2);
    tables.keep_repository(KEY, tables::repository(&repository()));
    tables.keep_user(KEY.forge, 2, Box::from(*b"bob"));
    tables.keep_user(KEY.forge, 3, Box::from(*b"alice"));
    let (mut plan, step) = start(if added { &[3] } else { &[] }, &[2], &mut tables);
    assert_eq!(sent(step), mutation(added, if added { &[b"alice"] } else { &[b"bob"] }));
    if added {
        head(&mut plan, 201, &mut tables);
        assert_eq!(sent(finish(&mut plan, Body::Document(Document::Done), &mut tables)), mutation(false, &[b"bob"]));
    }
    (plan, tables)
}

#[test]
fn github_reviewers_prerequisite_failures() {
    for body in [
        user(4, b"alice"),
        Body::Document(Document::Done),
        Body::Malformed(decoding::Malformed::Shape { field: b"login" }),
    ] {
        let mut tables = cached_tables();
        let (mut plan, step) = start(&[3], &[2], &mut tables);
        assert_eq!(sent(step), Request::User { id: 3 });
        head(&mut plan, 200, &mut tables);
        assert_eq!(finish(&mut plan, body, &mut tables), ended(Err(api::Error::Unavailable)));
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), Err(api::Error::InvalidAnswer));
    }
    for (status, expected) in [
        (500, api::Error::Unavailable),
        (401, api::Error::Forbidden),
        (404, api::Error::Missing),
        (422, api::Error::InvalidAnswer),
    ] {
        let mut tables = cached_tables();
        let (mut plan, step) = start(&[3], &[2], &mut tables);
        assert_eq!(sent(step), Request::User { id: 3 });
        head(&mut plan, status, &mut tables);
        assert_eq!(finish(&mut plan, refusal(b"unknown"), &mut tables), ended(Err(expected)));
    }
    // Failure at the last lookup must not send the already-resolved addition.
    for failed in [Failure::Unseen, Failure::Seen] {
        let mut tables = cached_tables();
        tables.keep_user(KEY.forge, 3, Box::from(*b"alice"));
        let (mut plan, step) = start(&[3], &[2], &mut tables);
        assert_eq!(sent(step), Request::User { id: 2 });
        assert_eq!(plan.heard(Heard::Failed(failed), bounds(), WORD, &mut tables), ended(Err(api::Error::Unavailable)));
    }
    let mut tables = cached_tables();
    let mut plan = write_plan(reviewers(&[]));
    let request = begin(&mut plan, &mut tables);
    let Step::Send(exchange) = Step::send(request) else { panic!("exchange") };
    plan.set_pending(exchange.clone());
    assert_eq!(plan.pending(), Some(exchange));
    assert_eq!(plan.pending(), None);
    assert_eq!(finish(&mut plan, pull(&[]), &mut tables), Err(api::Error::InvalidAnswer));
    head(&mut plan, 200, &mut tables);
    assert_eq!(
        plan.heard(Heard::Head { status: 200, head: Head::new() }, bounds(), WORD, &mut tables),
        Err(api::Error::InvalidAnswer)
    );
    let mut wrong = update_pull();
    wrong.number = NUMBER + 1;
    assert_eq!(
        finish(&mut plan, Body::Document(Document::Pull(wrong)), &mut tables),
        ended(Err(api::Error::Unavailable))
    );
}

#[test]
fn github_reviewers_plan_bounds_before_mutation() {
    // Each id is 8 bytes; each retained login adds the measured string width and its bytes.
    let login_bytes = 16 + 2 * u32::try_from(crate::measure::STRING_BYTES).unwrap() + 1 + 5;
    for (answer_bytes, body_bytes, rows, succeeds) in [
        (login_bytes, 25, 2, true),
        (login_bytes - 1, 25, 2, false),
        (login_bytes, 24, 2, false),
        (login_bytes, 25, 1, false),
    ] {
        let b = Bounds { rows, answer_bytes, body_bytes, ..bounds() };
        let mut tables = Tables::new(1, 2);
        tables.keep_repository(KEY, tables::repository(&repository()));
        tables.keep_user(KEY.forge, 3, Box::from(*b"a"));
        tables.keep_user(KEY.forge, 2, Box::from(*b"b\"o\\b"));
        let mut plan = write_plan(reviewers(&[3]));
        plan.begin(b, WORD, &mut tables).unwrap();
        assert_eq!(plan.heard(Heard::Head { status: 200, head: Head::new() }, b, WORD, &mut tables), Ok(None));
        let step = plan.heard(Heard::Done(pull(&[2])), b, WORD, &mut tables);
        if succeeds {
            assert_eq!(sent(step), mutation(true, &[b"a"]));
        } else {
            assert_eq!(step, ended(Err(api::Error::TooLarge)));
        }
    }
    let mut tables = cached_tables();
    let b = Bounds { rows: 1, ..bounds() };
    let mut plan = write_plan(reviewers(&[2, 3]));
    assert_eq!(plan.begin(b, WORD, &mut tables), Err(api::Error::TooLarge));
    let b = Bounds { answer_bytes: 7, ..bounds() };
    let mut plan = write_plan(reviewers(&[2]));
    assert_eq!(plan.begin(b, WORD, &mut tables), Err(api::Error::TooLarge));
    let mut plan = write_plan(reviewers(&[]));
    begin(&mut plan, &mut tables);
    head(&mut plan, 200, &mut tables);
    assert_eq!(finish(&mut plan, pull(&[1, 2, 3, 4, 5]), &mut tables), ended(Err(api::Error::TooLarge)));
}

#[test]
fn github_reviewers_streamed_documents_and_limits() {
    for chunk in [1, 7] {
        let good = br#"{"id":9007199254740993,"login":"alice","unknown":{"nested":[true,"bob"]}}"#;
        assert_eq!(
            decode(Kind::User, good, chunk, &limits()).1,
            Ok(Document::User(document::User { id: NUMBER, login: Box::from(*b"alice") }))
        );
        for bad in [
            br#"{"id":9007199254740993,"login":"alice","id":3}"#.as_slice(),
            br#"{"id":3,"login":"alice","login":"bob"}"#,
            br#"{"login":"alice"}"#,
            br#"{"id":3,"login":true}"#,
        ] {
            decode(Kind::User, bad, chunk, &limits()).1.unwrap_err();
        }
        for (string_bytes, ok) in [(5, true), (4, false)] {
            assert_eq!(
                decode(Kind::User, good, chunk, &crate::github::limits::Limits { string_bytes, ..limits() }).1.is_ok(),
                ok
            );
        }
        assert_eq!(decode(Kind::Pull, UPDATE_PULL, chunk, &limits()).1, Ok(Document::Pull(update_pull())));
        let mut zero = limits();
        zero.inner = 0;
        decode(Kind::Pull, UPDATE_PULL, chunk, &zero).1.unwrap_err();
        let mut out = skein_lib::Writer::new(2);
        crate::github::server::done(&mut out).unwrap();
        assert_eq!(decode(Kind::Done, &out.finish(), chunk, &limits()).1, Ok(Document::Done));
        decode(Kind::Done, b"null", chunk, &limits()).1.unwrap_err();
    }
}

#[test]
fn github_reviewers_replay_rereads_and_diffs() {
    let (mut first, mut tables) = removal(true);
    assert_eq!(
        first.heard(Heard::Failed(Failure::Unseen), bounds(), WORD, &mut tables),
        ended(Err(api::Error::Timeout))
    );
    // A later call sees that alice was added, and sends only bob's removal.
    let (mut later, step) = start(&[3, 3], &[2, 3], &mut tables);
    assert_eq!(sent(step), mutation(false, &[b"bob"]));
    head(&mut later, 200, &mut tables);
    assert_eq!(finish(&mut later, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    let (_, step) = start(&[3], &[3], &mut tables);
    assert_eq!(step, ended(Ok(api::Answer::Done)));
}

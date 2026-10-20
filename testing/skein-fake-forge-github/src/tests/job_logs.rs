use super::{fixtures as f, read_support as r};
use crate::service::{Outcome, Response};
use skein_fake_forge_domain::api as a;
use skein_forge::github::{self, request::Method};
use skein_lib::{Duration, ReplyTo, Token, Wall};

fn failing() -> (f::Harness, u64) {
    let mut h = f::Harness::with_checks(
        f::limits(),
        f::config(),
        r::domain_config(),
        None,
        a::Checks {
            contexts: Box::from([f::boxed(b"ci")]),
            latency_min: Duration::ZERO,
            latency_max: Duration::ZERO,
            silent: 0,
            passes: 0,
            reruns: 0,
            cue: None,
        },
    );
    let (_, head) = r::pull(&mut h);
    (h, head)
}
fn blob(h: &mut f::Harness, target: &[u8], range: bool, now: Wall) -> Response {
    let outcome = h.service.blob(Method::Get, target, false, range, now, ReplyTo::new(Token::new(77)));
    match outcome {
        Outcome::Reply(response) => response,
        Outcome::Call(pending, event) => {
            let result = h.drive(event);
            let outcome = h.service.finish(pending, result, ReplyTo::new(Token::new(77)), &h.domain);
            let Outcome::Reply(response) = outcome else { panic!("one domain log read") };
            response
        }
    }
}
#[test]
fn github_fake_job_log_redirect_expires_at_600_seconds_and_blob_ignores_range() {
    let (mut h, _) = failing();
    let token = r::token(&mut h);
    let issued = h.now;
    let answer = h.ask(Method::Get, b"/repos/acme/widgets/actions/jobs/1/logs", Some(token.expose()), b"");
    assert_eq!(answer.status, 302);
    assert!(answer.body.is_empty());
    let redirect = github::redirect::parse(answer.location.as_deref().expect("Location"), 1024, 1024).unwrap();
    assert_eq!(redirect.origin.host.as_ref(), b"logs.example.test");
    let a::Answer::File(expected) =
        h.domain.inspect(&h.env.limits, b"acme/widgets", &a::Read::Log { job: 1, attempt: 1, step: None }).unwrap()
    else {
        panic!("whole domain log")
    };
    for (seconds, range, status) in [(0, false, 200), (599, true, 200), (600, false, 403)] {
        let now = Wall::from_nanos(issued.as_nanos().checked_add(seconds * 1_000_000_000).unwrap());
        let response = blob(&mut h, &redirect.target, range, now);
        assert_eq!(response.status, status);
        if status == 200 {
            assert_eq!(response.body, expected);
        } else {
            assert!(response.body.is_empty());
        }
        let record = h.service.blob_records().iter().last().unwrap();
        assert!(record.path == redirect.target);
        assert!(!record.authorization);
        assert_eq!(record.range, range);
    }
    for part in [b"/logs/2/".as_slice(), b"/1?expires=", b"?expires=", b"&signature="] {
        let mut tampered = f::boxed(&redirect.target);
        let at = skein_lib::bytes::find(&tampered, part).unwrap() + part.len();
        tampered[at] = if tampered[at] == b'1' { b'2' } else { b'1' };
        let response = blob(&mut h, &tampered, false, issued);
        assert_eq!(response.status, 403);
        assert!(response.body.is_empty());
    }
    for (target, status) in [(b"/other".as_slice(), 404), (b"/logs/invalid", 403)] {
        let response = blob(&mut h, target, false, issued);
        assert_eq!(response.status, status);
        assert!(response.body.is_empty());
    }
    assert_eq!(h.ask(Method::Get, b"/repos/acme/widgets/actions/jobs/999/logs", Some(token.expose()), b"").status, 404);
}

#[test]
fn github_fake_blob_record_and_origins_stay_within_admitted_bounds() {
    let limits = f::limits();
    let mut service = crate::Service::new(f::config(), &limits, 5).unwrap();
    let path = f::boxed(&[b'x'; 4096]);
    assert_eq!(path.len(), usize::try_from(limits.http.head).unwrap());
    for _ in 0..=limits.blob_records {
        let Outcome::Reply(response) =
            service.blob(Method::Get, &path, true, true, Wall::EPOCH, ReplyTo::new(Token::new(1)))
        else {
            panic!("unknown blob path")
        };
        assert_eq!(response.status, 404);
    }
    let records = service.blob_records();
    assert_eq!(records.len(), limits.blob_records);
    let mut held = skein_lib::Queue::<crate::logs::Record>::worst_case(limits.blob_records).unwrap();
    for row in records {
        held = held.checked_add(u64::try_from(row.path.len()).unwrap()).unwrap();
    }
    assert!(held <= crate::logs::worst_case(&limits).unwrap());
    service.set_blob_origin(f::boxed(b"http://logs.example.test")).unwrap();
    let oversized = crate::service::joined(b"http://", &[b'a'; 1024]).unwrap();
    assert!(service.set_blob_origin(oversized).is_err());
}

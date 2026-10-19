use super::respond;
use crate::api::{Error, Failure, Finish, Line, Part, Piece, Query, Report, Script, Turn};
use crate::tests::{CONFIG, query, says, script, user};
use alloc::boxed::Box;
use skein_lib::bytes::copy_of;
use skein_lib::{Duration, Rng};

const FIRST: u32 = 0;
const OTHER: u32 = 1;
const CALLS: u32 = 2; // The failed call and its retry.

fn report() -> Report {
    Report { code: copy_of(b"spent"), request: copy_of(b"req_42"), field: copy_of(b"top_p") }
}
fn fail(error: Error, credentials: Box<[u32]>) -> Line {
    Line::Fail { error, report: report(), credentials }
}
fn setup(lines: Box<[Line]>, credential: Option<u32>) -> (Query, [Script; 1]) {
    let mut query = query(Box::new([user(Box::new([Part::Text { text: copy_of(b"hi") }]))]));
    query.system = copy_of(b"@main");
    query.credential = credential;
    let turn = Turn { lines, ..says(b"") };
    (query, [script(b"@main", Box::new([turn]))])
}
fn reply(query: &Query, scripts: &[Script]) -> Result<(crate::api::Answer, Option<crate::pieces::Place>), Failure> {
    respond(
        &mut Rng::new(1),
        &mut 0,
        &CONFIG,
        scripts,
        &crate::api::Menu { arguments: Box::new([]), invalid: Box::new([]) },
        query,
    )
}

#[test]
fn a_failing_line_answers_its_class_and_report() {
    let rows = [
        Error::Exhausted { retry_after: Some(Duration::from_secs(2)) },
        Error::Exhausted { retry_after: None },
        Error::Refused,
        Error::Unsupported,
        Error::Unauthorized,
    ];
    for error in rows {
        let (query, scripts) = setup(Box::new([fail(error, Box::new([]))]), None);
        assert_eq!(reply(&query, &scripts), Err(Failure { error, report: report(), parts: Box::new([]) }), "{error:?}");
    }
}

#[test]
fn a_failure_leaves_the_turn_for_the_call_made_again() {
    let (query, scripts) = setup(Box::new([fail(Error::Refused, Box::new([]))]), None);
    for _ in 0..CALLS {
        assert_eq!(
            reply(&query, &scripts),
            Err(Failure { error: Error::Refused, report: report(), parts: Box::new([]) })
        );
    }
}

#[test]
fn a_failing_line_naming_credentials_fails_only_their_calls() {
    let rows: [(Box<[u32]>, _, _); 5] = [
        (Box::new([OTHER]), Some(OTHER), true),
        (Box::new([OTHER]), Some(FIRST), false),
        (Box::new([OTHER]), None, false),
        (Box::new([]), Some(FIRST), true),
        (Box::new([]), None, true),
    ];
    for (credentials, credential, fails) in rows {
        let mut lines = skein_lib::List::with_capacity(2);
        lines.push(fail(Error::Refused, credentials.clone())).expect("room for failing line");
        if !credentials.is_empty() {
            lines.push(Line::Text { text: copy_of(b"through") }).expect("room for text");
        }
        let (query, scripts) = setup(lines.into_boxed(), credential);
        let expected: Result<(Box<[Part]>, Finish), Failure> = if fails {
            Err(Failure { error: Error::Refused, report: report(), parts: Box::new([]) })
        } else {
            Ok((Box::new([Part::Text { text: copy_of(b"through") }]), Finish::Stop))
        };
        let got = match reply(&query, &scripts) {
            Ok((answer, _)) => Ok((answer.parts, answer.finish)),
            Err(failure) => Err(failure),
        };
        assert_eq!(got, expected, "{credentials:?} {credential:?}");
    }
}

#[test]
fn a_failing_line_after_text_keeps_the_text_before_it() {
    let rows = [
        Line::Text { text: copy_of(b"before") },
        Line::Pieces {
            pieces: Box::new([
                Piece { text: copy_of(b"be"), hold: None },
                Piece { text: copy_of(b"fore"), hold: None },
            ]),
        },
    ];
    for line in rows {
        let (query, scripts) = setup(Box::new([line, fail(Error::Refused, Box::new([]))]), None);
        assert_eq!(
            reply(&query, &scripts),
            Err(Failure {
                error: Error::Refused,
                report: report(),
                parts: Box::new([Part::Text { text: copy_of(b"before") }])
            })
        );
    }
}

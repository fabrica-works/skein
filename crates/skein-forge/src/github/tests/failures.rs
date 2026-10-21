use super::{Asked, Drop, Duration, Head, RateFields, Verdict, Wall, api, failure, header};
struct Case {
    label: &'static str,
    asked: Asked,
    status: u16,
    message: &'static [u8],
    errors: &'static [&'static [u8]],
    head: Head,
    expected: Verdict,
}
fn fail(error: api::Error) -> Verdict {
    Verdict::Fail(error, Drop::Nothing)
}
#[expect(
    clippy::too_many_lines,
    reason = "one table keeps every section 7 failure beside its neighbors of the same status"
)]
pub(super) fn check() {
    let rate = RateFields { limit: 5000, remaining: 0, reset: Wall::from_nanos(1_792_483_260_000_000_000) };
    let mut primary = Head::new();
    primary.rate = Some(rate);
    primary.date = Some(super::wall());
    let mut retry = primary.clone();
    retry.retry_after = Some(Duration::from_nanos(12_000_000_000));
    let mut secondary = Head::new();
    secondary.retry_after = Some(Duration::from_nanos(12_000_000_000));
    let mut no_date = Head::new();
    no_date.rate = Some(rate);
    let mut past = primary.clone();
    past.date = Some(Wall::from_nanos(1_792_483_261_000_000_000));
    let mut partial = Head::new();
    for (name, value) in [
        (b"x-ratelimit-remaining".as_slice(), b"0".as_slice()),
        (b"x-ratelimit-reset", b"1792483290"),
        (b"Date", b"Tue, 20 Oct 2026 08:00:00 GMT"),
    ] {
        header::field(&mut partial, name, value, &super::limits());
    }
    let cases = [
        Case {
            label: "moved",
            asked: Asked::Read,
            status: 301,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::Unavailable, Drop::Row),
        },
        Case {
            label: "redirected",
            asked: Asked::Write { empty: false },
            status: 307,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::Unavailable, Drop::Row),
        },
        Case {
            label: "partial rate uses reset",
            asked: Asked::Read,
            status: 403,
            message: b"",
            errors: &[],
            head: partial,
            expected: fail(api::Error::RateLimited { after: Duration::from_nanos(90_000_000_000) }),
        },
        Case {
            label: "primary reset",
            asked: Asked::Read,
            status: 403,
            message: b"",
            errors: &[],
            head: primary,
            expected: fail(api::Error::RateLimited { after: Duration::from_nanos(60_000_000_000) }),
        },
        Case {
            label: "primary retry wins",
            asked: Asked::Read,
            status: 429,
            message: b"",
            errors: &[],
            head: retry,
            expected: fail(api::Error::RateLimited { after: Duration::from_nanos(12_000_000_000) }),
        },
        Case {
            label: "primary missing date",
            asked: Asked::Read,
            status: 403,
            message: b"",
            errors: &[],
            head: no_date,
            expected: fail(api::Error::RateLimited { after: Duration::from_nanos(60_000_000_000) }),
        },
        Case {
            label: "primary past reset",
            asked: Asked::Read,
            status: 429,
            message: b"",
            errors: &[],
            head: past,
            expected: fail(api::Error::RateLimited { after: Duration::ZERO }),
        },
        Case {
            label: "secondary message",
            asked: Asked::Read,
            status: 403,
            message: b"You have exceeded a secondary rate limit",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::RateLimited { after: Duration::from_nanos(60_000_000_000) }),
        },
        Case {
            label: "secondary retry",
            asked: Asked::Read,
            status: 429,
            message: b"unknown",
            errors: &[],
            head: secondary,
            expected: fail(api::Error::RateLimited { after: Duration::from_nanos(12_000_000_000) }),
        },
        Case {
            label: "server error",
            asked: Asked::Read,
            status: 503,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Timeout),
        },
        Case {
            label: "unauthorized",
            asked: Asked::Read,
            status: 401,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Forbidden),
        },
        Case {
            label: "integration",
            asked: Asked::Read,
            status: 403,
            message: b"Resource not accessible by integration",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Forbidden),
        },
        Case {
            label: "unknown forbidden",
            asked: Asked::Write { empty: false },
            status: 403,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Forbidden),
        },
        Case {
            label: "missing",
            asked: Asked::Read,
            status: 404,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::Missing, Drop::Row),
        },
        Case {
            label: "gone",
            asked: Asked::Write { empty: false },
            status: 410,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::Missing, Drop::Row),
        },
        Case {
            label: "missing job",
            asked: Asked::Job,
            status: 404,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::MissingJob, Drop::Row),
        },
        Case {
            label: "gone job",
            asked: Asked::Job,
            status: 410,
            message: b"",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::MissingJob, Drop::Row),
        },
        Case {
            label: "merge stale",
            asked: Asked::Merge,
            status: 409,
            message: b"Head branch was modified. Review and try the merge again.",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Stale),
        },
        Case {
            label: "not mergeable",
            asked: Asked::Merge,
            status: 405,
            message: b"Pull Request is not mergeable",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Conflict),
        },
        Case {
            label: "queue",
            asked: Asked::Merge,
            status: 405,
            message: b"Repository rule violations found\nChanges must be made through the merge queue",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Enqueue,
        },
        Case {
            label: "other rule",
            asked: Asked::Merge,
            status: 405,
            message: b"Repository rule violations found\nAt least one approving review is required",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Protected),
        },
        Case {
            label: "method",
            asked: Asked::Merge,
            status: 405,
            message: b"Squash merges are not allowed on this repository.",
            errors: &[],
            head: Head::new(),
            expected: Verdict::Fail(api::Error::Refused, Drop::Row),
        },
        Case {
            label: "update stale",
            asked: Asked::Update,
            status: 422,
            message: "expected head sha didn’t match current head ref.".as_bytes(),
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Stale),
        },
        Case {
            label: "update conflict",
            asked: Asked::Update,
            status: 422,
            message: b"merge conflict between base and head",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Conflict),
        },
        Case {
            label: "no commits",
            asked: Asked::OpenPull { empty: false },
            status: 422,
            message: b"Validation Failed",
            errors: &[b"No commits between main and feature"],
            head: Head::new(),
            expected: fail(api::Error::NothingToMerge),
        },
        Case {
            label: "pull exists",
            asked: Asked::OpenPull { empty: false },
            status: 422,
            message: b"Validation Failed",
            errors: &[b"A pull request already exists for o:b."],
            head: Head::new(),
            expected: fail(api::Error::Exists),
        },
        Case {
            label: "own approval",
            asked: Asked::Review { empty: false },
            status: 422,
            message: b"Can not approve your own pull request",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
        Case {
            label: "empty write",
            asked: Asked::Write { empty: true },
            status: 422,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Empty),
        },
        Case {
            label: "empty pull",
            asked: Asked::OpenPull { empty: true },
            status: 422,
            message: b"No commits between main and feature",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::NothingToMerge),
        },
        Case {
            label: "empty review",
            asked: Asked::Review { empty: true },
            status: 422,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Empty),
        },
        Case {
            label: "nonempty write",
            asked: Asked::Write { empty: false },
            status: 422,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
        Case {
            label: "nonempty pull",
            asked: Asked::OpenPull { empty: false },
            status: 422,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
        Case {
            label: "nonempty review",
            asked: Asked::Review { empty: false },
            status: 422,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
        Case {
            label: "branch exists",
            asked: Asked::CreateBranch,
            status: 422,
            message: b"Reference already exists",
            errors: &[],
            head: Head::new(),
            expected: Verdict::BranchExists,
        },
        Case {
            label: "branch absent",
            asked: Asked::DeleteBranch,
            status: 422,
            message: b"Reference does not exist",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Missing),
        },
        Case {
            label: "unknown read",
            asked: Asked::Read,
            status: 422,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::InvalidAnswer),
        },
        Case {
            label: "unknown write",
            asked: Asked::Enqueue,
            status: 400,
            message: b"unknown",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
        Case {
            label: "whole match only",
            asked: Asked::Merge,
            status: 409,
            message: b"Head branch was modified. Review and try the merge again. extra",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
        Case {
            label: "wrong status",
            asked: Asked::Merge,
            status: 422,
            message: b"Head branch was modified. Review and try the merge again.",
            errors: &[],
            head: Head::new(),
            expected: fail(api::Error::Refused),
        },
    ];
    for case in cases {
        let mut errors = skein_lib::List::with_capacity(super::INNER);
        for error in case.errors {
            errors.push(skein_lib::bytes::copy_of(error)).unwrap();
        }
        let refusal = super::super::document::Refusal {
            message: skein_lib::bytes::copy_of(case.message),
            errors: errors.into_boxed(),
        };
        assert_eq!(
            failure::classify(case.asked, case.status, &case.head, Some(&refusal)),
            case.expected,
            "{}",
            case.label
        );
    }
    for (label, kind, message, expected) in [
        (
            "stale",
            b"UNPROCESSABLE".as_slice(),
            b"enqueuePullRequest: expected head oid does not match the current head oid".as_slice(),
            api::Error::Stale,
        ),
        (
            "pending ci",
            b"UNPROCESSABLE",
            b"Pull request Required status check \"ci\" is in progress.",
            api::Error::Protected,
        ),
        (
            "pending build",
            b"UNPROCESSABLE",
            b"Pull request Required status check \"build\" is in progress.",
            api::Error::Protected,
        ),
        (
            "check wrong type",
            b"FORBIDDEN",
            b"Pull request Required status check \"ci\" is in progress.",
            api::Error::Refused,
        ),
        (
            "check not prefix",
            b"UNPROCESSABLE",
            b"other Pull request Required status check \"ci\" is in progress.",
            api::Error::Refused,
        ),
        ("unknown", b"UNPROCESSABLE", b"other refusal", api::Error::Refused),
        ("wrong type", b"FORBIDDEN", b"expected head oid does not match the current head oid", api::Error::Refused),
        (
            "whole suffix",
            b"UNPROCESSABLE",
            b"expected head oid does not match the current head oid extra",
            api::Error::Refused,
        ),
    ] {
        assert_eq!(failure::graphql(kind, message), expected, "{label}");
    }
    let mut head = Head::new();
    header::field(&mut head, b"x-ratelimit-remaining", b"0", &super::limits());
    assert_eq!(
        failure::classify(Asked::Read, 403, &head, None),
        fail(api::Error::RateLimited { after: Duration::from_nanos(60_000_000_000) })
    );
    assert_eq!(failure::classify(Asked::Read, 400, &Head::new(), None), fail(api::Error::InvalidAnswer));
}

use super::fixtures::{boxed, fake_config};
use crate::observations::{self, Reason};
use alloc::boxed::Box;
use skein_fake_forge_domain::{Branches, Observation, Operation, api as a, promises::Recorded};
use skein_lib::{List, Time};

fn assert_record(actual: Recorded, expected: &Recorded) {
    match (actual, expected) {
        (Recorded::Lost(actual), Recorded::Lost(expected)) => assert_eq!(actual, *expected),
        (
            Recorded::Observed { observation, forward },
            Recorded::Observed { observation: expected, forward: expected_forward },
        ) => {
            assert_eq!(&observation, expected);
            assert_eq!(&forward, expected_forward);
        }
        (Recorded::Lost(_), Recorded::Observed { .. }) | (Recorded::Observed { .. }, Recorded::Lost(_)) => {
            panic!("record kind")
        }
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one table specifies every variant and enum spelling and its exact JSON line"
)]
fn observation_lines_round_trip() {
    let rows = [
        (
            "Moved",
            Recorded::Observed {
                observation: Observation::Moved {
                    repository: boxed(b"org/repo"),
                    branch: boxed(b"work"),
                    from: Some(3),
                    to: 7,
                    by: 2,
                },
                forward: Some(true),
            },
            r#"{"observation":"moved","repository":"org/repo","branch":"work","from":3,"to":7,"by":2,"forward":true}"#,
        ),
        (
            "Deleted",
            Recorded::Observed {
                observation: Observation::Deleted {
                    repository: boxed(b"org/repo"),
                    branch: boxed(b"work"),
                    at: 7,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"deleted","repository":"org/repo","branch":"work","at":7,"by":2}"#,
        ),
        (
            "Opened",
            Recorded::Observed {
                observation: Observation::Opened {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    kind: a::Kind::Pull,
                    title: boxed(b"title"),
                    body: boxed(b"body"),
                    labels: Box::new([boxed(b"bug"), boxed(b"ready")]),
                    branches: Some(Branches { head: boxed(b"work"), base: boxed(b"main"), commit: 7 }),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"opened","repository":"org/repo","number":7,"kind":"pull","title":"dGl0bGU=","body":"Ym9keQ==","labels":["bug","ready"],"branches":{"head":"work","base":"main","commit":7},"by":2}"#,
        ),
        (
            "Closed",
            Recorded::Observed {
                observation: Observation::Closed { repository: boxed(b"org/repo"), number: 7, by: 2 },
                forward: None,
            },
            r#"{"observation":"closed","repository":"org/repo","number":7,"by":2}"#,
        ),
        (
            "Reopened",
            Recorded::Observed {
                observation: Observation::Reopened { repository: boxed(b"org/repo"), number: 7, by: 2 },
                forward: None,
            },
            r#"{"observation":"reopened","repository":"org/repo","number":7,"by":2}"#,
        ),
        (
            "Labelled",
            Recorded::Observed {
                observation: Observation::Labelled {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    labels: Box::new([boxed(b"bug"), boxed(b"ready")]),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"labelled","repository":"org/repo","number":7,"labels":["bug","ready"],"by":2}"#,
        ),
        (
            "Revised",
            Recorded::Observed {
                observation: Observation::Revised {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    title: boxed(b"title"),
                    body: boxed(b"body"),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"revised","repository":"org/repo","number":7,"title":"dGl0bGU=","body":"Ym9keQ==","by":2}"#,
        ),
        (
            "Depends",
            Recorded::Observed {
                observation: Observation::Depends {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    dependencies: Box::new([3, 4]),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"depends","repository":"org/repo","number":7,"dependencies":[3,4],"by":2}"#,
        ),
        (
            "Requested",
            Recorded::Observed {
                observation: Observation::Requested {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    reviewers: Box::new([3, 4]),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"requested","repository":"org/repo","number":7,"reviewers":[3,4],"by":2}"#,
        ),
        (
            "Defined",
            Recorded::Observed {
                observation: Observation::Defined { repository: boxed(b"org/repo"), label: boxed(b"bug"), by: 2 },
                forward: None,
            },
            r#"{"observation":"defined","repository":"org/repo","label":"bug","by":2}"#,
        ),
        (
            "Commented",
            Recorded::Observed {
                observation: Observation::Commented {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    id: 7,
                    body: boxed(b"body"),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"commented","repository":"org/repo","number":7,"id":7,"body":"Ym9keQ==","by":2}"#,
        ),
        (
            "Edited",
            Recorded::Observed {
                observation: Observation::Edited {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    id: 7,
                    body: boxed(b"body"),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"edited","repository":"org/repo","number":7,"id":7,"body":"Ym9keQ==","by":2}"#,
        ),
        (
            "Removed",
            Recorded::Observed {
                observation: Observation::Removed { repository: boxed(b"org/repo"), number: 7, id: 7, by: 2 },
                forward: None,
            },
            r#"{"observation":"removed","repository":"org/repo","number":7,"id":7,"by":2}"#,
        ),
        (
            "Reviewed",
            Recorded::Observed {
                observation: Observation::Reviewed {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    id: 7,
                    commit: 7,
                    verdict: a::Verdict::RequestChanges,
                    body: boxed(b"body"),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"reviewed","repository":"org/repo","number":7,"id":7,"commit":7,"verdict":"request_changes","body":"Ym9keQ==","by":2}"#,
        ),
        (
            "Reported",
            Recorded::Observed {
                observation: Observation::Reported {
                    repository: boxed(b"org/repo"),
                    commit: 7,
                    context: boxed(b"ci"),
                    state: a::Check::Passed,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"reported","repository":"org/repo","commit":7,"context":"ci","state":"passed","by":2}"#,
        ),
        (
            "Merged",
            Recorded::Observed {
                observation: Observation::Merged {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    base: boxed(b"main"),
                    head: 7,
                    commit: 7,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"merged","repository":"org/repo","number":7,"base":"main","head":7,"commit":7,"by":2}"#,
        ),
        (
            "Enqueued",
            Recorded::Observed {
                observation: Observation::Enqueued {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    head: 7,
                    position: 7,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"enqueued","repository":"org/repo","number":7,"head":7,"position":7,"by":2}"#,
        ),
        (
            "Group",
            Recorded::Observed {
                observation: Observation::Group {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    commit: 7,
                    ended: true,
                    landed: false,
                },
                forward: None,
            },
            r#"{"observation":"group","repository":"org/repo","number":7,"commit":7,"ended":true,"landed":false}"#,
        ),
        (
            "Dequeued",
            Recorded::Observed {
                observation: Observation::Dequeued {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    reason: a::QueueRemoval::MergeConflict,
                },
                forward: None,
            },
            r#"{"observation":"dequeued","repository":"org/repo","number":7,"reason":"merge_conflict"}"#,
        ),
        (
            "Refused",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: Some(3),
                    commit: Some(3),
                    error: a::Error::Unavailable,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":3,"commit":3,"error":{"name":"unavailable"},"by":2}"#,
        ),
        (
            "Rejected",
            Recorded::Observed {
                observation: Observation::Rejected {
                    repository: boxed(b"org/repo"),
                    branch: boxed(b"work"),
                    commit: 7,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"rejected","repository":"org/repo","branch":"work","commit":7,"by":2}"#,
        ),
        (
            "Wiki",
            Recorded::Observed {
                observation: Observation::Wiki {
                    repository: boxed(b"org/repo"),
                    name: boxed(b"page"),
                    content: Some(Box::new([0, 255, 10])),
                    revision: 7,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"wiki","repository":"org/repo","name":"page","content":"AP8K","revision":7,"by":2}"#,
        ),
        (
            "error Unavailable",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Unavailable,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"unavailable"},"by":2}"#,
        ),
        (
            "error Timeout",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Timeout,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"timeout"},"by":2}"#,
        ),
        (
            "error RateLimited",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::RateLimited { reset: Time::from_nanos(u64::MAX) },
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"rate_limited","reset":18446744073709551615},"by":2}"#,
        ),
        (
            "error Forbidden",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "error Missing",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Repository),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"repository"},"by":2}"#,
        ),
        (
            "error TooLarge",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::TooLarge,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"too_large"},"by":2}"#,
        ),
        (
            "error Full",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Full,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"full"},"by":2}"#,
        ),
        (
            "error Exists",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Exists,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"exists"},"by":2}"#,
        ),
        (
            "error AlreadyQueued",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::AlreadyQueued(a::QueueEntry { number: 7, head: 8, position: u32::MAX }),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"already_queued","number":7,"head":8,"position":4294967295},"by":2}"#,
        ),
        (
            "error Circular",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Circular,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"circular"},"by":2}"#,
        ),
        (
            "error NothingToMerge",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::NothingToMerge,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"nothing_to_merge"},"by":2}"#,
        ),
        (
            "error Empty",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Empty,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"empty"},"by":2}"#,
        ),
        (
            "error Closed",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Closed,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"closed"},"by":2}"#,
        ),
        (
            "error Stale",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Stale,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"stale"},"by":2}"#,
        ),
        (
            "error Conflict",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Conflict,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"conflict"},"by":2}"#,
        ),
        (
            "error Protected",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Protected,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"protected"},"by":2}"#,
        ),
        (
            "error Unreachable",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Unreachable,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"unreachable"},"by":2}"#,
        ),
        (
            "error Refused",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Refused,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"refused"},"by":2}"#,
        ),
        (
            "missing Item",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Item),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"item"},"by":2}"#,
        ),
        (
            "missing Pull",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Pull),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"pull"},"by":2}"#,
        ),
        (
            "missing Comment",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Comment),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"comment"},"by":2}"#,
        ),
        (
            "missing Label",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Label),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"label"},"by":2}"#,
        ),
        (
            "missing Branch",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Branch),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"branch"},"by":2}"#,
        ),
        (
            "missing Commit",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Commit),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"commit"},"by":2}"#,
        ),
        (
            "missing File",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::File),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"file"},"by":2}"#,
        ),
        (
            "missing Page",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Page),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"page"},"by":2}"#,
        ),
        (
            "missing Review",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Review),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"review"},"by":2}"#,
        ),
        (
            "missing Job",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::CreateIssue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Missing(a::What::Job),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create_issue","number":null,"commit":18446744073709551615,"error":{"name":"missing","what":"job"},"by":2}"#,
        ),
        (
            "operation EditItem",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::EditItem,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"edit_item","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation SetDependencies",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::SetDependencies,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"set_dependencies","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Comment",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Comment,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"comment","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation EditComment",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::EditComment,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"edit_comment","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation DeleteComment",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::DeleteComment,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"delete_comment","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation SetLabels",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::SetLabels,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"set_labels","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation AddLabels",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::AddLabels,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"add_labels","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation RemoveLabels",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::RemoveLabels,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"remove_labels","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation DefineLabel",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::DefineLabel,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"define_label","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation OpenPull",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::OpenPull,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"open_pull","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation SetReviewers",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::SetReviewers,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"set_reviewers","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Review",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Review,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"review","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Submit",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Submit,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"submit","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Merge",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Merge,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"merge","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Enqueue",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Enqueue,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"enqueue","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Update",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Update,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"update","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Close",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Close,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"close","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Reopen",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Reopen,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"reopen","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation DeleteBranch",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::DeleteBranch,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"delete_branch","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Status",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Status,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"status","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation PutPage",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::PutPage,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"put_page","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation DeletePage",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::DeletePage,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"delete_page","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Clone",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Clone,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"clone","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Fetch",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Fetch,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"fetch","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Push",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Push,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"push","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "operation Create",
            Recorded::Observed {
                observation: Observation::Refused {
                    repository: boxed(b"org/repo"),
                    what: Operation::Create,
                    number: None,
                    commit: Some(u64::MAX),
                    error: a::Error::Forbidden,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"refused","repository":"org/repo","what":"create","number":null,"commit":18446744073709551615,"error":{"name":"forbidden"},"by":2}"#,
        ),
        (
            "first creation",
            Recorded::Observed {
                observation: Observation::Moved {
                    repository: boxed(b"org/repo"),
                    branch: boxed(b"work"),
                    from: None,
                    to: 1,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"moved","repository":"org/repo","branch":"work","from":null,"to":1,"by":2,"forward":null}"#,
        ),
        (
            "backward",
            Recorded::Observed {
                observation: Observation::Moved {
                    repository: boxed(b"org/repo"),
                    branch: boxed(b"work"),
                    from: Some(2),
                    to: 1,
                    by: 2,
                },
                forward: Some(false),
            },
            r#"{"observation":"moved","repository":"org/repo","branch":"work","from":2,"to":1,"by":2,"forward":false}"#,
        ),
        (
            "issue",
            Recorded::Observed {
                observation: Observation::Opened {
                    repository: boxed(b"org/repo"),
                    number: 1,
                    kind: a::Kind::Issue,
                    title: boxed(b""),
                    body: Box::new([255]),
                    labels: Box::new([]),
                    branches: None,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"opened","repository":"org/repo","number":1,"kind":"issue","title":"","body":"/w==","labels":[],"branches":null,"by":2}"#,
        ),
        (
            "wiki deletion",
            Recorded::Observed {
                observation: Observation::Wiki {
                    repository: boxed(b"org/repo"),
                    name: boxed(b"page"),
                    content: None,
                    revision: 9,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"wiki","repository":"org/repo","name":"page","content":null,"revision":9,"by":2}"#,
        ),
        ("checkpoint", Recorded::Lost(u64::MAX), r#"{"observations_lost":18446744073709551615}"#),
        (
            "Approve",
            Recorded::Observed {
                observation: Observation::Reviewed {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    id: 7,
                    commit: 7,
                    verdict: a::Verdict::Approve,
                    body: boxed(b"body"),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"reviewed","repository":"org/repo","number":7,"id":7,"commit":7,"verdict":"approve","body":"Ym9keQ==","by":2}"#,
        ),
        (
            "Comment",
            Recorded::Observed {
                observation: Observation::Reviewed {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    id: 7,
                    commit: 7,
                    verdict: a::Verdict::Comment,
                    body: boxed(b"body"),
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"reviewed","repository":"org/repo","number":7,"id":7,"commit":7,"verdict":"comment","body":"Ym9keQ==","by":2}"#,
        ),
        (
            "Pending",
            Recorded::Observed {
                observation: Observation::Reported {
                    repository: boxed(b"org/repo"),
                    commit: 7,
                    context: boxed(b"ci"),
                    state: a::Check::Pending,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"reported","repository":"org/repo","commit":7,"context":"ci","state":"pending","by":2}"#,
        ),
        (
            "Failed",
            Recorded::Observed {
                observation: Observation::Reported {
                    repository: boxed(b"org/repo"),
                    commit: 7,
                    context: boxed(b"ci"),
                    state: a::Check::Failed,
                    by: 2,
                },
                forward: None,
            },
            r#"{"observation":"reported","repository":"org/repo","commit":7,"context":"ci","state":"failed","by":2}"#,
        ),
        (
            "CiFailure",
            Recorded::Observed {
                observation: Observation::Dequeued {
                    repository: boxed(b"org/repo"),
                    number: 7,
                    reason: a::QueueRemoval::CiFailure,
                },
                forward: None,
            },
            r#"{"observation":"dequeued","repository":"org/repo","number":7,"reason":"ci_failure"}"#,
        ),
    ];
    for (name, record, line) in rows {
        let size = observations::encoded_len(&record).expect("UTF-8 row");
        let mut output = List::with_capacity(size);
        assert_eq!(observations::encode(&record, &mut output), Some(()), "{name}");
        let output = output.into_boxed();
        assert_eq!(output.last(), Some(&b'\n'), "{name}");
        assert_eq!(&output[..output.len().checked_sub(1).expect("newline")], line.as_bytes(), "{name}");
        assert_record(observations::decode(line.as_bytes()).expect(name), &record);
        assert_record(observations::decode_with_limits(line.as_bytes(), &fake_config().limits).expect(name), &record);
        let mut short = List::with_capacity(size.checked_sub(1).expect("nonempty line"));
        short.push(b'!').expect("prefix");
        assert_eq!(observations::encode(&record, &mut short), None, "{name}");
        assert_eq!(short.into_boxed().as_ref(), b"!", "{name}");
    }
}

#[test]
fn non_utf8_names_have_no_line() {
    let rows = [
        Observation::Moved { repository: boxed(b"repo"), branch: Box::new([255]), from: None, to: 1, by: 2 },
        Observation::Defined { repository: boxed(b"repo"), label: Box::new([255]), by: 2 },
        Observation::Closed { repository: Box::new([255]), number: 1, by: 2 },
        Observation::Opened {
            repository: boxed(b"repo"),
            number: 1,
            kind: a::Kind::Pull,
            title: boxed(b""),
            body: boxed(b""),
            labels: Box::new([Box::new([255])]),
            branches: None,
            by: 2,
        },
        Observation::Opened {
            repository: boxed(b"repo"),
            number: 1,
            kind: a::Kind::Pull,
            title: boxed(b""),
            body: boxed(b""),
            labels: Box::new([]),
            branches: Some(Branches { head: Box::new([255]), base: boxed(b"main"), commit: 1 }),
            by: 2,
        },
        Observation::Reported {
            repository: boxed(b"repo"),
            commit: 1,
            context: Box::new([255]),
            state: a::Check::Pending,
            by: 2,
        },
        Observation::Wiki { repository: boxed(b"repo"), name: Box::new([255]), content: None, revision: 1, by: 2 },
    ];
    for observation in rows {
        let record = Recorded::Observed { observation, forward: None };
        let mut output = List::with_capacity(4096);
        for byte in b"prefix" {
            output.push(*byte).expect("prefix fits");
        }
        assert_eq!(observations::encode(&record, &mut output), None);
        assert_eq!(observations::encoded_len(&record), None);
        assert_eq!(output.into_boxed().as_ref(), b"prefix");
    }
}

#[test]
fn refused_lines_name_the_member() {
    let rows = [
        (r#"{"observation":"closed","repository":"r","number":1,"by":2,"unknown":0}"#, "unknown", Reason::Unknown),
        (
            r#"{"repository":"r","repository":"s","number":1,"by":2,"observation":"closed"}"#,
            "repository",
            Reason::Repeated,
        ),
        (r#"{"repository":"r","number":1,"observation":"closed"}"#, "by", Reason::Missing),
        (r#"{"observation":"closed","repository":"r","number":1,"by":2,"forward":null}"#, "forward", Reason::Unknown),
        (
            r#"{"observation":"revised","repository":"r","number":1,"title":"%","body":"","by":2}"#,
            "title",
            Reason::Value,
        ),
        (r#"{"observation":"closed","repository":"r","number":18446744073709551616,"by":2}"#, "number", Reason::Value),
        (r#"{"observation":"closed","repository":4,"number":1,"by":2}"#, "repository", Reason::Type),
        (
            r#"{"observation":"opened","repository":"r","number":1,"kind":"pull","title":"","body":"","labels":[],"branches":{"head":"a","base":"b","commit":1,"extra":0},"by":2}"#,
            "extra",
            Reason::Unknown,
        ),
        (
            r#"{"observation":"refused","repository":"r","what":"push","number":null,"commit":null,"error":{"name":"rate_limited"},"by":2}"#,
            "reset",
            Reason::Missing,
        ),
        (
            r#"{"observation":"refused","repository":"r","what":"push","number":null,"commit":null,"error":{"name":"missing","what":"branch","what":"commit"},"by":2}"#,
            "what",
            Reason::Repeated,
        ),
        (r#"{"observation":"closed","repository":"r","number":-1,"by":2}"#, "number", Reason::Value),
        (r#"{"observation":"closed","repository":"r","number":1.0,"by":2}"#, "number", Reason::Value),
        (r#"{"observation":"closed","repository":"r","number":1e2,"by":2}"#, "number", Reason::Value),
        (r#"{"observation":"closed","repository":"r","number":1,"by":2,"by":3}"#, "by", Reason::Repeated),
        (
            r#"{"observation":"moved","repository":"r","branch":"b","from":null,"to":1,"by":2}"#,
            "forward",
            Reason::Missing,
        ),
        (
            r#"{"observation":"closed","repository":"r","number":1,"by":2,"k0":0,"k1":0,"k2":0,"k3":0,"k4":0,"k5":0,"k6":0,"k7":0,"k8":0}"#,
            "k8",
            Reason::Limit,
        ),
    ];
    for (line, member, reason) in rows {
        let refusal = observations::decode(line.as_bytes()).expect_err("refused row");
        assert_eq!(refusal.member.as_ref(), member.as_bytes(), "{line}");
        assert_eq!(refusal.reason, reason, "{line}");
        assert!(refusal.variant.is_some(), "known variant");
    }
}

#[test]
fn reordered_members_and_escaped_names_decode() {
    let line = br#"{"by":2,"branches":{"commit":7,"base":"main","head":"w\u006frk"},"labels":["b\u0075g"],"body":"AP8=","title":"dA==","kind":"pull","number":1,"repository":"o\u0072g/\"repo","observation":"opened"}"#;
    let expected = Recorded::Observed {
        observation: Observation::Opened {
            repository: boxed(b"org/\"repo"),
            number: 1,
            kind: a::Kind::Pull,
            title: boxed(b"t"),
            body: Box::new([0, 255]),
            labels: Box::new([boxed(b"bug")]),
            branches: Some(Branches { head: boxed(b"work"), base: boxed(b"main"), commit: 7 }),
            by: 2,
        },
        forward: None,
    };
    assert_record(observations::decode(line).expect("reordered object"), &expected);
}

#[test]
fn domain_limits_bound_the_reader() {
    let rows = [
        (r#"{"observation":"closed","repository":"ab","number":1,"by":2}"#, 2, 1, 1, 1, None),
        (r#"{"observation":"closed","repository":"abc","number":1,"by":2}"#, 2, 1, 1, 1, Some("repository")),
        (r#"{"observation":"labelled","repository":"r","number":1,"labels":["a"],"by":2}"#, 2, 1, 1, 1, None),
        (
            r#"{"observation":"labelled","repository":"r","number":1,"labels":["a","b"],"by":2}"#,
            2,
            1,
            1,
            1,
            Some("labels"),
        ),
        (r#"{"observation":"revised","repository":"r","number":1,"title":"","body":"/w==","by":2}"#, 2, 1, 1, 1, None),
        (
            r#"{"observation":"revised","repository":"r","number":1,"title":"","body":"//8=","by":2}"#,
            2,
            1,
            1,
            1,
            Some("body"),
        ),
        (r#"{"observation":"depends","repository":"r","number":1,"dependencies":[1],"by":2}"#, 2, 1, 1, 1, None),
        (
            r#"{"observation":"depends","repository":"r","number":1,"dependencies":[1,2],"by":2}"#,
            2,
            1,
            1,
            1,
            Some("dependencies"),
        ),
        (
            r#"{"observation":"opened","repository":"r","number":1,"kind":"pull","title":"","body":"","labels":[],"branches":{"head":"a","base":"b","commit":[[1]]},"by":2}"#,
            2,
            1,
            1,
            1,
            Some("commit"),
        ),
    ];
    for (line, name_bytes, labels, body_bytes, dependencies, refused) in rows {
        let mut limits = fake_config().limits;
        limits.name_bytes = name_bytes;
        limits.labels = labels;
        limits.body_bytes = body_bytes;
        limits.dependencies = dependencies;
        let result = observations::decode_with_limits(line.as_bytes(), &limits);
        match refused {
            Some(member) => assert_eq!(result.expect_err("past limit").member.as_ref(), member.as_bytes()),
            None => {
                result.expect("within limit");
            }
        }
    }
}

use super::fixtures::{decode, limits, wall};
use crate::{
    api,
    forgejo::document::Body,
    github::{
        decode::Malformed,
        document::{Document, Kind, Row},
    },
};

const ISSUE: &str = r#"{"number":3,"state":"open","user":{"id":9},"updated_at":"2026-10-20T08:00:00Z","title":"widgets","labels":[{"name":"bug"},{"name":"help"}],"body":"description"}"#;
const PULL: &str = r#"{"number":5,"state":"open","user":{"id":10},"updated_at":"2026-10-20T08:00:00Z","title":"fix","pull_request":{"merged_at":"2026-10-20T08:00:00Z"}}"#;
const COMMENT: &str = r#"{"id":7,"user":{"id":9},"created_at":"2026-10-20T08:00:00Z","updated_at":"2026-10-20T08:00:00Z","body":"hello"}"#;
fn issue(row: &Row) -> &crate::github::document::Issue {
    match row {
        Row::Issue(issue) => issue,
        Row::Comment(_)
        | Row::Branch(_)
        | Row::Installation(_)
        | Row::Commit(_)
        | Row::File(_)
        | Row::Review(_)
        | Row::Remark(_)
        | Row::Collaborator(_)
        | Row::Reachable(_)
        | Row::Pull(_)
        | Row::Context(_)
        | Row::Run(_) => panic!("issue row"),
    }
}
fn comment(row: &Row) -> &crate::github::document::Comment {
    match row {
        Row::Comment(comment) => comment,
        Row::Issue(_)
        | Row::Branch(_)
        | Row::Installation(_)
        | Row::Commit(_)
        | Row::File(_)
        | Row::Review(_)
        | Row::Remark(_)
        | Row::Collaborator(_)
        | Row::Reachable(_)
        | Row::Pull(_)
        | Row::Context(_)
        | Row::Run(_) => panic!("comment row"),
    }
}
#[test]
fn github_items_rows_decode() {
    let json = alloc::fmt::format(format_args!("[{ISSUE},{PULL}]"));
    for chunk in [1, 7, u32::try_from(json.len()).unwrap()] {
        let (rows, result) = decode(Kind::IssueRows, json.as_bytes(), chunk, &limits());
        assert_eq!(result, Ok(Document::Rows));
        assert_eq!(rows.len(), 2);
        let first = issue(rows.get(0).unwrap());
        assert_eq!(
            (first.number, first.author, first.pull, first.merged, first.state),
            (3, 9, false, false, api::State::Open)
        );
        assert_eq!(first.labels.as_ref(), [Box::<[u8]>::from(*b"bug"), Box::from(*b"help")]);
        assert_eq!(first.updated_at, wall());
        assert_eq!(first.title.as_ref(), b"widgets");
        assert_eq!(first.body, Body::Kept(Box::from(*b"description")));
        let pull = issue(rows.get(1).unwrap());
        assert_eq!(
            (pull.number, pull.author, pull.pull, pull.merged, pull.state),
            (5, 10, true, true, api::State::Closed)
        );
        assert_eq!(pull.body, Body::Kept(Box::from([])));
        assert!(pull.labels.is_empty());
    }
}
#[test]
fn github_items_nulls_and_unknown_fields() {
    let unknown =
        alloc::fmt::format(format_args!(r#", "unknown":{{"array":[{{"long":"{}"}},[true,null,3]]}}"#, "x".repeat(200)));
    let first = ISSUE
        .replace(r#""body":"description""#, r#""body":null,"pull_request":null"#)
        .replace(r#""labels":[{"name":"bug"},{"name":"help"}]"#, r#""labels":null"#)
        .replace(r#""id":9"#, &alloc::fmt::format(format_args!(r#""id":9{unknown}"#)));
    let second = PULL.replace(
        r#""merged_at":"2026-10-20T08:00:00Z""#,
        &alloc::fmt::format(format_args!(r#""merged_at":null{unknown}"#)),
    );
    let third = ISSUE.replace(r#""name":"bug""#, &alloc::fmt::format(format_args!(r#""name":"bug"{unknown}"#)));
    let json = alloc::fmt::format(format_args!(
        "[{}, {}, {}]",
        first.replace(r#""number":3"#, &alloc::fmt::format(format_args!(r#""number":3{unknown}"#))),
        second,
        third
    ));
    let (rows, result) = decode(Kind::IssueRows, json.as_bytes(), 1, &limits());
    assert_eq!(result, Ok(Document::Rows));
    let first = issue(rows.get(0).unwrap());
    assert!(!first.pull);
    assert!(first.labels.is_empty());
    assert_eq!(first.body, Body::Kept(Box::from([])));
    let second = issue(rows.get(1).unwrap());
    assert!(second.pull);
    assert!(!second.merged);
    assert_eq!(second.state, api::State::Open);
    assert_eq!(issue(rows.get(2).unwrap()).labels.len(), 2);
}
#[test]
fn github_items_rows_malformed() {
    for (field, member) in [
        (b"number".as_slice(), r#""number":3,"#),
        (b"state", r#""state":"open","#),
        (b"user", r#""user":{"id":9},"#),
        (b"updated_at", r#""updated_at":"2026-10-20T08:00:00Z","#),
        (b"title", r#""title":"widgets","#),
    ] {
        let json = alloc::fmt::format(format_args!("[{}]", ISSUE.replace(member, "")));
        assert_eq!(decode(Kind::IssueRows, json.as_bytes(), 1, &limits()).1, Err(Malformed::Missing { field }));
    }
    for (json, expected) in [
        (ISSUE.replace(r#""id":9"#, ""), Malformed::Missing { field: b"id" }),
        (ISSUE.replace("open", "draft"), Malformed::Value { field: b"state" }),
        (ISSUE.replace(r#""number":3"#, r#""number":18446744073709551616"#), Malformed::Range { field: b"number" }),
        (ISSUE.replace(r#""number":3"#, r#""number":0"#), Malformed::Range { field: b"number" }),
        (ISSUE.replace("2026-10-20T08:00:00Z", "yesterday"), Malformed::Time { field: b"updated_at" }),
        (ISSUE.replace(r#""number":3"#, r#""number":3,"number":3"#), Malformed::Shape { field: b"number" }),
        (ISSUE.replace("widgets", &"x".repeat(129)), Malformed::TooLong { field: b"title" }),
        (
            ISSUE.replace(
                r#"[{"name":"bug"},{"name":"help"}]"#,
                r#"[{"name":"a"},{"name":"b"},{"name":"c"},{"name":"d"},{"name":"e"}]"#,
            ),
            Malformed::Bound { field: b"labels" },
        ),
        ("3".into(), Malformed::Shape { field: b"" }),
        (ISSUE.replace(r#""title":"widgets""#, r#""title":4"#), Malformed::Shape { field: b"title" }),
    ] {
        let json = alloc::fmt::format(format_args!("[{json}]"));
        assert_eq!(decode(Kind::IssueRows, json.as_bytes(), 7, &limits()).1, Err(expected), "{json}");
    }
    for (json, expected) in [
        (COMMENT.replace(r#""id":7"#, r#""id":0"#), Malformed::Range { field: b"id" }),
        (COMMENT.replace("2026-10-20T08:00:00Z", "bad"), Malformed::Time { field: b"created_at" }),
        (COMMENT.replace(r#""user":{"id":9}"#, r#""user":{}"#), Malformed::Missing { field: b"id" }),
    ] {
        assert_eq!(
            decode(Kind::CommentRows, alloc::fmt::format(format_args!("[{json}]")).as_bytes(), 1, &limits()).1,
            Err(expected)
        );
    }
}
#[test]
fn github_item_issue_decodes() {
    let (rows, _) = decode(Kind::IssueRows, alloc::fmt::format(format_args!("[{ISSUE}]")).as_bytes(), 7, &limits());
    for chunk in [1, 7] {
        let (none, result) = decode(Kind::Issue, ISSUE.as_bytes(), chunk, &limits());
        assert!(none.is_empty());
        assert_eq!(result, Ok(Document::Issue(issue(rows.get(0).unwrap()).clone())));
    }
}
#[test]
fn github_item_comment_rows_decode() {
    let json = alloc::fmt::format(format_args!(
        r#"[{COMMENT},{{"id":8,"user":{{"id":10,"login":"alice"}},"html_url":"ignored"}},{{"id":9,"user":{{"id":11}},"body":null,"created_at":null,"updated_at":null}}]"#
    ));
    for chunk in [1, 7] {
        let (rows, result) = decode(Kind::CommentRows, json.as_bytes(), chunk, &limits());
        assert_eq!(result, Ok(Document::Rows));
        assert_eq!(rows.len(), 3);
        let first = comment(rows.get(0).unwrap());
        assert_eq!((first.id, first.author, first.created_at, first.updated_at), (7, 9, Some(wall()), Some(wall())));
        assert_eq!(first.body, Body::Kept(Box::from(*b"hello")));
        for index in [1, 2] {
            let row = comment(rows.get(index).unwrap());
            assert_eq!(row.created_at, None);
            assert_eq!(row.updated_at, None);
            assert_eq!(row.body, Body::Kept(Box::from([])));
        }
    }
}
#[test]
fn github_item_long_body() {
    let mut limits = limits();
    limits.body_bytes = 16;
    for (length, expected) in [(16, Body::Kept(Box::from(&b"xxxxxxxxxxxxxxxx"[..]))), (17, Body::Long(17))] {
        for (kind, json) in [
            (Kind::IssueRows, ISSUE.replace("description", &"x".repeat(length))),
            (Kind::CommentRows, COMMENT.replace("hello", &"x".repeat(length))),
        ] {
            let (rows, result) = decode(kind, alloc::fmt::format(format_args!("[{json}]")).as_bytes(), 1, &limits);
            assert_eq!(result, Ok(Document::Rows));
            let body = match rows.get(0).unwrap() {
                Row::Issue(issue) => &issue.body,
                Row::Comment(comment) => &comment.body,
                Row::Branch(_)
                | Row::Installation(_)
                | Row::Commit(_)
                | Row::File(_)
                | Row::Review(_)
                | Row::Remark(_)
                | Row::Collaborator(_)
                | Row::Reachable(_)
                | Row::Pull(_)
                | Row::Context(_)
                | Row::Run(_) => panic!("body row"),
            };
            assert_eq!(body, &expected);
        }
    }
    assert_eq!(
        decode(Kind::Issue, ISSUE.replace("widgets", &"x".repeat(129)).as_bytes(), 1, &limits).1,
        Err(Malformed::TooLong { field: b"title" })
    );
    // Bodies can exceed the ordinary string bound while still being kept whole.
    limits.string_bytes = 32;
    limits.body_bytes = 128;
    let json = ISSUE.replace("description", &"x".repeat(64));
    let (_, result) = decode(Kind::Issue, json.as_bytes(), 1, &limits);
    match result {
        Ok(Document::Issue(issue)) => assert_eq!(issue.body, Body::Kept(Box::from(&[b'x'; 64][..]))),
        other => panic!("{other:?}"),
    }
}

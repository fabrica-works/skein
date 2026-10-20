//! Repository setup facts are recorded separately from organization hooks.
use crate::{
    Result,
    cases::{Auth, Recorder},
    facts::{Fact, add, case},
    json::{self, At},
    write::SecretKind,
};
use skein_throwaway_forgejo::{Token, fresh_secret};
use std::{collections::BTreeSet, path::Path};
const ROUTE: &str = "/api/v1/repos/acme/widgets/hooks";
const COLLABORATOR: &str = "/api/v1/repos/bob/widgets/collaborators/alice";
const EVENTS: &[&str] = &[
    "issues",
    "issue_assign",
    "issue_label",
    "issue_milestone",
    "pull_request",
    "pull_request_assign",
    "pull_request_label",
    "pull_request_milestone",
    "pull_request_sync",
    "pull_request_review_request",
    "issue_comment",
    "pull_request_comment",
    "pull_request_review_approved",
    "pull_request_review_rejected",
    "pull_request_review_comment",
    "push",
    "create",
    "delete",
    "workflow_job_success",
    "workflow_job_failure",
    "workflow_job_cancelled",
    "workflow_job_skipped",
];
fn body(port: u16, path: &str, secret: &str, active: bool, create: bool) -> Result<String> {
    Ok(String::from_utf8(
        json::write(|out| {
            out.object_start();
            if create {
                out.key(b"type");
                out.string(b"forgejo");
            }
            out.key(b"config");
            out.object_start();
            out.key(b"url");
            out.string(format!("http://127.0.0.1:{port}/{path}").as_bytes());
            out.key(b"content_type");
            out.string(b"json");
            out.key(b"secret");
            out.string(secret.as_bytes());
            out.object_end();
            out.key(b"events");
            json::Put::put(&EVENTS, out);
            out.key(b"active");
            out.boolean(active);
            out.object_end();
        })?
        .into_vec(),
    )?)
}
/// Provisions collaborator access and duplicate repository hooks for recorded evaluation.
pub(crate) fn record(r: &mut Recorder<'_>, admin: &Token) -> Result<()> {
    r.send("setup-collaborator-put", "PUT", COLLABORATOR, Some(r#"{"permission":"write"}"#), Some(admin), None)?;
    r.send("setup-collaborator-permission", "GET", &format!("{COLLABORATOR}/permission"), None, Some(admin), None)?;
    let secret = fresh_secret().map_err(|e| crate::failure(&e))?;
    let port = r.f.instance.port();
    let mut ids = Vec::new();
    for (name, path) in [("made", "hooks/1"), ("twin", "hooks/1"), ("other", "other")] {
        let body = body(port, path, &secret, true, true)?;
        let response = r.send_new(
            &format!("setup-repository-hook-{name}"),
            "POST",
            ROUTE,
            Some(&body),
            Auth::Token(admin),
            &[(SecretKind::HookSecret, &secret)],
        )?;
        ids.push(crate::json(&response)?.root().get("id").u64().ok_or("repository hook id absent")?);
    }
    for (name, query) in
        [("list", ""), ("page-1", "?page=1&limit=2"), ("page-2", "?page=2&limit=2"), ("past-end", "?page=3&limit=2")]
    {
        r.send(&format!("setup-repository-hook-{name}"), "GET", &format!("{ROUTE}{query}"), None, Some(admin), None)?;
    }
    let first = ids.first().ok_or("first repository hook id absent")?;
    let twin = ids.get(1).ok_or("second repository hook id absent")?;
    let replacement = fresh_secret().map_err(|e| crate::failure(&e))?;
    let patch = body(port, "hooks/1", &replacement, false, false)?;
    r.send_new(
        "setup-repository-hook-patched",
        "PATCH",
        &format!("{ROUTE}/{first}"),
        Some(&patch),
        Auth::Token(admin),
        &[(SecretKind::HookSecret, &replacement)],
    )?;
    r.send("setup-repository-hook-deleted", "DELETE", &format!("{ROUTE}/{twin}"), None, Some(admin), None)?;
    r.send("setup-repository-hook-final", "GET", ROUTE, None, Some(admin), None)?;
    Ok(())
}
fn event_set(value: At<'_>) -> Option<BTreeSet<&str>> {
    value.array().map(|rows| rows.iter().filter_map(|row| row.str()).collect())
}
/// Evaluates each repository setup contract using captured answers only.
#[expect(clippy::too_many_lines, reason = "the nine named facts share the same captured repository setup snapshots")]
pub(crate) fn facts(out: &Path) -> Result<Vec<Fact>> {
    let put = case(out, "setup-collaborator-put")?;
    let permission = case(out, "setup-collaborator-permission")?;
    let made = case(out, "setup-repository-hook-made")?;
    let twin = case(out, "setup-repository-hook-twin")?;
    let other = case(out, "setup-repository-hook-other")?;
    let list = case(out, "setup-repository-hook-list")?;
    let page1 = case(out, "setup-repository-hook-page-1")?;
    let page2 = case(out, "setup-repository-hook-page-2")?;
    let end = case(out, "setup-repository-hook-past-end")?;
    let patch = case(out, "setup-repository-hook-patched")?;
    let deleted = case(out, "setup-repository-hook-deleted")?;
    let final_list = case(out, "setup-repository-hook-final")?;
    let first_id = made.body.root().get("id").u64();
    let twin_id = twin.body.root().get("id").u64();
    let other_id = other.body.root().get("id").u64();
    let rows = list.body.root().array();
    let kept = final_list.body.root().array();
    let granted = permission.status == 200 && permission.body.root().get("permission").is("write");
    let mut hidden = true;
    for name in ["made", "twin", "other", "list", "page-1", "page-2", "past-end", "patched", "final"] {
        hidden &= !case(out, &format!("setup-repository-hook-{name}"))?.body.has_key("secret");
    }
    let mut facts = Vec::new();
    for (name, holds) in [
        ("setup-collaborator-put-by-an-admin-answers-204-and-grants-write", put.status == 204 && granted),
        ("setup-collaborator-permission-reads-back-write", granted),
        (
            "setup-repository-hook-for-a-loopback-address-is-made",
            made.status == 201
                && first_id.is_some()
                && made.body.root().get("config").get("url").is("http://127.0.0.1:3000/hooks/1"),
        ),
        (
            "setup-repository-hook-is-json-and-active",
            made.body.root().get("config").get("content_type").is("json")
                && made.body.root().get("active").bool() == Some(true),
        ),
        (
            "setup-repository-hook-lists-the-events-it-was-made-with",
            event_set(made.body.root().get("events")) == Some(EVENTS.iter().copied().collect())
                && rows.as_ref().is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.get("id").u64() == first_id
                            && event_set(row.get("events")) == event_set(made.body.root().get("events"))
                    })
                }),
        ),
        ("setup-repository-hook-never-shows-its-secret", hidden),
        (
            "setup-repository-hook-of-one-url-can-be-made-twice",
            made.status == 201
                && twin.status == 201
                && first_id.is_some()
                && twin_id.is_some()
                && first_id != twin_id
                && made.body.root().get("config").get("url").str() == twin.body.root().get("config").get("url").str()
                && rows.as_ref().is_some_and(|rows| {
                    rows.iter().any(|row| row.get("id").u64() == first_id)
                        && rows.iter().any(|row| row.get("id").u64() == twin_id)
                }),
        ),
        (
            "setup-repository-hook-patch-and-delete-leave-the-others",
            patch.status == 200
                && patch.body.root().get("active").bool() == Some(false)
                && deleted.status == 204
                && other_id.is_some()
                && kept.as_ref().is_some_and(|rows| {
                    rows.len() == 2
                        && rows
                            .iter()
                            .any(|row| row.get("id").u64() == first_id && row.get("active").bool() == Some(false))
                        && rows.iter().any(|row| {
                            row.get("id").u64() == other_id
                                && row.get("active").bool() == Some(true)
                                && row.get("config").get("url").str()
                                    == other.body.root().get("config").get("url").str()
                                && event_set(row.get("events")) == event_set(other.body.root().get("events"))
                        })
                        && rows.iter().all(|row| row.get("id").u64() != twin_id)
                }),
        ),
        (
            "setup-repository-hook-listing-pages-by-limit",
            [list.status, page1.status, page2.status, end.status].iter().all(|status| *status == 200)
                && rows.as_ref().is_some_and(|rows| {
                    let combined: Vec<_> = page1
                        .body
                        .root()
                        .items()
                        .iter()
                        .chain(page2.body.root().items().iter())
                        .map(|row| row.get("id").u64())
                        .collect();
                    rows.len() == 3
                        && page1.body.root().items().len() == 2
                        && page2.body.root().items().len() == 1
                        && rows.iter().map(|row| row.get("id").u64()).eq(combined)
                })
                && end.body.root().array().is_some_and(|rows| rows.is_empty()),
        ),
    ] {
        add(&mut facts, name, holds, "captured repository setup answer differs");
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::{EVENTS, facts, json};
    use crate::facts::Outcome;
    use std::path::Path;
    fn hook(id: u64, path: &str, active: bool) -> String {
        String::from_utf8(
            json::write(|out| {
                out.object_start();
                out.key(b"id");
                out.unsigned(id);
                out.key(b"config");
                out.object_start();
                out.key(b"url");
                out.string(format!("http://127.0.0.1:3000/{path}").as_bytes());
                out.key(b"content_type");
                out.string(b"json");
                out.object_end();
                out.key(b"events");
                json::Put::put(&EVENTS, out);
                out.key(b"active");
                out.boolean(active);
                out.object_end();
            })
            .expect("canned hook")
            .into_vec(),
        )
        .expect("JSON text")
    }
    fn canned(name: &str) -> std::path::PathBuf {
        let out = std::env::temp_dir().join(format!("skein-repo-setup-{}-{name}", std::process::id()));
        let first = hook(1, "hooks/1", true);
        let twin = hook(2, "hooks/1", true);
        let other = hook(3, "other", true);
        let patch = hook(1, "hooks/1", false);
        for (case, status, body) in [
            ("setup-collaborator-put", 204, String::new()),
            ("setup-collaborator-permission", 200, r#"{"permission":"write"}"#.into()),
            ("setup-repository-hook-made", 201, first.clone()),
            ("setup-repository-hook-twin", 201, twin.clone()),
            ("setup-repository-hook-other", 201, other.clone()),
            ("setup-repository-hook-list", 200, format!("[{first},{twin},{other}]")),
            ("setup-repository-hook-page-1", 200, format!("[{first},{twin}]")),
            ("setup-repository-hook-page-2", 200, format!("[{other}]")),
            ("setup-repository-hook-past-end", 200, "[]".into()),
            ("setup-repository-hook-patched", 200, patch.clone()),
            ("setup-repository-hook-deleted", 204, String::new()),
            ("setup-repository-hook-final", 200, format!("[{patch},{other}]")),
        ] {
            response(&out, case, status, &body);
        }
        out
    }
    fn response(out: &Path, name: &str, status: u16, body: &str) {
        let directory = out.join("transcripts").join(name);
        std::fs::create_dir_all(&directory).expect("canned directory");
        std::fs::write(directory.join("response"), format!("HTTP/1.1 {status} Canned\r\n\r\n{body}"))
            .expect("canned response");
    }
    #[test]
    fn repository_setup_facts_hold_for_canned_answers() {
        let out = canned("holds");
        let facts = facts(&out).expect("facts");
        assert_eq!(facts.len(), 9);
        for fact in facts {
            assert_eq!(fact.outcome, Outcome::Holds, "{}", fact.name);
        }
        std::fs::remove_dir_all(out).expect("remove canned answers");
    }
    #[test]
    fn repository_setup_facts_detect_changed_answers() {
        for (index, case, status, body, fact) in [
            (
                0,
                "setup-collaborator-put",
                403,
                "{}".into(),
                "setup-collaborator-put-by-an-admin-answers-204-and-grants-write",
            ),
            (
                1,
                "setup-collaborator-permission",
                200,
                r#"{"permission":"read"}"#.into(),
                "setup-collaborator-permission-reads-back-write",
            ),
            (
                2,
                "setup-repository-hook-made",
                400,
                hook(1, "hooks/1", true),
                "setup-repository-hook-for-a-loopback-address-is-made",
            ),
            (
                3,
                "setup-repository-hook-made",
                201,
                hook(1, "hooks/1", false),
                "setup-repository-hook-is-json-and-active",
            ),
            (
                4,
                "setup-repository-hook-made",
                201,
                hook(1, "hooks/1", true).replace("\"push\",", ""),
                "setup-repository-hook-lists-the-events-it-was-made-with",
            ),
            (
                5,
                "setup-repository-hook-list",
                200,
                r#"[{"secret":"example"}]"#.into(),
                "setup-repository-hook-never-shows-its-secret",
            ),
            (
                6,
                "setup-repository-hook-twin",
                201,
                hook(1, "hooks/1", true),
                "setup-repository-hook-of-one-url-can-be-made-twice",
            ),
            (
                7,
                "setup-repository-hook-final",
                200,
                "[]".into(),
                "setup-repository-hook-patch-and-delete-leave-the-others",
            ),
            (8, "setup-repository-hook-page-2", 200, "[]".into(), "setup-repository-hook-listing-pages-by-limit"),
        ] {
            let out = canned(&format!("changed-{index}"));
            response(&out, case, status, &body);
            let facts = facts(&out).expect("facts");
            assert!(facts.iter().any(|row| row.name == fact && matches!(row.outcome, Outcome::Fails(_))), "{fact}");
            std::fs::remove_dir_all(out).expect("remove canned answers");
        }
    }
}

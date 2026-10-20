use crate::json;
use crate::json::{At, Json};
use crate::{
    Result,
    cases::Recorder,
    facts::{Fact, add, case},
    fixture::Fixture,
};
use skein_throwaway_forgejo::Token;
use std::{collections::BTreeSet, path::Path, thread, time::Duration};
const ROUTE: &str = "/api/v1/orgs/ensemble/hooks";
const FIRST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const SECOND: &str = "2222222222222222222222222222222222222222222222222222222222222222";
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
fn body(port: u16, path: &str, secret: &str, events: &[&str], active: bool, create: bool) -> Result<String> {
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
            json::Put::put(&events, out);
            out.key(b"active");
            out.boolean(active);
            out.object_end();
        })?
        .into_vec(),
    )?)
}
pub(crate) fn record(r: &mut Recorder<'_>, admin: &Token) -> Result<()> {
    r.send("setup-hook-list-none", "GET", ROUTE, None, Some(admin), None)?;
    let port = r.f.instance.port();
    let make = body(port, "hooks/1", FIRST, EVENTS, true, true)?;
    let other = body(port, "other", FIRST, &["push"], true, true)?;
    let mut ids = Vec::new();
    for (name, body) in [("setup-hook-made", &make), ("setup-hook-made-twin", &make), ("setup-hook-made-other", &other)]
    {
        // Every captured creation timestamp first appears in its own second.
        thread::sleep(Duration::from_secs(1));
        let answer = r.send(name, "POST", ROUTE, Some(body), Some(admin), None)?;
        ids.push(crate::json(&answer)?.root().get("id").u64().ok_or("hook id absent")?);
    }
    for (name, query) in [
        ("setup-hook-list", ""),
        ("setup-hook-list-page-1", "?page=1&limit=1"),
        ("setup-hook-list-page-4", "?page=4&limit=1"),
    ] {
        r.send(name, "GET", &format!("{ROUTE}{query}"), None, Some(admin), None)?;
    }
    let first = format!("{ROUTE}/{}", ids[0]);
    for (name, events, active) in
        [("setup-hook-patched", EVENTS, true), ("setup-hook-patched-narrow", &["push"][..], false)]
    {
        let patch = body(port, "hooks/1", SECOND, events, active, false)?;
        r.send(name, "PATCH", &first, Some(&patch), Some(admin), None)?;
    }
    let patch = body(port, "hooks/1", SECOND, EVENTS, true, false)?;
    r.send("setup-hook-patch-missing", "PATCH", &format!("{ROUTE}/999999"), Some(&patch), Some(admin), None)?;
    r.send("setup-hook-deleted", "DELETE", &format!("{ROUTE}/{}", ids[1]), None, Some(admin), None)?;
    r.send("setup-hook-delete-missing", "DELETE", &format!("{ROUTE}/999999"), None, Some(admin), None)?;
    Ok(())
}
fn no_secret(value: &Json) -> bool {
    !value.has_key("secret")
}
pub(crate) fn facts(f: &Fixture, out: &Path) -> Result<Vec<Fact>> {
    let made = case(out, "setup-hook-made")?;
    let twin = case(out, "setup-hook-made-twin")?;
    let other = case(out, "setup-hook-made-other")?;
    let listed = case(out, "setup-hook-list")?;
    let patch = case(out, "setup-hook-patched-narrow")?;
    let response = f.exchange("GET", ROUTE, None, Some(&f.owner.token), None)?;
    let live = crate::json(&response)?;
    let live_text = String::from_utf8_lossy(&response.body);
    let live_hides_secret = no_secret(&live);
    let live = live.root().array().ok_or("hook list absent")?;
    let secrets_hidden = captures_hide_secrets(out)?;
    let mut facts = Vec::new();
    for (name, holds) in [
        ("setup-hook-for-a-loopback-address-is-made", made.status == 201),
        (
            "setup-hook-lists-the-events-it-was-made-with",
            event_set(made.body.root().get("events")) == Some(EVENTS.iter().copied().collect())
                && listed.body.root().array().is_some_and(|hooks| {
                    hooks.iter().any(|hook| {
                        hook.get("id").u64() == made.body.root().get("id").u64()
                            && event_set(hook.get("events")) == Some(EVENTS.iter().copied().collect())
                    })
                }),
        ),
        (
            "setup-hook-answers-its-target-in-config-url",
            made.body.root().get("config").get("url").is("http://127.0.0.1:3000/hooks/1"),
        ),
        (
            "setup-hook-is-json-and-active",
            made.body.root().get("config").get("content_type").is("json")
                && made.body.root().get("active").bool() == Some(true),
        ),
        (
            "setup-hook-never-shows-its-secret",
            secrets_hidden && live_hides_secret && !live_text.contains(FIRST) && !live_text.contains(SECOND),
        ),
        (
            "setup-hook-patch-narrows-events-but-keeps-the-workflow-job-ones",
            patch.status == 200
                && patch.body.root().get("events").items().iter().map(|event| event.str()).eq([
                    Some("push"),
                    Some("workflow_job_cancelled"),
                    Some("workflow_job_failure"),
                    Some("workflow_job_skipped"),
                    Some("workflow_job_success"),
                ])
                && patch.body.root().get("active").bool() == Some(false),
        ),
        (
            "setup-hook-patch-leaves-the-orgs-other-hook",
            live.iter().any(|hook| {
                hook.get("id").u64() == other.body.root().get("id").u64()
                    && hook.get("events").items().iter().map(|event| event.str()).eq(other
                        .body
                        .root()
                        .get("events")
                        .items()
                        .iter()
                        .map(|event| event.str()))
                    && hook.get("active").bool() == other.body.root().get("active").bool()
                    && hook.get("config").get("url").str().is_some_and(|url| url.ends_with("/other"))
            }),
        ),
        ("setup-hook-patch-of-a-missing-id-is-404", case(out, "setup-hook-patch-missing")?.status == 404),
        (
            "setup-hook-listing-pages-by-limit",
            case(out, "setup-hook-list-none")?.body.root().array().is_some_and(|a| a.is_empty())
                && case(out, "setup-hook-list-page-1")?.body.root().array().is_some_and(|hooks| hooks.len() == 1)
                && case(out, "setup-hook-list-page-4")?.body.root().array().is_some_and(|a| a.is_empty()),
        ),
        (
            "setup-hook-of-one-url-can-be-made-twice",
            twin.status == 201
                && twin.body.root().get("id").u64() != made.body.root().get("id").u64()
                && twin.body.root().get("config").get("url").str() == made.body.root().get("config").get("url").str()
                && listed.body.root().array().is_some_and(|hooks| {
                    hooks.len() == 3
                        && hooks.iter().any(|hook| hook.get("id").u64() == twin.body.root().get("id").u64())
                        && hooks.iter().any(|hook| hook.get("id").u64() == made.body.root().get("id").u64())
                }),
        ),
        (
            "setup-hook-delete-is-204-and-leaves-the-orgs-other-hooks",
            case(out, "setup-hook-deleted")?.status == 204
                && live.len() == 2
                && live.iter().any(|hook| hook.get("id").u64() == made.body.root().get("id").u64())
                && live.iter().any(|hook| hook.get("id").u64() == other.body.root().get("id").u64())
                && live.iter().all(|hook| hook.get("id").u64() != twin.body.root().get("id").u64()),
        ),
        ("setup-hook-delete-of-a-missing-id-is-404", case(out, "setup-hook-delete-missing")?.status == 404),
    ] {
        add(&mut facts, name, holds, "captured hook or final live state differs");
    }
    Ok(facts)
}

fn event_set(value: At<'_>) -> Option<BTreeSet<&str>> {
    value.array().map(|items| items.iter().filter_map(|item| item.str()).collect())
}

fn captures_hide_secrets(out: &Path) -> Result<bool> {
    [
        "list-none",
        "made",
        "made-twin",
        "made-other",
        "list",
        "list-page-1",
        "list-page-4",
        "patched",
        "patched-narrow",
        "patch-missing",
        "deleted",
        "delete-missing",
    ]
    .iter()
    .try_fold(true, |holds, name| -> Result<bool> {
        Ok(holds && no_secret(&case(out, &format!("setup-hook-{name}"))?.body))
    })
}

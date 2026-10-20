use crate::json;
use crate::json::At;
use crate::{
    Result,
    cases::Recorder,
    facts::{Fact, add, case},
    fixture::Fixture,
    setup,
};
use skein_throwaway_forgejo::Token;
use std::{path::Path, thread, time::Duration};
const ORG: &str = "/api/v1/orgs/ensemble";
const TEAM: &str = r#"{"name":"writer","permission":"write","includes_all_repositories":true,"units_map":{"repo.code":"write","repo.issues":"write","repo.pulls":"write","repo.actions":"write"}}"#;
const EDIT: &str = r#"{"name":"writer","permission":"write","includes_all_repositories":false,"units_map":{"repo.code":"write","repo.issues":"write","repo.pulls":"read","repo.actions":"none"}}"#;
pub(crate) fn record(r: &mut Recorder<'_>, admin: &Token) -> Result<()> {
    let body = r#"{"username":"ensemble","visibility":"private"}"#;
    thread::sleep(Duration::from_secs(1));
    r.send("setup-org-made", "POST", "/api/v1/admin/users/fixture/orgs", Some(body), Some(admin), None)?;
    r.send("setup-org-made-again", "POST", "/api/v1/admin/users/fixture/orgs", Some(body), Some(admin), None)?;
    r.send("setup-org-read", "GET", ORG, None, Some(admin), None)?;
    r.send("setup-org-missing", "GET", "/api/v1/orgs/missing", None, Some(admin), None)?;
    let only = setup::scoped(r.f, r.secrets, "fixture", "org-admin-only", &["write:admin"])?;
    for (name, route, org, token) in [
        ("setup-org-route-orgs-refused", "/api/v1/orgs", "probe-a", &only),
        ("setup-org-route-orgs-made", "/api/v1/orgs", "probe-b", admin),
        ("setup-org-made-by-write-admin-alone", "/api/v1/admin/users/fixture/orgs", "probe-c", &only),
    ] {
        thread::sleep(Duration::from_secs(1));
        r.send(
            name,
            "POST",
            route,
            Some(std::str::from_utf8(&json::object(&[("username", &org), ("visibility", &"private")])?)?),
            Some(token),
            None,
        )?;
    }
    let teams = format!("{ORG}/teams");
    r.send("setup-team-list-new-org", "GET", &teams, None, Some(admin), None)?;
    thread::sleep(Duration::from_secs(1));
    let made = r.send("setup-team-made", "POST", &teams, Some(TEAM), Some(admin), None)?;
    let id = crate::json(&made)?.root().get("id").u64().ok_or("team id absent")?;
    r.send("setup-team-made-again", "POST", &teams, Some(TEAM), Some(admin), None)?;
    for (name, suffix) in [
        ("setup-team-list", ""),
        ("setup-team-list-page-1", "?page=1&limit=1"),
        ("setup-team-list-page-3", "?page=3&limit=1"),
    ] {
        r.send(name, "GET", &format!("{teams}{suffix}"), None, Some(admin), None)?;
    }
    let team = format!("/api/v1/teams/{id}");
    r.send("setup-team-edit", "PATCH", &team, Some(EDIT), Some(admin), None)?;
    r.send("setup-team-edit-nameless", "PATCH", &team, Some(r#"{"permission":"write"}"#), Some(admin), None)?;
    r.send("setup-team-members-none", "GET", &format!("{team}/members"), None, Some(admin), None)?;
    r.send("setup-team-member-added", "PUT", &format!("{team}/members/writer"), None, Some(admin), None)?;
    r.send("setup-team-members", "GET", &format!("{team}/members"), None, Some(admin), None)?;
    r.send("setup-team-member-missing-user", "PUT", &format!("{team}/members/missing"), None, Some(admin), None)?;
    r.f.api("PATCH", &team, TEAM.as_bytes())?;
    r.f.api(
        "POST",
        &format!("{ORG}/repos"),
        &json::object(&[("auto_init", &true), ("name", &"stage"), ("private", &true)])?,
    )?;
    thread::sleep(Duration::from_secs(1));
    r.send(
        "setup-org-issue-by-member",
        "POST",
        "/api/v1/repos/ensemble/stage/issues",
        Some(r#"{"title":"member writes","body":"fixture"}"#),
        Some(&r.f.writer.token),
        None,
    )?;
    r.send(
        "setup-org-repo-for-a-stranger",
        "GET",
        "/api/v1/repos/ensemble/stage",
        None,
        Some(&r.f.stranger.token),
        None,
    )?;
    Ok(())
}
fn write_team(value: At<'_>) -> bool {
    value.get("permission").is("write")
        && value.get("includes_all_repositories").bool() == Some(true)
        && ["repo.code", "repo.issues", "repo.pulls", "repo.actions"]
            .iter()
            .all(|unit| value.get("units_map").get(unit).is("write"))
}
pub(crate) fn facts(_f: &Fixture, out: &Path) -> Result<Vec<Fact>> {
    let mut facts = Vec::new();
    let made = case(out, "setup-org-made")?;
    let team = case(out, "setup-team-made")?;
    let list = case(out, "setup-team-list")?;
    let edit = case(out, "setup-team-edit")?;
    let nameless = case(out, "setup-team-edit-nameless")?;
    let orgs_refused = case(out, "setup-org-route-orgs-refused")?;
    let orgs_made = case(out, "setup-org-route-orgs-made")?;
    let only = case(out, "setup-org-made-by-write-admin-alone")?;
    let initial = case(out, "setup-team-list-new-org")?;
    let members = case(out, "setup-team-members")?;
    for (name, holds) in [
        (
            "setup-org-made-for-a-user-by-an-admin-is-201-and-private",
            made.status == 201 && made.body.root().get("visibility").is("private"),
        ),
        ("setup-org-made-again-is-422", case(out, "setup-org-made-again")?.status == 422),
        (
            "setup-org-read-by-name-names-its-id",
            case(out, "setup-org-read")?.body.root().get("id").u64() == made.body.root().get("id").u64(),
        ),
        ("setup-org-missing-is-404", case(out, "setup-org-missing")?.status == 404),
        (
            "setup-org-route-orgs-needs-write-organization",
            orgs_refused.status == 403
                && orgs_refused.body.root().get("message").str().is_some_and(|s| s.contains("write:organization"))
                && orgs_made.status == 201
                && orgs_made.body.root().get("visibility").is("private"),
        ),
        (
            "setup-org-made-for-a-user-takes-write-admin-alone",
            only.status == 201 && only.body.root().get("visibility").is("private"),
        ),
        (
            "setup-org-starts-with-only-the-owners-team",
            initial.body.root().array().is_some_and(|a| a.len() == 1 && a[0].get("name").is("Owners")),
        ),
        (
            "setup-team-made-with-write-all-repositories-and-four-units",
            team.status == 201 && write_team(team.body.root()),
        ),
        (
            "setup-team-listing-reads-back-what-was-made",
            list.body.root().array().is_some_and(|a| {
                a.iter().any(|t| t.get("id").u64() == team.body.root().get("id").u64() && write_team(*t))
            }),
        ),
        ("setup-team-made-again-is-422", case(out, "setup-team-made-again")?.status == 422),
        (
            "setup-team-edit-without-a-name-keeps-the-name",
            nameless.status == 200 && nameless.body.root().get("name").is("writer"),
        ),
        (
            "setup-team-edit-replaces-units-and-flags",
            edit.status == 200
                && edit.body.root().get("includes_all_repositories").bool() == Some(false)
                && edit.body.root().get("units_map").get("repo.pulls").is("read")
                && edit.body.root().get("units_map").get("repo.code").is("write")
                && edit.body.root().get("units_map").get("repo.issues").is("write")
                && edit.body.root().get("units_map").get("repo.actions").is("none"),
        ),
        ("setup-team-member-put-is-204", case(out, "setup-team-member-added")?.status == 204),
        (
            "setup-team-members-list-the-added-user",
            members.body.root().array().is_some_and(|a| a.len() == 1 && a[0].get("login").is("writer")),
        ),
        ("setup-team-member-put-of-a-missing-user-is-404", case(out, "setup-team-member-missing-user")?.status == 404),
        ("setup-team-member-writes-in-a-repository-made-later", case(out, "setup-org-issue-by-member")?.status == 201),
        (
            "setup-team-non-member-cannot-see-the-private-repository",
            case(out, "setup-org-repo-for-a-stranger")?.status == 404,
        ),
        (
            "setup-team-listing-pages-by-limit",
            case(out, "setup-team-list-page-1")?.body.root().array().is_some_and(|a| a.len() == 1)
                && case(out, "setup-team-list-page-3")?.body.root().array().is_some_and(|a| a.is_empty()),
        ),
    ] {
        add(&mut facts, name, holds, "captured status or members differ");
    }
    Ok(facts)
}

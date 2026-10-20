use crate::json;
use crate::{
    Result,
    cases::{Auth, Recorder},
    facts::{Fact, add, case},
    fixture::R,
    setup,
};
use skein_throwaway_forgejo::Token;
use std::path::Path;
const ROUTE: &str = "/api/v1/admin/users/bot/tokens";
pub(crate) const API: &[&str] = &["read:user", "write:issue", "write:repository"];
const STORED_API: &[&str] = &["write:issue", "write:repository", "read:user"];
const GIT: &[&str] = &["write:repository", "read:user"];
fn make(r: &mut Recorder<'_>, admin: &Token, case: &str, name: &str, scopes: &[&str]) -> Result<Token> {
    // Forgejo timestamps token creation to the second. Keep each captured token
    // apart from earlier user timestamps and the other captured token.
    std::thread::sleep(std::time::Duration::from_secs(1));
    let response = r.send_new(
        case,
        "POST",
        ROUTE,
        Some(std::str::from_utf8(&json::object(&[("name", &name), ("scopes", &scopes)])?)?),
        Auth::Token(admin),
        &[],
    )?;
    let body = crate::json(&response)?;
    Ok(Token::new(body.root().get("sha1").str().ok_or("made token missing")?.to_owned()))
}
pub(crate) fn record(r: &mut Recorder<'_>, admin: &Token) -> Result<()> {
    r.f.api("PUT", &format!("{R}/collaborators/bot"), &json::object(&[("permission", &"write")])?)?;
    let api = make(r, admin, "setup-token-make", "kit-api", API)?;
    let git = make(r, admin, "setup-token-make-git", "kit-git", GIT)?;
    r.send_new(
        "setup-token-make-again",
        "POST",
        ROUTE,
        Some(std::str::from_utf8(&json::object(&[("name", &"kit-api"), ("scopes", &API)])?)?),
        Auth::Token(admin),
        &[],
    )?;
    r.send_new("setup-token-list", "GET", &format!("{ROUTE}?page=1&limit=1"), None, Auth::Token(admin), &[])?;
    let listed = r.send_new(
        "setup-token-list-page-2",
        "GET",
        &format!("{ROUTE}?page=2&limit=1"),
        None,
        Auth::Token(admin),
        &[],
    )?;
    let id = crate::json(&listed)?.root().nth(0).get("id").u64().ok_or("listed token id missing")?;
    r.send_new("setup-token-as-the-bot", "GET", "/api/v1/user", None, Auth::Token(&api), &[])?;
    r.send_new("setup-token-git-as-the-bot", "GET", "/api/v1/user", None, Auth::Token(&git), &[])?;
    let alone = setup::scoped(r.f, r.secrets, "bot", "git-alone", &["write:repository"])?;
    r.send_new("setup-token-git-alone", "GET", "/api/v1/user", None, Auth::Token(&alone), &[])?;
    r.send_new("setup-token-delete", "DELETE", &format!("{ROUTE}/{id}"), None, Auth::Token(admin), &[])?;
    r.send_new("setup-token-deleted-use", "GET", "/api/v1/user", None, Auth::Token(&api), &[])?;
    r.send_new("setup-token-delete-missing", "DELETE", &format!("{ROUTE}/999999"), None, Auth::Token(admin), &[])?;
    let no_issue = setup::scoped(r.f, r.secrets, "bot", "no-issue", &["read:user", "write:repository"])?;
    r.send_new(
        "setup-token-narrow-write",
        "POST",
        &format!("{R}/issues"),
        Some("{\"title\":\"scope probe\",\"body\":\"scope probe\"}"),
        Auth::Token(&no_issue),
        &[],
    )?;
    let user = setup::scoped(r.f, r.secrets, "bot", "user-only", &["read:user"])?;
    let discovery = "/fixture/specimen.git/info/refs?service=git-receive-pack";
    r.send_new("setup-token-git-discovery", "GET", discovery, None, Auth::Basic("bot", git.value()), &[])?;
    r.send_new("setup-token-git-discovery-refused", "GET", discovery, None, Auth::Basic("bot", user.value()), &[])?;
    for (name, scopes, target) in [
        ("no-admin", ["write:organization", "write:user"].as_slice(), ROUTE),
        ("no-user", &["write:admin", "write:organization"], "/api/v1/user"),
        ("read-user", &["write:admin", "write:organization", "read:user"], "/api/v1/user"),
    ] {
        let token = setup::scoped(r.f, r.secrets, "fixture", name, scopes)?;
        r.send_new(&format!("setup-scope-{name}"), "GET", target, None, Auth::Token(&token), &[])?;
    }
    Ok(())
}
pub(crate) fn facts(out: &Path) -> Result<Vec<Fact>> {
    let made = case(out, "setup-token-make")?;
    let list = case(out, "setup-token-list")?;
    let second = case(out, "setup-token-list-page-2")?;
    let mut facts = Vec::new();
    for (name, holds) in [
        (
            "setup-admin-route-makes-a-token-answering-201-with-name-scopes-and-value",
            made.status == 201
                && made.body.root().get("name").is("kit-api")
                && made
                    .body
                    .root()
                    .get("scopes")
                    .items()
                    .iter()
                    .map(|scope| scope.str())
                    .eq(STORED_API.iter().copied().map(Some))
                && made.body.root().get("sha1").str().is_some_and(|s| s.len() == 40),
        ),
        ("setup-token-of-an-existing-name-is-refused", case(out, "setup-token-make-again")?.status == 400),
        (
            "setup-token-list-shows-names-and-scopes-without-values",
            list.status == 200
                && list.body.root().nth(0).get("name").is("kit-git")
                && list
                    .body
                    .root()
                    .nth(0)
                    .get("scopes")
                    .items()
                    .iter()
                    .map(|scope| scope.str())
                    .eq(GIT.iter().copied().map(Some))
                && second.body.root().nth(0).get("name").is("kit-api")
                && second
                    .body
                    .root()
                    .nth(0)
                    .get("scopes")
                    .items()
                    .iter()
                    .map(|scope| scope.str())
                    .eq(STORED_API.iter().copied().map(Some))
                && [&list, &second].iter().all(|c| c.body.root().nth(0).get("sha1").str().is_none_or(str::is_empty)),
        ),
        (
            "setup-token-list-pages-by-page-and-limit",
            list.body.root().array().is_some_and(|v| v.len() == 1)
                && second.status == 200
                && second.body.root().array().is_some_and(|v| v.len() == 1)
                && list.body.root().nth(0).get("id").u64() != second.body.root().nth(0).get("id").u64(),
        ),
        ("setup-a-made-token-answers-user-as-the-bot", bot(out, "setup-token-as-the-bot")?),
        (
            "setup-a-git-token-of-write-repository-alone-is-refused-user",
            case(out, "setup-token-git-alone")?.status == 403,
        ),
        (
            "setup-a-git-token-of-write-repository-and-read-user-answers-user-as-the-bot",
            bot(out, "setup-token-git-as-the-bot")?,
        ),
        (
            "setup-token-delete-answers-204-and-ends-the-token",
            case(out, "setup-token-delete")?.status == 204 && case(out, "setup-token-deleted-use")?.status == 401,
        ),
        ("setup-token-delete-of-a-missing-id-is-404", case(out, "setup-token-delete-missing")?.status == 404),
        (
            "setup-admin-token-without-write-admin-is-refused-the-token-routes",
            case(out, "setup-scope-no-admin")?.status == 403,
        ),
        ("setup-admin-token-without-a-user-scope-is-refused-user", case(out, "setup-scope-no-user")?.status == 403),
        (
            "setup-admin-token-with-read-user-in-place-of-write-user-reads-user",
            case(out, "setup-scope-read-user")?.status == 200,
        ),
        (
            "setup-api-token-without-write-issue-is-refused-an-issue-write",
            case(out, "setup-token-narrow-write")?.status == 403,
        ),
        (
            "setup-git-token-with-write-repository-opens-the-push-discovery",
            case(out, "setup-token-git-discovery")?.status == 200,
        ),
        ("setup-git-token-without-it-is-refused", case(out, "setup-token-git-discovery-refused")?.status == 403),
    ] {
        add(&mut facts, name, holds, "captured status or members differ");
    }
    Ok(facts)
}
fn bot(out: &Path, name: &str) -> Result<bool> {
    let row = case(out, name)?;
    Ok(row.status == 200 && row.body.root().get("login").is("bot"))
}

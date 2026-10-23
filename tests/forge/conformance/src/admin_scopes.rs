//! Setup route scopes, measured with site-admin tokens on disposable Forgejo.
use crate::{
    Result,
    cases::{Auth, Recorder},
    facts::{Fact, Outcome, add, case},
    fixture::Fixture,
    mask::Mask,
    setup,
    write::{SecretKind, Secrets},
};
use skein_throwaway_forgejo::{Forgejo, Settings};
use std::path::Path;
const HOOK: &str = r#"{"type":"forgejo","config":{"url":"http://127.0.0.1:7200/hooks/1","content_type":"json","secret":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"events":["push"],"active":true}"#;
const NAMES: &[(&str, &str, u16)] = &[
    ("user", "read:user", 200),
    ("user-of", "read:user", 200),
    ("create-user", "write:admin", 201),
    ("tokens", "read:admin", 200),
    ("create-token", "write:admin", 201),
    ("delete-token", "write:admin", 204),
    ("org", "read:organization", 200),
    ("create-org", "write:admin", 201),
    ("create-repository", "write:organization", 201),
    ("create-file", "write:repository", 201),
    ("repository", "read:repository", 200),
    ("permission", "read:repository", 200),
    ("add-collaborator", "write:repository", 204),
    ("repo-hooks", "read:repository", 200),
    ("create-repo-hook", "write:repository", 201),
    ("edit-repo-hook", "write:repository", 200),
    ("delete-repo-hook", "write:repository", 204),
    ("repo-search", "read:repository", 200),
    ("settings", "", 200),
];
const FACTS: &[&str] = &[
    "setup-scope-user-is-read-user",
    "setup-scope-user-of-is-read-user",
    "setup-scope-create-user-is-write-admin",
    "setup-scope-tokens-is-read-admin",
    "setup-scope-create-token-is-write-admin",
    "setup-scope-delete-token-is-write-admin",
    "setup-scope-org-is-read-organization",
    "setup-scope-create-org-is-write-admin",
    "setup-scope-create-repository-is-write-organization",
    "setup-scope-create-file-is-write-repository",
    "setup-scope-repository-is-read-repository",
    "setup-scope-permission-is-read-repository",
    "setup-scope-add-collaborator-is-write-repository",
    "setup-scope-repo-hooks-is-read-repository",
    "setup-scope-create-repo-hook-is-write-repository",
    "setup-scope-edit-repo-hook-is-write-repository",
    "setup-scope-delete-repo-hook-is-write-repository",
    "setup-scope-repo-search-is-read-repository",
    "setup-scope-settings-needs-no-scope",
];
type Route = (&'static str, &'static str, String, Option<String>);
fn routes(repository: u64, token: u64, hook: u64, suffix: &str) -> Vec<Route> {
    vec![
        ("user", "GET", "/api/v1/user".into(), None),
        ("user-of", "GET", "/api/v1/users/writer".into(), None),
        (
            "create-user",
            "POST",
            "/api/v1/admin/users".into(),
            Some(format!(
                r#"{{"username":"scope-user-{suffix}","email":"scope-{suffix}@example.invalid","full_name":"scope:bot","password":"1111111111111111111111111111111111111111111111111111111111111111","must_change_password":false}}"#
            )),
        ),
        ("tokens", "GET", "/api/v1/admin/users/writer/tokens".into(), None),
        (
            "create-token",
            "POST",
            "/api/v1/admin/users/writer/tokens".into(),
            Some(format!(r#"{{"name":"made-{suffix}","scopes":["read:user"]}}"#)),
        ),
        ("delete-token", "DELETE", format!("/api/v1/admin/users/writer/tokens/{token}"), None),
        ("org", "GET", "/api/v1/orgs/scope-org".into(), None),
        (
            "create-org",
            "POST",
            "/api/v1/admin/users/fixture/orgs".into(),
            Some(format!(r#"{{"username":"scope-org-{suffix}","visibility":"private"}}"#)),
        ),
        (
            "create-repository",
            "POST",
            "/api/v1/orgs/scope-org/repos".into(),
            Some(format!(
                r#"{{"name":"scope-repo-{suffix}","private":true,"auto_init":true,"default_branch":"main"}}"#
            )),
        ),
        ("create-file", "POST", format!("/api/v1/repos/fixture/scopes/contents/scope-{suffix}.txt"), Some(r#"{"content":"c2NvcGVzCg==","branch":"main","message":"Add scope file","author":{"name":"kit","email":"kit@example.invalid"},"committer":{"name":"kit","email":"kit@example.invalid"}}"#.into())),
        ("repository", "GET", format!("/api/v1/repositories/{repository}"), None),
        ("permission", "GET", "/api/v1/repos/fixture/scopes/collaborators/writer/permission".into(), None),
        (
            "add-collaborator",
            "PUT",
            "/api/v1/repos/fixture/scopes/collaborators/writer".into(),
            Some(r#"{"permission":"write"}"#.into()),
        ),
        ("repo-hooks", "GET", "/api/v1/repos/fixture/scopes/hooks".into(), None),
        ("create-repo-hook", "POST", "/api/v1/repos/fixture/scopes/hooks".into(), Some(HOOK.into())),
        ("edit-repo-hook", "PATCH", format!("/api/v1/repos/fixture/scopes/hooks/{hook}"), Some(HOOK.into())),
        ("delete-repo-hook", "DELETE", format!("/api/v1/repos/fixture/scopes/hooks/{hook}"), None),
        ("repo-search", "GET", "/api/v1/repos/search?page=1&limit=1".into(), None),
        ("settings", "GET", "/api/v1/settings/api".into(), None),
    ]
}
pub(crate) fn record(r: &mut Recorder<'_>) -> Result<()> {
    let repo =
        r.f.api("POST", "/api/v1/user/repos", br#"{"name":"scopes","auto_init":true,"default_branch":"main"}"#)?;
    let repository = repo.root().get("id").u64().ok_or("repository id missing")?;
    r.f.api("POST", "/api/v1/admin/users/fixture/orgs", br#"{"username":"scope-org","visibility":"private"}"#)?;
    // Forgejo refuses empty scope lists at minting; read:issue grants none of these routes.
    r.secrets.add(SecretKind::Password, "1111111111111111111111111111111111111111111111111111111111111111");
    r.secrets.add(SecretKind::HookSecret, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let writer = r.f.writer.token.value();
    let tail = writer.get(writer.len().saturating_sub(8)..).ok_or("writer token tail missing")?;
    r.secrets.add(SecretKind::ApiToken, tail);
    r.mask.literal(tail);
    for mode in ["least", "without", "admin"] {
        let deletion = r.f.api(
            "POST",
            "/api/v1/admin/users/writer/tokens",
            format!(r#"{{"name":"delete-{mode}","scopes":["read:user"]}}"#).as_bytes(),
        )?;
        r.secrets.add(SecretKind::ApiToken, deletion.root().get("sha1").str().ok_or("token missing")?);
        let tail = deletion.root().get("token_last_eight").str().ok_or("token tail missing")?;
        r.secrets.add(SecretKind::ApiToken, tail);
        r.mask.literal(tail);
        let id = deletion.root().get("id").u64().ok_or("token id missing")?;
        let hook = r.f.api("POST", "/api/v1/repos/fixture/scopes/hooks", HOOK.as_bytes())?;
        let hook_id = hook.root().get("id").u64().ok_or("hook id missing")?;
        for ((name, method, target, body), (expected, scope, _)) in
            routes(repository, id, hook_id, mode).into_iter().zip(NAMES)
        {
            assert_eq!(name, *expected);
            let scopes = if mode == "admin" {
                setup::ADMIN.to_vec()
            } else if mode == "least" && !scope.is_empty() {
                vec![*scope]
            } else {
                vec!["read:issue"]
            };
            let token = setup::scoped(r.f, r.secrets, "fixture", &format!("scope-{name}-{mode}"), &scopes)?;
            r.send_new(
                &format!("admin-scope-{name}-{mode}"),
                method,
                &target,
                body.as_deref(),
                Auth::Token(&token),
                &[],
            )?;
        }
    }
    for (name, omitted) in [
        ("user", "read:user"),
        ("create-user", "write:admin"),
        ("create-repository", "write:organization"),
        ("add-collaborator", "write:repository"),
    ] {
        let scopes: Vec<_> = setup::ADMIN.iter().copied().filter(|scope| *scope != omitted).collect();
        let token = setup::scoped(r.f, r.secrets, "fixture", &format!("scope-omit-{name}"), &scopes)?;
        let (_, method, target, body) = routes(repository, 0, 0, "omitted")
            .into_iter()
            .find(|route| route.0 == name)
            .ok_or("scope witness missing")?;
        r.send_new(&format!("admin-scope-omit-{name}"), method, &target, body.as_deref(), Auth::Token(&token), &[])?;
    }
    r.send("admin-scope-settings-anonymous", "GET", "/api/v1/settings/api", None, None, None)?;
    Ok(())
}
pub(crate) fn facts(out: &Path) -> Result<Vec<Fact>> {
    let mut facts = Vec::new();
    for ((name, scope, status), fact) in NAMES.iter().zip(FACTS) {
        let least = case(out, &format!("admin-scope-{name}-least"))?;
        let without = case(out, &format!("admin-scope-{name}-without"))?;
        let admin = case(out, &format!("admin-scope-{name}-admin"))?;
        let denied = if scope.is_empty() {
            without.status == 200 && case(out, "admin-scope-settings-anonymous")?.status == 200
        } else {
            without.status == 403
                && without
                    .body
                    .root()
                    .get("message")
                    .is(&format!("token does not have at least one of required scope(s): [{scope}]"))
        };
        add(
            &mut facts,
            fact,
            least.status == *status && admin.status == *status && denied,
            &format!("least={}, without={}", least.status, without.status),
        );
    }
    let mut omissions = true;
    for name in ["user", "create-user", "create-repository", "add-collaborator"] {
        omissions &= case(out, &format!("admin-scope-omit-{name}"))?.status == 403;
    }
    add(
        &mut facts,
        "setup-admin-scopes-need-each-of-the-four-categories",
        omissions,
        "a scope's omission was admitted",
    );
    Ok(facts)
}
/// Records the least scope and its removal for every setup request.
/// # Errors
/// Returns a provisioning, recording, exchange or conformance failure.
pub fn run(binary: &Path, scratch: &Path, out: &Path) -> Result<()> {
    let forgejo = Forgejo::from_path(binary).map_err(|e| format!("{e:?}"))?;
    let f = Fixture::bare(&forgejo, scratch, &Settings::small())?;
    let mut mask = Mask::new(f.instance.port());
    let mut secrets = Secrets::from_instance(&f.instance);
    let mut r = Recorder { f: &f, out, mask: &mut mask, secrets: &mut secrets };
    record(&mut r)?;
    let facts = facts(out)?;
    crate::facts::write(out, &facts)?;
    f.instance.stop().map_err(|e| crate::failure(&e))?;
    if facts.iter().any(|fact| !matches!(fact.outcome, Outcome::Holds)) {
        return Err("admin scope conformance failed; see facts.txt".into());
    }
    println!("admin scopes: {} facts hold", facts.len());
    Ok(())
}

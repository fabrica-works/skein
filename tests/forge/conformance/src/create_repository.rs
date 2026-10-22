//! Repository creation on an isolated Forgejo, with the setup recorder's masking.
use crate::{
    Result,
    cases::Recorder,
    facts::{Fact, Outcome, add, case},
    fixture::Fixture,
    mask::Mask,
    write::Secrets,
};
use skein_throwaway_forgejo::{Forgejo, Settings};
use std::path::Path;
const ROUTE: &str = "/api/v1/orgs/creation/repos";
const PRIVATE: &str = r#"{"name":"starter","private":true,"auto_init":true,"default_branch":"main"}"#;
const PUBLIC: &str = r#"{"name":"public","private":false,"auto_init":true,"default_branch":"main"}"#;

pub(crate) fn record(r: &mut Recorder<'_>) -> Result<()> {
    r.f.api("POST", "/api/v1/admin/users/fixture/orgs", br#"{"username":"creation","visibility":"public"}"#)?;
    for (case, target, body, token) in [
        ("setup-repository-created", ROUTE, PRIVATE, Some(&r.f.owner.token)),
        ("setup-repository-created-public", ROUTE, PUBLIC, Some(&r.f.owner.token)),
        ("setup-repository-created-again", ROUTE, PRIVATE, Some(&r.f.owner.token)),
        (
            "setup-repository-invalid-name",
            ROUTE,
            r#"{"name":"bad name","private":true,"auto_init":true,"default_branch":"main"}"#,
            Some(&r.f.owner.token),
        ),
        ("setup-repository-missing-org", "/api/v1/orgs/missing/repos", PRIVATE, Some(&r.f.owner.token)),
        ("setup-repository-unauthorized", ROUTE, PRIVATE, None),
        ("setup-repository-forbidden", ROUTE, PRIVATE, Some(&r.f.stranger.token)),
    ] {
        r.send(case, "POST", target, Some(body), token, None)?;
    }
    for (case, target) in [
        ("setup-repository-created-read", "/api/v1/repos/creation/starter"),
        ("setup-repository-created-readme", "/api/v1/repos/creation/starter/raw/README.md?ref=main"),
    ] {
        r.send(case, "GET", target, None, Some(&r.f.owner.token), None)?;
    }
    Ok(())
}
pub(crate) fn facts(out: &Path) -> Result<Vec<Fact>> {
    let mut facts = Vec::new();
    let made = case(out, "setup-repository-created")?;
    let public = case(out, "setup-repository-created-public")?;
    let read = case(out, "setup-repository-created-read")?;
    let row = made.body.root();
    add(
        &mut facts,
        "setup-repository-creation-returns-adoption-and-clone-fields",
        made.status == 201
            && row.get("id").u64().is_some_and(|id| id > 0)
            && row.get("id").u64() == read.body.root().get("id").u64()
            && row.get("full_name").is("creation/starter")
            && row.get("default_branch").is("main")
            && row.get("clone_url").is("http://127.0.0.1:3000/creation/starter.git")
            && row.get("ssh_url").str().is_some_and(|url| url.ends_with("/creation/starter.git"))
            && row.get("ssh_url").str() == read.body.root().get("ssh_url").str(),
        "created identity, main branch or clone URLs differ",
    );
    add(
        &mut facts,
        "setup-repository-creation-honors-both-visibilities",
        row.get("private").bool() == Some(true)
            && public.status == 201
            && public.body.root().get("private").bool() == Some(false),
        "private or public creation differs",
    );
    // README is text, so inspect the raw transcript rather than the JSON fact reader.
    let readme = std::fs::read(out.join("transcripts/setup-repository-created-readme/response"))?;
    add(
        &mut facts,
        "setup-repository-creation-auto-initializes-main-with-a-readme",
        read.status == 200
            && read.body.root().get("empty").bool() == Some(false)
            && readme.starts_with(b"HTTP/1.1 200 ")
            && readme.windows(b"starter".len()).any(|part| part == b"starter"),
        "main has no README",
    );
    for (case_name, status) in [
        ("setup-repository-created-again", 409),
        ("setup-repository-invalid-name", 422),
        ("setup-repository-missing-org", 404),
        ("setup-repository-unauthorized", 401),
        ("setup-repository-forbidden", 403),
    ] {
        add(&mut facts, case_name, case(out, case_name)?.status == status, "refusal status differs");
    }
    Ok(facts)
}
/// Records just repository creation against the pinned, disposable Forgejo.
///
/// # Errors
/// Returns an invalid binary, setup, transcript, or conformance failure.
pub fn run(binary: &Path, scratch: &Path, out: &Path) -> Result<()> {
    let forgejo = Forgejo::from_path(binary).map_err(|e| format!("{e:?}"))?;
    let fixture = Fixture::bare(&forgejo, scratch, &Settings::small())?;
    let mut mask = Mask::new(fixture.instance.port());
    let mut secrets = Secrets::from_instance(&fixture.instance);
    let mut recorder = Recorder { f: &fixture, out, secrets: &mut secrets, mask: &mut mask };
    record(&mut recorder)?;
    let facts = facts(out)?;
    crate::facts::write(out, &facts)?;
    fixture.instance.stop().map_err(|e| crate::failure(&e))?;
    if facts.iter().any(|fact| !matches!(fact.outcome, Outcome::Holds)) {
        return Err("repository creation conformance failed; see facts.txt".into());
    }
    println!("repository creation: {} facts hold", facts.len());
    Ok(())
}

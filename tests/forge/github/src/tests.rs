use super::*;
use crate::{
    cases,
    fake::World,
    journal::{Journal, Made},
    live::Fixtures,
    runner::{Runner, Target},
};
use skein_forge::{
    api,
    github::{document as github_document, request::Method},
};
use skein_forge_conformance::{
    facts::Outcome,
    write::{self, SecretKind, Secrets},
};
use std::{
    collections::BTreeSet,
    fs,
    io::Write as _,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};
struct Temp(PathBuf);
impl Temp {
    fn new(name: &str) -> Self {
        let p = PathBuf::from("/tmp").join(format!("github-conformance-{name}-{}", std::process::id()));
        if p.exists() {
            fs::remove_dir_all(&p).expect("remove stale invented test output");
        }
        fs::create_dir(&p).expect("test directory");
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        fs::remove_dir_all(&self.0).expect("remove invented test output");
    }
}
fn runner(out: &Path) -> Runner<'_> {
    let w = World::new().expect("hosted fixture");
    let fixtures = World::fixtures().expect("invented fixtures");
    Runner::new(Target::Fake(w), fixtures, out).expect("runner")
}
fn shutdown(r: &mut Runner<'_>) {
    if let Target::Fake(w) = &mut r.target {
        w.shutdown().expect("quiescent simulator");
    }
}
fn scan(dir: &Path, secrets: &Secrets) {
    for entry in fs::read_dir(dir).expect("output directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            scan(&path, secrets);
        } else {
            let bytes = fs::read(path).expect("output file");
            assert!(write::check("all output", &bytes, b"", secrets).is_ok(), "output contains a registered secret");
        }
    }
}
#[test]
fn github_conformance_holds_every_fact_on_the_fake() {
    let out = Temp::new("whole");
    let report = run_fake(&out.0).expect("whole hosted fake run");
    assert_eq!(report.facts.len(), cases::NAMES.len());
    for fact in report.facts {
        match fact.outcome {
            Outcome::Holds => {}
            Outcome::Unchecked(_) => assert_eq!(fact.name, "signatures-push-gh013"),
            Outcome::Observed(reason) | Outcome::Fails(reason) => panic!("{}: {reason}", fact.name),
        }
    }
    assert!(report.requests <= 400);
    assert!(report.leftover_mints.is_empty());
}
#[test]
fn github_conformance_names_every_fact_of_forge_github_13() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/design/forge-github.md");
    let doc = fs::read_to_string(root).expect("design contract");
    let section = doc.split("## 13.").nth(1).expect("section 13").split("## 14.").next().expect("section end");
    let mut names: BTreeSet<_> = section.split('`').enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, s)| s).collect();
    names.remove("unchecked");
    names.retain(|s| !s.contains('/'));
    names.insert("installation-repository-removal-refusals");
    assert_eq!(cases::NAMES.iter().copied().collect::<BTreeSet<_>>(), names);
    assert_eq!(cases::NAMES.len(), names.len());
}
#[test]
fn github_conformance_masks_secrets_and_ids() {
    let mut m = mask::Mask::default();
    let head = m.headers(b"GET /repositories/987654321/pulls/735 HTTP/1.1\r\nAuthorization: Bearer invented-secret\r\nETag: W/\"invented-tag\"\r\nX-GitHub-Request-Id: invented-request\r\nLocation: https://invented.invalid/log?signature=invented-signature\r\n");
    for value in ["invented-secret", "invented-tag", "invented-request", "invented-signature", "987654321", "/735"] {
        assert!(!head.windows(value.len()).any(|w| w == value.as_bytes()), "header mask missed a category");
    }
    let body = m.body(br#"{"id":987654321,"number":735,"node_id":"PR_invented","guid":"aabbccdd-1234-5678-9012-0123456789ab","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","token":"invented-minted","pem":"invented-key","webhook_secret":"invented-hook","expires_at":"2026-10-20T20:00:00Z"}"#, true);
    let text = std::str::from_utf8(&body).expect("masked JSON text");
    assert!(text.contains("\"id\":1,\"number\":2"));
    for value in
        ["PR_invented", "aabbccdd", "aaaaaaaa", "invented-minted", "invented-key", "invented-hook", "2026-10-20"]
    {
        assert!(!text.contains(value), "body mask missed a category");
    }
    assert_eq!(m.body(br#"{"id":735,"repository_id":987654321}"#, false), br#"{"id":2,"repository_id":1}"#);
    let query = m.body(
        br#"{"query":"mutation { enqueuePullRequest(input:{pullRequestId:\"PR_invented\"}) { clientMutationId } }"}"#,
        false,
    );
    assert!(!std::str::from_utf8(&query).expect("query").contains("PR_invented"));
    let date = m.headers(b"Date: Tue, 20 Oct 2026 20:00:00 GMT\r\n");
    assert!(std::str::from_utf8(&date).expect("header").contains("Wed, 01 Jan 2020"));
    assert_eq!(m.body(br#"{"databaseId":987654321}"#, false), br#"{"databaseId":1}"#);
    let signed = m.headers(b"GET /signed/blob?signature=invented-signature HTTP/1.1\r\n");
    assert!(!signed.windows(18).any(|v| v == b"invented-signature"));
}
#[test]
fn github_conformance_stops_on_a_leaked_secret() {
    let out = Temp::new("leak");
    for kind in
        [SecretKind::AppKey, SecretKind::PersonToken, SecretKind::Jwt, SecretKind::MintedToken, SecretKind::HookSecret]
    {
        let secrets = Secrets::of(&[(kind, "invented-secret-value")]);
        let mut m = mask::Mask::default();
        let unexpected = m.body(br#"{"unexpected":"invented-secret-value"}"#, false);
        let leak = write::case(&out.0, "unexpected-field", b"GET / HTTP/1.1\r\n", &unexpected, &secrets)
            .expect_err("leak refused");
        let error = leak.to_string();
        assert!(error.contains("unexpected-field"));
        assert!(error.contains(&format!("{kind:?}")));
        assert!(!error.contains("invented-secret-value"));
        assert!(!out.0.join("transcripts").exists());
    }
}
fn private_fixture(dir: &Path) {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).expect("private directory");
    let fixtures = World::fixtures().expect("invented fixtures");
    let app = br#"{"id":8,"slug":"example-app","name":"Example App","client_id":"invented-client","node_id":"invented-node","html_url":"https://example-app.invalid/","permissions":{"contents":"write"},"events":["push"],"created_at":"2026-01-01T00:00:00Z","owner":"acme","installations":[{"id":1,"account":"acme","repositories":["acme/widgets"]},{"id":2,"account":"globex","repositories":["globex/gadgets"]}],"spare_repository":"acme/spare","hook_url":"https://example-app.invalid/","api_version":"2026-03-10"}"#;
    // Pin the invented document to the same dialect constant as the runner.
    let app =
        std::str::from_utf8(app).expect("metadata text").replace("2026-03-10", skein_forge::github::pin::API_VERSION);
    for (name, bytes) in [
        ("app.json", app.as_bytes()),
        ("app.pem", fixtures.pem.as_bytes()),
        ("webhook_secret", fixtures.webhook.as_bytes()),
        ("person-acme.token", fixtures.people["acme"].as_bytes()),
        ("person-globex.token", fixtures.people["globex"].as_bytes()),
    ] {
        fs::write(dir.join(name), bytes).expect("invented fixture file");
        fs::set_permissions(dir.join(name), fs::Permissions::from_mode(0o600)).expect("private file");
    }
}
#[test]
fn github_conformance_live_refuses_a_loose_directory_or_file() {
    let valid = Temp::new("valid-fixture");
    private_fixture(&valid.0);
    fs::write(valid.0.join("ignored"), b"unneeded").expect("ignored loose file");
    assert!(Fixtures::load(&valid.0).is_ok(), "valid invented fixtures refused");
    for name in ["app.json", "app.pem", "webhook_secret", "person-acme.token", "person-globex.token"] {
        for kind in ["file", "link", "non-file"] {
            let out = Temp::new(&format!("{kind}-{name}"));
            private_fixture(&out.0);
            let path = out.0.join(name);
            match kind {
                "file" => fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loose file"),
                "link" => {
                    let target = out.0.join("invented-target");
                    fs::rename(&path, &target).expect("move invented file");
                    symlink(&target, &path).expect("file link");
                }
                "non-file" => {
                    fs::remove_file(&path).expect("remove invented file");
                    fs::create_dir(&path).expect("directory in file slot");
                }
                _ => unreachable!(),
            }
            let attempted = Fixtures::load(&out.0).map(|_| -> () {
                panic!("connector reached after insecure fixture");
            });
            assert!(attempted.is_err());
        }
    }
    fs::set_permissions(&valid.0, fs::Permissions::from_mode(0o755)).expect("loose directory");
    assert!(Fixtures::load(&valid.0).is_err());
    fs::set_permissions(&valid.0, fs::Permissions::from_mode(0o700)).expect("private directory");
    let link = Temp::new("directory-link");
    symlink(&valid.0, link.0.join("link")).expect("directory link");
    assert!(Fixtures::load(&link.0.join("link")).is_err());
}
#[test]
fn github_conformance_live_refuses_the_apps_changes_before_any_byte() {
    let fixtures = World::fixtures().expect("invented fixtures").guard_fixtures();
    for (method, path) in [
        (Method::Delete, "/app/installations/123"),
        (Method::Patch, "/app/hook/config"),
        (Method::Post, "/app-manifests/invented/conversions"),
        (Method::Post, "/settings/apps/new"),
        (Method::Patch, "/app"),
        (Method::Post, "/app/installations"),
        (Method::Put, "/app/hook/config"),
        (Method::Post, "/apps/example-app/installations/new"),
    ] {
        assert!(
            live::guarded(&fixtures, method, path, b"", || -> Result<()> {
                panic!("connector called for App mutation");
            })
            .is_err()
        );
    }
    for (method, path) in [
        (Method::Get, "/app"),
        (Method::Get, "/app/hook/config"),
        (Method::Post, "/app/installations/123/access_tokens"),
        (Method::Delete, "/installation/token"),
    ] {
        assert!(live::guarded(&fixtures, method, path, b"", || Ok(())).is_ok());
    }
}
#[test]
fn github_conformance_live_refuses_targets_outside_the_fixtures() {
    let dir = Temp::new("guard-fixtures");
    private_fixture(&dir.0);
    let fixtures = Fixtures::load(&dir.0).expect("invented private fixtures").guard_fixtures();
    let delete =
        br#"{"query":"mutation { deleteRepository(input: {repositoryId: \"invented\"}) { clientMutationId } }"}"#
            .as_slice();
    let mixed = br#"{"query":"mutation { enqueuePullRequest(input: {pullRequestId: \"invented\"}) { clientMutationId } deleteRepository(input: {repositoryId: \"invented\"}) { clientMutationId } }"}"#.as_slice();
    for (method, verb, target, body) in [
        (Method::Post, "POST", "/repos/acme/other/pulls", b"".as_slice()),
        (Method::Get, "GET", "/repos/acme/other", b""),
        (Method::Get, "GET", "/repos/other/widgets/pulls", b""),
        (Method::Get, "GET", "/repos/Acme/widgets", b""),
        (Method::Get, "GET", "/repos/acme/Widgets", b""),
        (Method::Delete, "DELETE", "/repos/acme/widgets", b""),
        (Method::Patch, "PATCH", "/repos/acme/widgets?ignored=1", b""),
        (Method::Post, "POST", "/repos/acme/spare", b""),
        (Method::Put, "PUT", "/repos/globex/gadgets", b""),
        (Method::Delete, "DELETE", "/repos/acme/widgets/", b""),
        (Method::Post, "POST", "/user/repos", b""),
        (Method::Patch, "PATCH", "/user", b""),
        (Method::Put, "PUT", "/user/installations/999/repositories/1", b""),
        (Method::Delete, "DELETE", "/user/installations/999/repositories/1", b""),
        (Method::Post, "POST", "/user/installations/1/repositories/1", b""),
        (Method::Put, "PUT", "/user/installations/1/repositories/not-digits", b""),
        (Method::Put, "PUT", "/user/installations/1/repositories/1/extra", b""),
        (Method::Put, "PUT", "/user/installations/1/repositories/", b""),
        (Method::Put, "PUT", "/user/installations//repositories/1", b""),
        (Method::Put, "PUT", "/user/installations/18446744073709551616/repositories/1", b""),
        (Method::Delete, "DELETE", "/users/acme", b""),
        (Method::Patch, "PATCH", "/repositories/2", b""),
        (Method::Post, "POST", "/graphql", delete),
        (Method::Post, "POST", "/graphql", mixed),
        (Method::Get, "GET", "/graphql", br#"{"query":"query { viewer { login databaseId } }"}"#),
        (Method::Post, "POST", "/graphql", br#"{"query":"subscription { viewer { login } }"}"#),
        (Method::Post, "POST", "/graphql", br#"{"query":"query { viewer { login } }"}"#),
        (Method::Post, "POST", "/graphql", br#"{"query":"query { viewer { login databaseId } } "}"#),
        (Method::Post, "POST", "/graphql", b"{}"),
        (Method::Post, "POST", "/graphql", br#"{"query":42}"#),
        (Method::Post, "POST", "/graphql", b"invalid JSON"),
        (Method::Get, "GET", "/repos/acme/widgets%2F..", b""),
        (Method::Get, "GET", "/repos/ac%6de/widgets", b""),
        (Method::Get, "GET", "/repos/acme%2Fother/widgets", b""),
        (Method::Get, "GET", "/repos//widgets", b""),
        (Method::Get, "GET", "/repos/acme/", b""),
        (Method::Get, "GET", "/repos/./widgets", b""),
        (Method::Get, "GET", "/repos/../widgets", b""),
        (Method::Get, "GET", "/repos/acme/.", b""),
        (Method::Get, "GET", "/repos/acme/..", b""),
        (Method::Get, "GET", "/repos/acme/widgets/../other", b""),
        (Method::Get, "GET", "/repos/acme/widgets#ignored", b""),
        (Method::Get, "GET", "/repos/acme/widgets\\other", b""),
        (Method::Get, "GET", "/repos/acme", b""),
        (Method::Get, "GET", "/user-other", b""),
        (Method::Get, "GET", "/installation/token", b""),
        (Method::Post, "POST", "/installation/repositories", b""),
        (Method::Get, "GET", "/installation/repositories-other", b""),
    ] {
        let error = live::guarded(&fixtures, method, target, body, || {
            panic!("connector called for a refused target");
        })
        .expect_err("target must be refused before the connector");
        let path = target.split('?').next().expect("path");
        assert_eq!(error.to_string(), format!("live target refuses {verb} {path} before the connector"));
    }
}
#[test]
fn github_conformance_live_allows_fixture_requests() {
    let fixtures = World::fixtures().expect("invented fixtures").guard_fixtures();
    for (method, path) in [
        (Method::Get, "/repos/acme/widgets?per_page=1"),
        (Method::Get, "/repos/globex/gadgets"),
        (Method::Get, "/repos/acme/spare"),
        (Method::Post, "/repos/acme/widgets/pulls"),
        (Method::Patch, "/repos/globex/gadgets/issues/1"),
        (Method::Delete, "/repos/acme/spare/git/refs/heads/conformance-owned"),
        (Method::Get, "/repos/acme/widgets/branches/conformance%2Fslash"),
        (Method::Get, "/user"),
        (Method::Get, "/user/31"),
        (Method::Get, "/users/acme"),
        (Method::Get, "/repositories/2"),
        (Method::Put, "/user/installations/1/repositories/6"),
        (Method::Delete, "/user/installations/2/repositories/4?ignored=1"),
        (Method::Get, "/installation/repositories?per_page=100&page=1"),
    ] {
        assert!(live::guarded(&fixtures, method, path, b"", || Ok(())).is_ok(), "{method:?} {path}");
    }
    for document in [github_document::VIEWER, github_document::PULL, github_document::ENQUEUE] {
        let document = std::str::from_utf8(document).expect("constant GraphQL document");
        let body = skein_forge_conformance::json::object(&[("query", &document)]).expect("GraphQL body");
        assert!(live::guarded(&fixtures, Method::Post, "/graphql", &body, || Ok(())).is_ok());
    }
}
#[test]
fn github_conformance_fake_requests_replay_through_the_live_guard() {
    let out = Temp::new("guard-replay");
    let mut r = runner(&out.0);
    let fixtures = r.fixtures.guard_fixtures();
    r.start().expect("start fake run");
    let run = cases::run(&mut r);
    r.complete(run).expect("complete fake run and cleanup");
    assert_eq!(r.replay.len(), r.requests);
    let mut allowed = 0;
    let mut refused = 0;
    let mut blobs = 0;
    let mut documents = BTreeSet::new();
    for (phase, request) in &r.replay {
        if request.blob {
            blobs += 1;
            continue;
        }
        if *phase == "setup-lifecycle" {
            if request.method != Method::Get {
                assert!(
                    live::guarded(&fixtures, request.method, &request.target, &request.body, || {
                        panic!("connector called for fake-only setup mutation");
                    })
                    .is_err()
                );
                refused += 1;
            }
            continue;
        }
        live::guarded(&fixtures, request.method, &request.target, &request.body, || Ok(()))
            .expect("a case's request was refused by the live guard");
        allowed += 1;
        if request.target == "/graphql" {
            let body = skein_forge_conformance::json::Json::parse(&request.body).expect("GraphQL body");
            documents.insert(body.root().get("query").str().expect("query document").as_bytes().to_vec());
        }
    }
    assert!(allowed > 100);
    assert_eq!(refused, 5, "bootstrap manifest, conversion, hook change and two removals");
    assert!(blobs > 0, "blob redirects keep their separate boundary");
    assert_eq!(
        documents,
        [github_document::VIEWER, github_document::PULL, github_document::ENQUEUE].map(<[u8]>::to_vec).into()
    );
    shutdown(&mut r);
}
#[test]
fn github_conformance_names_a_token_expiring_within_30_days() {
    let out = Temp::new("expiry");
    let mut r = runner(&out.0);
    r.start().expect("start");
    assert!(r.provenance.iter().any(|p| p == "# person-acme.token expires within 30 days"));
    assert!(!r.provenance.iter().any(|p| p.contains("person-globex.token expires")));
    r.finish().expect("cleanup");
    r.report().expect("report");
    scan(&out.0, &r.secrets);
    shutdown(&mut r);
}
#[test]
fn github_conformance_stays_within_its_request_budget() {
    let out = Temp::new("budget");
    let mut r = runner(&out.0);
    r.start().expect("start");
    let token = r.token(0).expect("memory token");
    r.case_limit = r.requests;
    assert!(r.raw(Method::Get, "/installation/repositories?per_page=100&page=1", &token, b"").is_err());
    r.finish().expect("revocation has reserved budget");
    assert!(r.requests <= 400);
    assert!(r.journal.pending.is_empty());
    assert_eq!(
        r.raw(Method::Get, "/installation/repositories?per_page=100&page=1", &token, b"").expect("revoked read").status,
        401
    );
    shutdown(&mut r);
}
#[test]
fn github_conformance_failed_case_revokes_its_tokens() {
    for stop in ["case", "leak"] {
        let out = Temp::new(stop);
        let mut r = runner(&out.0);
        r.start().expect("start");
        let token = r.token(0).expect("memory token");
        let stopped: Result<()> = if stop == "leak" {
            write::check("failed-case", token.as_bytes(), b"", &r.secrets).map_err(Into::into)
        } else {
            Err("deliberately failed case".into())
        };
        assert!(r.complete(stopped).is_err(), "failed run must retain its failure after cleanup");
        assert!(r.journal.pending.is_empty());
        assert!(r.leftover_mints.is_empty());
        assert_eq!(
            r.raw(Method::Get, "/installation/repositories?per_page=100&page=1", &token, b"")
                .expect("revoked read")
                .status,
            401
        );
        scan(&out.0, &r.secrets);
        shutdown(&mut r);
    }
}
#[test]
fn github_conformance_clean_finishes_a_run_stopped_partway() {
    let out = Temp::new("stopped");
    let mut r = runner(&out.0);
    r.start().expect("start");
    let initial = r.base(0).expect("base");
    let base = r.copy_branch(0, &initial).expect("owned base");
    let (head, _) = r
        .branch(
            0,
            &initial,
            "probe",
            vec![skein_fake_forge_domain::api::File {
                path: crate::fake::boxed(b"probe.txt"),
                content: crate::fake::boxed(b"probe"),
            }],
        )
        .expect("branch");
    r.pull(0, &head, &base, false).expect("pull");
    r.issue(0).expect("issue");
    let repo = r.repo(0).expect("repo");
    let person = r.person(0).expect("person");
    r.before(Made::Rules { repo: repo.clone(), name: "stopped-rule".into() }).expect("write ahead");
    assert_eq!(r.raw(Method::Post, format!("/repos/{repo}/rulesets"), &person, br#"{"name":"stopped-rule","target":"branch","enforcement":"active","conditions":{"ref_name":{"include":["refs/heads/main"],"exclude":[]}},"bypass_actors":[],"rules":[{"type":"pull_request","parameters":{"required_approving_review_count":1,"dismiss_stale_reviews_on_push":false,"require_code_owner_review":false,"require_last_push_approval":false,"required_review_thread_resolution":false}}]}"#).expect("rule").status,201);
    r.before(Made::Access { installation: 1, repository: 6, owner: "acme".into(), add: false }).expect("write ahead");
    assert_eq!(r.raw(Method::Put, "/user/installations/1/repositories/6", &person, b"").expect("grant").status, 204);
    let minted = r.token(0).expect("memory token");
    // A request whose answer never arrived has the same durable mint record.
    r.before(Made::Mint { installation: 1, at: r.now().as_nanos() / 1_000_000_000 }).expect("lost mint reply");
    r.flush().expect("captured partial run");
    // A request-budget stop models the last controlled action before an external kill.
    r.case_limit = r.requests;
    assert!(r.raw(Method::Get, "/app", &person, b"").is_err());
    r.tokens.clear(); // Model a killed process: none of the token values reaches cleanup.
    let oracle = r.secrets;
    let mut r = Runner::resume(r.target, r.fixtures, &out.0).expect("fresh cleanup runner with retained hosted state");
    r.finish().expect("resumed cleanup");
    r.report().expect("report");
    assert!(r.journal.pending.is_empty());
    assert_eq!(r.leftover_mints.len(), 3);
    assert!(r.leftover_mints.iter().all(|p| p.contains("not revoked, expires by")));
    let journal_text = fs::read_to_string(out.0.join("made.txt")).expect("journal");
    assert!(!journal_text.contains("not revoked"));
    for entry in fs::read_dir(out.0.join("transcripts")).expect("transcripts") {
        let dir = entry.expect("case").path();
        for name in ["request", "response"] {
            assert!(!fs::read_to_string(dir.join(name)).expect("masked transcript").contains("not revoked"));
        }
    }
    assert_eq!(
        r.raw(Method::Get, "/installation/repositories?per_page=100&page=1", &minted, b"")
            .expect("unavailable values cannot revoke")
            .status,
        200
    );
    let branches = r
        .raw(Method::Get, format!("/repos/{repo}/branches?per_page=100&page=1"), &person, b"")
        .expect("branches")
        .json()
        .expect("branches document");
    assert_eq!(branches.root().items().len(), 1);
    assert!(branches.root().nth(0).get("name").is("main"));
    let issues = r.raw(Method::Get, format!("/repos/{repo}/issues?state=all&since=1970-01-01T00%3A00%3A00Z&sort=updated&direction=asc&per_page=100&page=1"), &person, b"").expect("active objects").json().expect("document");
    assert!(issues.root().items().iter().all(|row| row.get("state").is("closed")));
    let protection = r
        .plan_as(0, api::Op::Read(api::Read::Protection { branch: crate::fake::boxed(b"main") }), true)
        .expect("protection plan");
    assert!(matches!(protection, Ok(api::Answer::Protection(None))));
    assert_eq!(r.raw(Method::Get, "/repos/acme/spare", &minted, b"").expect("restored grant").status, 404);
    scan(&out.0, &oracle);
    shutdown(&mut r);
}

#[test]
fn github_conformance_clean_preserves_facts_and_ignores_an_incomplete_journal_tail() {
    let out = Temp::new("clean-file");
    let secrets = Secrets::of(&[]);
    let mut journal = Journal::create(&out.0, false, &secrets).expect("journal");
    journal.before(Made::Mint { installation: 123, at: 100 }, &secrets).expect("mint intention");
    journal
        .before(Made::Branch { repo: "acme/widgets".into(), name: "conformance-owned".into() }, &secrets)
        .expect("branch intention");
    let mut file = fs::OpenOptions::new().append(true).open(out.0.join("made.txt")).expect("journal file");
    file.write_all(b"make\t3\tmint\t").expect("partial interrupted write");
    fs::write(out.0.join("facts.txt"), "# target fake\nholds retained-fact\n").expect("prior facts");
    clean(&out.0, None).expect("fresh fake cleanup");
    let facts = fs::read_to_string(out.0.join("facts.txt")).expect("facts");
    assert!(facts.contains("holds retained-fact"));
    assert!(facts.contains("minted token for installation 123: not revoked, expires by 3700"));
    assert!(!fs::read_to_string(out.0.join("made.txt")).expect("journal").contains("not revoked"));
    assert!(Journal::load(&out.0).expect("reloaded").0.pending.is_empty());
    clean(&out.0, None).expect("idempotent cleanup");
    assert_eq!(fs::read_to_string(out.0.join("facts.txt")).expect("facts"), facts);
}

#[test]
fn github_conformance_checks_logged_payload_signatures() {
    let payload = br#"{"nested":[{"body":"a } brace and \" quote"}]}"#;
    let signature = skein_forge::github::delivery::sign(b"invented-hook-secret", payload);
    let mut body = format!(
        "{{\"request\":{{\"headers\":{{\"X-Hub-Signature-256\":\"{}\"}},\"payload\":",
        std::str::from_utf8(&signature).expect("signature text")
    )
    .into_bytes();
    body.extend_from_slice(payload);
    body.extend_from_slice(b"}}");
    let response = crate::wire::Response { status: 200, head: vec![], body };
    assert!(response.logged_signature(b"invented-hook-secret").expect("logged payload"));
    assert!(!response.logged_signature(b"different-invented-secret").expect("wrong secret"));
    let mut secrets = Secrets::of(&[]);
    live::register(&mut secrets, SecretKind::HookSecret, "invented\nsecret");
    assert!(write::check("escaped", br#"{"unexpected":"invented\nsecret"}"#, b"", &secrets).is_err());
}

#[test]
fn github_conformance_cleanup_finds_an_issue_after_more_than_one_page() {
    for known_number in [false, true] {
        let out = Temp::new(if known_number { "number-cleanup" } else { "title-cleanup" });
        let mut r = runner(&out.0);
        r.start().expect("start");
        let repo = r.repo(0).expect("repository");
        let Target::Fake(w) = &mut r.target else { unreachable!() };
        for i in 0..101 {
            w.call(
                &repo,
                crate::fake::PERSON,
                skein_fake_forge_domain::api::Op::Write(skein_fake_forge_domain::api::Write::CreateIssue {
                    title: crate::fake::boxed(format!("older issue {i}").as_bytes()),
                    body: crate::fake::boxed(b"older fixture"),
                    labels: Box::new([]),
                }),
            )
            .expect("older issue");
        }
        let number = if known_number {
            let number = r.issue(0).expect("made issue");
            assert!(Journal::load(&out.0).expect("durable journal").0.numbers.values().any(|n| *n == number));
            number
        } else {
            let title = "conformance-killed-issue";
            r.before(Made::Issue { repo: repo.clone(), title: title.into() })
                .expect("intention before interrupted reply");
            let Target::Fake(w) = &mut r.target else { unreachable!() };
            let answer = w
                .call(
                    &repo,
                    crate::fake::PERSON,
                    skein_fake_forge_domain::api::Op::Write(skein_fake_forge_domain::api::Write::CreateIssue {
                        title: crate::fake::boxed(title.as_bytes()),
                        body: crate::fake::boxed(b"interrupted reply"),
                        labels: Box::new([]),
                    }),
                )
                .expect("issue created without received response");
            let skein_fake_forge_domain::api::Answer::Created(number) = answer else { panic!("created issue") };
            number
        };
        r.wait(4);
        r.finish().expect("verified cleanup");
        assert!(r.journal.pending.is_empty());
        let person = r.person(0).expect("person");
        let read =
            r.raw(Method::Get, format!("/repos/{repo}/issues/{number}"), &person, b"").expect("read closed issue");
        assert!(read.json().expect("issue").root().get("state").is("closed"));
        if !known_number {
            assert!(r.replay.iter().any(|(_, request)| request.target.ends_with("page=2")));
        }
        shutdown(&mut r);
    }
}

#[test]
fn github_conformance_cleanup_retains_an_unfound_issue() {
    let out = Temp::new("missing-cleanup");
    let mut r = runner(&out.0);
    r.start().expect("start");
    r.before(Made::Issue { repo: "acme/widgets".into(), title: "conformance-no-reply".into() }).expect("intention");
    assert!(r.finish().is_err());
    assert_eq!(r.journal.pending.len(), 1);
    assert_eq!(Journal::load(&out.0).expect("durable pending intention").0.pending.len(), 1);
    shutdown(&mut r);
}

#[test]
fn github_conformance_transcripts_put_every_status_and_headers_on_their_own_lines() {
    let out = Temp::new("all-statuses");
    let mut r = runner(&out.0);
    r.start().expect("start");
    r.next("many-responses").expect("case");
    let person = r.person(0).expect("person");
    for _ in 0..3 {
        r.raw(Method::Get, "/repos/acme/widgets", &person, b"").expect("response");
    }
    r.flush().expect("transcript");
    let dir = out.0.join("transcripts/many-responses");
    let response = fs::read_to_string(dir.join("response")).expect("responses");
    let request = fs::read_to_string(dir.join("request")).expect("requests");
    assert_eq!(response.lines().filter(|line| line.starts_with("HTTP/1.1 200")).count(), 3);
    assert_eq!(
        response.lines().filter(|line| line.to_ascii_lowercase().starts_with("x-ratelimit-remaining:")).count(),
        3
    );
    assert_eq!(request.lines().filter(|line| line.starts_with("GET ")).count(), 3);
    r.finish().expect("cleanup");
    shutdown(&mut r);
}

#[test]
fn github_conformance_raw_sends_the_supplied_narrowed_token() {
    let out = Temp::new("raw-narrowed");
    let mut r = runner(&out.0);
    r.start().expect("start");
    let full = r.token(0).expect("full token");
    let narrowed = r.mint(0, true).expect("narrowed token");
    assert!(full != narrowed, "minted credentials differ");
    let response = r.raw(Method::Get, "/repos/acme/widgets", &narrowed, b"").expect("raw read");
    assert_eq!(response.status, 200);
    let Target::Fake(w) = &r.target else { panic!("fake target") };
    let expected = format!("Authorization: Bearer {narrowed}\r\n");
    let other = format!("Authorization: Bearer {full}\r\n");
    assert!(
        w.last_wire.windows(expected.len()).any(|part| part == expected.as_bytes()),
        "supplied authorization on wire"
    );
    assert!(!w.last_wire.windows(other.len()).any(|part| part == other.as_bytes()), "full token is not substituted");
    r.finish().expect("cleanup");
    shutdown(&mut r);
}

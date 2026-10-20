use crate::json::{At, Json};
use crate::{
    Result,
    facts::{Fact, add},
    failure,
    fixture::Fixture,
    runner,
    write::{SecretKind, Secrets},
};
use skein_throwaway_forgejo::Output;
use skein_throwaway_forgejo::RUNNER_VERSION;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
};
pub(crate) const FACT_SECRET: &str = "0123456789abcdef0123456789abcdef01234567";
pub(crate) const FACT_UUID: &str = "30313233-3435-3637-3839-616263646566";
pub(crate) const RUNNER_SECRET: &str = "fedcba9876543210fedcba9876543210fedcba98";
pub(crate) const RUNNER_UUID: &str = "66656463-6261-3938-3736-353433323130";
pub(crate) const NAMES: [&str; 10] = [
    "runner-register-secret-file-prints-the-uuid",
    "runner-register-refuses-a-secret-of-another-length",
    "runner-register-refuses-a-secret-that-is-not-hexadecimal",
    "runner-register-again-keeps-one-global-runner",
    "runner-create-runner-file-takes-its-secret-only-as-an-argument",
    "runner-uuid-is-the-secrets-first-sixteen-characters-in-hex",
    "runner-token-is-the-secret",
    "runner-connection-with-a-token-file-comes-online",
    "runner-connection-alone-takes-the-jobs",
    "runner-a-jobs-token-clones-its-private-repository",
];
/// One connection, whose credential stays in its private file.
pub(crate) fn configuration(base: &str, uuid: &str, token_file: &Path, jobs: &Path) -> String {
    format!(
        "log:\n  level: info\nrunner:\n  capacity: 1\n  timeout: 1m\n  shutdown_timeout: 1s\n  fetch_interval: 100ms\n  report_interval: 100ms\ncache:\n  enabled: false\nhost:\n  workdir_parent: {}\nserver:\n  connections:\n    forgejo:\n      url: {}/\n      uuid: {uuid}\n      token_url: file:{}\n      labels:\n        - kit-host:host\n",
        jobs.display(),
        base.trim_end_matches('/'),
        token_file.display()
    )
}
pub(crate) fn listed<'a>(body: &'a Json, uuid: &str) -> Option<At<'a>> {
    body.root().items().into_iter().find(|row| row.get("uuid").is(uuid))
}
pub(crate) fn private(path: &Path, secret: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(secret.as_bytes())?;
    Ok(())
}
pub(crate) fn register(f: &Fixture, path: &Path, name: &str, labels: &str) -> Result<Output> {
    f.instance
        .cli(&[
            "forgejo-cli",
            "actions",
            "register",
            "--secret-file",
            &path.to_string_lossy(),
            "--name",
            name,
            "--labels",
            labels,
        ])
        .map_err(|e| failure(&e))
}
pub(crate) fn capture(cmd: &mut Command) -> Result<std::process::Output> {
    Ok(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?.wait_with_output()?)
}
fn held(out: &Output, code: i32, stderr: &str) -> bool {
    out.code == Some(code) && out.stderr == stderr
}
pub(crate) fn facts(f: &Fixture, binary: &Path, secrets: &mut Secrets) -> Result<Vec<Fact>> {
    let scratch = f.instance.scratch().join("runner-facts");
    fs::create_dir_all(scratch.join("home"))?;
    runner::version(binary, &scratch)?;
    secrets.add(SecretKind::RunnerSecret, FACT_SECRET);
    let path = scratch.join("secret");
    private(&path, FACT_SECRET)?;
    let first = register(f, &path, "fact-runner", "fact:host")?;
    let mut facts = Vec::new();
    add(&mut facts, NAMES[0], held(&first, 0, "") && first.stdout == FACT_UUID, &format!("{first:?}"));
    let mut lengths = true;
    for secret in [&FACT_SECRET[..39], &format!("{FACT_SECRET}\n")] {
        secrets.add(SecretKind::RunnerSecret, secret);
        private(&path, secret)?;
        let out = register(f, &path, "fact-runner", "fact:host")?;
        lengths &= held(
            &out,
            1,
            &format!(
                "Command error: the secret must be exactly 40 characters long, not {}: generate-secret can provide a secret matching the requirements\n",
                secret.len()
            ),
        );
    }
    add(&mut facts, NAMES[1], lengths, "secret-length refusals differed");
    let invalid = "z".repeat(40);
    secrets.add(SecretKind::RunnerSecret, &invalid);
    private(&path, &invalid)?;
    let out = register(f, &path, "fact-runner", "fact:host")?;
    add(
        &mut facts,
        NAMES[2],
        held(
            &out,
            1,
            "Command error: the secret must be an hexadecimal string: encoding/hex: invalid byte: U+007A 'z'\n",
        ),
        &format!("{out:?}"),
    );
    private(&path, FACT_SECRET)?;
    let out = register(f, &path, "renamed", "other:host")?;
    let all = f.get("/api/v1/admin/actions/runners")?;
    let one = all.root().items().iter().filter(|r| r.get("uuid").is(FACT_UUID)).count() == 1;
    let matches = listed(&all, FACT_UUID).is_some_and(|row| {
        row.get("owner_id").u64() == Some(0)
            && row.get("repo_id").u64() == Some(0)
            && row.get("name").is("renamed")
            && row.get("labels").items().iter().map(|label| label.str()).eq([Some("other:host")])
    });
    add(
        &mut facts,
        NAMES[3],
        held(&out, 0, "") && out.stdout == FACT_UUID && one && matches,
        &format!("{out:?}; matching rows {}", usize::from(one)),
    );
    offline(binary, &scratch, &path, &mut facts)?;
    Ok(facts)
}
fn offline(binary: &Path, scratch: &Path, secret: &Path, facts: &mut Vec<Fact>) -> Result<()> {
    let config = scratch.join("config.yaml");
    let file = scratch.join("runner.file");
    fs::write(&config, format!("runner:\n  file: {}\n", file.display()))?;
    let args = [
        "--config",
        &config.to_string_lossy(),
        "create-runner-file",
        "--instance",
        "http://127.0.0.1:1",
        "--name",
        "n",
    ];
    let a = capture(runner::command(binary, scratch).args(args).arg("--secret-file").arg(secret))?;
    let b = capture(runner::command(binary, scratch).args(args))?;
    let c = capture(runner::command(binary, scratch).args(args).args(["--secret", FACT_SECRET]))?;
    let expected = a.status.code() == Some(1)
        && String::from_utf8_lossy(&a.stderr).contains("Error: unknown flag: --secret-file")
        && b.status.code() == Some(1)
        && String::from_utf8_lossy(&b.stderr).contains("Error: required flag(s) \"secret\" not set")
        && c.status.success()
        && String::from_utf8_lossy(&c.stderr)
            .contains("has been deprecated; declare connections in the runner configuration instead")
        && file.exists();
    add(
        facts,
        NAMES[4],
        expected,
        &format!("codes {:?}, {:?}, {:?}", a.status.code(), b.status.code(), c.status.code()),
    );
    let row = crate::json::Json::parse(&fs::read(file)?)?;
    add(facts, NAMES[5], row.root().get("uuid").is(FACT_UUID), "runner file uuid differed");
    add(facts, NAMES[6], row.root().get("token").is(FACT_SECRET), "runner file token differed");
    Ok(())
}
pub(crate) fn online(f: &Fixture) -> Result<bool> {
    for _ in 0..300 {
        if let Some(row) = listed(&f.get("/api/v1/admin/actions/runners")?, RUNNER_UUID)
            && (row.get("status").is("idle") || row.get("status").is("active"))
            && row.get("version").is(&format!("v{RUNNER_VERSION}"))
        {
            return Ok(!f.instance.scratch().join("runner/.runner").exists());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(false)
}

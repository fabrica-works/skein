use crate::{
    Result,
    facts::{Fact, add, matches},
    failure,
    fixture::{Fixture, R},
    write::{SecretKind, Secrets},
};
use skein_throwaway_forgejo::{Forgejo, Output, Settings, Token, WorkPath};
use std::fs;

pub(crate) fn password_of(stdout: &str) -> Option<&str> {
    let line = stdout.lines().next()?;
    let first = line.find('\'')?;
    let last = line.rfind('\'')?;
    line.get(first + 1..last)
}
pub(crate) fn raw_token(stdout: &str) -> Option<&str> {
    let token = stdout.strip_suffix('\n')?;
    (token.len() == 40 && token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))).then_some(token)
}
fn seen(facts: &mut Vec<Fact>, name: &'static str, held: bool, out: &Output) {
    add(facts, name, held, &format!("{out:?}"));
}
fn remember(out: &Output, secrets: &mut Secrets) {
    if let Some(password) = password_of(&out.stdout) {
        secrets.add(SecretKind::Password, password);
    }
    if let Some(token) = raw_token(&out.stdout) {
        secrets.add(SecretKind::ApiToken, token);
    }
}
fn create(name: &str, person: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        "admin",
        "user",
        "create",
        "--username",
        name,
        "--email",
        &format!("{name}@example.invalid"),
        "--admin",
        "--random-password",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if !person {
        args.push("--must-change-password=false".into());
    }
    args
}
fn created(out: &Output, name: &str, length: usize) -> bool {
    password_of(&out.stdout).is_some_and(|p| {
        p.len() == length
            && out.stdout
                == format!("generated random password is '{p}'\nNew user '{name}' has been successfully created!\n")
    }) && out.code == Some(0)
        && out.stderr.is_empty()
}
/// Records the fresh work path's and running instance's command-line facts.
pub fn facts(forgejo: &Forgejo, f: &mut Fixture, secrets: &mut Secrets) -> Result<Vec<Fact>> {
    let work =
        forgejo.work_path(&f.instance.scratch().join("cli-fresh"), &Settings::small()).map_err(|e| failure(&e))?;
    let mut facts = Vec::new();
    let out = work.run(&["migrate"]).map_err(|e| failure(&e))?;
    seen(
        &mut facts,
        "cli-migrate-runs-on-a-fresh-work-path",
        out.code == Some(0)
            && out.stderr.is_empty()
            && work.scratch().join("forgejo.db").exists()
            && !work.scratch().join("repositories").exists(),
        &out,
    );
    let out = work.run(&["migrate"]).map_err(|e| failure(&e))?;
    seen(&mut facts, "cli-migrate-runs-again-on-a-migrated-work-path", out.code == Some(0), &out);
    for (name, length, fact) in [
        ("kit-admin", 12, "cli-user-create-prints-the-random-password-first"),
        ("other", 32, "cli-user-create-random-password-length-is-asked"),
    ] {
        let mut args = create(name, false);
        if length == 32 {
            args.extend(["--random-password-length".into(), "32".into()]);
        }
        let out = work.run(&args.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| failure(&e))?;
        remember(&out, secrets);
        seen(&mut facts, fact, created(&out, name, length), &out);
    }
    let args = create("kit-admin", false);
    let out = work.run(&args.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| failure(&e))?;
    remember(&out, secrets);
    seen(
        &mut facts,
        "cli-user-create-again-is-refused-naming-the-user",
        out.code == Some(1)
            && out.stderr == "Command error: CreateUser: user already exists [name: kit-admin]\n"
            && out.stdout.starts_with("generated random password is '")
            && password_of(&out.stdout).is_some(),
        &out,
    );
    let out = work.run(&["admin", "user", "list"]).map_err(|e| failure(&e))?;
    let rows: Vec<Vec<&str>> = out.stdout.lines().map(|line| line.split_whitespace().collect()).collect();
    let held = out.code == Some(0)
        && rows.len() == 3
        && rows[0] == ["ID", "Username", "Email", "IsActive", "IsAdmin", "2FA"]
        && rows[1].len() == 6
        && rows[2].len() == 6
        && rows[1][1] == "kit-admin"
        && rows[2][1] == "other";
    seen(&mut facts, "cli-user-list-prints-a-header-and-a-row-per-user", held, &out);
    let args = create("cli-user", false);
    let out = f.instance.cli(&args.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| failure(&e))?;
    remember(&out, secrets);
    let password = password_of(&out.stdout).ok_or("CLI password missing")?;
    let signed_in = f.instance.token("cli-user", password).is_ok();
    for secret in f.instance.secrets() {
        secrets.add(SecretKind::ApiToken, &secret);
    }
    seen(&mut facts, "cli-user-create-random-password-signs-in", out.code == Some(0) && signed_in, &out);
    tokens(f, secrets, &mut facts)?;
    dumps(&work, &mut facts)?;
    Ok(facts)
}
fn token(f: &Fixture, secrets: &mut Secrets, login: &str, name: &str, scopes: &str) -> Result<Output> {
    let out = f
        .instance
        .cli(&[
            "admin",
            "user",
            "generate-access-token",
            "--username",
            login,
            "--token-name",
            name,
            "--scopes",
            scopes,
            "--raw",
        ])
        .map_err(|e| failure(&e))?;
    remember(&out, secrets);
    Ok(out)
}
fn get(f: &Fixture, out: &Output, target: &str) -> Result<skein_throwaway_forgejo::Exchange> {
    let token = Token::new(raw_token(&out.stdout).unwrap_or_default().into());
    f.exchange("GET", target, None, Some(&token), None)
}
fn tokens(f: &Fixture, secrets: &mut Secrets, facts: &mut Vec<Fact>) -> Result<()> {
    let out = token(f, secrets, "writer", "cli-raw", "all")?;
    let user = get(f, &out, "/api/v1/user")?;
    seen(
        facts,
        "cli-token-raw-prints-the-token-alone",
        out.code == Some(0)
            && out.stderr.is_empty()
            && raw_token(&out.stdout).is_some()
            && user.status == 200
            && crate::json(&user)?.root().get("login").is("writer"),
        &out,
    );
    let repo = token(f, secrets, "writer", "cli-repo", "read:repository")?;
    let user = token(f, secrets, "writer", "cli-user", "read:user")?;
    let first = get(f, &repo, R)?;
    let second = get(f, &user, R)?;
    seen(
        facts,
        "cli-token-scopes-limit-what-the-token-reads",
        first.status == 200
            && matches(&second, 403, "token does not have at least one of required scope(s): [read:repository]"),
        &user,
    );
    let first = token(f, secrets, "writer", "twice", "all")?;
    let second = token(f, secrets, "writer", "twice", "all")?;
    seen(
        facts,
        "cli-token-name-used-twice-is-refused",
        first.code == Some(0)
            && second.code == Some(1)
            && second.stdout.is_empty()
            && second.stderr == "Command error: access token name has been used already\n",
        &second,
    );
    let first = token(f, secrets, &f.owner.login, "kit-admin-a", "write:admin,write:organization,write:user")?;
    let second = token(f, secrets, &f.owner.login, "kit-admin-b", "write:admin,write:organization,write:user")?;
    let held =
        [&first, &second].iter().all(|o| o.code == Some(0) && o.stderr.is_empty() && raw_token(&o.stdout).is_some())
            && first.stdout != second.stdout
            && get(f, &first, "/api/v1/admin/users?limit=1")?.status == 200
            && get(f, &second, "/api/v1/admin/users?limit=1")?.status == 200;
    seen(facts, "cli-token-admin-scopes-comma-joined-are-taken-under-two-suffixes", held, &second);
    let args = create("cli-person", true);
    let made = f.instance.cli(&args.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| failure(&e))?;
    remember(&made, secrets);
    let out = token(f, secrets, "cli-person", "person", "all")?;
    let first = get(f, &out, "/api/v1/user")?;
    let patched = f.exchange(
        "PATCH",
        "/api/v1/admin/users/cli-person",
        Some(br#"{"login_name":"cli-person","source_id":0,"must_change_password":false}"#),
        Some(&f.owner.token),
        None,
    )?;
    let second = get(f, &out, "/api/v1/user")?;
    let held = made.code == Some(0)
        && first.status == 403
        && crate::json(&first)?.root().get("message").str().is_some_and(|s| s.contains("change your password"))
        && (200..300).contains(&patched.status)
        && second.status == 200
        && crate::json(&second)?.root().get("login").is("cli-person");
    seen(facts, "cli-user-made-to-change-its-password-is-refused-the-api-until-cleared", held, &out);
    Ok(())
}
fn zip(path: &std::path::Path, ini: bool) -> Result<bool> {
    let bytes = fs::read(path)?;
    Ok(bytes.starts_with(b"PK")
        && bytes.windows(14).any(|w| w == b"forgejo-db.sql")
        && (!ini || bytes.windows(7).any(|w| w == b"app.ini")))
}
fn dump(work: &WorkPath, name: &str, temp: &str) -> Result<Output> {
    work.run(&[
        "dump",
        "--file",
        &work.scratch().join(name).to_string_lossy(),
        "--tempdir",
        &work.scratch().join(temp).to_string_lossy(),
    ])
    .map_err(|e| failure(&e))
}
fn dumps(work: &WorkPath, facts: &mut Vec<Fact>) -> Result<()> {
    let w = work.scratch();
    let out = dump(work, "dump.zip", "tmp")?;
    let expected =
        format!("Failed to include repositories: open {}/repositories: no such file or directory", w.display());
    seen(
        facts,
        "cli-dump-needs-the-repositories-directory",
        out.code == Some(1) && out.stderr.lines().next() == Some(&expected) && fs::read(w.join("dump.zip"))?.is_empty(),
        &out,
    );
    fs::create_dir(w.join("repositories"))?;
    let out = dump(work, "dump2.zip", "tmp")?;
    seen(
        facts,
        "cli-dump-writes-the-database-and-app-ini",
        out.code == Some(0) && out.stdout.is_empty() && out.stderr.is_empty() && zip(&w.join("dump2.zip"), true)?,
        &out,
    );
    let out = dump(work, "x.zip.partial", "tmp")?;
    seen(
        facts,
        "cli-dump-appends-zip-to-a-file-name-without-it",
        out.code == Some(0) && zip(&w.join("x.zip.partial.zip"), false)? && !w.join("x.zip.partial").exists(),
        &out,
    );
    let out = dump(work, "dump3.zip", "absent")?;
    seen(
        facts,
        "cli-dump-needs-its-tempdir-to-exist",
        out.code == Some(1)
            && !out.stderr.is_empty()
            && !w.join("absent").exists()
            && fs::read(w.join("dump3.zip"))?.is_empty(),
        &out,
    );
    fs::write(w.join("over.zip"), b"junk")?;
    let out = dump(work, "over.zip", "tmp")?;
    seen(
        facts,
        "cli-dump-over-an-existing-file-overwrites-it",
        out.code == Some(0) && out.stdout.is_empty() && out.stderr.is_empty() && zip(&w.join("over.zip"), false)?,
        &out,
    );
    Ok(())
}

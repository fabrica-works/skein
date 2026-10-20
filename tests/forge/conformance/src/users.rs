use crate::json;
use crate::{
    Result,
    cases::{Auth, Recorder},
    facts::{Fact, add, case},
    setup,
    write::SecretKind,
};
use skein_throwaway_forgejo::{Token, fresh_secret};
use std::path::Path;
pub(crate) fn record(r: &mut Recorder<'_>, admin: &Token) -> Result<()> {
    let password = fresh_secret().map_err(|e| crate::failure(&e))?;
    let body = String::from_utf8(
        json::write(|out| {
            out.object_start();
            out.key(b"username");
            out.string(b"bot");
            out.key(b"email");
            out.string(b"bot@example.invalid");
            out.key(b"password");
            out.string(password.as_bytes());
            out.key(b"full_name");
            out.string(b"kit:bot");
            out.key(b"must_change_password");
            out.boolean(false);
            out.object_end();
        })?
        .into_vec(),
    )?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    r.send_new(
        "setup-user-create",
        "POST",
        "/api/v1/admin/users",
        Some(&body),
        Auth::Token(admin),
        &[(SecretKind::Password, &password)],
    )?;
    r.send_new("setup-user-create-again", "POST", "/api/v1/admin/users", Some(&body), Auth::Token(admin), &[])?;
    r.send_new(
        "setup-user-create-by-writer",
        "POST",
        "/api/v1/admin/users",
        Some(&body),
        Auth::Token(&r.f.writer.token),
        &[],
    )?;
    for (name, target) in [
        ("setup-user-of", "/api/v1/users/bot"),
        ("setup-user-missing", "/api/v1/users/missing"),
        ("setup-user-admin", "/api/v1/user"),
    ] {
        r.send_new(name, "GET", target, None, Auth::Token(admin), &[])?;
    }
    let password = fresh_secret().map_err(|e| crate::failure(&e))?;
    let body = String::from_utf8(
        json::write(|out| {
            out.object_start();
            out.key(b"username");
            out.string(b"person");
            out.key(b"email");
            out.string(b"person@example.invalid");
            out.key(b"password");
            out.string(password.as_bytes());
            out.key(b"full_name");
            out.string(b"Person");
            out.key(b"must_change_password");
            out.boolean(true);
            out.object_end();
        })?
        .into_vec(),
    )?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    r.send_new(
        "setup-user-create-person",
        "POST",
        "/api/v1/admin/users",
        Some(&body),
        Auth::Token(admin),
        &[(SecretKind::Password, &password)],
    )?;
    r.send_new("setup-user-person-basic", "GET", "/api/v1/user", None, Auth::Basic("person", &password), &[])?;
    let person = setup::scoped(r.f, r.secrets, "person", "setup-person", &["read:user"])?;
    r.send_new("setup-user-person-token", "GET", "/api/v1/user", None, Auth::Token(&person), &[])?;
    r.f.api(
        "PATCH",
        "/api/v1/admin/users/person",
        &json::object(&[("login_name", &"person"), ("must_change_password", &false), ("source_id", &0u64)])?,
    )?;
    r.send_new("setup-user-person-cleared", "GET", "/api/v1/user", None, Auth::Token(&person), &[])?;
    Ok(())
}
pub(crate) fn facts(out: &Path) -> Result<Vec<Fact>> {
    let made = case(out, "setup-user-create")?;
    let user = case(out, "setup-user-of")?;
    let again = case(out, "setup-user-create-again")?;
    let basic = case(out, "setup-user-person-basic")?;
    let token = case(out, "setup-user-person-token")?;
    let cleared = case(out, "setup-user-person-cleared")?;
    let admin = case(out, "setup-user-admin")?;
    let mut facts = Vec::new();
    for (name, holds) in [
        (
            "setup-create-user-answers-201-with-login-email-and-full-name",
            made.status == 201
                && made.body.root().get("login").is("bot")
                && made.body.root().get("email").is("bot@example.invalid")
                && made.body.root().get("full_name").is("kit:bot"),
        ),
        (
            "setup-created-user-is-no-site-admin-and-keeps-its-full-name",
            made.body.root().get("is_admin").bool() == Some(false) && made.body.root().get("full_name").is("kit:bot"),
        ),
        (
            "setup-create-user-again-is-refused-with-a-message",
            again.status >= 400 && again.body.root().get("message").str().is_some(),
        ),
        (
            "setup-user-of-a-login-reads-admin-flag-and-full-name",
            user.status == 200
                && user.body.root().get("login").is("bot")
                && user.body.root().get("is_admin").bool() == Some(false)
                && user.body.root().get("full_name").is("kit:bot"),
        ),
        ("setup-user-of-a-missing-login-is-404", case(out, "setup-user-missing")?.status == 404),
        (
            "setup-a-non-admin-token-is-refused-the-admin-routes",
            case(out, "setup-user-create-by-writer")?.status == 403,
        ),
        (
            "setup-the-admin-token-reads-user-as-a-site-admin",
            admin.status == 200 && admin.body.root().get("is_admin").bool() == Some(true),
        ),
        (
            "setup-a-user-made-to-change-its-password-is-refused-the-api-under-basic-auth",
            basic.status == 403
                && basic.body.root().get("message").str().is_some_and(|s| s.contains("change your password")),
        ),
        (
            "setup-a-user-made-to-change-its-password-is-refused-under-its-token-until-the-admin-clears-the-flag",
            token.status == 403
                && token.body.root().get("message").str().is_some_and(|s| s.contains("change your password"))
                && cleared.status == 200
                && cleared.body.root().get("login").is("person"),
        ),
    ] {
        add(&mut facts, name, holds, "captured status or members differ");
    }
    let mut whole = true;
    for name in [
        "user",
        "user-fixture",
        "setup-user-create",
        "setup-user-of",
        "setup-user-admin",
        "setup-user-create-person",
        "setup-user-person-cleared",
    ] {
        let row = case(out, name)?;
        whole &= row.body.root().get("is_admin").bool().is_some() && row.body.root().get("full_name").str().is_some();
    }
    add(&mut facts, "setup-every-recorded-user-carries-is-admin-and-full-name", whole, "user members absent");
    Ok(facts)
}

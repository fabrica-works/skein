use crate::json;
use crate::{
    Result,
    cases::Recorder,
    facts::Fact,
    fixture::Fixture,
    mask::Mask,
    write::{SecretKind, Secrets},
};
use skein_throwaway_forgejo::Token;
use std::path::Path;
pub(crate) const ADMIN: &[&str] = &["write:admin", "write:organization", "write:user"];
pub(crate) const ADOPT: &[&str] = &["write:admin", "read:user", "write:repository"];
pub(crate) fn scoped(f: &Fixture, secrets: &mut Secrets, login: &str, name: &str, scopes: &[&str]) -> Result<Token> {
    let row = f.api(
        "POST",
        &format!("/api/v1/admin/users/{login}/tokens"),
        &json::object(&[("name", &name), ("scopes", &scopes)])?,
    )?;
    let value = row.root().get("sha1").str().ok_or("token value missing")?;
    secrets.add(SecretKind::ApiToken, value);
    Ok(Token::new(value.to_owned()))
}
pub(crate) fn record(f: &Fixture, out: &Path, secrets: &mut Secrets) -> Result<()> {
    let admin = scoped(f, secrets, "fixture", "setup-admin", ADMIN)?;
    let mut mask = Mask::new(f.instance.port());
    let mut recorder = Recorder { f, out, secrets, mask: &mut mask };
    crate::users::record(&mut recorder, &admin)?;
    crate::tokens::record(&mut recorder, &admin)?;
    crate::orgs::record(&mut recorder, &admin)?;
    crate::hooks::record(&mut recorder, &admin)?;
    crate::repos::record(&mut recorder, &admin)?;
    let adopt = scoped(f, recorder.secrets, "fixture", "setup-adopt", ADOPT)?;
    crate::search::record(&mut recorder, &adopt)?;
    crate::repo_setup::record(&mut recorder, &adopt)?;
    crate::create_repository::record(&mut recorder)
}
pub(crate) fn facts(f: &Fixture, out: &Path) -> Result<Vec<Fact>> {
    let mut facts = crate::users::facts(out)?;
    facts.extend(crate::tokens::facts(out)?);
    facts.extend(crate::orgs::facts(f, out)?);
    facts.extend(crate::hooks::facts(f, out)?);
    facts.extend(crate::repos::facts(out)?);
    facts.extend(crate::search::facts(out)?);
    facts.extend(crate::repo_setup::facts(out, &f.owner.login)?);
    facts.extend(crate::create_repository::facts(out)?);
    Ok(facts)
}

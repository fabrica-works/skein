use crate::json;
use crate::{
    Result,
    fixture::{Fixture, R, encode},
    mask::{self, Mask},
    write::{self, SecretKind, Secrets},
};
use skein_throwaway_forgejo::{Exchange, Token};
use std::path::Path;
pub(crate) struct Recorder<'a> {
    pub(crate) f: &'a Fixture,
    pub(crate) out: &'a Path,
    pub(crate) mask: &'a mut Mask,
    pub(crate) secrets: &'a mut Secrets,
}
#[derive(Clone, Copy)]
pub(crate) enum Auth<'a> {
    Token(&'a Token),
    Basic(&'a str, &'a str),
}
impl Recorder<'_> {
    pub(crate) fn send_new(
        &mut self,
        name: &str,
        method: &str,
        target: &str,
        body: Option<&str>,
        auth: Auth<'_>,
        fresh: &[(SecretKind, &str)],
    ) -> Result<Exchange> {
        for (kind, value) in fresh {
            self.secrets.add(*kind, value);
        }
        let exchange = match auth {
            Auth::Token(token) => self.f.exchange(method, target, body.map(str::as_bytes), Some(token), None)?,
            Auth::Basic(login, secret) => {
                self.secrets.add(SecretKind::GitHeader, &crate::fixture::base64(&format!("{login}:{secret}")));
                self.f.exchange_basic(method, target, body.map(str::as_bytes), login, secret)?
            }
        };
        made_token(&exchange, self.mask, self.secrets);
        let request = mask::request(self.mask, &exchange);
        let response = mask::response(self.mask, &exchange);
        write::case(self.out, name, &request, &response, self.secrets)?;
        Ok(exchange)
    }

    pub(crate) fn send(
        &mut self,
        name: &str,
        method: &str,
        target: &str,
        body: Option<&str>,
        token: Option<&Token>,
        range: Option<skein_throwaway_forgejo::Range>,
    ) -> Result<Exchange> {
        let exchange = self.f.exchange(method, target, body.map(str::as_bytes), token, range)?;
        write::check(name, &exchange.sent, &exchange.body, self.secrets)?;
        let request = mask::request(self.mask, &exchange);
        let response = mask::response(self.mask, &exchange);
        write::case(self.out, name, &request, &response, self.secrets)?;
        Ok(exchange)
    }
    fn w(&mut self, name: &str, method: &str, target: &str, body: Option<&str>) -> Result<Exchange> {
        self.send(name, method, target, body, Some(&self.f.writer.token), None)
    }
}
struct Written {
    issue: u64,
    own: u64,
    own_head: String,
    remark: u64,
}
pub fn record(f: &Fixture, out: &Path, mask: &mut Mask, secrets: &mut Secrets) -> Result<()> {
    let mut r = Recorder { f, out, mask, secrets };
    start(&mut r)?;
    let written = writes(&mut r)?;
    reads(&mut r, &written)?;
    failures(&mut r, &written)
}
fn start(r: &mut Recorder<'_>) -> Result<()> {
    let f = r.f;
    let owner = &f.owner.token;
    let writer = &f.writer.token;
    for (name, target, token) in [
        ("user", "/api/v1/user".into(), writer),
        ("user-fixture", "/api/v1/user".into(), owner),
        ("settings-api", "/api/v1/settings/api".into(), writer),
        ("repository-by-id", format!("/api/v1/repositories/{}", f.repository), writer),
        ("repository-by-path", R.into(), writer),
        ("users-search-uid", format!("/api/v1/users/search?uid={}&limit=1", f.owner.id), writer),
    ] {
        r.send(name, "GET", &target, None, Some(token), None)?;
    }
    Ok(())
}
fn writes(r: &mut Recorder<'_>) -> Result<Written> {
    let f = r.f;
    let feature = f.feature;
    let head = &f.head;
    let issue = crate::json(&r.w(
        "create-issue",
        "POST",
        &format!("{R}/issues"),
        Some("{\"title\":\"fixture issue\",\"body\":\"<!-- kit:key 01000001 -->\\nissue text\"}"),
    )?)?
    .root()
    .get("number")
    .u64()
    .ok_or("issue number missing")?;
    let issue_route = format!("{R}/issues/{issue}");
    let pull_route = format!("{R}/pulls/{feature}");
    r.w(
        "post-comment",
        "POST",
        &format!("{issue_route}/comments"),
        Some("{\"body\":\"<!-- kit:key 01000002 -->\\ncomment text\"}"),
    )?;
    r.w("post-comment-plain", "POST", &format!("{issue_route}/comments"), Some("{\"body\":\"plain comment\"}"))?;
    r.w("edit-issue-title", "PATCH", &issue_route, Some("{\"title\":\"edited title\"}"))?;
    let own = crate::json(&r.w(
        "open-pull",
        "POST",
        &format!("{R}/pulls"),
        Some("{\"head\":\"writer-head\",\"base\":\"landing\",\"title\":\"writer pull\",\"body\":\"writer text\"}"),
    )?)?
    .root()
    .get("number")
    .u64()
    .ok_or("writer pull missing")?;
    for (name, event, key) in [
        ("post-review-approved", "APPROVED", "03"),
        ("post-review-request-changes", "REQUEST_CHANGES", "04"),
        ("post-review-comment", "COMMENT", "05"),
    ] {
        r.w(name, "POST", &format!("{pull_route}/reviews"), Some(std::str::from_utf8(&review_event(event, key)?)?))?;
    }
    r.w(
        "request-reviewers",
        "POST",
        &format!("{pull_route}/requested_reviewers"),
        Some("{\"reviewers\":[\"stranger\"]}"),
    )?;
    r.w(
        "remove-reviewers",
        "DELETE",
        &format!("{pull_route}/requested_reviewers"),
        Some("{\"reviewers\":[\"stranger\"]}"),
    )?;
    r.w(
        "post-status",
        "POST",
        &format!("{R}/statuses/{head}"),
        Some("{\"state\":\"success\",\"context\":\"fixture/test\"}"),
    )?;
    r.w(
        "post-status-pending",
        "POST",
        &format!("{R}/statuses/{head}"),
        Some("{\"state\":\"pending\",\"context\":\"fixture/other\"}"),
    )?;
    let branch_body =
        String::from_utf8(json::object(&[("new_branch_name", &"recorded"), ("old_ref_name", &head)])?.into_vec())?;
    r.w("create-branch", "POST", &format!("{R}/branches"), Some(&branch_body))?;
    r.w("create-branch-exists", "POST", &format!("{R}/branches"), Some(&branch_body))?;
    r.w("close-issue", "PATCH", &issue_route, Some("{\"state\":\"closed\"}"))?;
    r.w("reopen-issue", "PATCH", &issue_route, Some("{\"state\":\"open\"}"))?;
    f.ready(R, own, None)?;
    let previous =
        f.get(&format!("{R}/pulls/{own}"))?.root().get("head").get("sha").str().ok_or("old head missing")?.to_owned();
    r.w("update-pull", "POST", &format!("{R}/pulls/{own}/update?style=merge"), None)?;
    f.ready(R, own, Some(&previous))?;
    // A fresh pull avoids observing the update's old mergeability result.
    f.branch(R, "mergeable", "landing")?;
    f.put(R, "mergeable", "mergeable.txt", "mergeable\n")?;
    let merged = f.pull(R, "mergeable", "landing")?;
    let own = merged.root().get("number").u64().ok_or("merge pull missing")?;
    let own_head = merged.root().get("head").get("sha").str().ok_or("merge head missing")?.to_owned();
    f.ready(R, own, None)?;
    r.w(
        "merge-pull",
        "POST",
        &format!("{R}/pulls/{own}/merge"),
        Some(std::str::from_utf8(&json::object(&[("Do", &"merge"), ("head_commit_id", &own_head)])?)?),
    )?;
    r.w("delete-branch", "DELETE", &format!("{R}/branches/recorded"), None)?;
    let remark = f
        .api("POST", &format!("{pull_route}/reviews"), &crate::fixture::remark_review("remark review", "remark text")?)?
        .root()
        .get("id")
        .u64()
        .ok_or("remark review missing")?;
    Ok(Written { issue, own, own_head, remark })
}
fn review_event(event: &str, key: &str) -> Result<Box<[u8]>> {
    json::write(|out| {
        out.object_start();
        out.key(b"event");
        out.string(event.as_bytes());
        out.key(b"body");
        out.string(format!("<!-- kit:key 010000{key} -->\nreview text").as_bytes());
        out.object_end();
    })
}

fn reads(r: &mut Recorder<'_>, written: &Written) -> Result<()> {
    let f = r.f;
    let owner = &f.owner.token;
    let writer = &f.writer.token;
    let feature = f.feature;
    let head = &f.head;
    let Written { issue, own, remark, .. } = written;
    let since = encode("1970-01-01T00:00:00Z");
    let far = encode("2554-07-21T23:34:33Z");
    for (name, suffix) in [
        ("issues-page-1", format!("/issues?state=all&since={since}&sort=leastupdate&page=1&limit=2")),
        ("issues-page-2", format!("/issues?state=all&since={since}&sort=leastupdate&page=2&limit=2")),
        ("issues-type-issues", format!("/issues?state=all&type=issues&since={since}&sort=leastupdate&page=1&limit=2")),
        ("issues-type-pulls", format!("/issues?state=all&type=pulls&since={since}&sort=leastupdate&page=1&limit=2")),
        ("issues-since-2554", format!("/issues?state=all&since={far}&sort=leastupdate&page=1&limit=2")),
        ("issue", format!("/issues/{issue}")),
        ("issue-of-pull", format!("/issues/{feature}")),
        ("issue-comments", format!("/issues/{issue}/comments")),
        ("pull", format!("/pulls/{feature}")),
        ("pull-merged", format!("/pulls/{own}")),
        ("pull-for-encoded-head", "/pulls/main/feature%2Fslash".into()),
        ("combined-status", format!("/commits/{head}/status?page=1&limit=1")),
        ("combined-status-empty", format!("/commits/{}/status?page=1&limit=1", f.main)),
        ("statuses-page-2", format!("/commits/{head}/status?page=2&limit=2")),
        ("reviews", format!("/pulls/{feature}/reviews?page=1&limit=2")),
        ("review-comments", format!("/pulls/{feature}/reviews/{remark}/comments")),
        ("branch", "/branches/main".into()),
        ("branch-with-slash", "/branches/feature%2Fslash".into()),
        ("branches-page-1", "/branches?page=1&limit=2".into()),
        ("branches-page-2", "/branches?page=2&limit=2".into()),
        ("branch-protection", "/branch_protections/main".into()),
        ("branch-protections", "/branch_protections".into()),
        ("collaborators", "/collaborators?page=1&limit=2".into()),
        ("collaborator-permission", "/collaborators/writer/permission".into()),
        ("owner-permission", "/collaborators/fixture/permission".into()),
        ("pull-files-page-1", format!("/pulls/{feature}/files?page=1&limit=2")),
        ("pull-files-page-2", format!("/pulls/{feature}/files?page=2&limit=2")),
        ("compare", format!("/compare/{}...{head}", f.main)),
        ("compare-reverse", format!("/compare/{head}...{}", f.main)),
        ("raw-file", format!("/raw/file-0.txt?ref={head}")),
        ("raw-file-range", format!("/raw/file-0.txt?ref={head}")),
    ] {
        let token = if name.starts_with("branch-protection") { owner } else { writer };
        let range = (name == "raw-file-range").then_some(skein_throwaway_forgejo::Range::Head(3));
        r.send(name, "GET", &format!("{R}{suffix}"), None, Some(token), range)?;
    }
    Ok(())
}
fn failures(r: &mut Recorder<'_>, written: &Written) -> Result<()> {
    let f = r.f;
    let writer = &f.writer.token;
    let feature = f.feature;
    let head = &f.head;
    let own = written.own;
    let own_head = written.own_head.clone();
    r.send("wrong-token", "GET", "/api/v1/user", None, Some(&Token::new("0".repeat(40))), None)?;
    r.send("no-token", "GET", "/api/v1/user", None, None, None)?;
    for (name, target) in [
        ("missing-issue", format!("{R}/issues/9999")),
        ("raw-file-missing", format!("{R}/raw/missing.txt?ref={head}")),
        ("writer-branch-protection", format!("{R}/branch_protections/main")),
        ("writer-branch-protections", format!("{R}/branch_protections")),
        ("stranger-repository-by-path", "/api/v1/repos/fixture/private".into()),
        ("stranger-repository-by-id", format!("/api/v1/repositories/{}", f.private)),
        ("moved-repository", "/api/v1/repos/fixture/moved".into()),
    ] {
        r.send(
            name,
            "GET",
            &target,
            None,
            Some(if name.starts_with("stranger") { &f.stranger.token } else { writer }),
            None,
        )?;
    }
    r.w(
        "archived-write",
        "POST",
        "/api/v1/repos/fixture/archived/issues",
        Some("{\"title\":\"archived\",\"body\":\"text\"}"),
    )?;
    r.w(
        "open-pull-exists",
        "POST",
        &format!("{R}/pulls"),
        Some("{\"head\":\"feature/slash\",\"base\":\"main\",\"title\":\"exists\",\"body\":\"text\"}"),
    )?;
    f.branch(R, "no-changes", "main")?;
    r.w(
        "open-pull-no-changes",
        "POST",
        &format!("{R}/pulls"),
        Some("{\"head\":\"no-changes\",\"base\":\"main\",\"title\":\"empty\",\"body\":\"text\"}"),
    )?;
    r.w("create-issue-empty-title", "POST", &format!("{R}/issues"), Some("{\"title\":\"\",\"body\":\"text\"}"))?;
    f.branch(R, "stale", "landing")?;
    f.put(R, "stale", "stale.txt", "first\n")?;
    let stale = f.pull(R, "stale", "landing")?;
    let old = stale.root().get("head").get("sha").str().ok_or("stale head missing")?.to_owned();
    let number = stale.root().get("number").u64().ok_or("stale pull missing")?;
    f.put(R, "stale", "later.txt", "later\n")?;
    f.ready(R, number, Some(&old))?;
    for (name, repo, number, style, sha) in [
        ("merge-stale-head", R, number, "merge", old),
        ("merge-closed", R, own, "merge", own_head),
        (
            "merge-conflict",
            R,
            f.conflict,
            "merge",
            f.get(&format!("{R}/pulls/{}", f.conflict))?
                .root()
                .get("head")
                .get("sha")
                .str()
                .ok_or("conflict head missing")?
                .into(),
        ),
        ("merge-protected", R, feature, "merge", head.clone()),
        (
            "merge-style-refused",
            "/api/v1/repos/fixture/narrow",
            f.narrow,
            "squash",
            f.get(&format!("/api/v1/repos/fixture/narrow/pulls/{}", f.narrow))?
                .root()
                .get("head")
                .get("sha")
                .str()
                .ok_or("narrow head missing")?
                .into(),
        ),
    ] {
        r.w(
            name,
            "POST",
            &format!("{repo}/pulls/{number}/merge"),
            Some(std::str::from_utf8(&json::object(&[("Do", &style), ("head_commit_id", &sha)])?)?),
        )?;
    }
    r.w("update-conflict", "POST", &format!("{R}/pulls/{}/update?style=merge", f.conflict), None)?;
    r.w("delete-protected-branch", "DELETE", &format!("{R}/branches/main"), None)?;
    r.w("delete-missing-branch", "DELETE", &format!("{R}/branches/missing"), None)?;
    Ok(())
}

pub fn flags(f: &Fixture, out: &Path, mask: &mut Mask, secrets: &mut Secrets) -> Result<()> {
    let mut r = Recorder { f, out, mask, secrets };
    for (name, before, after, flags) in [
        ("compare-verification-false", &f.main, &f.head, "verification=false"),
        ("compare-files-false", &f.main, &f.head, "files=false&verification=false"),
        ("compare-reverse-files-false", &f.head, &f.main, "files=false&verification=false"),
    ] {
        r.w(name, "GET", &format!("{R}/compare/{before}...{after}?{flags}"), None)?;
    }
    Ok(())
}

pub(crate) fn made_token(exchange: &Exchange, mask: &mut Mask, secrets: &mut Secrets) {
    if (200..300).contains(&exchange.status)
        && let Ok(body) = crate::json::Json::parse(&exchange.body)
        && let Some(value) = body.root().get("sha1").str().filter(|s| !s.is_empty())
    {
        secrets.add(SecretKind::ApiToken, value);
        if let Some(tail) = body.root().get("token_last_eight").str().filter(|s| !s.is_empty()) {
            secrets.add(SecretKind::ApiToken, tail);
            mask.literal(tail);
        }
    }
}

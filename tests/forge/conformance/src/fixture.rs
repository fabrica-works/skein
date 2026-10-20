use crate::json;
use crate::{Result, failure, json::Json};
use skein_throwaway_forgejo::{Account, Exchange, Forgejo, Init, Instance, Outgoing, Settings, Token};
use std::path::Path;
pub const R: &str = "/api/v1/repos/fixture/specimen";
pub struct Fixture {
    pub instance: Instance,
    pub owner: Account,
    pub writer: Account,
    pub stranger: Account,
    pub repository: u64,
    pub private: u64,
    pub feature: u64,
    pub conflict: u64,
    pub narrow: u64,
    pub main: String,
    pub head: String,
}
impl Fixture {
    pub fn new(forgejo: &Forgejo, scratch: &Path) -> Result<Self> {
        let mut f = Self::bare(forgejo, scratch, &Settings::small())?;
        let mut ids = Vec::new();
        for name in ["specimen", "private", "archived", "moved", "narrow"] {
            ids.push(f.instance.create_repository(&f.owner, name, Init::Readme).map_err(|e| failure(&e))?.id);
        }
        for name in ["specimen", "archived", "narrow"] {
            f.instance.add_collaborator(&f.owner, name, &f.writer, "write").map_err(|e| failure(&e))?;
        }
        f.repository = ids[0];
        f.private = ids[1];
        f.api("PATCH", "/api/v1/repos/fixture/private", &json::object(&[("private", &true)])?)?;
        f.api("PATCH", "/api/v1/repos/fixture/archived", &json::object(&[("archived", &true)])?)?;
        f.api("PATCH", "/api/v1/repos/fixture/moved", &json::object(&[("name", &"renamed")])?)?;
        f.api("PATCH", "/api/v1/repos/fixture/narrow", &json::object(&[("allow_squash_merge", &false)])?)?;
        f.api("POST", &format!("{R}/labels"), &json::object(&[("color", &"abcdef"), ("name", &"kit / label")])?)?;
        f.put(R, "main", "conflict.txt", "original\n")?;
        f.main =
            f.get(&format!("{R}/branches/main"))?.root().get("commit").get("id").str().ok_or("main id missing")?.into();
        f.branch(R, "feature/slash", "main")?;
        for n in 0..3 {
            f.put(R, "feature/slash", &format!("file-{n}.txt"), &format!("file {n}\n"))?;
        }
        let feature = f.pull(R, "feature/slash", "main")?;
        f.feature = feature.root().get("number").u64().ok_or("pull number missing")?;
        f.head = feature.root().get("head").get("sha").str().ok_or("head missing")?.into();
        f.branch(R, "conflict", "main")?;
        f.put(R, "conflict", "conflict.txt", "head\n")?;
        f.conflict = f.pull(R, "conflict", "main")?.root().get("number").u64().ok_or("conflict pull missing")?;
        f.put(R, "main", "conflict.txt", "base\n")?;
        f.branch(R, "landing", "main")?;
        f.branch(R, "writer-head", "main")?;
        f.put(R, "writer-head", "writer.txt", "writer\n")?;
        f.put(R, "landing", "base.txt", "base\n")?;
        let narrow = "/api/v1/repos/fixture/narrow";
        f.branch(narrow, "feature", "main")?;
        f.put(narrow, "feature", "narrow.txt", "narrow\n")?;
        f.narrow = f.pull(narrow, "feature", "main")?.root().get("number").u64().ok_or("narrow pull missing")?;
        f.api(
            "POST",
            &format!("{R}/branch_protections"),
            &json::object(&[
                ("branch_name", &"main"),
                ("dismiss_stale_approvals", &true),
                ("enable_push", &false),
                ("enable_status_check", &true),
                ("required_approvals", &1u64),
                ("status_check_contexts", &["fixture/test"]),
            ])?,
        )?;
        Ok(f)
    }
    pub fn bare(forgejo: &Forgejo, scratch: &Path, settings: &Settings) -> Result<Self> {
        let mut instance = forgejo.start(scratch, settings).map_err(|e| failure(&e))?;
        let owner = instance.admin().clone();
        let writer = instance.create_user("writer").map_err(|e| failure(&e))?;
        let stranger = instance.create_user("stranger").map_err(|e| failure(&e))?;
        Ok(Self {
            instance,
            owner,
            writer,
            stranger,
            repository: 0,
            private: 0,
            feature: 0,
            conflict: 0,
            narrow: 0,
            main: String::new(),
            head: String::new(),
        })
    }
    pub fn exchange(
        &self,
        method: &str,
        target: &str,
        body: Option<&[u8]>,
        token: Option<&Token>,
        range: Option<skein_throwaway_forgejo::Range>,
    ) -> Result<Exchange> {
        self.instance.exchange(&Outgoing { method, target, token, body, range }).map_err(|e| failure(&e))
    }
    pub fn exchange_basic(
        &self,
        method: &str,
        target: &str,
        body: Option<&[u8]>,
        login: &str,
        secret: &str,
    ) -> Result<Exchange> {
        self.instance
            .exchange_basic(&Outgoing { method, target, body, token: None, range: None }, login, secret)
            .map_err(|e| failure(&e))
    }
    pub fn api(&self, method: &str, target: &str, body: &[u8]) -> Result<Json> {
        let response = self.exchange(method, target, Some(body), Some(&self.owner.token), None)?;
        api_answer(target, &response)
    }
    pub fn get(&self, target: &str) -> Result<Json> {
        crate::json(&self.exchange("GET", target, None, Some(&self.owner.token), None)?)
    }
    pub fn branch(&self, repo: &str, name: &str, from: &str) -> Result<Json> {
        self.api(
            "POST",
            &format!("{repo}/branches"),
            &json::object(&[("new_branch_name", &name), ("old_ref_name", &from)])?,
        )
    }
    pub fn pull(&self, repo: &str, head: &str, base: &str) -> Result<Json> {
        let target = format!("{repo}/pulls");
        let body = json::object(&[("base", &base), ("body", &"fixture pull"), ("head", &head), ("title", &head)])?;
        for attempt in 0..=3 {
            let response = self.exchange("POST", &target, Some(&body), Some(&self.owner.token), None)?;
            eprintln!(
                "provisioning {target}, attempt {}: HTTP {}: {}",
                attempt + 1,
                response.status,
                body_preview(&response)
            );
            if response.status != 500 || attempt == 3 {
                return api_answer(&target, &response);
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        unreachable!("the last attempt returns")
    }
    pub fn put(&self, repo: &str, branch: &str, path: &str, content: &str) -> Result<Json> {
        let route = format!("{repo}/contents/{path}");
        let old = self.get(&format!("{route}?ref={}", encode(branch)))?;
        let sha = old.root().get("sha").str();
        let content = base64(content);
        let body = if let Some(sha) = sha {
            json::object(&[("branch", &branch), ("content", &content), ("message", &path), ("sha", &sha)])?
        } else {
            json::object(&[("branch", &branch), ("content", &content), ("message", &path)])?
        };
        let method = if sha.is_some() { "PUT" } else { "POST" };
        self.api(method, &route, &body)
    }
    pub fn ready(&self, repo: &str, pull: u64, previous: Option<&str>) -> Result<()> {
        for _ in 0..100 {
            let row = self.get(&format!("{repo}/pulls/{pull}"))?;
            if row.root().get("mergeable").bool() == Some(true)
                && previous.is_none_or(|old| !row.root().get("head").get("sha").is(old))
            {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err("pull did not become mergeable".into())
    }
}
pub(crate) fn remark_review(body: &str, remark: &str) -> Result<Box<[u8]>> {
    json::write(|out| {
        out.object_start();
        out.key(b"body");
        out.string(body.as_bytes());
        out.key(b"comments");
        out.array_start();
        out.object_start();
        out.key(b"body");
        out.string(remark.as_bytes());
        out.key(b"new_position");
        out.unsigned(1);
        out.key(b"path");
        out.string(b"file-0.txt");
        out.object_end();
        out.array_end();
        out.key(b"event");
        out.string(b"COMMENT");
        out.object_end();
    })
}

pub(crate) fn api_answer(target: &str, response: &Exchange) -> Result<Json> {
    if !(200..300).contains(&response.status) {
        return Err(format!("provisioning {target}: HTTP {}: {}", response.status, body_preview(response)).into());
    }
    if response.body.is_empty() { Json::parse(b"null") } else { crate::json(response) }
}
fn body_preview(response: &Exchange) -> String {
    String::from_utf8_lossy(&response.body[..response.body.len().min(200)]).into_owned()
}
pub fn encode(text: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            write!(out, "%{b:02X}").expect("string write");
        }
    }
    out
}
pub fn base64(text: &str) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        for n in [a >> 2, ((a & 3) << 4) | (b >> 4), ((b & 15) << 2) | (c >> 6), c & 63] {
            out.push(char::from(TABLE[usize::from(n)]));
        }
        if chunk.len() < 3 {
            out.pop();
            out.push('=');
        }
        if chunk.len() < 2 {
            out.pop();
            out.pop();
            out.push('=');
            out.push('=');
        }
    }
    out
}

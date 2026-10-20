use crate::json;
use crate::json::Json;
use crate::{
    Result, failure,
    fixture::{Fixture, R, encode},
};
use skein_throwaway_forgejo::{Forgejo, Outgoing, Settings};
use std::{fs, path::Path, time::Duration};
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Holds,
    Observed(String),
    Fails(String),
    Unchecked(String),
}
#[derive(Debug)]
pub struct Fact {
    pub name: &'static str,
    pub outcome: Outcome,
}
impl Fact {
    #[must_use]
    pub fn line(&self) -> String {
        match &self.outcome {
            Outcome::Holds => format!("holds {}\n", self.name),
            Outcome::Observed(seen) => {
                format!("holds {}\n# {}: {}\n", self.name, self.name, seen.replace(['\r', '\n'], " "))
            }
            Outcome::Fails(why) => format!("fails {}: {}\n", self.name, why.replace(['\r', '\n'], " ")),
            Outcome::Unchecked(why) => format!("unchecked {}: {}\n", self.name, why.replace(['\r', '\n'], " ")),
        }
    }
}
/// # Errors
/// Returns a file write failure.
pub fn write(out: &Path, facts: &[Fact]) -> Result<()> {
    fs::write(out.join("facts.txt"), facts.iter().map(Fact::line).collect::<String>())?;
    Ok(())
}
pub fn add(facts: &mut Vec<Fact>, name: &'static str, holds: bool, seen: &str) {
    facts.push(Fact { name, outcome: if holds { Outcome::Holds } else { Outcome::Fails(seen.into()) } });
}
const DIAGNOSTIC_BYTES: usize = 4096;
pub(crate) fn diagnostic(body: &[u8]) -> String {
    let end = body.len().min(DIAGNOSTIC_BYTES);
    let mut text = String::from_utf8_lossy(&body[..end]).into_owned();
    if end < body.len() {
        use std::fmt::Write;
        write!(text, "… ({} more bytes)", body.len() - end).expect("string write");
    }
    text
}
pub(crate) struct Case {
    pub(crate) status: u16,
    pub(crate) body: Json,
    raw: Vec<u8>,
    head: String,
}
pub(crate) fn case(out: &Path, name: &str) -> Result<Case> {
    let bytes = fs::read(out.join("transcripts").join(name).join("response"))?;
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").ok_or("case head missing")?;
    let head = std::str::from_utf8(&bytes[..end])?.to_owned();
    let raw = bytes[end + 4..].to_vec();
    let status = head.split_whitespace().nth(1).ok_or("case status missing")?.parse()?;
    Ok(Case { status, body: Json::parse(&raw).or_else(|_| Json::parse(b"null"))?, raw, head })
}
#[must_use]
pub fn matches(exchange: &skein_throwaway_forgejo::Exchange, status: u16, message: &str) -> bool {
    exchange.status == status && Json::parse(&exchange.body).is_ok_and(|body| body.root().get("message").is(message))
}
/// # Errors
/// Returns an exchange, provisioning or transcript-read failure.
pub fn evaluate(forgejo: &Forgejo, f: &Fixture, out: &Path) -> Result<Vec<Fact>> {
    let mut facts = Vec::new();
    initial(forgejo, f, out, &mut facts)?;
    listing(f, out, &mut facts)?;
    let changed = updated(f, &mut facts)?;
    bodies(f, out, &changed, &mut facts)?;
    pulls(f, out, &mut facts)?;
    orders(f, out, &mut facts)?;
    errors(out, &mut facts)?;
    for (name, token) in [
        ("effective-protection-name-for-a-writer", &f.writer.token),
        ("effective-protection-name-for-an-admin", &f.owner.token),
    ] {
        let row = crate::json(&f.exchange("GET", &format!("{R}/branches/main"), None, Some(token), None)?)?;
        add(
            &mut facts,
            name,
            row.root().get("effective_branch_protection_name").is("main"),
            &format!("effective_branch_protection_name={:?}", row.root().get("effective_branch_protection_name").str()),
        );
    }
    let rate = fs::read_dir(out.join("transcripts"))?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| fs::read(e.path().join("response")))
        .collect::<std::io::Result<Vec<_>>>()?;
    add(
        &mut facts,
        "no-rate-limit-headers",
        rate.iter().all(|bytes| {
            !String::from_utf8_lossy(bytes).lines().any(|line| line.to_ascii_lowercase().starts_with("x-ratelimit-"))
        }),
        "X-RateLimit header present",
    );
    Ok(facts)
}
fn initial(forgejo: &Forgejo, f: &Fixture, out: &Path, facts: &mut Vec<Fact>) -> Result<()> {
    let mut settings = Settings::small();
    settings.max_response_items = None;
    settings.actions = false;
    let instance = forgejo.start(&f.instance.scratch().join("default-api"), &settings).map_err(|e| failure(&e))?;
    let response = instance.exchange(&Outgoing::get("/api/v1/settings/api", None)).map_err(|e| failure(&e))?;
    let defaults = crate::json(&response)?;
    add(
        facts,
        "api-default-page-cap-50-and-30",
        defaults.root().get("max_response_items").u64() == Some(50)
            && defaults.root().get("default_paging_num").u64() == Some(30),
        &diagnostic(&response.body),
    );
    instance.stop().map_err(|e| failure(&e))?;
    for (name, recorded, status, message) in [
        ("pull-for-accepts-encoded-slash", "pull-for-encoded-head", 200, ""),
        ("update-conflict-is-409-merge-failed", "update-conflict", 409, "merge failed because of conflict"),
        ("merge-old-head-is-409-head-out-of-date", "merge-stale-head", 409, "head out of date"),
        ("merge-answers-200-with-no-body", "merge-pull", 200, ""),
    ] {
        let c = case(out, recorded)?;
        add(
            facts,
            name,
            c.status == status
                && (message.is_empty() || c.body.root().get("message").is(message))
                && (recorded != "merge-pull" || c.raw.is_empty()),
            &format!("HTTP {}: {:?}", c.status, c.body.root().get("message").str()),
        );
    }
    let first = case(out, "pull-files-page-1")?;
    let second = case(out, "pull-files-page-2")?;
    add(
        facts,
        "pull-files-page-with-x-hasmore",
        first.body.root().array().is_some_and(|a| a.len() == 2)
            && second.body.root().array().is_some_and(|a| a.len() == 1)
            && first.head.to_ascii_lowercase().contains("x-hasmore: true")
            && second.head.to_ascii_lowercase().contains("x-hasmore: false"),
        &format!("{}; {}", first.head, second.head),
    );
    let compare = f.get(&format!("{R}/compare/{}...{}?page=1&limit=1", f.main, f.head))?;
    add(
        facts,
        "compare-ignores-page-and-limit",
        compare.root().get("files").array().is_some_and(|a| a.len() == 3)
            && compare.root().get("commits").array().is_some_and(|a| a.len() == 3),
        &format!(
            "files={}, commits={}",
            compare.root().get("files").array().map_or(0, |rows| rows.len()),
            compare.root().get("commits").array().map_or(0, |rows| rows.len())
        ),
    );
    let made = case(out, "create-branch")?;
    let pull = case(out, "pull")?;
    add(
        facts,
        "create-branch-at-full-commit-id",
        made.status == 201
            && made.body.root().get("commit").get("id").str() == pull.body.root().get("head").get("sha").str(),
        &format!("{:?}", made.body.root().get("commit").get("id").str()),
    );
    add(
        facts,
        "writer-protection-reads-are-403",
        case(out, "writer-branch-protection")?.status == 403 && case(out, "writer-branch-protections")?.status == 403,
        "writer protection read was not 403",
    );
    Ok(())
}
fn listing(f: &Fixture, out: &Path, facts: &mut Vec<Fact>) -> Result<()> {
    let mut pages = Vec::new();
    for page in 1..20 {
        let read = f.get(&format!(
            "{R}/issues?state=all&since=1970-01-01T00%3A00%3A00Z&sort=leastupdate&page={page}&limit=2"
        ))?;
        let length = read.root().array().ok_or("issue page missing")?.len();
        pages.push(read);
        if length < 2 {
            break;
        }
    }
    let rows: Vec<_> = pages.iter().flat_map(|page| page.root().items()).collect();
    add(
        facts,
        "leastupdate-orders-least-recently-updated-first",
        rows.windows(2).all(|r| r[0].get("updated_at").str() <= r[1].get("updated_at").str()),
        &rows.iter().map(|r| format!("{:?}", r.get("updated_at").str())).collect::<Vec<_>>().join(","),
    );
    add(
        facts,
        "leastupdate-ties-ordered-by-number",
        rows.windows(2).all(|r| {
            r[0].get("updated_at").str() != r[1].get("updated_at").str()
                || r[0].get("number").u64() < r[1].get("number").u64()
        }),
        "equal timestamps did not sort by number",
    );
    let at = utc(rows[0].get("updated_at").str().ok_or("updated time missing")?)?;
    let response = f.exchange(
        "GET",
        &format!("{R}/issues?state=all&since={}&sort=leastupdate&page=1&limit=2", encode(&at)),
        None,
        Some(&f.owner.token),
        None,
    )?;
    let since = crate::json(&response)?;
    add(
        facts,
        "since-is-inclusive-at-the-second",
        since.root().array().is_some_and(|a| a.iter().any(|r| r.get("number").u64() == rows[0].get("number").u64())),
        &diagnostic(&response.body),
    );
    let far = case(out, "issues-since-2554")?;
    add(
        facts,
        "since-in-2554-answers-an-empty-page-with-date",
        far.status == 200
            && far.body.root().array().is_some_and(|a| a.is_empty())
            && far.head.lines().any(|l| l.starts_with("Date:")),
        &format!("HTTP {} {}", far.status, diagnostic(&far.raw)),
    );
    Ok(())
}
fn utc(time: &str) -> Result<String> {
    let mut year: u32 = time[..4].parse()?;
    let mut month: usize = time[5..7].parse()?;
    let mut day: u32 = time[8..10].parse()?;
    let hour: u32 = time[11..13].parse()?;
    let minute: u32 = time[14..16].parse()?;
    let mut minutes = hour * 60 + minute;
    if minutes < 330 {
        minutes += 1440;
        if day == 1 {
            if month == 1 {
                year -= 1;
                month = 12;
            } else {
                month -= 1;
            }
            day = [
                31,
                if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ][month - 1];
        } else {
            day -= 1;
        }
    }
    minutes -= 330;
    Ok(format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{}Z", minutes / 60, minutes % 60, &time[17..19]))
}
fn pause() {
    std::thread::sleep(Duration::from_millis(1100));
}
fn updated(f: &Fixture, facts: &mut Vec<Fact>) -> Result<(Json, Json)> {
    let issue = f
        .api("POST", &format!("{R}/issues"), &json::object(&[("body", &"initial"), ("title", &"time facts")])?)?
        .root()
        .get("number")
        .u64()
        .ok_or("time issue missing")?;
    let route = format!("{R}/issues/{issue}");
    let before = f.get(&route)?;
    pause();
    let original = f.api("POST", &format!("{route}/comments"), &json::object(&[("body", &"original")])?)?;
    let posted = f.get(&route)?;
    add(
        facts,
        "updated-moves-on-comment",
        posted.root().get("updated_at").str() != before.root().get("updated_at").str(),
        &format!("{:?}", posted.root().get("updated_at").str()),
    );
    pause();
    let comment = format!("{R}/issues/comments/{}", original.root().get("id").u64().ok_or("comment id missing")?);
    let edited = f.api("PATCH", &comment, &json::object(&[("body", &"edited")])?)?;
    add(
        facts,
        "updated-stays-on-comment-edit",
        f.get(&route)?.root().get("updated_at").str() == posted.root().get("updated_at").str(),
        &format!("{:?}", f.get(&route)?.root().get("updated_at").str()),
    );
    pause();
    f.api("DELETE", &comment, &json::write(skein_json::writer::Encoder::null)?)?;
    add(
        facts,
        "updated-stays-on-comment-delete",
        f.get(&route)?.root().get("updated_at").str() == posted.root().get("updated_at").str(),
        &format!("{:?}", f.get(&route)?.root().get("updated_at").str()),
    );
    f.branch(R, "facts-base", "landing")?;
    f.branch(R, "facts-head", "facts-base")?;
    f.put(R, "facts-head", "facts.txt", "first\n")?;
    let pull = f.pull(R, "facts-head", "facts-base")?.root().get("number").u64().ok_or("facts pull missing")?;
    let route = format!("{R}/issues/{pull}");
    for (name, operation) in [
        ("updated-moves-on-review", 0),
        ("updated-moves-on-head-push", 1),
        ("updated-stays-on-head-status", 2),
        ("updated-stays-on-base-push", 3),
        ("updated-moves-on-close", 4),
        ("updated-moves-on-reopen", 5),
    ] {
        let before = f.get(&route)?;
        pause();
        match operation {
            0 => {
                f.api(
                    "POST",
                    &format!("{R}/pulls/{pull}/reviews"),
                    &json::object(&[("body", &"time review"), ("event", &"COMMENT")])?,
                )?;
            }
            1 => {
                f.put(R, "facts-head", "facts-new.txt", "new head\n")?;
            }
            2 => time_status(f)?,
            3 => {
                f.put(R, "facts-base", "facts-base.txt", "new base\n")?;
            }
            4 => {
                f.api("PATCH", &route, &json::object(&[("state", &"closed")])?)?;
            }
            5 => {
                f.api("PATCH", &route, &json::object(&[("state", &"open")])?)?;
            }
            _ => return Err("unknown time operation".into()),
        }
        let mut after = f.get(&route)?;
        // Head pushes update the issue through Forgejo's background queue.
        for _ in 0..100 {
            if operation != 1 || after.root().get("updated_at").str() != before.root().get("updated_at").str() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
            after = f.get(&route)?;
        }
        add(
            facts,
            name,
            (after.root().get("updated_at").str() != before.root().get("updated_at").str())
                == (operation != 2 && operation != 3),
            &format!("{:?} -> {:?}", before.root().get("updated_at").str(), after.root().get("updated_at").str()),
        );
    }
    Ok((original, edited))
}
fn time_status(f: &Fixture) -> Result<()> {
    let head = f.get(&format!("{R}/branches/facts-head"))?;
    let head = head.root().get("commit").get("id").str().ok_or("fact head missing")?;
    f.api(
        "POST",
        &format!("{R}/statuses/{head}"),
        &json::object(&[("context", &"time/test"), ("state", &"success")])?,
    )?;
    Ok(())
}

fn bodies(f: &Fixture, out: &Path, changed: &(Json, Json), facts: &mut Vec<Fact>) -> Result<()> {
    // The existing largest ASCII x string (1,048,576 bytes) plus the complete
    // sorted object's {"body":"","title":"body fact"} framing (31 bytes).
    const LARGE_FACT_REQUEST_BYTES: u32 = 1_048_607;
    let comment = case(out, "post-comment")?;
    let review = case(out, "post-review-approved")?;
    add(
        facts,
        "unedited-comment-updated-equals-created",
        comment.body.root().get("updated_at").str() == comment.body.root().get("created_at").str(),
        &format!("{:?}", comment.body.root().get("updated_at").str()),
    );
    add(
        facts,
        "unedited-review-updated-equals-submitted",
        review.body.root().get("updated_at").str() == review.body.root().get("submitted_at").str(),
        &format!("{:?}", review.body.root().get("updated_at").str()),
    );
    add(
        facts,
        "edit-moves-comment-updated",
        changed.0.root().get("updated_at").str() != changed.1.root().get("updated_at").str(),
        &format!("{:?}", changed.1.root().get("updated_at").str()),
    );
    add(
        facts,
        "marker-line-feed-kept",
        comment.body.root().get("body").is("<!-- kit:key 01000002 -->\ncomment text"),
        &format!("{:?}", comment.body.root().get("body").str()),
    );
    for (name, size) in [("body-of-65536-bytes-kept-whole", 65536), ("body-of-1048576-bytes-kept-whole", 1_048_576)] {
        let text = "x".repeat(size);
        let body = json::write_bounded(LARGE_FACT_REQUEST_BYTES, |out| {
            out.object_start();
            out.key(b"body");
            out.string(text.as_bytes());
            out.key(b"title");
            out.string(b"body fact");
            out.object_end();
        })?;
        let made = f.api("POST", &format!("{R}/issues"), &body)?;
        let read = f.get(&format!("{R}/issues/{}", made.root().get("number").u64().ok_or("issue number missing")?))?;
        add(
            facts,
            name,
            read.root().get("body").str() == Some(text.as_str()),
            &format!("{} bytes", read.root().get("body").str().unwrap_or_default().len()),
        );
    }
    Ok(())
}
fn pulls(f: &Fixture, out: &Path, facts: &mut Vec<Fact>) -> Result<()> {
    let edited = f.api(
        "PATCH",
        &format!("{R}/issues/{}", f.feature),
        &json::object(&[("body", &"pull edited body"), ("title", &"pull edited")])?,
    )?;
    add(
        facts,
        "patch-issue-edits-a-pull-request",
        edited.root().get("title").is("pull edited")
            && f.get(&format!("{R}/pulls/{}", f.feature))?.root().get("body").is("pull edited body"),
        &format!("{:?}", edited.root().get("title").str()),
    );
    f.branch(R, "repeat-head", "landing")?;
    f.put(R, "repeat-head", "repeat.txt", "repeat\n")?;
    let first = f.pull(R, "repeat-head", "landing")?;
    f.api(
        "PATCH",
        &format!("{R}/issues/{}", first.root().get("number").u64().ok_or("pull number missing")?),
        &json::object(&[("state", &"closed")])?,
    )?;
    let newest = f.pull(R, "repeat-head", "landing")?;
    let found = f.get(&format!("{R}/pulls/landing/repeat-head"))?;
    add(
        facts,
        "pull-for-names-the-newest-of-several",
        found.root().get("number").u64() == newest.root().get("number").u64(),
        &format!("{:?}", found.root().get("number").u64()),
    );
    add(
        facts,
        "branch-path-takes-encoded-slash",
        case(out, "branch-with-slash")?.status == 200,
        "branch with encoded slash did not answer 200",
    );
    let pull = f.get(&format!("{R}/pulls/{}", f.feature))?;
    let base = f.get(&format!("{R}/branches/main"))?;
    add(
        facts,
        "base-sha-is-the-base-tip",
        pull.root().get("base").get("sha").str() == base.root().get("commit").get("id").str(),
        &format!("{:?}", pull.root().get("base").get("sha").str()),
    );
    let empty = case(out, "combined-status-empty")?;
    add(
        facts,
        "combined-status-without-status-counts-0",
        empty.body.root().get("total_count").u64() == Some(0),
        &format!("{:?}", empty.body.root().get("total_count").u64()),
    );
    let head = newest.root().get("head").get("sha").str().ok_or("status head missing")?;
    for (state, context) in [("skipped", "skip/test"), ("success", "pass/test")] {
        f.api("POST", &format!("{R}/statuses/{head}"), &json::object(&[("context", &context), ("state", &state)])?)?;
    }
    let combined = f.get(&format!("{R}/commits/{head}/status?page=1&limit=2"))?;
    add(
        facts,
        "skipped-and-success-combine-as-passed",
        ["success", "warning", "skipped"].iter().any(|s| combined.root().get("state").is(s)),
        &format!("{:?}", combined.root().get("state").str()),
    );
    Ok(())
}
fn orders(f: &Fixture, out: &Path, facts: &mut Vec<Fact>) -> Result<()> {
    for (name, recorded) in [("comments-come-oldest-first", "issue-comments"), ("reviews-come-oldest-first", "reviews")]
    {
        let read = case(out, recorded)?;
        let rows = read.body.root().array().ok_or("ordered rows missing")?;
        add(
            facts,
            name,
            rows.windows(2).all(|a| a[0].get("id").u64() < a[1].get("id").u64()),
            &rows.iter().map(|r| format!("{:?}", r.get("id").u64())).collect::<Vec<_>>().join(","),
        );
    }
    let route = format!("{R}/pulls/{}/reviews", f.feature);
    let response = f.exchange(
        "POST",
        &route,
        Some(b"{\"event\":\"APPROVED\",\"body\":\"dismissed body\"}"),
        Some(&f.writer.token),
        None,
    )?;
    let review = crate::json(&response)?;
    let id = review.root().get("id").u64().ok_or("dismissal review missing")?;
    f.api("POST", &format!("{route}/{id}/dismissals"), &json::object(&[("message", &"dismiss fact")])?)?;
    let dismissed = f.get(&format!("{route}/{id}"))?;
    add(
        facts,
        "dismissed-review-keeps-its-body",
        dismissed.root().get("body").is("dismissed body") && dismissed.root().get("dismissed").bool() == Some(true),
        &format!("{:?} {:?}", dismissed.root().get("state").str(), dismissed.root().get("body").str()),
    );
    let remarks = case(out, "review-comments")?;
    add(
        facts,
        "remark-position-is-its-line",
        remarks.body.root().nth(0).get("position").u64() == Some(1),
        &format!("{:?}", remarks.body.root().nth(0).get("position").u64()),
    );
    add(facts, "raw-range-answers-206", case(out, "raw-file-range")?.status == 206, "raw Range did not answer 206");
    Ok(())
}
fn errors(out: &Path, facts: &mut Vec<Fact>) -> Result<()> {
    for (name, recorded, expected) in [
        ("merge-closed-is-405", "merge-closed", 405),
        ("merge-conflict-is-405", "merge-conflict", 405),
        ("merge-protected-is-405", "merge-protected", 405),
        ("merge-style-is-405", "merge-style-refused", 405),
        ("open-pull-exists-is-409", "open-pull-exists", 409),
        ("open-pull-no-changes-is-422", "open-pull-no-changes", 422),
        ("empty-title-is-422", "create-issue-empty-title", 422),
        ("moved-repository-is-a-redirect", "moved-repository", 301),
        ("archived-write-is-423", "archived-write", 423),
        ("delete-protected-branch-is-403", "delete-protected-branch", 403),
        ("delete-missing-branch-is-404", "delete-missing-branch", 404),
        ("create-branch-exists-is-409", "create-branch-exists", 409),
        ("unreadable-repository-by-path-is-404", "stranger-repository-by-path", 404),
        ("unreadable-repository-by-id-is-404", "stranger-repository-by-id", 404),
    ] {
        let read = case(out, recorded)?;
        add(
            facts,
            name,
            if expected == 301 { (300..400).contains(&read.status) } else { read.status == expected },
            &format!("HTTP {}: {:?}", read.status, read.body.root().get("message").str()),
        );
    }
    Ok(())
}
/// Replaces only facts whose names begin with `prefix`, preserving every other line.
#[must_use]
pub fn replace_block(text: &str, prefix: &str, facts: &[Fact]) -> String {
    let mut result = String::new();
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        let mut words = line.split_whitespace();
        let component = matches!(words.next(), Some("holds" | "fails" | "unchecked"))
            && words.next().is_some_and(|name| name.starts_with(prefix));
        if component {
            if !replaced {
                result.extend(facts.iter().map(Fact::line));
                replaced = true;
            }
        } else {
            result.push_str(line);
        }
    }
    if !replaced {
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.extend(facts.iter().map(Fact::line));
    }
    result
}

/// # Errors
/// Returns a transcript-read failure.
pub fn flags(out: &Path) -> Result<Vec<Fact>> {
    let plain = case(out, "compare")?.body;
    let reverse = case(out, "compare-reverse")?.body;
    let verification = case(out, "compare-verification-false")?;
    let files = case(out, "compare-files-false")?;
    let backwards = case(out, "compare-reverse-files-false")?;
    let shas = |body: &Json| {
        body.root()
            .get("commits")
            .array()
            .map(|rows| rows.iter().map(|row| row.get("sha").str().map(str::to_owned)).collect::<Vec<_>>())
    };
    let names = |body: &Json| {
        body.root()
            .get("files")
            .array()
            .map(|rows| rows.iter().map(|row| row.get("filename").str().map(str::to_owned)).collect::<Vec<_>>())
    };
    let commits = files.body.root().get("commits");
    let no_files = files.status == 200
        && commits.array().is_some_and(|rows| {
            !rows.is_empty()
                && rows.iter().all(|row| row.get("files").kind().is_none_or(|kind| kind == skein_json::Kind::Null))
        });
    let empty = files.body.root().get("files").array().is_some_and(|rows| rows.is_empty())
        && backwards.body.root().get("files").array().is_some_and(|rows| rows.is_empty());
    let verified = verification.status == 200
        && verification.body.root().get("total_commits").u64() == plain.root().get("total_commits").u64()
        && shas(&verification.body) == shas(&plain)
        && names(&verification.body) == names(&plain);
    let kept = files.body.root().get("total_commits").u64() == plain.root().get("total_commits").u64()
        && shas(&files.body) == shas(&plain)
        && backwards.body.root().get("total_commits").u64() == reverse.root().get("total_commits").u64();
    let seen = format!(
        "Forward {}, reverse {}, verification {}",
        diagnostic(&files.raw),
        diagnostic(&backwards.raw),
        diagnostic(&verification.raw)
    );
    let mut facts = Vec::new();
    for (name, holds) in [
        ("compare-files-false-lists-no-commits-files", no_files),
        ("compare-files-false-keeps-its-own-files-empty", empty),
        ("compare-verification-false-changes-nothing-kept", verified),
        ("compare-files-false-keeps-total-commits-and-shas", kept),
    ] {
        add(&mut facts, name, holds, &seen);
    }
    Ok(facts)
}

#[cfg(test)]
#[test]
fn a_diagnostic_is_the_bodys_bytes_cut_at_its_bound_and_marked() {
    let body = vec![b'x'; DIAGNOSTIC_BYTES];
    assert_eq!(diagnostic(&body), "x".repeat(DIAGNOSTIC_BYTES));
    let mut over = body;
    over.push(b'y');
    assert_eq!(diagnostic(&over), format!("{}… (1 more bytes)", "x".repeat(DIAGNOSTIC_BYTES)));
}

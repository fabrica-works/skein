use crate::json;
use crate::json::Json;
use crate::{
    Result,
    facts::{self, Fact, Outcome},
    failure,
    fixture::Fixture,
    mask::{self, Mask},
    runner,
    write::{self, SecretKind, Secrets},
};
use skein_throwaway_forgejo::{Exchange, Forgejo, Settings};
#[expect(clippy::disallowed_types, reason = "the recorder probes real deliveries against a wall clock deadline")]
use std::time::Instant;
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
const REPO: &str = "/api/v1/repos/hooked/specimen";
const SECRET: &str = "kit-conformance-hook-secret";
const EVENTS: [&str; 22] = [
    "issues",
    "issue_assign",
    "issue_label",
    "issue_milestone",
    "pull_request",
    "pull_request_assign",
    "pull_request_label",
    "pull_request_milestone",
    "pull_request_sync",
    "pull_request_review_request",
    "issue_comment",
    "pull_request_comment",
    "pull_request_review_approved",
    "pull_request_review_rejected",
    "pull_request_review_comment",
    "push",
    "create",
    "delete",
    "workflow_job_success",
    "workflow_job_failure",
    "workflow_job_cancelled",
    "workflow_job_skipped",
];
pub(crate) struct Received {
    pub(crate) head: Vec<u8>,
    pub(crate) body: Vec<u8>,
}
impl Received {
    fn header(&self, name: &str) -> &str {
        std::str::from_utf8(&self.head)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map_or("", |(_, value)| value.trim())
    }
}
pub(crate) struct Reply(Sender<u16>);
impl Reply {
    pub(crate) fn answer(self, status: u16) {
        let _answered = self.0.send(status);
    }
}
pub(crate) struct Listener {
    pub(crate) port: u16,
    received: Receiver<(Received, Reply)>,
}
impl Listener {
    #[expect(
        clippy::disallowed_methods,
        reason = "the recording listener holds each real connection on its own thread"
    )]
    pub(crate) fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let (tx, received) = mpsc::channel();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                thread::spawn(move || {
                    let _handled = receive(stream, &tx);
                });
            }
        });
        Ok(Self { port, received })
    }
    pub(crate) fn next(&self, within: Duration) -> Option<(Received, Reply)> {
        self.received.recv_timeout(within).ok()
    }
}
fn receive(mut stream: TcpStream, tx: &Sender<(Received, Reply)>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = Vec::new();
    loop {
        let start = head.len();
        if reader.read_until(b'\n', &mut head)? == 0 {
            return Ok(());
        }
        if &head[start..] == b"\r\n" {
            break;
        }
    }
    let mut received = Received { head, body: Vec::new() };
    let length = received.header("Content-Length").parse::<usize>().map_err(io::Error::other)?;
    received.body.resize(length, 0);
    reader.read_exact(&mut received.body)?;
    let (answer, reply) = mpsc::channel();
    if tx.send((received, Reply(answer))).is_ok()
        && let Ok(status) = reply.recv_timeout(Duration::from_secs(20))
    {
        stream.write_all(&response(status))?;
    }
    Ok(())
}
fn response(status: u16) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        if status == 204 { "No Content" } else { "Internal Server Error" }
    )
    .into_bytes()
}
pub(crate) fn signed(received: &Received) -> bool {
    use std::fmt::Write;
    let mut signature = String::new();
    let mut hmac = skein_lib::sha256::Hmac::new(SECRET.as_bytes());
    hmac.update(&received.body);
    for byte in hmac.finish() {
        write!(signature, "{byte:02x}").expect("string write");
    }
    skein_lib::sha256::equal(received.header("X-Forgejo-Signature").as_bytes(), signature.as_bytes())
        && skein_lib::sha256::equal(
            received.header("X-Hub-Signature-256").as_bytes(),
            format!("sha256={signature}").as_bytes(),
        )
}
struct Capture {
    received: Received,
    value: Json,
    status: u16,
}
struct Scenario<'a> {
    fixture: &'a Fixture,
    listener: &'a Listener,
    captures: Vec<Capture>,
    jobs: Vec<bool>,
    pulls: Vec<Vec<bool>>,
    settled: bool,
    checking: bool,
}
impl Scenario<'_> {
    #[expect(clippy::disallowed_types, reason = "the recorder probes real deliveries against a wall clock deadline")]
    fn take(&mut self, events: &[&str], probe: Option<&str>, status: u16) -> Result<()> {
        let until = Instant::now() + Duration::from_secs(20);
        let mut missing = events.to_vec();
        let mut checking = None;
        while !missing.is_empty() {
            let Some((received, reply)) = self.listener.next(until.saturating_duration_since(Instant::now())) else {
                if missing == ["workflow_job_cancelled"] {
                    return Ok(());
                }
                return Err(format!("delivery timeout: {missing:?}").into());
            };
            let value = Json::parse(&received.body)?;
            let event = received.header("X-Forgejo-Event-Type");
            if event.starts_with("workflow_job_") {
                let status = self.fixture.get(&format!(
                    "{REPO}/commits/{}/status?limit=10",
                    value.root().get("run").get("commit_sha").str().ok_or("job head missing")?
                ))?;
                let name = value.root().get("job").get("name").str().ok_or("job name missing")?;
                if ["pass", "fail"].contains(&name) {
                    self.jobs.push(status.root().get("statuses").array().is_some_and(|rows| {
                        rows.iter().any(|row| {
                            row.get("context").str().is_some_and(|context| context.contains(&format!("/ {name} ")))
                                && (row.get("status").is("success") || row.get("status").is("failure"))
                        })
                    }));
                }
            }
            if event == "push" && probe.is_some_and(|head| value.root().get("after").is(head)) {
                let pull = self.fixture.get(&format!("{REPO}/pulls/2"))?;
                let settled = pull.root().get("mergeable").bool() == Some(true);
                self.pulls.push(vec![settled]);
                if !settled {
                    checking = Some((self.pulls.len() - 1, Instant::now()));
                }
                self.checking |= !settled;
            }
            if event != "push" || probe.is_none_or(|head| value.root().get("after").is(head)) {
                missing.retain(|name| *name != event);
            }
            reply.answer(status);
            self.captures.push(Capture { received, value, status });
        }
        if let Some((index, answered)) = checking {
            self.reread(index, answered)?;
        }
        Ok(())
    }
    #[expect(clippy::disallowed_types, reason = "the recorder bounds mergeability rereads by settle")]
    fn reread(&mut self, index: usize, answered: Instant) -> Result<()> {
        let until = answered + Duration::from_mins(1);
        let mut delay = Duration::from_secs(1);
        let mut next = answered + delay;
        loop {
            thread::sleep(next.min(until).saturating_duration_since(Instant::now()));
            let mergeable = self.fixture.get(&format!("{REPO}/pulls/2"))?.root().get("mergeable").bool() == Some(true);
            self.pulls[index].push(mergeable);
            if Instant::now() >= until || mergeable {
                self.settled &= mergeable && Instant::now() <= until;
                return Ok(());
            }
            delay *= 2;
            next = Instant::now() + delay;
        }
    }
    fn act(&mut self, method: &str, route: &str, body: &[u8], writer: bool, events: &[&str]) -> Result<Json> {
        let answer = self.fixture.exchange(
            method,
            &format!("{REPO}{route}"),
            Some(body),
            Some(if writer { &self.fixture.writer.token } else { &self.fixture.owner.token }),
            None,
        )?;
        if !(200..300).contains(&answer.status) {
            return Err(format!("delivery scenario {method} {route}: HTTP {}", answer.status).into());
        }
        self.take(events, None, 204)?;
        Json::parse(&answer.body).or_else(|_| Json::parse(b"null"))
    }
}
fn provision(f: &Fixture, port: u16) -> Result<Json> {
    f.api("POST", "/api/v1/orgs", &json::object(&[("username", &"hooked"), ("visibility", &"private")])?)?;
    f.api(
        "POST",
        "/api/v1/orgs/hooked/repos",
        &json::object(&[("auto_init", &true), ("default_branch", &"main"), ("name", &"specimen"), ("private", &true)])?,
    )?;
    f.api("PUT", &format!("{REPO}/collaborators/writer"), &json::object(&[("permission", &"write")])?)?;
    f.api(
        "POST",
        "/api/v1/orgs/hooked/hooks",
        &json::write(|out| {
            out.object_start();
            out.key(b"active");
            json::Put::put(&true, out);
            out.key(b"config");
            out.object_start();
            out.key(b"content_type");
            json::Put::put(&"json", out);
            out.key(b"secret");
            json::Put::put(&SECRET, out);
            out.key(b"url");
            json::Put::put(&(format!("http://127.0.0.1:{port}/hooks/1")), out);
            out.object_end();
            out.key(b"events");
            json::Put::put(&EVENTS, out);
            out.key(b"type");
            json::Put::put(&"forgejo", out);
            out.object_end();
        })?,
    )
}
pub(crate) fn record(
    binary: Option<&Path>,
    forgejo: &Forgejo,
    scratch: &Path,
    out: &Path,
    secrets: &mut Secrets,
) -> Result<Vec<Fact>> {
    let listener = Listener::start()?;
    secrets.add(SecretKind::HookSecret, SECRET);
    let a = Fixture::bare(forgejo, &scratch.join("hooks-default"), &Settings::small())?;
    provision(&a, listener.port)?;
    a.api("POST", &format!("{REPO}/issues"), &json::object(&[("body", &"marker"), ("title", &"refused")])?)?;
    let refused = listener.next(Duration::from_secs(3)).is_none();
    a.instance.stop().map_err(|e| failure(&e))?;
    let mut settings = Settings::small();
    settings.webhook_hosts = Some("127.0.0.1".into());
    settings.max_response_items = Some(10);
    let f = Fixture::bare(forgejo, &scratch.join("hooks-allowed"), &settings)?;
    let hook = provision(&f, listener.port)?;
    let mut secrets_b = Secrets::from_instance(&f.instance);
    secrets_b.add(SecretKind::HookSecret, SECRET);
    let binary = binary.ok_or("--runner required for delivery CI")?;
    let (runner, _) = runner::start(&f, binary, &mut secrets_b)?;
    let mut s = Scenario {
        fixture: &f,
        listener: &listener,
        captures: Vec::new(),
        jobs: Vec::new(),
        pulls: Vec::new(),
        settled: true,
        checking: false,
    };
    scenario(&mut s)?;
    let retry =
        s.act("POST", "/issues", &json::object(&[("body", &"marker"), ("title", &"failed delivery")])?, false, &[])?;
    s.take(&["issues"], None, 500)?;
    let no_retry = listener.next(Duration::from_secs(3)).is_none();
    let mut facts = evaluate(&s, refused, no_retry, &hook, retry.root().get("number").u64().ok_or("issue number")?);
    let mut mask = Mask::new(f.instance.port());
    mask.listener(listener.port);
    let mut written = std::collections::BTreeSet::new();
    for capture in &s.captures {
        let event = capture.received.header("X-Forgejo-Event-Type");
        let name = format!("{event}{}", if capture.value.root().get("ref_type").is("tag") { "-tag" } else { "" });
        if written.insert(name.clone()) {
            let received = &capture.received;
            write::check(&name, &received.head, &received.body, &secrets_b)?;
            let exchange =
                Exchange { sent: Vec::new(), status: 0, head: received.head.clone(), body: received.body.clone() };
            let request = mask::response(&mut mask, &exchange);
            write::delivery(out, &name, &request, &response(capture.status), &secrets_b)?;
        }
    }
    facts.push(Fact {
        name: "webhook-deliveries",
        outcome: if roll_up(&facts) { Outcome::Holds } else { Outcome::Fails("delivery facts failed".into()) },
    });
    drop(s);
    drop(runner);
    f.instance.stop().map_err(|e| failure(&e))?;
    Ok(facts)
}
fn scenario(s: &mut Scenario<'_>) -> Result<()> {
    s.act(
        "POST",
        "/issues",
        &json::object(&[("body", &"<!-- kit:key 01 -->\n"), ("title", &"issue")])?,
        false,
        &["issues"],
    )?;
    s.act("PATCH", "/issues/1", &json::object(&[("assignees", &["writer"])])?, true, &["issue_assign"])?;
    let label = s
        .fixture
        .api("POST", &format!("{REPO}/labels"), &json::object(&[("color", &"abcdef"), ("name", &"hook")])?)?
        .root()
        .get("id")
        .u64()
        .ok_or("fixture id missing")?;
    s.act("POST", "/issues/1/labels", &json::object(&[("labels", &[label])])?, false, &["issue_label"])?;
    let milestone = s
        .fixture
        .api("POST", &format!("{REPO}/milestones"), &json::object(&[("title", &"hook")])?)?
        .root()
        .get("id")
        .u64()
        .ok_or("fixture id missing")?;
    s.act("PATCH", "/issues/1", &json::object(&[("milestone", &milestone)])?, false, &["issue_milestone"])?;
    s.act(
        "POST",
        "/issues/1/comments",
        &json::object(&[("body", &"<!-- kit:key 02 -->\n")])?,
        true,
        &["issue_comment"],
    )?;
    s.fixture.branch(REPO, "topic", "main")?;
    s.take(&["create"], None, 204)?;
    s.fixture.put(REPO, "topic", "topic.txt", "topic\n")?;
    s.take(&["push"], None, 204)?;
    let pull = s.act(
        "POST",
        "/pulls",
        &json::object(&[("base", &"main"), ("head", &"topic"), ("title", &"topic")])?,
        false,
        &["pull_request"],
    )?;
    s.checking |= pull.root().get("mergeable").bool() == Some(false);
    s.act("PATCH", "/issues/2", &json::object(&[("assignees", &["writer"])])?, true, &["pull_request_assign"])?;
    s.act("POST", "/issues/2/labels", &json::object(&[("labels", &[label])])?, true, &["pull_request_label"])?;
    s.act("PATCH", "/issues/2", &json::object(&[("milestone", &milestone)])?, true, &["pull_request_milestone"])?;
    s.act(
        "POST",
        "/issues/2/comments",
        &json::object(&[("body", &"<!-- kit:key 03 -->\n")])?,
        true,
        &["pull_request_comment"],
    )?;
    s.act(
        "POST",
        "/pulls/2/requested_reviewers",
        &json::object(&[("reviewers", &["writer"])])?,
        false,
        &["pull_request_review_request"],
    )?;
    for (event, kind) in [("APPROVED", "approved"), ("REQUEST_CHANGES", "rejected"), ("COMMENT", "comment")] {
        s.act(
            "POST",
            "/pulls/2/reviews",
            &json::object(&[("body", &"<!-- kit:key 04 -->\n"), ("event", &event)])?,
            true,
            &[&format!("pull_request_review_{kind}")],
        )?;
    }
    let moved = s.act(
        "POST",
        "/contents/writer.txt",
        &json::object(&[
            ("branch", &"topic"),
            ("content", &(crate::fixture::base64("writer\n"))),
            ("message", &"writer"),
        ])?,
        true,
        &[],
    )?;
    s.take(
        &["push", "pull_request_sync"],
        Some(moved.root().get("commit").get("sha").str().ok_or("writer head missing")?),
        204,
    )?;
    let moved = s.fixture.put(REPO, "main", "base.txt", "base\n")?;
    s.take(&["push"], Some(moved.root().get("commit").get("sha").str().ok_or("base head missing")?), 204)?;
    s.fixture.branch(REPO, "gone", "main")?;
    s.take(&["create"], None, 204)?;
    s.act("DELETE", "/branches/gone", &json::write(skein_json::writer::Encoder::null)?, false, &["delete"])?;
    s.act("POST", "/tags", &json::object(&[("tag_name", &"v1"), ("target", &"main")])?, false, &["create"])?;
    s.act("DELETE", "/tags/v1", &json::write(skein_json::writer::Encoder::null)?, false, &["delete"])?;
    workflow(s)
}
fn workflow(s: &mut Scenario<'_>) -> Result<()> {
    s.fixture.branch(REPO, "ci", "main")?;
    s.take(&["create"], None, 204)?;
    s.fixture.api("PATCH", REPO, &json::object(&[("has_actions", &true)])?)?;
    s.fixture.put(REPO,"ci",".forgejo/workflows/deliveries.yaml","name: deliveries\non:\n  push:\n    branches: [ci]\njobs:\n  pass:\n    runs-on: kit-host\n    steps:\n      - run: exit 0\n  fail:\n    runs-on: kit-host\n    steps:\n      - run: exit 1\n  after:\n    needs: [fail]\n    runs-on: kit-host\n    steps:\n      - run: exit 0\n  held:\n    runs-on: kit-held\n    steps:\n      - run: exit 0\n")?;
    s.take(&["push", "workflow_job_success", "workflow_job_failure", "workflow_job_skipped"], None, 204)?;
    let run = s
        .captures
        .iter()
        .find_map(|capture| capture.value.root().get("job").get("run_id").u64())
        .ok_or("run id missing")?;
    s.act(
        "POST",
        &format!("/actions/runs/{run}/cancel"),
        &json::write(skein_json::writer::Encoder::null)?,
        false,
        &[],
    )?;
    s.take(&["workflow_job_cancelled"], None, 204)
}
fn event_facts(s: &Scenario<'_>, facts: &mut Vec<Fact>) {
    let mut rows = BTreeMap::new();
    for c in &s.captures {
        rows.entry(c.received.header("X-Forgejo-Event-Type")).or_insert(c);
    }
    let groups: &[(&str, &[&str], &str, &[&str])] = &[
        ("delivery-issues-row", &EVENTS[..4], "issues", &["/number", "/issue/body"]),
        ("delivery-pull-requests-row", &EVENTS[4..10], "pull_request", &["/number"]),
        ("delivery-comments-row", &EVENTS[10..12], "issue_comment", &["/issue/number", "/comment/body"]),
        ("delivery-reviews-row", &EVENTS[12..15], "", &["/number", "/review/content"]),
        ("delivery-push-row", &EVENTS[15..16], "push", &["/ref", "/after", "/pusher/id"]),
        (
            "delivery-cancelled-job-end-is-delivered",
            &["workflow_job_cancelled"],
            "workflow_job_cancelled",
            &["/run/repository/id", "/run/commit_sha"],
        ),
        ("delivery-create-delete-row", &EVENTS[16..18], "", &["/ref", "/ref_type"]),
        (
            "delivery-job-ends-row",
            &["workflow_job_success", "workflow_job_failure", "workflow_job_skipped"],
            "",
            &["/run/repository/id", "/run/commit_sha"],
        ),
    ];
    for (name, events, header, paths) in groups {
        let holds = events.iter().all(|event| {
            rows.get(event).is_some_and(|c| {
                let expected = if header.is_empty() { event.replace("_review", "") } else { (*header).into() };
                c.received.header("X-Forgejo-Event") == expected
                    && paths.iter().all(|path| {
                        let v = path.split('/').skip(1).fold(c.value.root(), crate::json::At::get);
                        if path.ends_with("id") || path.ends_with("number") {
                            v.u64().is_some()
                        } else {
                            v.str().is_some()
                        }
                    })
            })
        });
        facts::add(facts, name, holds, "event header or payload fields differ");
    }
}
fn evaluate(s: &Scenario<'_>, refused: bool, no_retry: bool, hook: &Json, failed: u64) -> Vec<Fact> {
    let mut facts = Vec::new();
    event_facts(s, &mut facts);
    let repository = s.fixture.get(REPO).ok().and_then(|v| v.root().get("id").u64());
    let same_repo = s.captures.iter().all(|c| {
        c.value
            .root()
            .get("repository")
            .get("id")
            .u64()
            .or_else(|| c.value.root().get("run").get("repository").get("id").u64())
            == repository
            && repository.is_some()
    });
    let subscribed = hook.root().get("events").array().is_some_and(|events| {
        events.len() == EVENTS.len() && EVENTS.iter().all(|name| events.iter().any(|v| v.is(name)))
    });
    let refs = s.captures.iter().filter(|c| c.value.root().get("ref_type").str().is_some()).all(|c| {
        c.value.root().get("ref").str().is_some_and(|reference| {
            reference.starts_with(if c.value.root().get("ref_type").is("branch") {
                "refs/heads/"
            } else {
                "refs/tags/"
            })
        })
    });
    let deletion = !s
        .captures
        .iter()
        .any(|c| c.received.header("X-Forgejo-Event") == "push" && c.value.root().get("ref").is("refs/heads/gone"));
    let echo =
        s.captures.iter().any(|c| {
            c.received.header("X-Forgejo-Event-Type") == "issue_assign"
                && c.value.root().get("sender").get("id").u64() == Some(s.fixture.writer.id)
        }) && s.captures.iter().any(|c| c.value.root().get("pusher").get("id").u64() == Some(s.fixture.writer.id));
    let (pull_settled, pull_checking) = pull_pair(&s.pulls, s.settled);
    for (name, holds) in [
        ("delivery-carries-the-repository-id", same_repo),
        ("delivery-subscribed-events-read-back", subscribed),
        ("delivery-signature-is-hmac-sha256-hex", s.captures.iter().all(|c| signed(&c.received))),
        ("delivery-refs-are-full-and-tags-are-told-apart", refs),
        ("delivery-branch-deletion-sends-no-push", deletion),
        ("delivery-echoes-the-actors-own-writes", echo),
        ("delivery-loopback-is-refused-by-default", refused),
        ("delivery-loopback-is-allowed-when-named", !s.captures.is_empty()),
        (
            "delivery-failed-delivery-is-not-sent-again",
            no_retry
                && s.captures.iter().any(|c| c.value.root().get("number").u64() == Some(failed) && c.status == 500),
        ),
        ("delivery-pull-reads-not-mergeable-while-checked", s.checking),
        ("delivery-job-end-follows-its-commit-status", s.jobs.len() == 2 && s.jobs.iter().all(|seen| *seen)),
        ("delivery-job-end-precedes-its-commit-status", s.jobs.len() == 2 && s.jobs.iter().all(|seen| !*seen)),
        ("delivery-pull-is-settled-when-its-push-is-delivered", pull_settled),
        ("delivery-pull-is-checking-when-its-push-is-delivered", pull_checking),
    ] {
        let seen = format!("job statuses={:?}, pull mergeability={:?}", s.jobs, s.pulls);
        if holds && name.starts_with("delivery-pull-is-") {
            facts.push(Fact { name, outcome: Outcome::Observed(seen) });
        } else {
            facts::add(&mut facts, name, holds, &seen);
        }
    }
    facts
}
pub(crate) fn pull_pair(reads: &[Vec<bool>], settled: bool) -> (bool, bool) {
    let all = reads.len() == 2 && reads.iter().all(|probe| probe.first() == Some(&true));
    let any = reads.len() == 2 && reads.iter().any(|probe| probe.first() == Some(&false));
    (all, any && settled)
}
pub(crate) fn roll_up(facts: &[Fact]) -> bool {
    let pairs = [
        ("delivery-job-end-follows-its-commit-status", "delivery-job-end-precedes-its-commit-status"),
        ("delivery-pull-is-settled-when-its-push-is-delivered", "delivery-pull-is-checking-when-its-push-is-delivered"),
    ];
    let paired = |name| pairs.iter().any(|(a, b)| name == *a || name == *b);
    facts.iter().all(|fact| {
        matches!(fact.outcome, Outcome::Holds | Outcome::Observed(_))
            || paired(fact.name)
            || ["delivery-branch-deletion-sends-no-push", "delivery-cancelled-job-end-is-delivered"]
                .contains(&fact.name)
    }) && pairs.iter().all(|(a, b)| {
        facts
            .iter()
            .filter(|fact| {
                (fact.name == *a || fact.name == *b) && matches!(fact.outcome, Outcome::Holds | Outcome::Observed(_))
            })
            .count()
            == 1
    })
}

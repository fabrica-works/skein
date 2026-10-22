//! The hand's conversation with one real Chromium (lab-browser.md, 8.9).
#[path = "../src/json.rs"]
mod json;
#[expect(dead_code, reason = "the shared capture helpers also read JSON from stdout and numeric fields")]
mod support;
use json::{Value, number, object, string};
use skein_lib::Duration;
use skein_scratch::Scratch;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
use support::{Child, Reader, field, integer, text};
const QUERY: &str = "invented0123456789abcdefghijklmn";
struct Origin {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Origin {
    #[expect(clippy::disallowed_methods, reason = "C-hand.md permits a small fixture origin in a std listener thread")]
    fn start(off: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture origin");
        let address = listener.local_addr().expect("bound fixture address");
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture/hand");
        let index = fs::read_to_string(fixture.join("index.html"))
            .expect("fixture browser conversation")
            .replace("{off}", &off.to_string());
        let sign_in = format!(
            "{index}{}",
            fs::read_to_string(fixture.join("sign-in.html")).expect("fixture browser conversation")
        );
        let second = fs::read_to_string(fixture.join("second.html")).expect("fixture browser conversation");
        let thread = std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("fixture browser conversation");
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).expect("fixture browser conversation");
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .split('?')
                    .next()
                    .expect("fixture browser conversation");
                let body = match path {
                    "/second" => &second,
                    "/sign-in" => &sign_in,
                    _ => &index,
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        Self { address, stop, thread: Some(thread) }
    }
}
impl Drop for Origin {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        self.thread.take().expect("fixture thread").join().expect("fixture origin served its requests");
    }
}
struct Client {
    address: SocketAddr,
    answers: Vec<u8>,
    stderr: PathBuf,
}
impl Client {
    fn call(&mut self, call: &Value) -> Value {
        let mut stream = TcpStream::connect(self.address).expect("connect hand listener");
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).expect("fixture browser conversation");
        let mut bytes = json::encode(call);
        bytes.push(b'\n');
        stream.write_all(&bytes).expect("fixture browser conversation");
        let mut line = Vec::new();
        BufReader::new(&mut stream).read_until(b'\n', &mut line).unwrap_or_else(|e| {
            panic!("answer: {e}; {}", fs::read_to_string(&self.stderr).expect("fixture browser conversation"))
        });
        assert_eq!(
            line.pop(),
            Some(b'\n'),
            "{call:?}: {line:?}; {}",
            fs::read_to_string(&self.stderr).expect("fixture browser conversation")
        );
        let answer = json::parse(&line).expect("canonical JSON answer");
        assert_eq!(json::encode(&answer), line);
        assert!(!String::from_utf8_lossy(&line).contains(QUERY));
        self.answers.extend_from_slice(&line);
        let mut tail = Vec::new();
        stream.read_to_end(&mut tail).expect("fixture browser conversation");
        assert!(tail.is_empty(), "one answer per connection");
        answer
    }
    fn simple(&mut self, call: &str) -> Value {
        self.call(&object([("call", string(call))]))
    }
}
fn part(call: &str, role: &str, name: &str, extra: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    object(
        [
            ("call", string(call)),
            ("role", string(role)),
            ("name", string(name)),
            ("match", string("exact")),
            ("in", Value::Array(vec![])),
        ]
        .into_iter()
        .chain(extra),
    )
}
fn wait(client: &mut Client, role: &str, name: &str, gone: bool) -> Value {
    client.call(&part("wait", role, name, [("gone", Value::Bool(gone)), ("seconds", number(1))]))
}
// lab-browser.md, 8.9: the hand's own desktop and the live scene's phone.
const DESKTOP: (i64, i64) = (800, 600);
const PHONE: (i64, i64) = (390, 844);
fn look_finder(call: &str, role: &str, name: &str, extra: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    object(
        [("call", string(call)), ("role", string(role)), ("name", string(name)), ("match", string("exact"))]
            .into_iter()
            .chain(extra),
    )
}
fn look_shot(file: &str, clip: Value, scale: i64) -> Value {
    object([("call", string("look-shot")), ("clip", clip), ("scale", number(scale)), ("file", string(file))])
}
fn check_png(answer: &Value, out: &Path, file: &str, size: (i64, i64)) {
    let shot = field(answer, "shot");
    assert_eq!(field(shot, "file"), &string(file));
    let png = fs::read(out.join(file)).expect("read look shot PNG");
    assert_eq!(&png[..16], b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    assert_eq!(
        (
            i64::from(u32::from_be_bytes(png[16..20].try_into().expect("four-byte PNG width"))),
            i64::from(u32::from_be_bytes(png[20..24].try_into().expect("four-byte PNG height")))
        ),
        size
    );
    assert_eq!((integer(field(shot, "width")), integer(field(shot, "height"))), size);
    assert_eq!(integer(field(shot, "bytes")), i64::try_from(png.len()).expect("PNG length fits i64"));
    let digest = Command::new("sha256sum").arg(out.join(file)).output().expect("run sha256sum for shot evidence");
    assert!(digest.status.success());
    let digest = String::from_utf8(digest.stdout).expect("sha256sum emits UTF-8");
    assert_eq!(text(field(shot, "sha256")), digest.split_whitespace().next().expect("sha256sum emits a digest"));
}
fn look_reads(client: &mut Client, out: &Path) {
    let focused = object([("focus", object([("role", string("textbox")), ("name", string("Message"))]))]);
    assert_eq!(client.simple("look-focus"), focused);
    let viewport = object([("width", number(PHONE.0)), ("height", number(PHONE.1)), ("mobile", Value::Bool(true))]);
    let Value::Object(mut fields) = viewport.clone() else { panic!("viewport") };
    fields.insert("call".into(), string("look-viewport"));
    assert_eq!(client.call(&Value::Object(fields)), object([("viewport", viewport)]));
    let found = client.call(&look_finder("look-find", "textbox", "Message", []));
    let Value::Array(entries) = field(&found, "found") else { panic!("found") };
    let caret_style = object([
        ("call", string("look-style")),
        ("node", field(&entries[0], "node").clone()),
        ("properties", Value::Array(vec![string("caret-color")])),
    ]);
    let before = client.call(&caret_style);
    let shot = client.call(&look_shot("phone.png", Value::Null, 1));
    check_png(&shot, out, "look/phone.png", PHONE);
    // browser.md, 5.1: a focused textarea draws one picture through its
    // blink. Chromium toggles a blinking caret every 500 ms, so three shots
    // 300 ms apart span a toggle wherever the phase starts.
    let digest = field(field(&shot, "shot"), "sha256");
    let clock = skein_shell::Clock::new();
    let mut next = clock.now().now;
    for index in 1..3 {
        next = next.saturating_add(Duration::from_millis(300));
        let delay = next.as_nanos().saturating_sub(clock.now().now.as_nanos());
        std::thread::sleep(std::time::Duration::from_nanos(delay));
        let answer = client.call(&look_shot(&format!("caret-{index}.png"), Value::Null, 1));
        assert_eq!(field(field(&answer, "shot"), "sha256"), digest);
    }
    let after = client.call(&caret_style);
    assert_eq!(
        field(field(field(&after, "style"), "values"), "caret-color"),
        field(field(field(&before, "style"), "values"), "caret-color")
    );
    assert_eq!(client.simple("look-focus"), focused);
    let met = client.call(&look_finder("look-await", "textbox", "Message", [("seconds", number(1))]));
    assert!(matches!(field(&met, "met"), Value::Array(entries) if !entries.is_empty()));
    let missed = client.call(&look_finder("look-await", "button", "Absent", [("seconds", number(0))]));
    assert!(matches!(field(&missed, "missed"), Value::Array(_)));
    let found = client.call(&look_finder("look-find", "textbox", "Message", []));
    assert_eq!(field(&found, "more"), &number(0));
    let Value::Array(entries) = field(&found, "found") else { panic!("found") };
    let node = field(&entries[0], "node").clone();
    let style = client.call(&object([
        ("call", string("look-style")),
        ("node", node),
        ("properties", Value::Array(vec![string("display")])),
    ]));
    let style = field(&style, "style");
    assert_eq!(field(field(style, "values"), "display"), &string("inline-block"));
    let clip = field(style, "box").clone();
    let size = (integer(field(&clip, "width")) * 2, integer(field(&clip, "height")) * 2);
    let shot = client.call(&look_shot("regions/message.png", clip, 2));
    check_png(&shot, out, "look/regions/message.png", size);
    for dir in ["look", "look/regions"] {
        assert_eq!(
            fs::metadata(out.join(dir)).expect("look shot directory exists").permissions().mode() & 0o777,
            0o700
        );
    }
    let tree = client.simple("look-tree");
    assert!(matches!(field(&tree, "tree"), Value::Array(entries) if !entries.is_empty()));
    assert_eq!(
        client.simple("look-viewport"),
        object([(
            "viewport",
            object([("width", number(DESKTOP.0)), ("height", number(DESKTOP.1)), ("mobile", Value::Bool(false))])
        )])
    );
    // A zero-sized clip would be refused protocol if it reached the kit.
    let invalid_clip = object([("x", number(0)), ("y", number(0)), ("width", number(0)), ("height", number(0))]);
    let original = fs::read(out.join("look/phone.png")).expect("read reserved look shot");
    for file in ["../escape.png", "phone.png"] {
        assert_eq!(field(&client.call(&look_shot(file, invalid_clip.clone(), 1)), "refused"), &string("call"));
    }
    assert_eq!(fs::read(out.join("look/phone.png")).expect("read reserved look shot"), original);
    assert!(!out.join("escape.png").exists());
    let shot = client.simple("shot");
    check_png(&shot, out, "shots/0001.png", DESKTOP);
}
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one ordered hand conversation uses one Chromium from readiness through SIGTERM"
)]
fn browser_hand() {
    assert_eq!(QUERY.len(), 32);
    let off = TcpListener::bind("127.0.0.1:0").unwrap();
    off.set_nonblocking(true).unwrap();
    let origin = Origin::start(off.local_addr().unwrap());
    let base = format!("http://{}", origin.address);
    let scratch = Scratch::new("lab-hand");
    let profile = scratch.path().join("scratch");
    let out = scratch.path().join("out");
    fs::create_dir(&profile).unwrap();
    fs::create_dir(&out).unwrap();
    let stderr = scratch.path().join("stderr");
    let mut child = Child(
        Command::new(env!("CARGO_BIN_EXE_skein-lab-browser"))
            .arg("hand")
            .arg("--chromium")
            .arg(support::chromium())
            .arg("--fonts")
            .arg(support::fonts())
            .arg("--scratch")
            .arg(&profile)
            .arg("--out")
            .arg(&out)
            .arg("--origin")
            .arg(&base)
            .args(["--size", "800x600", "--listen", "127.0.0.1:0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut reader = Reader::new(child.0.stdout.take().unwrap(), stderr.clone());
    let start = reader.now();
    let ready = String::from_utf8(reader.line()).unwrap();
    let address: SocketAddr =
        ready.strip_prefix("listening 127.0.0.1:").map(|port| format!("127.0.0.1:{port}")).unwrap().parse().unwrap();
    assert_ne!(address.port(), 0);
    let mut client = Client { address, answers: Vec::new(), stderr };
    assert_eq!(client.simple("address"), object([("address", json::bytes(b"about:blank"))]));
    assert_eq!(
        client.call(&object([("call", string("sign-in")), ("link", string(format!("{base}/sign-in?{QUERY}")))])),
        object([("opened", string(format!("{base}/")))])
    );
    let answer = client.call(&object([("call", string("outline")), ("in", Value::Array(vec![]))]));
    let outline = text(field(&answer, "outline"));
    for expected in [
        "banner \"Workbench\"\n  link \"Home\"\n",
        "main \"Draft\"\n  heading 2 \"Hand fixture\"\n",
        "  text field \"Title\" = \"Words already here\"",
        "  text area \"Message\" = \"Earlier message\"",
        "  button \"Save\"",
        "  check \"Confirm\"",
        "  select \"Color\"",
        "  link \"Next page\"",
        "  button \"Disabled\" [disabled]",
    ] {
        assert!(outline.contains(expected), "{expected:?}: {outline}");
    }
    assert_eq!(field(&answer, "left"), &number(0));
    let scoped = client.call(&object([
        ("call", string("outline")),
        (
            "in",
            Value::Array(vec![object([
                ("role", string("heading")),
                ("name", string("Hand fixture")),
                ("match", string("exact")),
            ])]),
        ),
    ]));
    assert_eq!(field(&scoped, "outline"), &string("heading 2 \"Hand fixture\"\n"));
    let typed = client.call(&part("type", "text field", "Title", [("text", string("Replacement"))]));
    assert_eq!(field(field(&typed, "typed"), "value"), &string("Replacement"));
    let typed = client.call(&part("type", "text area", "Message", [("text", string("First"))]));
    assert_eq!(field(field(&typed, "typed"), "value"), &string("First"));
    let look_start = reader.now();
    look_reads(&mut client, &out);
    eprintln!("hand look reads: {} ms", reader.now().as_nanos().saturating_sub(look_start.as_nanos()) / 1_000_000);
    assert_eq!(
        client.call(&object([("call", string("key")), ("key", string("Enter")), ("shift", Value::Bool(true))])),
        object([("keyed", string("Shift+Enter"))])
    );
    let message = wait(&mut client, "text area", "Message", false);
    assert_eq!(field(field(&message, "met"), "value"), &string("First\n"));
    for key in ["Tab", "Escape", "Enter"] {
        assert_eq!(
            client.call(&object([("call", string("key")), ("key", string(key)), ("shift", Value::Bool(false))])),
            object([("keyed", string(key))])
        );
    }
    client.call(&part("press", "check", "Confirm", []));
    let check = wait(&mut client, "check", "Confirm", false);
    assert!(matches!(field(field(&check,"met"),"states"),Value::Array(states) if states.contains(&string("checked"))));
    let choice = client.call(&part(
        "choose",
        "select",
        "Color",
        [("option", object([("name", string("Blue")), ("match", string("exact"))]))],
    ));
    assert_eq!(field(field(&choice, "chose"), "role"), &string("option"));
    let select = wait(&mut client, "select", "Color", false);
    assert_eq!(field(field(&select, "met"), "value"), &string("Blue"));
    assert!(
        matches!(client.call(&part("press","button","Save",[])),Value::Object(fields) if fields.contains_key("pressed"))
    );
    assert_eq!(field(field(&wait(&mut client, "heading", "Saved", false), "met"), "name"), &string("Saved"));
    assert_eq!(wait(&mut client, "heading", "Hand fixture", true), object([("gone", Value::Bool(true))]));
    let missed = wait(&mut client, "button", "Not present", false);
    let Value::Array(missed) = field(&missed, "missed") else { panic!("missed array") };
    assert!(missed.iter().any(|part| field(part, "name") == &string("Save")));
    for (call, role, name, why) in [
        ("press", "button", "Absent", "missing"),
        ("press", "text field", "Title", "takes"),
        ("type", "button", "Save", "takes"),
        ("choose", "button", "Save", "takes"),
        ("press", "button", "Disabled", "disabled"),
    ] {
        let extra = match call {
            "type" => vec![("text", string("bad"))],
            "choose" => vec![("option", object([("name", string("Red")), ("match", string("exact"))]))],
            _ => vec![],
        };
        let answer = client.call(&part(call, role, name, extra));
        assert_eq!(field(&answer, "refused"), &string(why));
        if why == "missing" {
            assert_eq!(field(&answer, "detail"), &string("no button named exactly Absent"));
        }
    }
    let scoped_missing = client.call(&object([
        ("call", string("press")),
        ("role", string("button")),
        ("name", string("Absent")),
        ("match", string("words")),
        (
            "in",
            Value::Array(vec![object([
                ("role", string("main")),
                ("name", string("Draft")),
                ("match", string("words")),
            ])]),
        ),
    ]));
    assert_eq!(field(&scoped_missing, "detail"), &string("no button named Absent in the main named Draft"));
    for invalid in [
        object([("call", string("go")), ("path", string("//elsewhere/"))]),
        object([
            ("call", string("sign-in")),
            ("link", string(format!("http://{}/?{QUERY}", off.local_addr().unwrap()))),
        ]),
    ] {
        assert_eq!(field(&client.call(&invalid), "refused"), &string("origin"));
    }
    for name in ["serve", "open", "tree", "select", "style", "unknown"] {
        assert_eq!(field(&client.simple(name), "refused"), &string("call"));
    }
    client.call(&part("press", "link", "Next page", []));
    wait(&mut client, "heading", "Second page", false);
    assert_eq!(client.simple("address"), object([("address", string(format!("{base}/second")))]));
    for (to, path) in [("back", "/"), ("forward", "/second"), ("reload", "/second"), ("back", "/")] {
        assert_eq!(
            client.call(&object([("call", string("history")), ("to", string(to))])),
            object([("went", string(format!("{base}{path}")))])
        );
    }
    assert_eq!(
        client.call(&object([("call", string("go")), ("path", string("/second"))])),
        object([("went", string(format!("{base}/second")))])
    );
    client.call(&object([("call", string("go")), ("path", string("/"))]));
    let shot = client.simple("shot");
    let shot = field(&shot, "shot");
    assert_eq!(field(shot, "file"), &string("shots/0002.png"));
    let png = fs::read(out.join(text(field(shot, "file")))).unwrap();
    assert_eq!(integer(field(shot, "bytes")), i64::try_from(png.len()).unwrap());
    assert_eq!((integer(field(shot, "width")), integer(field(shot, "height"))), (800, 600));
    client.call(&part("press", "link", "Other origin", []));
    let host = off.local_addr().unwrap().to_string();
    let refusal = format!("skein-lab-browser: {host} is not this page's origin");
    let visible = client.call(&part(
        "wait",
        "text",
        &format!("{refusal}\n"),
        [("gone", Value::Bool(false)), ("seconds", number(1))],
    ));
    assert!(matches!(&visible,Value::Object(fields) if fields.contains_key("met")), "{visible:?}");
    let mut connections = 0;
    while off.accept().is_ok() {
        connections += 1;
    }
    assert_eq!(connections, 0, "another loopback origin must never receive a connection");
    let trouble = client.simple("trouble");
    let Value::Array(entries) = field(&trouble, "trouble") else { panic!("trouble array") };
    assert!(
        entries
            .iter()
            .any(|entry| field(entry, "kind") == &string("network") && field(entry, "text") == &string(&host)),
        "{trouble:?}"
    );
    // lab-browser.md, 2.4: the fields typed into above leave no request of
    // Chromium's own (spellcheck's dictionary); the other origin is the
    // page's only network entry.
    assert!(
        entries
            .iter()
            .all(|entry| field(entry, "kind") != &string("network") || field(entry, "text") == &string(&host)),
        "{trouble:?}"
    );
    assert!(
        entries
            .iter()
            .any(|entry| field(entry, "kind") == &string("console") && field(entry, "text") == &string("<link>")),
        "{trouble:?}"
    );
    assert_eq!(field(&client.simple("trouble"), "more"), &number(0));
    client.call(&object([("call", string("history")), ("to", string("back"))]));
    assert!(Command::new("/bin/kill").args(["-TERM", &child.0.id().to_string()]).status().unwrap().success());
    let deadline = reader.now().saturating_add(Duration::from_secs(10));
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{status}; {}", reader.stderr());
            break;
        }
        assert!(reader.now() < deadline, "SIGTERM did not finish: {}", reader.stderr());
        reader.pause();
    }
    let log = fs::read_to_string(out.join("trouble.jsonl")).unwrap();
    assert!(!log.contains(QUERY));
    assert!(!reader.stderr().contains(QUERY));
    for (n, line) in log.lines().enumerate() {
        let entry = json::parse(line.as_bytes()).unwrap();
        assert_eq!(json::encode(&entry), line.as_bytes());
        assert_eq!(field(&entry, "n"), &number(n + 1));
        for key in ["kind", "text", "address", "call", "during"] {
            field(&entry, key);
        }
    }
    assert!(log.contains("<link>"));
    eprintln!("browser hand story: {} ms", reader.now().as_nanos().saturating_sub(start.as_nanos()) / 1_000_000);
}

//! The built program's story on real Chromium (lab-browser.md, 6).
#[path = "../src/json.rs"]
mod json;
mod support;
use json::{Value, number, object, string};
use skein_lib::Duration;
use skein_scratch::Scratch;
use std::{
    fmt::Write as _,
    fs,
    io::Write,
    process::{Command, Stdio},
};
use support::{Child, Reader, field, integer, text};

#[test]
#[expect(clippy::too_many_lines, reason = "the program story checks one ordered conversation from ready through EOF")]
fn browser_program() {
    // lab-browser.md, 4: desktop and phone viewports; a small desktop shot
    // exposes the scrollbar gutter on tall.html's 2000px body.
    const DESKTOP: (i64, i64) = (1440, 900);
    const PHONE: (i64, i64) = (390, 844);
    const TALL_WIDTH: i64 = 100;
    const TALL_HEIGHT: i64 = 100;
    let scratch = Scratch::new("lab-program");
    let profile = scratch.path().join("scratch");
    let out = scratch.path().join("out");
    fs::create_dir(&profile).unwrap();
    fs::create_dir(&out).unwrap();
    let stderr = scratch.path().join("stderr");
    let fonts = support::fonts();
    let chromium = support::chromium();
    let mut child = Child(
        Command::new(env!("CARGO_BIN_EXE_skein-lab-browser"))
            .args([
                "--chromium",
                chromium.to_str().unwrap(),
                "--fonts",
                fonts.to_str().unwrap(),
                "--scratch",
                profile.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut stdin = child.0.stdin.take().unwrap();
    let mut reader = Reader::new(child.0.stdout.take().unwrap(), stderr);
    let start = reader.now();
    let ready = reader.answer();
    assert!(text(field(field(&ready, "ready"), "browser")).contains("Chrome"));
    let mut call = |value: Value| {
        let mut bytes = json::encode(&value);
        bytes.push(b'\n');
        stdin.write_all(&bytes).unwrap();
        reader.answer()
    };
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let served = call(object([
        ("call", string("serve")),
        ("origin", string("fixture")),
        ("dir", string(fixture.to_str().unwrap())),
    ]));
    let address = text(field(field(&served, "served"), "address")).to_owned();
    assert_eq!(field(field(&served, "served"), "origin"), &json::bytes(b"fixture"));
    // lab-browser.md, 6: both emulations precede the story's reads.
    for (page, size, mobile) in [("blank-desktop", DESKTOP, false), ("blank-phone", PHONE, true)] {
        call(object([
            ("call", string("open")),
            ("page", string(page)),
            ("width", number(size.0)),
            ("height", number(size.1)),
            ("mobile", Value::Bool(mobile)),
        ]));
        call(object([
            ("call", string("go")),
            ("page", string(page)),
            ("url", string(format!("{address}/blank.html"))),
        ]));
    }
    // The first shot is an unclipped desktop viewport on a scrolling page.
    call(object([
        ("call", string("open")),
        ("page", string("tall")),
        ("width", number(TALL_WIDTH)),
        ("height", number(TALL_HEIGHT)),
        ("mobile", Value::Bool(false)),
    ]));
    call(object([("call", string("go")), ("page", string("tall")), ("url", string(format!("{address}/tall.html")))]));
    let viewport = call(object([
        ("call", string("shot")),
        ("page", string("tall")),
        ("clip", Value::Null),
        ("scale", number(1)),
        ("file", string("tall.png")),
    ]));
    let viewport_png = fs::read(out.join("tall.png")).unwrap();
    assert_eq!(integer(field(field(&viewport, "shot"), "width")), TALL_WIDTH);
    assert_eq!(integer(field(field(&viewport, "shot"), "height")), TALL_HEIGHT);
    assert_eq!(i64::from(u32::from_be_bytes(viewport_png[16..20].try_into().unwrap())), TALL_WIDTH);
    assert_eq!(i64::from(u32::from_be_bytes(viewport_png[20..24].try_into().unwrap())), TALL_HEIGHT);
    assert_eq!(
        call(object([
            ("call", string("open")),
            ("page", string("p")),
            ("width", number(PHONE.0)),
            ("height", number(PHONE.1)),
            ("mobile", Value::Bool(true))
        ])),
        object([("opened", string("p"))])
    );
    assert_eq!(
        call(object([("call", string("go")), ("page", string("p")), ("url", string(format!("{address}/index.html")))])),
        object([("done", string("p"))])
    );
    let met = call(object([
        ("call", string("await")),
        ("page", string("p")),
        ("role", string("heading")),
        ("name", string("Fixture heading")),
        ("match", string("exact")),
        ("seconds", number(3)),
    ]));
    let Value::Array(met) = field(&met, "met") else { panic!("met array") };
    assert_eq!(met.len(), 1);
    assert_eq!(field(&met[0], "name"), &string("Fixture heading"));
    assert_ne!(field(&met[0], "box"), &Value::Null);
    assert_eq!(
        call(object([
            ("call", string("go")),
            ("page", string("p")),
            ("url", string(format!("{address}/index.html?x=1")))
        ])),
        object([("done", string("p"))])
    );
    let written = call(object([
        ("call", string("find")),
        ("page", string("p")),
        ("role", string("StaticText")),
        ("name", string("Served as written: {address}")),
        ("match", string("exact")),
    ]));
    let Value::Array(written) = field(&written, "found") else { panic!("found array") };
    assert_eq!(written.len(), 1, "a served file is its bytes, its placeholders as written");
    let tree = call(object([("call", string("tree")), ("page", string("p"))]));
    let Value::Array(tree) = field(&tree, "tree") else { panic!("tree array") };
    assert!(tree.iter().any(|entry| field(entry, "role") == &string("heading")
        && field(entry, "name") == &string("Fixture heading")
        && field(entry, "level") == &number(2)));
    let selected = call(object([("call", string("select")), ("page", string("p")), ("selector", string("#part"))]));
    let node = field(&selected, "selected").clone();
    assert!(integer(&node) > 0);
    let style = call(object([
        ("call", string("style")),
        ("page", string("p")),
        ("node", node),
        ("properties", Value::Array(["color", "font-size", "font-family"].into_iter().map(string).collect())),
    ]));
    let style = field(&style, "style");
    let values = field(style, "values");
    assert_eq!(field(values, "color"), &string("rgb(17, 34, 51)"));
    assert_eq!(field(values, "font-size"), &string("20px"));
    assert_eq!(field(values, "font-family"), &string("\"DejaVu Sans\""));
    let Value::Array(faces) = field(style, "faces") else { panic!("faces array") };
    assert!(
        faces
            .iter()
            .any(|face| text(field(face, "family")).starts_with("DejaVu")
                && field(face, "custom") == &Value::Bool(false))
    );
    let clip = field(style, "box").clone();
    assert_eq!(integer(field(&clip, "width")), 180);
    assert_eq!(integer(field(&clip, "height")), 60);
    let shot = call(object([
        ("call", string("shot")),
        ("page", string("p")),
        ("clip", clip.clone()),
        ("scale", number(2)),
        ("file", string("parts/p.png")),
    ]));
    let shot = field(&shot, "shot");
    let png = fs::read(out.join("parts/p.png")).unwrap();
    assert_eq!(integer(field(shot, "bytes")), i64::try_from(png.len()).unwrap());
    assert_eq!(field(shot, "file"), &string("parts/p.png"));
    assert_eq!(integer(field(shot, "width")), integer(field(&clip, "width")) * 2);
    assert_eq!(integer(field(shot, "height")), integer(field(&clip, "height")) * 2);
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"));
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 360);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 120);
    let digest = skein_lib::sha256::digest(&png).iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    });
    assert_eq!(field(shot, "sha256"), &string(digest));
    assert_eq!(
        call(object([
            ("call", string("go")),
            ("page", string("p")),
            ("url", string(format!("{address}/external.html")))
        ])),
        object([("done", string("p"))])
    );
    let trouble = call(object([("call", string("trouble")), ("page", string("p"))]));
    let Value::Array(trouble) = field(&trouble, "trouble") else { panic!("trouble array") };
    assert!(
        trouble
            .iter()
            .any(|entry| field(entry, "kind") == &string("network")
                && field(entry, "text") == &string("styles.example:80")),
        "{trouble:?}"
    );
    let network: Vec<_> = trouble
        .iter()
        .filter(|entry| field(entry, "kind") == &string("network"))
        .map(|entry| text(field(entry, "text")))
        .collect();
    assert_eq!(network, ["styles.example:80"], "only the page's external stylesheet asks the proxy");
    let selected = call(object([("call", string("select")), ("page", string("p")), ("selector", string("#part"))]));
    let style = call(object([
        ("call", string("style")),
        ("page", string("p")),
        ("node", field(&selected, "selected").clone()),
        ("properties", Value::Array(vec![string("color")])),
    ]));
    assert_eq!(field(field(field(&style, "style"), "values"), "color"), &string("rgb(51, 68, 85)"));
    let refused = call(object([("call", string("fly"))]));
    assert_eq!(field(&refused, "refused"), &string("call"));
    for page in ["blank-desktop", "blank-phone"] {
        let trouble = call(object([("call", string("trouble")), ("page", string(page))]));
        assert_eq!(field(&trouble, "trouble"), &Value::Array(vec![]), "{page}: {trouble:?}");
    }
    drop(stdin);
    let deadline = reader.now().saturating_add(Duration::from_secs(10));
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "exit {status}: {}", reader.stderr());
            break;
        }
        assert!(reader.now() < deadline, "program did not exit: {}", reader.stderr());
        reader.pause();
    }
    eprintln!("browser program story: {} ms", reader.now().as_nanos().saturating_sub(start.as_nanos()) / 1_000_000);
}

//! Pure decisions of the lab's browser.
use super::{
    calls, config, files,
    json::{self, Value, number, object, string},
    proxy,
};
use skein_browser::boundary::{self as b, Rect};
use skein_lib::List;
use skein_scratch::Scratch;
use std::{ffi::OsString, fs, os::unix::fs::symlink};
fn canonical(text: &str) -> Vec<u8> {
    json::encode(&json::parse(text.as_bytes()).unwrap())
}
#[test]
fn each_call_parses_and_rejects_missing_unknown_and_mistyped_members() {
    let cases = [
        r#"{"call":"serve","origin":"o","dir":"/tmp"}"#,
        r#"{"call":"open","page":"p","width":390,"height":844,"mobile":true}"#,
        r#"{"call":"go","page":"p","url":"http://127.0.0.1:41/"}"#,
        r#"{"call":"await","page":"p","role":"heading","name":"Hi","match":"exact","seconds":3}"#,
        r#"{"call":"find","page":"p","role":"heading","name":"Hi","match":"words"}"#,
        r#"{"call":"tree","page":"p"}"#,
        r##"{"call":"select","page":"p","selector":"#part"}"##,
        r#"{"call":"style","page":"p","node":42,"properties":["color"]}"#,
        r#"{"call":"shot","page":"p","clip":null,"scale":1,"file":"p.png"}"#,
        r#"{"call":"shot","page":"p","clip":{"x":-1,"y":2,"width":3,"height":4},"scale":2,"file":"nested/p.png"}"#,
        r#"{"call":"trouble","page":"p"}"#,
        r#"{"call":"close","page":"p"}"#,
    ];
    for case in cases {
        let bytes = canonical(case);
        assert!(calls::parse(&bytes).is_ok(), "{case}");
        let Value::Object(fields) = json::parse(&bytes).unwrap() else { panic!("object") };
        for key in fields.keys() {
            let mut missing = fields.clone();
            missing.remove(key);
            assert!(calls::parse(&json::encode(&Value::Object(missing))).is_err(), "{case} missing {key}");
            let mut mistyped = fields.clone();
            mistyped.insert(
                key.clone(),
                if matches!(fields.get(key), Some(Value::Array(_))) {
                    Value::Bool(false)
                } else {
                    Value::Array(vec![])
                },
            );
            assert!(calls::parse(&json::encode(&Value::Object(mistyped))).is_err(), "{case} mistyped {key}");
        }
        let mut unknown = fields;
        unknown.insert("unknown".into(), Value::Null);
        assert!(calls::parse(&json::encode(&Value::Object(unknown))).is_err());
    }
}
#[test]
fn malformed_and_noncanonical_calls_are_refused() {
    for line in [
        r#"{"call":"fly"}"#,
        "[]",
        "null",
        r#"{"call":"tree","call":"close","page":"p"}"#,
        r#"{"page":"p","call":"tree"}"#,
        r#"{ "call":"tree","page":"p"}"#,
        r#"{"call":"tr\u0065e","page":"p"}"#,
        r#"{"call":"tree","page":"p","n":-0}"#,
        r#"{"call":"tree","page":"p","n":1.0}"#,
        r#"{"call":"tree","page":"p","n":1e0}"#,
        r#"{"call":"tree","page":"p","n":9007199254740992}"#,
        r#"{"call":"tree","page":"\ud800"}"#,
        r#"{"call":"tree","page":"p"}{}"#,
        r#"{"call":"find","match":"regexp","name":"x","page":"p","role":"heading"}"#,
    ] {
        assert!(calls::parse(line.as_bytes()).is_err(), "{line}");
    }
    assert!(calls::parse(&vec![b'x'; json::LINE + 1]).is_err());
    assert!(calls::parse(b"{\"call\":\"tree\",\"page\":\"\xff\"}").is_err());
}
#[test]
fn canonical_answers_cover_all_shapes_escapes_and_empty_lists() {
    let samples = [
        (object([("ready", object([("browser", string("Chromium"))]))]), r#"{"ready":{"browser":"Chromium"}}"#),
        (
            object([("served", object([("origin", string("o")), ("address", string("http://127.0.0.1:41"))]))]),
            r#"{"served":{"address":"http://127.0.0.1:41","origin":"o"}}"#,
        ),
        (object([("opened", string("p"))]), r#"{"opened":"p"}"#),
        (object([("done", string("p"))]), r#"{"done":"p"}"#),
        (object([("met", Value::Array(vec![]))]), r#"{"met":[]}"#),
        (object([("missed", Value::Array(vec![]))]), r#"{"missed":[]}"#),
        (object([("found", Value::Array(vec![])), ("more", number(0))]), r#"{"found":[],"more":0}"#),
        (calls::looked(&b::Looked::Tree(List::with_capacity(0))).unwrap(), r#"{"tree":[]}"#),
        (calls::looked(&b::Looked::Selected(None)).unwrap(), r#"{"selected":null}"#),
        (calls::looked(&b::Looked::Selected(Some(41))).unwrap(), r#"{"selected":41}"#),
        (
            calls::looked(&b::Looked::Style {
                values: List::with_capacity(0),
                faces: List::with_capacity(0),
                rect: None,
            })
            .unwrap(),
            r#"{"style":{"box":null,"faces":[],"values":{}}}"#,
        ),
        (
            object([(
                "shot",
                object([
                    ("width", number(2)),
                    ("height", number(4)),
                    ("file", string("p.png")),
                    ("bytes", number(9)),
                    ("sha256", string("abc")),
                ]),
            )]),
            r#"{"shot":{"bytes":9,"file":"p.png","height":4,"sha256":"abc","width":2}}"#,
        ),
        (
            object([("trouble", Value::Array(vec![calls::trouble("console", string("\"\\\n\t\u{1}é"))]))]),
            r#"{"trouble":[{"kind":"console","text":"\"\\\n\t\u0001é"}]}"#,
        ),
        (object([("closed", string("p"))]), r#"{"closed":"p"}"#),
        (calls::refused("call", "bad"), r#"{"detail":"bad","refused":"call"}"#),
    ];
    for (value, wanted) in samples {
        assert_eq!(String::from_utf8(json::encode(&value)).unwrap(), wanted);
    }
    assert_eq!(json::bytes(b"a\xff\xfe\xe2\x82b"), string("a���b"));
    for why in [
        b::Refusal::Gone,
        b::Refusal::Hidden,
        b::Refusal::Covered,
        b::Refusal::Disabled,
        b::Refusal::Crashed,
        b::Refusal::Limit,
        b::Refusal::Timeout,
        b::Refusal::Protocol,
    ] {
        let text = String::from_utf8(json::encode(&calls::kit_refusal(why))).unwrap();
        assert!(text.contains(&format!("\"refused\":\"{}\"", format!("{why:?}").to_lowercase())));
    }
}
#[test]
fn populated_kit_answers_sort_properties_and_keep_evidence() {
    let mut entries = List::with_capacity(1);
    entries
        .push(b::Entry {
            node: None,
            depth: 1,
            role: b"heading".as_slice().into(),
            name: b"Title".as_slice().into(),
            level: 2,
        })
        .unwrap();
    assert_eq!(
        String::from_utf8(json::encode(&calls::looked(&b::Looked::Tree(entries)).unwrap())).unwrap(),
        r#"{"tree":[{"depth":1,"level":2,"name":"Title","node":null,"role":"heading"}]}"#
    );
    let r = Rect { x: -1, y: 2, width: 3, height: 4 };
    assert_eq!(
        String::from_utf8(json::encode(&calls::rect(Some(r)))).unwrap(),
        r#"{"height":4,"width":3,"x":-1,"y":2}"#
    );
    let mut values = List::with_capacity(2);
    for (name, value) in [("z-index", "auto"), ("color", "red")] {
        values.push(b::Property { name: name.as_bytes().into(), value: value.as_bytes().into() }).unwrap();
    }
    let mut faces = List::with_capacity(1);
    faces.push(b::Face { family: b"DejaVu Sans".as_slice().into(), custom: false, glyphs: 3 }).unwrap();
    let value = calls::looked(&b::Looked::Style { values, faces, rect: Some(r) }).unwrap();
    assert_eq!(
        String::from_utf8(json::encode(&value)).unwrap(),
        r#"{"style":{"box":{"height":4,"width":3,"x":-1,"y":2},"faces":[{"custom":false,"family":"DejaVu Sans","glyphs":3}],"values":{"color":"red","z-index":"auto"}}}"#
    );
    let mut seen = List::with_capacity(1);
    seen.push(b::Seen {
        node: 7,
        role: b"heading".as_slice().into(),
        name: b"Title".as_slice().into(),
        value: Box::new([]),
        states: b::States { focused: false, disabled: false, checked: false, expanded: false, selected: false },
        rect: None,
    })
    .unwrap();
    assert_eq!(
        String::from_utf8(json::encode(&calls::seen(&seen))).unwrap(),
        r#"[{"box":null,"name":"Title","node":7,"role":"heading"}]"#
    );
}
#[test]
fn origin_matching_and_shot_components_have_boundaries() {
    for url in ["http://127.0.0.1:41", "http://127.0.0.1:41/", "http://127.0.0.1:41/a?b"] {
        assert!(calls::on_origin(url, ["http://127.0.0.1:41"].into_iter()));
    }
    for url in
        ["http://127.0.0.1:4100/", "http://127.0.0.1:41.example/", "https://127.0.0.1:41/", "http://127.0.0.1:41@evil/"]
    {
        assert!(!calls::on_origin(url, ["http://127.0.0.1:41"].into_iter()));
    }
    for file in ["a.png", "a/b.png", "é space.png"] {
        assert!(calls::shot_file(file));
    }
    for file in ["", "/a", "../a", "a/../b", "./a", "a//b", "a/", "a/./b", "a\0b"] {
        assert!(!calls::shot_file(file), "{file:?}");
    }
}
#[test]
fn directories_types_links_and_each_limit() {
    assert_eq!(files::LIMITS.file, 8 << 20);
    assert_eq!(files::LIMITS.bytes, 64 << 20);
    assert_eq!(files::LIMITS.count, 4096);
    let scratch = Scratch::new("lab-files");
    fs::create_dir(scratch.path().join("nested")).unwrap();
    fs::write(scratch.path().join("index.html"), b"hello").unwrap();
    fs::write(scratch.path().join("nested/a.css"), b"css").unwrap();
    let pages = files::pages(scratch.path(), files::Limits { file: 5, bytes: 8, count: 2 }).unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].path.as_ref(), b"/index.html");
    assert_eq!(pages[0].kind.as_ref(), b"text/html");
    assert_eq!(pages[1].kind.as_ref(), b"text/css");
    assert_eq!(pages[1].body.as_ref(), b"css");
    for limits in [
        files::Limits { file: 4, bytes: 8, count: 2 },
        files::Limits { file: 5, bytes: 7, count: 2 },
        files::Limits { file: 5, bytes: 8, count: 1 },
    ] {
        assert_eq!(files::pages(scratch.path(), limits).err().unwrap().0, "limit");
    }
    symlink(scratch.path().join("index.html"), scratch.path().join("link")).unwrap();
    assert_eq!(files::pages(scratch.path(), files::LIMITS).err().unwrap().0, "origin");
    assert_eq!(files::pages(&scratch.path().join("link"), files::LIMITS).err().unwrap().0, "origin");
    for (ext, mime) in [
        ("html", "text/html"),
        ("CSS", "text/css"),
        ("js", "text/javascript"),
        ("mjs", "text/javascript"),
        ("json", "application/json"),
        ("svg", "image/svg+xml"),
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("webp", "image/webp"),
        ("ico", "image/x-icon"),
        ("woff", "font/woff"),
        ("woff2", "font/woff2"),
        ("ttf", "font/ttf"),
        ("otf", "font/otf"),
        ("wasm", "application/wasm"),
        ("txt", "text/plain"),
        ("other", "application/octet-stream"),
    ] {
        assert_eq!(files::mime(ext), mime);
    }
}
#[test]
fn proxy_first_lines_and_font_paths() {
    for (line, want) in [
        ("CONNECT example.org:443 HTTP/1.1\r\n", "example.org:443"),
        ("GET http://styles.example/look.css HTTP/1.1\r\n", "styles.example:80"),
        ("GET http://styles.example:81/look.css HTTP/1.1\r\n", "styles.example:81"),
        ("CONNECT [::1]:443 HTTP/1.1\r\n", "[::1]:443"),
    ] {
        assert_eq!(proxy::first_line(line.as_bytes()).as_deref(), Some(want));
    }
    for line in [
        b"bad\r\n".as_slice(),
        b"GET /x HTTP/1.1\r\n",
        b"CONNECT x HTTP/1.1\r\n",
        b"GET http://x:65536/ HTTP/1.1\r\n",
        b"GET http://x/a HTTP/1.1",
    ] {
        assert_eq!(proxy::first_line(line), None);
    }
    let prefix = "GET http://x/";
    let suffix = " HTTP/1.1\r\n";
    let exact = format!("{prefix}{}{suffix}", "a".repeat(proxy::LINE - prefix.len() - suffix.len()));
    assert_eq!(proxy::first_line(exact.as_bytes()).as_deref(), Some("x:80"));
    assert_eq!(proxy::first_line(exact.replacen("/a", "/aa", 1).as_bytes()), None);
    assert_eq!(
        config::fonts("/fonts<&\"'", "/scratch>"),
        "<fontconfig><dir>/fonts&lt;&amp;&quot;&apos;</dir><cachedir>/scratch&gt;/fontconfig</cachedir><include ignore_missing=\"yes\">/fonts&lt;&amp;&quot;&apos;/aliases.conf</include></fontconfig>\n"
    );
}
#[test]
fn command_line_requires_each_absolute_path_once() {
    let args = ["--chromium", "/bin/chromium", "--fonts", "/fonts", "--scratch", "/scratch", "--out", "/out"];
    assert!(config::Config::parse(args.into_iter().map(OsString::from)).is_ok());
    for args in
        [vec![], vec!["--out", "relative"], vec!["--unknown", "/x"], vec!["--out"], vec!["--out", "/x", "--out", "/x"]]
    {
        assert!(config::Config::parse(args.into_iter().map(OsString::from)).is_err());
    }
}

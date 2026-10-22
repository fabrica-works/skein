//! Process contracts: lab-browser.md, 2, 3 and 6; lab.md, 2.2.
// Reader uses the program's JSON vocabulary; calls below include raw invalid lines.
#[expect(
    dead_code,
    reason = "the existing Reader shares this JSON module, whose object and byte builders it does not use"
)]
#[path = "../src/json.rs"]
mod json;
mod support;

use std::fs::{self, File};
use std::io::Write;
use std::process::{Command, Stdio};

use json::Value;
use skein_lib::Duration;
use skein_scratch::Scratch;
use support::{Child, Reader, field, integer, text};

// lab-browser.md, 2.3: a regular file may hold exactly 8 MiB.
const FILE_LIMIT: u64 = 8 << 20;
const PAGE: &str = "specimen";
const FIXTURE: &str = r#"<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>
@font-face {font-family:Fixture;src:url('/fixture.ttf')}
body {margin:0}
#part {position:absolute;left:25px;top:1800px;box-sizing:border-box;width:110px;height:50px;padding:4px;border:3px solid black;color:rgb(17,34,51);font:16px 'DejaVu Sans'}
#hidden {display:none}
#media {color:rgb(255,0,0)}
@media (width:390px) and (prefers-color-scheme:light) and (prefers-reduced-motion:reduce) {#media {color:rgb(0,128,0)}}
</style><h1>First</h1><div role="none"><h3>Second</h3></div>
<div id="media">Media</div><div id="part">Own text<span style="font-family:'DejaVu Serif'">Descendant</span><div><span style="font-family:Fixture">Nested web face</span></div></div>
<div id="hidden">Hidden</div><button aria-label='Quote " slash \ café'>Named</button>"#;

fn start() -> (Scratch, Child, Reader) {
    let scratch = Scratch::new("qa-lab-browser");
    fs::create_dir(scratch.path().join("profile")).expect("empty browser scratch");
    fs::create_dir(scratch.path().join("out")).expect("shot output");
    let stderr = scratch.path().join("stderr");
    let process = Command::new(env!("CARGO_BIN_EXE_skein-lab-browser"))
        .arg("--chromium")
        .arg(support::chromium())
        .arg("--fonts")
        .arg(support::fonts())
        .arg("--scratch")
        .arg(scratch.path().join("profile"))
        .arg("--out")
        .arg(scratch.path().join("out"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(File::create(&stderr).expect("stderr evidence"))
        .spawn()
        .expect("start built browser program");
    let mut child = Child(process);
    let mut reader = Reader::new(child.0.stdout.take().expect("stdout pipe"), stderr);
    let ready = reader.answer();
    assert!(!text(field(field(&ready, "ready"), "browser")).is_empty(), "{ready:?}");
    (scratch, child, reader)
}

fn call(child: &mut Child, reader: &mut Reader, line: &str) -> Value {
    let stdin = child.0.stdin.as_mut().expect("stdin pipe");
    writeln!(stdin, "{line}").expect("send call");
    stdin.flush().expect("flush call");
    reader.answer()
}

fn finish(child: &mut Child, reader: &mut Reader) {
    drop(child.0.stdin.take());
    let deadline = reader.now().saturating_add(Duration::from_secs(10));
    loop {
        if let Some(status) = child.0.try_wait().expect("wait program") {
            assert!(status.success(), "{status}; {}", reader.stderr());
            return;
        }
        assert!(reader.now() < deadline, "EOF must settle the browser: {}", reader.stderr());
        reader.pause();
    }
}

fn serve(scratch: &Scratch, child: &mut Child, reader: &mut Reader, name: &str, body: &str) -> String {
    let directory = scratch.path().join(name);
    fs::create_dir(&directory).expect("fixture directory");
    fs::write(directory.join("index.html"), body).expect("fixed page");
    fs::write(directory.join("favicon.ico"), []).expect("empty favicon");
    if body == FIXTURE {
        fs::copy(support::fonts().join("DejaVuSansMono.ttf"), directory.join("fixture.ttf")).expect("fixture web font");
    }
    let answer = call(
        child,
        reader,
        &format!(r#"{{"call":"serve","dir":{:?},"origin":{name:?}}}"#, directory.to_str().expect("UTF-8 fixture path")),
    );
    text(field(field(&answer, "served"), "address")).to_owned()
}

fn open(child: &mut Child, reader: &mut Reader, page: &str) {
    let answer =
        call(child, reader, &format!(r#"{{"call":"open","height":700,"mobile":true,"page":{page:?},"width":390}}"#));
    assert_eq!(text(field(&answer, "opened")), page);
}

fn go(child: &mut Child, reader: &mut Reader, page: &str, address: &str) {
    let answer = call(child, reader, &format!(r#"{{"call":"go","page":{page:?},"url":"{address}/index.html"}}"#));
    assert_eq!(text(field(&answer, "done")), page);
}

fn select(child: &mut Child, reader: &mut Reader, selector: &str) -> i64 {
    let answer = call(child, reader, &format!(r#"{{"call":"select","page":{PAGE:?},"selector":{selector:?}}}"#));
    integer(field(&answer, "selected"))
}

#[test]
fn phone_media_and_far_document_crop_keep_their_geometry() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(&scratch, &mut child, &mut reader, "fixture", FIXTURE);
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let media = select(&mut child, &mut reader, "#media");
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"style","node":{media},"page":{PAGE:?},"properties":["color"]}}"#),
    );
    assert_eq!(text(field(field(field(&answer, "style"), "values"), "color")), "rgb(0, 128, 0)");
    let node = select(&mut child, &mut reader, "#part");
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"style","node":{node},"page":{PAGE:?},"properties":["color","not-a-property"]}}"#),
    );
    let style = field(&answer, "style");
    assert_eq!(text(field(field(style, "values"), "color")), "rgb(17, 34, 51)");
    assert_eq!(text(field(field(style, "values"), "not-a-property")), "");
    let rect = field(style, "box");
    for (key, expected) in [("x", 25), ("y", 1800), ("width", 110), ("height", 50)] {
        assert_eq!(integer(field(rect, key)), expected, "border box {key}: {answer:?}");
    }
    let answer = call(
        &mut child,
        &mut reader,
        &format!(
            r#"{{"call":"shot","clip":{{"height":50,"width":110,"x":25,"y":1800}},"file":"part.png","page":{PAGE:?},"scale":2}}"#
        ),
    );
    let shot = field(&answer, "shot");
    assert_eq!(integer(field(shot, "width")), 220);
    assert_eq!(integer(field(shot, "height")), 100);
    let png = fs::read(scratch.path().join("out/part.png")).expect("shot artifact");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().expect("PNG width")), 220);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().expect("PNG height")), 100);
    assert_eq!(usize::try_from(integer(field(shot, "bytes"))).expect("PNG size"), png.len());
    finish(&mut child, &mut reader);
}

#[test]
fn tree_preserves_heading_levels_order_and_unicode_names() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(&scratch, &mut child, &mut reader, "fixture", FIXTURE);
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let answer = call(&mut child, &mut reader, &format!(r#"{{"call":"tree","page":{PAGE:?}}}"#));
    let Value::Array(entries) = field(&answer, "tree") else { panic!("{answer:?}") };
    let headings: Vec<_> = entries
        .iter()
        .filter(|entry| text(field(entry, "role")) == "heading")
        .map(|entry| (text(field(entry, "name")), integer(field(entry, "level"))))
        .collect();
    assert_eq!(headings, vec![("First", 1), ("Second", 3)]);
    assert!(entries.iter().any(|entry| text(field(entry, "name")) == "Quote \" slash \\ café"), "{answer:?}");
    assert!(!entries.iter().any(|entry| text(field(entry, "name")) == "Hidden"), "{answer:?}");
    finish(&mut child, &mut reader);
}

#[test]
fn closed_pages_release_capacity_and_stale_names_are_refused() {
    let (_scratch, mut child, mut reader) = start();
    // Six lifetimes exceed the kit's four concurrent persons (browser.md, 8).
    for index in 0..6 {
        let page = format!("page-{index}");
        open(&mut child, &mut reader, &page);
        let answer = call(&mut child, &mut reader, &format!(r#"{{"call":"close","page":{page:?}}}"#));
        assert_eq!(text(field(&answer, "closed")), page);
        let answer = call(&mut child, &mut reader, &format!(r#"{{"call":"tree","page":{page:?}}}"#));
        assert_eq!(text(field(&answer, "refused")), "page");
    }
    finish(&mut child, &mut reader);
}

#[test]
fn call_schema_rejects_unknown_members_and_wrong_types() {
    let (_scratch, mut child, mut reader) = start();
    let cases = [
        r#"{"call":"open","height":700,"mobile":true,"page":"bad","width":390,"zzz":0}"#,
        r#"{"call":"open","height":700,"mobile":"true","page":"bad","width":390}"#,
        r#"{"call":"open","height":700,"page":"bad","width":390}"#,
        r#"{"call":"evaluate","expression":"document"}"#,
    ];
    for line in cases {
        let answer = call(&mut child, &mut reader, line);
        assert_eq!(text(field(&answer, "refused")), "call", "{line}: {answer:?}");
    }
    finish(&mut child, &mut reader);
}

#[test]
fn noncanonical_and_duplicate_json_are_refused_as_calls() {
    let (_scratch, mut child, mut reader) = start();
    let cases = [
        r#"{ "call":"open","height":700,"mobile":true,"page":"space","width":390}"#,
        r#"{"page":"order","call":"open","height":700,"mobile":true,"width":390}"#,
        r#"{"call":"open","height":700,"mobile":true,"page":"dupe","page":"dupe","width":390}"#,
        r#"{"call":"open","height":700,"mobile":true,"page":"float","width":390.0}"#,
        r#"{"call":"open","height":700,"mobile":true,"page":"escape\u0064","width":390}"#,
    ];
    for line in cases {
        let answer = call(&mut child, &mut reader, line);
        assert_eq!(text(field(&answer, "refused")), "call", "noncanonical input: {line}: {answer:?}");
    }
    finish(&mut child, &mut reader);
}

#[test]
fn go_rejects_unserved_origins_without_leaving_the_page() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(&scratch, &mut child, &mut reader, "fixture", FIXTURE);
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let cases = [
        "http://127.0.0.1:1/index.html".to_owned(),
        format!("{address}@127.0.0.1:1/index.html"),
        "data:text/html,<h1>Outside</h1>".to_owned(),
    ];
    for url in cases {
        let answer = call(&mut child, &mut reader, &format!(r#"{{"call":"go","page":{PAGE:?},"url":{url:?}}}"#));
        assert_eq!(text(field(&answer, "refused")), "origin", "{url}: {answer:?}");
    }
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"find","match":"exact","name":"First","page":{PAGE:?},"role":"heading"}}"#),
    );
    let Value::Array(found) = field(&answer, "found") else { panic!("{answer:?}") };
    assert_eq!(found.len(), 1);
    finish(&mut child, &mut reader);
}

#[test]
fn serve_refuses_nested_links_and_one_byte_past_file_limit() {
    let (scratch, mut child, mut reader) = start();
    let linked = scratch.path().join("linked");
    fs::create_dir_all(linked.join("nested")).expect("nested directory");
    fs::write(scratch.path().join("outside.html"), "outside").expect("unserved file");
    std::os::unix::fs::symlink(scratch.path().join("outside.html"), linked.join("nested/link.html"))
        .expect("fixture link");
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"serve","dir":{:?},"origin":"linked"}}"#, linked.to_str().expect("UTF-8 path")),
    );
    assert_eq!(text(field(&answer, "refused")), "origin", "{answer:?}");
    for (name, length, refused) in [("at", FILE_LIMIT, false), ("past", FILE_LIMIT + 1, true)] {
        let directory = scratch.path().join(name);
        fs::create_dir(&directory).expect("origin directory");
        File::create(directory.join("large.bin")).expect("large file").set_len(length).expect("file size");
        let answer = call(
            &mut child,
            &mut reader,
            &format!(r#"{{"call":"serve","dir":{:?},"origin":{name:?}}}"#, directory.to_str().expect("UTF-8 path")),
        );
        if refused {
            assert_eq!(text(field(&answer, "refused")), "limit", "{answer:?}");
        } else {
            assert_eq!(text(field(field(&answer, "served"), "origin")), name);
        }
    }
    finish(&mut child, &mut reader);
}

#[test]
fn shot_paths_cannot_climb_out_of_output_directory() {
    let (scratch, mut child, mut reader) = start();
    open(&mut child, &mut reader, PAGE);
    for file in ["../escape.png", "nested/../../escape.png", "/tmp/skein-qa-escape.png"] {
        let answer = call(
            &mut child,
            &mut reader,
            &format!(r#"{{"call":"shot","clip":null,"file":{file:?},"page":{PAGE:?},"scale":1}}"#),
        );
        assert!(matches!(field(&answer, "refused"), Value::String(_)), "{file}: {answer:?}");
        assert!(!scratch.path().join("escape.png").exists());
    }
    finish(&mut child, &mut reader);
}

#[test]
fn proxy_trouble_is_reported_once_and_page_trouble_is_drained() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(
        &scratch,
        &mut child,
        &mut reader,
        "fixture",
        r#"<!doctype html><link rel="stylesheet" href="http://qa-network.invalid:4319/style.css"><h1>Ready</h1><script>console.error('qa console');throw Error('qa exception')</script>"#,
    );
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let answer = call(&mut child, &mut reader, &format!(r#"{{"call":"trouble","page":{PAGE:?}}}"#));
    let Value::Array(trouble) = field(&answer, "trouble") else { panic!("{answer:?}") };
    for (kind, message) in [("network", "qa-network.invalid"), ("console", "qa console"), ("exception", "")] {
        assert!(
            trouble
                .iter()
                .any(|entry| text(field(entry, "kind")) == kind && text(field(entry, "text")).contains(message)),
            "missing {kind}: {answer:?}"
        );
    }
    let answer = call(&mut child, &mut reader, &format!(r#"{{"call":"trouble","page":{PAGE:?}}}"#));
    let Value::Array(trouble) = field(&answer, "trouble") else { panic!("{answer:?}") };
    assert!(
        !trouble.iter().any(|entry| {
            let kind = text(field(entry, "kind"));
            kind == "console" || kind == "exception" || text(field(entry, "text")).contains("qa-network.invalid")
        }),
        "the page's reports and its refused stylesheet are drained: {answer:?}"
    );
    finish(&mut child, &mut reader);
}

#[test]
fn style_of_hidden_element_has_values_but_no_box() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(&scratch, &mut child, &mut reader, "fixture", FIXTURE);
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let node = select(&mut child, &mut reader, "#hidden");
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"style","node":{node},"page":{PAGE:?},"properties":["display"]}}"#),
    );
    let style = field(&answer, "style");
    assert_eq!(text(field(field(style, "values"), "display")), "none");
    assert!(matches!(field(style, "box"), Value::Null), "{answer:?}");
    finish(&mut child, &mut reader);
}

#[test]
fn style_faces_describe_the_text_within_the_node() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(&scratch, &mut child, &mut reader, "fixture", FIXTURE);
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let node = select(&mut child, &mut reader, "#part");
    let answer =
        call(&mut child, &mut reader, &format!(r#"{{"call":"style","node":{node},"page":{PAGE:?},"properties":[]}}"#));
    let Value::Array(faces) = field(field(&answer, "style"), "faces") else { panic!("{answer:?}") };
    let families: Vec<_> = faces.iter().map(|face| text(field(face, "family"))).collect();
    // A style includes nested blocks as well as inline text (browser.md, 5.1).
    assert!(families.contains(&"DejaVu Sans") && families.contains(&"DejaVu Serif"), "{answer:?}");
    assert!(
        faces.iter().any(
            |face| text(field(face, "family")) == "DejaVu Sans Mono" && field(face, "custom") == &Value::Bool(true)
        ),
        "nested web face: {answer:?}"
    );
    for face in faces {
        assert!(integer(field(face, "glyphs")) > 0, "{answer:?}");
    }
    finish(&mut child, &mut reader);
}

#[test]
fn style_refuses_a_node_from_a_previous_document() {
    let (scratch, mut child, mut reader) = start();
    let old = serve(&scratch, &mut child, &mut reader, "old", FIXTURE);
    let new = serve(&scratch, &mut child, &mut reader, "new", "<!doctype html><h1>New document</h1>");
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &old);
    let node = select(&mut child, &mut reader, "#part");
    go(&mut child, &mut reader, PAGE, &new);
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"style","node":{node},"page":{PAGE:?},"properties":["color"]}}"#),
    );
    assert_eq!(text(field(&answer, "refused")), "gone", "{answer:?}");
    finish(&mut child, &mut reader);
}

#[test]
fn serve_keeps_the_files_it_read_at_call_time() {
    let (scratch, mut child, mut reader) = start();
    let address = serve(&scratch, &mut child, &mut reader, "fixture", FIXTURE);
    fs::write(scratch.path().join("fixture/index.html"), "<!doctype html><h1>Changed on disk</h1>")
        .expect("change source after serve");
    open(&mut child, &mut reader, PAGE);
    go(&mut child, &mut reader, PAGE, &address);
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"find","match":"exact","name":"First","page":{PAGE:?},"role":"heading"}}"#),
    );
    let Value::Array(found) = field(&answer, "found") else { panic!("{answer:?}") };
    assert_eq!(found.len(), 1, "serve reads a directory whole: {answer:?}");
    finish(&mut child, &mut reader);
}

#[test]
fn shot_cannot_follow_an_output_link_outside_its_directory() {
    let (scratch, mut child, mut reader) = start();
    let outside = scratch.path().join("outside.png");
    std::os::unix::fs::symlink(&outside, scratch.path().join("out/alias.png")).expect("output link fixture");
    open(&mut child, &mut reader, PAGE);
    let answer = call(
        &mut child,
        &mut reader,
        &format!(r#"{{"call":"shot","clip":null,"file":"alias.png","page":{PAGE:?},"scale":1}}"#),
    );
    assert!(!outside.exists(), "shot must stay under --out: {answer:?}");
    assert!(matches!(field(&answer, "refused"), Value::String(_)), "{answer:?}");
    finish(&mut child, &mut reader);
}

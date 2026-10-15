use super::{Glue, Inputs, bundle, sha256};
use skein_scratch::Scratch;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn inputs(scratch: &Scratch) -> Inputs {
    let path = scratch.path();
    fs::create_dir_all(path.join("pictures")).expect("pictures");
    fs::write(path.join("pictures/icon.svg"), b"<svg/>").expect("picture");
    fs::create_dir_all(path.join("files")).expect("files");
    fs::write(path.join("files/about.txt"), b"About").expect("file");
    fs::write(path.join("a.css"), b"body { color: navy; }").expect("stylesheet");
    Inputs {
        name: "notes".into(),
        prefix: "/static/".into(),
        stylesheets: vec![path.join("a.css")],
        pictures: vec![("notes".into(), path.join("pictures"))],
        page: None,
        files: Some(path.join("files")),
        noscript: "Notes needs WebAssembly and scripts.".into(),
    }
}

fn glue() -> Glue {
    Glue {
        js: b"export default function init() {}".to_vec(),
        wasm: b"\0asm".to_vec(),
        snippets: vec![("notes/grow.js".into(), b"export function grow() {}".to_vec())],
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::new();
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("write hex");
    }
    hex
}

#[test]
fn sha256_gives_fips_examples() {
    const CASES: &[(&[u8], &str)] = &[
        (b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ];
    for (input, expected) in CASES {
        assert_eq!(hex(&sha256::digest(input)), *expected, "{input:?}");
    }
}

#[test]
fn the_digest_covers_paths_and_bytes() {
    let scratch = Scratch::new("bundle-digest");
    let mut inputs = inputs(&scratch);
    let out = scratch.path().join("out");
    let original = bundle(&inputs, &glue(), &out).expect("bundle");
    fs::write(scratch.path().join("files/about.txt"), b"about").expect("change one byte");
    let changed = bundle(&inputs, &glue(), &out).expect("bundle");
    assert_ne!(original, changed, "bytes affect the digest");
    inputs.name = "other".into();
    let renamed = bundle(&inputs, &glue(), &out).expect("bundle");
    assert_ne!(changed, renamed, "paths affect the digest");
}

fn contents(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).expect("directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).expect("relative").to_str().expect("utf8").to_owned(),
                    fs::read(&path).expect("file"),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(directory, directory, &mut files);
    files
}

#[test]
fn the_same_inputs_give_the_same_bundle() {
    let scratch = Scratch::new("bundle-same");
    let inputs = inputs(&scratch);
    let a = scratch.path().join("a");
    let b = scratch.path().join("b");
    fs::create_dir(&a).expect("out");
    fs::write(a.join("stale.txt"), b"stale").expect("old output");
    let first = bundle(&inputs, &glue(), &a).expect("first");
    let second = bundle(&inputs, &glue(), &b).expect("second");
    assert_eq!(first, second);
    let files = contents(&a);
    assert_eq!(files, contents(&b));
    assert_eq!(files.len(), 6);
    assert_eq!(files.get(&format!("{first}/files/about.txt")), Some(&b"About".to_vec()));
}

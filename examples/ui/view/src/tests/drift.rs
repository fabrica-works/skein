//! The family and golden bytes cannot drift (codec.md, 6).
#[test]
fn records_match_their_generated_code() {
    let root = std::path::Path::new(".");
    let source = std::fs::read_to_string(root.join("schema/notes-v1.schema")).expect("schema readable");
    let schema = skein_codegen::parse(&source).expect("schema valid");
    let output = skein_codegen::generate(&schema);
    let write = std::env::var_os("SKEIN_CODEGEN_WRITE").as_deref() == Some(std::ffi::OsStr::new("1"));
    let generated = root.join("src/generated/v1.rs");
    if write {
        std::fs::write(generated, output.rust).expect("write generated code");
    } else {
        assert_eq!(
            std::fs::read_to_string(generated).expect("code committed"),
            output.rust,
            "regenerate notes family with SKEIN_CODEGEN_WRITE=1"
        );
    }
    assert_eq!(output.goldens.len(), 12);
    for golden in output.goldens {
        let path = root.join("golden/v1").join(golden.name);
        if write {
            std::fs::write(path, golden.bytes).expect("write golden");
        } else {
            assert_eq!(std::fs::read(path).expect("golden committed"), golden.bytes, "notes golden matches schema");
        }
    }
}

// Removed when M1's corpus run shows equality; the owner decides.
//! The parser and checks as a test oracle (codec.md, section 8.3).

use core::fmt::Write;

mod check;
mod fixtures;
mod parse;

fn equal(source: &str) {
    let old = parse::parse(source).map_err(|error| crate::Error { line: error.line, message: error.message });
    assert_eq!(crate::parse(source), old, "{source}");
}

#[test]
fn schemas_and_generator_fixtures_equal_the_oracle() {
    for source in fixtures::sources() {
        equal(&source);
    }
}

#[test]
fn new_ceiling_messages_name_their_limits() {
    let source = format!("family f 1 #{}", "x".repeat(1_048_576));
    assert_eq!(
        crate::parse(&source).expect_err("text past ceiling"),
        crate::Error { line: 1, message: "schema longer than 1048576 bytes".into() }
    );
    let mut source = String::from("family f 1\n");
    for index in 0_u32..4097 {
        writeln!(&mut source, "record R{index} {{}}").expect("schema fixture");
    }
    assert_eq!(
        crate::parse(&source).expect_err("declarations past ceiling"),
        crate::Error { line: 4098, message: "more than 4096 declarations".into() }
    );
}

#[test]
fn held_clauses_leave_generated_output_identical() {
    let with = crate::parse("family f 1\nrecord Item { value: u8 }\nrecord R { a: bytes 8 holds other-family Value b: bytes 9 pieces Item }").expect("held clauses");
    let without = crate::parse("family f 1\nrecord Item { value: u8 }\nrecord R { a: bytes 8 b: bytes 9 }")
        .expect("opaque bytes");
    assert_eq!(with, without);
    assert_eq!(crate::generate(&with), crate::generate(&without));
}

#[test]
fn layout_refusals_precede_the_rust_name_checks() {
    assert_eq!(
        crate::parse("family f 1\nrecord lowercase {}\nrecord R { a: Missing }")
            .expect_err("layout precedes Rust names"),
        crate::Error { line: 3, message: "type Missing used before declaration".into() }
    );
}

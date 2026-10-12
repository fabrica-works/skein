//! Generator regressions for codec.md, sections 3 and 6.

use core::fmt::Write;

use crate::{generate, parse};

#[test]
fn a_family_of_120_records_has_no_function_past_100_lines() {
    let mut records = String::new();
    for index in 0_u32..120 {
        writeln!(&mut records, "record Record{index} {{}}").expect("schema string");
    }
    let schema = parse(&format!("family many 1\n{records}")).expect("valid family");
    let output = generate(&schema);
    let mut function = None;
    for (index, line) in output.rust.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
            let indent = line.len().checked_sub(trimmed.len()).expect("indent length");
            function = Some((index, format!("{}}}", " ".repeat(indent))));
        }
        if let Some((start, closing)) = &function
            && line == closing
        {
            assert!(index.checked_sub(*start).expect("function length") < 100, "function at line {start}");
            function = None;
        }
    }
}

#[test]
fn a_boxed_scalar_record_imports_its_box() {
    let schema = parse("family scalar 1\nrecord Value { n: u8 }\nenum Choice { value: boxed Value }")
        .expect("valid boxed variant");
    let output = generate(&schema);
    assert!(output.rust.contains("use alloc::boxed::Box;"));
    assert!(output.rust.contains("Value(Box<Value>)"));
}

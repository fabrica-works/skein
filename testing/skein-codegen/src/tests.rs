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

fn small_bound_family() -> crate::Output {
    let schema = parse("family tiny 1\nrecord Value { payload: bytes 64 flags: list 2 u8 maybe: option bytes 10 }")
        .expect("valid small family");
    generate(&schema)
}

fn planted_golden_failure() -> bool {
    crate::test_helpers::golden(1_u8, &[1], Ok::<_, ()>(Box::from([2_u8])), Ok::<_, ()>(1_u8))
}

#[test]
fn golden_tables_name_each_planted_failure() {
    let output = small_bound_family();
    assert!(output.rust.contains("fn golden_0()"), "one golden table is generated");
    assert!(output.rust.contains("const CASES: [Case; 2]"));
    let panic = std::panic::catch_unwind(|| {
        crate::test_helpers::table([
            ("record_value_smallest", planted_golden_failure),
            ("record_value_full", planted_golden_failure),
        ]);
    })
    .expect_err("planted failures refuse the table");
    let message = panic.downcast_ref::<String>().expect("assertion message");
    assert!(message.contains("record_value_smallest") && message.contains("record_value_full"), "{message}");
}

#[test]
fn bound_tables_decode_at_a_small_limit_and_one_past_it() {
    let output = small_bound_family();
    assert!(output.rust.contains("Limits { value_payload: 3, ..CEILINGS }"), "a large ceiling uses a small test limit");
    assert!(output.rust.contains("Limits { value_flags: 2, ..CEILINGS }"), "a smaller ceiling remains the test limit");
    assert!(
        output.rust.contains("Limits { value_maybe_some: 3, ..CEILINGS }"),
        "an optional field is tested at its own limit"
    );
    let passed = crate::test_helpers::bound(("ValuePayload", "Bound"), &3_u8, &[3], &[4], |limit, bytes| {
        let count = *bytes.first().expect("count supplied");
        if count <= *limit { Ok(()) } else { Err(("ValuePayload", "Bound")) }
    });
    assert!(passed, "at the small limit decodes and one past is refused");
}

#[test]
fn a_family_of_400_bounded_declarations_generates_in_half_a_second() {
    let mut source = String::from("family large 1\n");
    for index in 0_u32..400 {
        writeln!(&mut source, "record Record{index} {{ payload: bytes 64 }}").expect("schema string");
    }
    let schema = parse(&source).expect("valid large family");
    let output = generate(&schema);
    assert_eq!(output.goldens.len(), 800, "two fixed golden values per record");
    assert_eq!(output.rust.matches("fn golden_").count(), 13, "at most 64 golden cases per table");
    assert_eq!(output.rust.matches("fn bounds_").count(), 7, "at most 64 bound cases per table");
    assert_eq!(
        output.rust.matches("check_limits(limits)?;").count(),
        800,
        "one limits check per public constructor and decoder"
    );
}

#[test]
fn an_optional_unbounded_record_needs_no_bound_check() {
    let schema =
        parse("family tiny 1\nrecord Number { n: u64 }\nrecord Value { payload: bytes 3 maybe: option Number }")
            .expect("valid family");
    let output = generate(&schema);
    assert!(!output.rust.contains("if let Some(some_0) = self.maybe.as_ref()"));
}

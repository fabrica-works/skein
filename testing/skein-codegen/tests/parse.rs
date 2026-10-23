#[cfg(test)]
mod tests {
    use core::fmt::Write;

    use skein_codegen::{Declaration, Error, Type, generate, parse};

    #[test]
    fn all_types_and_declarations_parse() {
        let schema = parse("family sample 1\nrecord Earlier {}\nenum Choice { absent, present: Earlier }\nversioned record All {\n    a: u8\n    b: u16\n    c: u32\n    d: u64\n    e: bool\n    f: duration\n    g: fixed 7\n    h: bytes 8\n    i: text 9\n    j: list 10 option Choice\n}\n").expect("valid schema");
        assert_eq!(schema.family, "sample");
        assert_eq!(schema.version, 1);
        assert_eq!(schema.declarations.len(), 3);
        let Declaration::Record(record) = schema.declarations.get(2).expect("third declaration") else {
            unreachable!("the third declaration is a record");
        };
        assert!(record.versioned);
        assert_eq!(record.fields.len(), 10);
        assert_eq!(
            record.fields.get(9).expect("tenth field").ty,
            Type::List(10, Box::new(Type::Option(Box::new(Type::Named("Choice".into())))))
        );
    }

    #[test]
    fn each_refusal_names_its_line_and_exact_message() {
        let variants = (0_u16..257).map(|index| format!("v{index}")).collect::<Vec<_>>().join(", ");
        let too_many_variants = format!("family f 1\nenum A {{ {variants} }}");
        let cases = [
            ("family f 1\nrecord A { b: B }\nrecord B {}", 2, "type B used before declaration"),
            ("family f 1\nrecord A {}\nenum A { x }", 3, "duplicate type A"),
            ("family f 1\nrecord A { x: u8 x: u16 }", 2, "duplicate field x"),
            ("family f 1\nenum A { x, x }", 2, "duplicate variant x"),
            ("family f 1\nenum A { x: B }\nrecord B {}", 2, "type B used before declaration"),
            ("family f 1\nenum A { x }\nenum B { x: A }", 3, "variant x must hold one record"),
            ("family f 1\nenum A { x: { a: u8 } }", 2, "invalid Rust name {"),
            ("family f 1\nrecord A { b: list 4294967295 u64 }", 2, "encoded worst case for A exceeds u32"),
            ("family f 1\nrecord A { b: bytes 4294967295 }", 2, "encoded worst case for A exceeds u32"),
            ("family f 1\nrecord A {\n a: ?\n}", 3, "unexpected character '?'"),
            ("family f 1\nrecord A {\n a text 3\n}", 3, "expected :, found text"),
            ("family f 1\nrecord type {}", 2, "invalid generated type name type"),
            ("family f 1\nrecord lowercase {}", 2, "invalid generated type name lowercase"),
            ("family f 1\nrecord Box {}", 2, "generated type name Box is used twice"),
            ("family f 1\nrecord A {}\nrecord AParts {}", 3, "generated type name AParts is used twice"),
            ("family f 1\nrecord A { type: u8 }", 2, "invalid generated field name type"),
            ("family f 1\nrecord A { new: u8 }", 2, "invalid generated field name new"),
            ("family f 1\nrecord A { skein_bools: bool }", 2, "invalid generated field name skein_bools"),
            ("family f 1\nrecord A { Title: u8 }", 2, "invalid generated field name Title"),
            ("family f 1\nenum A { Read }", 2, "invalid generated variant name Read"),
            ("family f 1\nenum A { foo_bar, foo__bar }", 2, "generated variant FooBar is used twice"),
            ("family f 1\nrecord A { tail: u8 }", 2, "generated path ATail is used twice"),
            ("family f 1\nversioned record A { version: u8 }", 2, "generated path AVersion is used twice"),
            (
                "family f 1\nrecord A { b_item: bytes 1 b: list 1 bytes 1 }",
                2,
                "generated limit name a_b_item is used twice",
            ),
            ("family f 1\nrecord R { a: é }", 2, "unexpected character 'é'"),
            ("family f 1\nrecord R { a: ? }", 2, "unexpected character '?'"),
            ("family f\n", 1, "unexpected end of schema"),
            ("record f 1", 1, "expected family, found record"),
            ("family f 1\nrecord R :", 2, "expected {, found :"),
            ("family f 1\nrecord R { a u8 }", 2, "expected :, found u8"),
            ("family f 1\nversioned enum R {}", 2, "expected record, found enum"),
            ("family f 1\nrecord R { a: }", 2, "expected a type"),
            ("family f 1\nR {}", 2, "expected record, versioned record or enum, found R"),
            ("family f 1\nrecord 1R {}", 2, "invalid Rust name 1R"),
            ("family f 1\nrecord R { a: bytes nope }", 2, "invalid bound nope"),
            ("family {\n1", 2, "invalid family name"),
            ("family {", 1, "invalid family name"),
            ("family f 65536", 1, "invalid family version 65536"),
            ("family f 1\nrecord R { a:\nMissing }", 2, "type Missing used before declaration"),
            ("family f 1\nenum E { x }\nenum F { x: E }", 3, "variant x must hold one record"),
            ("family f 1\nenum E {}", 2, "enum has no variants"),
            ("family f 1\nrecord R {}\nrecord R {}", 3, "duplicate type R"),
            ("family f 1\nrecord R { a: u8 a: u16 }", 2, "duplicate field a"),
            ("family f 1\nenum E { x x }", 2, "duplicate variant x"),
            ("family f 1\nrecord R { a: list 4294967295 list 4294967295 u64 }", 2, "encoded worst case overflows u64"),
            ("family f 1\nrecord R { a: bytes 4294967295 }", 2, "encoded worst case for R exceeds u32"),
            (too_many_variants.as_str(), 2, "enum has more than 256 variants"),
        ];
        for (source, line, message) in cases {
            assert_eq!(
                parse(source).expect_err("schema should be refused"),
                Error { line, message: message.into() },
                "{source}"
            );
        }
    }

    #[test]
    fn a_family_without_declarations_parses() {
        let schema = parse("family large 1\n").expect("empty family");
        assert_eq!((schema.family.as_str(), schema.version, schema.declarations.len()), ("large", 1, 0));
    }

    #[test]
    fn an_unversioned_record_can_have_a_version_field() {
        parse("family f 1\nrecord A { version: u8 }").expect("unversioned record has no version path");
    }

    #[test]
    fn new_ceiling_messages_name_their_limits() {
        let source = format!("family f 1 #{}", "x".repeat(1_048_576));
        assert_eq!(
            parse(&source).expect_err("text past ceiling"),
            Error { line: 1, message: "schema longer than 1048576 bytes".into() }
        );
        let mut source = String::from("family f 1\n");
        for index in 0_u32..4097 {
            writeln!(&mut source, "record R{index} {{}}").expect("schema fixture");
        }
        assert_eq!(
            parse(&source).expect_err("declarations past ceiling"),
            Error { line: 4098, message: "more than 4096 declarations".into() }
        );
    }

    #[test]
    fn held_clauses_leave_generated_output_identical() {
        let with = parse("family f 1\nrecord Item { value: u8 }\nrecord R { a: bytes 8 holds other-family Value b: bytes 9 pieces Item }").expect("held clauses");
        let without =
            parse("family f 1\nrecord Item { value: u8 }\nrecord R { a: bytes 8 b: bytes 9 }").expect("opaque bytes");
        assert_eq!(with, without);
        assert_eq!(generate(&with), generate(&without));
    }

    #[test]
    fn layout_refusals_precede_the_rust_name_checks() {
        assert_eq!(
            parse("family f 1\nrecord lowercase {}\nrecord R { a: Missing }").expect_err("layout precedes Rust names"),
            Error { line: 3, message: "type Missing used before declaration".into() }
        );
    }
}

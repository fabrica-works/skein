// Schemas from the generator tests, collected for oracle equality.
use core::fmt::Write;

const SOURCES: &[&str] = &[
    include_str!("../../../../tests/codec/schema/scalars-v1.schema"),
    include_str!("../../../../tests/codec/schema/secrets-v1.schema"),
    include_str!("../../../../tests/codec/schema/lints-v1.schema"),
    include_str!("../../../../tests/codec/schema/sample-v1.schema"),
    include_str!("../../../../examples/ui/view/schema/notes-v1.schema"),
    "family scalar 1\nrecord Value { n: u8 }\nenum Choice { value: boxed Value }",
    "family tiny 1\nrecord Value { payload: bytes 64 flags: list 2 u8 maybe: option bytes 10 }",
    "family large 1\n",
    "family tiny 1\nrecord Number { n: u64 }\nrecord Value { payload: bytes 3 maybe: option Number }",
    "family sample 1\nrecord Earlier {}\nenum Choice { absent, present: Earlier }\nversioned record All {\n    a: u8\n    b: u16\n    c: u32\n    d: u64\n    e: bool\n    f: duration\n    g: fixed 7\n    h: bytes 8\n    i: text 9\n    j: list 10 option Choice\n}\n",
    "family f 1\nrecord A { b: B }\nrecord B {}",
    "family f 1\nrecord A {}\nenum A { x }",
    "family f 1\nrecord A { x: u8 x: u16 }",
    "family f 1\nenum A { x, x }",
    "family f 1\nenum A { x: B }\nrecord B {}",
    "family f 1\nenum A { x }\nenum B { x: A }",
    "family f 1\nenum A { x: { a: u8 } }",
    "family f 1\nrecord A { b: list 4294967295 u64 }",
    "family f 1\nrecord A { b: bytes 4294967295 }",
    "family f 1\nrecord A {\n a: ?\n}",
    "family f 1\nrecord A {\n a text 3\n}",
    "family f 1\nrecord type {}",
    "family f 1\nrecord lowercase {}",
    "family f 1\nrecord Box {}",
    "family f 1\nrecord A {}\nrecord AParts {}",
    "family f 1\nrecord A { type: u8 }",
    "family f 1\nrecord A { new: u8 }",
    "family f 1\nrecord A { skein_bools: bool }",
    "family f 1\nrecord A { Title: u8 }",
    "family f 1\nenum A { Read }",
    "family f 1\nenum A { foo_bar, foo__bar }",
    "family f 1\nrecord A { tail: u8 }",
    "family f 1\nversioned record A { version: u8 }",
    "family f 1\nrecord A { b_item: bytes 1 b: list 1 bytes 1 }",
    "family f 1\nrecord A { version: u8 }",
    "family f 1\nrecord R { a: é }", // character, decoded as a char
    "family f 1\nrecord R { a: ? }",
    "family f\n", // end line excludes the final newline
    "record f 1",
    "family f 1\nrecord R :",
    "family f 1\nrecord R { a u8 }",
    "family f 1\nversioned enum R {}",
    "family f 1\nrecord R { a: }",
    "family f 1\nR {}",
    "family f 1\nrecord 1R {}",
    "family f 1\nrecord R { a: bytes nope }",
    "family {\n1", // family error names the next token's line
    "family {",    // family error at the end line
    "family f 65536",
    "family f 1\nrecord R { a:\nMissing }", // layout names the field line
    "family f 1\nenum E { x }\nenum F { x: E }",
    "family f 1\nenum E {}",
    "family f 1\nrecord R {}\nrecord R {}",
    "family f 1\nrecord R { a: u8 a: u16 }",
    "family f 1\nenum E { x x }",
    "family f 1\nrecord R { a: list 4294967295 list 4294967295 u64 }",
    "family f 1\nrecord R { a: bytes 4294967295 }",
];

/// The repository schemas, generator inputs and each refusal in one table.
pub(super) fn sources() -> Vec<String> {
    let mut sources: Vec<String> = SOURCES.iter().map(|source| (*source).to_owned()).collect();
    let mut source = String::from("family many 1\n");
    for index in 0_u32..120 {
        writeln!(&mut source, "record Record{index} {{}}").expect("schema fixture");
    }
    sources.push(source);
    let mut source = String::from("family large 1\n");
    for index in 0_u32..400 {
        writeln!(&mut source, "record Record{index} {{ payload: bytes 64 }}").expect("schema fixture");
    }
    sources.push(source);
    let variants = (0_u16..257).map(|index| format!("v{index}")).collect::<Vec<_>>().join(", ");
    sources.push(format!("family f 1\nenum A {{ {variants} }}"));
    sources
}

//! Statement grammar, diagnostics and text-global numbering (codec.md, 11.1).

#![expect(
    clippy::disallowed_types,
    reason = "grammar fixtures and token projections use ordinary Rust test containers"
)]
#![expect(clippy::disallowed_macros, reason = "grammar rows are assembled as small patch texts")]
use super::*;

fn block(line: &str) -> String {
    format!("patch 1\nowner o 1\nfamily f 1 to 2\n{line}\n")
}

#[test]
fn every_statement_form_keeps_its_kind() {
    let rows = [
        ("add D", LineKind::Operation(Op::Add)),
        ("drop D", LineKind::Operation(Op::Drop)),
        ("rename D E", LineKind::Rename),
        ("reshape D", LineKind::Operation(Op::Reshape)),
        ("versioned D", LineKind::Operation(Op::Versioned)),
        ("unversioned D", LineKind::Operation(Op::Unversioned)),
        ("add D.f list 8 option text 32 []", LineKind::Operation(Op::Add)),
        ("add D.f bytes 8 holds family R written", LineKind::Operation(Op::Add)),
        ("add D.f bytes 8 pieces R 0x", LineKind::Operation(Op::Add)),
        ("drop D.f", LineKind::Operation(Op::Drop)),
        ("rename D.a b", LineKind::Rename),
        ("widen D.f u8 u16", LineKind::Operation(Op::Widen)),
        ("wrap D.f list 8 u8 list 8 option u8", LineKind::Operation(Op::Wrap)),
        ("retype D.f fixed 8 secret 8", LineKind::Operation(Op::Retype)),
        ("order D a b", LineKind::Operation(Op::Order)),
        ("add D.v", LineKind::Operation(Op::Add)),
        ("payload D.v R -", LineKind::Operation(Op::Payload)),
        ("written D", LineKind::Site),
        ("written D.f", LineKind::Site),
    ];
    for (line, kind) in rows {
        let parsed = parse(block(line).as_bytes()).expect(line);
        assert_eq!(parsed.lines.get(0).expect("one line").kind, kind, "{line}");
    }
}

#[test]
fn every_value_form_is_read_before_typing() {
    for form in ["?", "written", "none", "some some 1", "[]", "Word", "\"#text\""] {
        let text = block(&format!("add D.f option option u8 {form}"));
        let parsed = parse(text.as_bytes()).expect(form);
        let line = parsed.lines.get(0).expect("line");
        assert!(line.value.start < line.value.end, "{form}");
    }
}

#[test]
fn expected_refusals_name_the_offending_token() {
    let rows: &[(&str, &[u8], u32, Span, Expected)] = &[
        ("language", b"owner o 1", 1, Span { start: 0, end: 5 }, Expected::Language),
        ("owner", b"patch 1\nmark m", 2, Span { start: 8, end: 12 }, Expected::Owner),
        ("level", b"patch 1\nowner o nope", 2, Span { start: 16, end: 20 }, Expected::Level),
        ("family", b"patch 1\nowner ? 0", 2, Span { start: 14, end: 15 }, Expected::Family),
        ("version", b"patch 1\nowner o 0\nfamily f nope to 2", 3, Span { start: 27, end: 31 }, Expected::Version),
        ("to", b"patch 1\nowner o 0\nfamily f 1 nope 2", 3, Span { start: 29, end: 33 }, Expected::To),
        ("line", b"patch 1\nowner o 0\nbogus", 3, Span { start: 18, end: 23 }, Expected::Line),
        ("name", b"patch 1\nowner o 0\nfamily f 1 to 2\nadd ?", 4, Span { start: 38, end: 39 }, Expected::Name),
        ("type", b"patch 1\nowner o 0\nfamily f 1 to 2\nadd D.f ?", 4, Span { start: 42, end: 43 }, Expected::Type),
        (
            "value",
            b"patch 1\nowner o 0\nfamily f 1 to 2\nadd D.f u8 .",
            4,
            Span { start: 45, end: 46 },
            Expected::Value,
        ),
        (
            "text",
            b"patch 1\nowner o 0\nfamily f 1 to 2\nwritten D\nrefuses r word",
            5,
            Span { start: 54, end: 58 },
            Expected::Text,
        ),
        (
            "record",
            b"patch 1\nowner o 0\nfamily f 1 to 2\npayload D.v ? -",
            4,
            Span { start: 46, end: 47 },
            Expected::Record,
        ),
        ("end", b"patch 1 extra", 1, Span { start: 8, end: 13 }, Expected::End),
    ];
    for &(name, text, line, at, wanted) in rows {
        assert_eq!(parse(text).expect_err(name), error(line, at, PatchWrong::Expected(wanted)), "{name}");
    }
}

#[test]
fn character_scan_precedes_grammar() {
    let rows: &[(&str, &[u8], u32, Span)] = &[
        ("outside", b"bogus\n@", 2, Span { start: 6, end: 7 }),
        (
            "backslash",
            b"patch 1\nowner o 0\nfamily f 1 to 2\nwritten D\nrefuses r \"a\\b\"",
            5,
            Span { start: 56, end: 57 },
        ),
        (
            "control",
            b"patch 1\nowner o 0\nfamily f 1 to 2\nwritten D\nrefuses r \"a\x7fb\"",
            5,
            Span { start: 56, end: 57 },
        ),
    ];
    for &(name, text, line, at) in rows {
        assert_eq!(parse(text).expect_err(name), error(line, at, PatchWrong::Character), "{name}");
    }
}

#[test]
fn comments_and_texts_hold_nonword_bytes() {
    let text = b"# @ \\ \xff\npatch 1 # @\nowner o 0\nfamily f 1 to 2\nwritten D\nrefuses r \"@#text\"\n";
    let parsed = parse(text).expect("comments ignored, hash inside text");
    assert_eq!(spelled(text, parsed.lines.get(1).expect("refusal").other), b"\"@#text\"");
}

#[test]
fn number_and_language_refusals_keep_the_number_span() {
    let rows: &[(&str, &[u8], u32, Span, PatchWrong)] = &[
        ("u16 overflow", b"patch 65536", 1, Span { start: 6, end: 11 }, PatchWrong::Number),
        ("u32 overflow", b"patch 1\nowner o 4294967296", 2, Span { start: 16, end: 26 }, PatchWrong::Number),
        ("u16 max", b"patch 65535", 1, Span { start: 6, end: 11 }, PatchWrong::Language),
        ("language", b"patch 2", 1, Span { start: 6, end: 7 }, PatchWrong::Language),
    ];
    for &(name, text, line, at, wrong) in rows {
        assert_eq!(parse(text).expect_err(name), error(line, at, wrong), "{name}");
    }
}

#[test]
fn maximum_levels_and_versions_are_read() {
    parse(b"patch 1\nowner o 4294967295 after upstream 4294967295\nfamily f 65535 to 65535").expect("decimal widths");
}

#[test]
fn duplicate_and_stray_refusals_keep_their_lines() {
    let rows = [
        ("block", "family f 1 to 2", 4, Span { start: 34, end: 40 }, PatchWrong::Twice),
        ("stray", "refuses r \"no site\"", 4, Span { start: 42, end: 43 }, PatchWrong::Stray),
        (
            "refusal",
            "written D\nrefuses r \"one\"\nrefuses r \"two\"",
            6,
            Span { start: 68, end: 69 },
            PatchWrong::Twice,
        ),
    ];
    for (name, lines, line, at, wrong) in rows {
        assert_eq!(parse(block(lines).as_bytes()).expect_err(name), error(line, at, wrong), "{name}");
    }
}

#[test]
fn numbering_counts_sites_and_refusals_across_blocks() {
    let parsed = parse(b"patch 1\nowner o 0\nfamily other 1 to 2\nwritten A\nrefuses first \"a\"\nfamily f 1 to 2\nwritten B\nrefuses second \"b\"\nrefuses third \"c\"").expect("two blocks");
    let mut sites = Vec::new();
    let mut refusals = Vec::new();
    for line in &parsed.lines {
        if line.kind == LineKind::Site {
            sites.push(line.index);
        }
        if line.kind == LineKind::Refusal {
            refusals.push((line.index, line.site));
        }
    }
    assert_eq!(sites, [0, 1]);
    assert_eq!(refusals, [(0, 0), (1, 1), (2, 1)]);
}

#[test]
fn header_reads_levels_marks_and_leaves_the_block_unread() {
    let text = b"patch 1\nowner owner 41 after upstream 7 kit 3\nmark fresh\nmark mark\nfamily @ unread";
    let header = header(text).expect("header alone");
    assert_eq!(spelled(text, header.owner()), b"owner");
    assert_eq!(header.level(), 41);
    assert_eq!(header.after(text, b"upstream"), Some(7));
    assert_eq!(header.after(text, b"kit"), Some(3));
    assert_eq!(header.after(text, b"absent"), None);
    assert!(header.marked(text, b"fresh"));
    assert!(header.marked(text, b"mark"));
    assert!(!header.marked(text, b"absent"));
}

#[test]
fn parsed_marks_keep_a_mark_named_mark() {
    let text = b"patch 1\nowner o 1\nmark mark\nmark fresh\n";
    let parsed = parse(text).expect("marks alone");
    let mut marks = Vec::new();
    for &at in &parsed.marks {
        marks.push(spelled(text, at));
    }
    assert_eq!(marks, [b"mark".as_slice(), b"fresh".as_slice()]);
}

#[test]
fn expected_refusals_have_a_nearest_passing_statement() {
    let rows = [
        ("language", "patch 1\nowner o 0"),
        ("owner", "patch 1\nowner o 0\nmark m"),
        ("level", "patch 1\nowner o 1"),
        ("family", "patch 1\nowner f 0"),
        ("version", "patch 1\nowner o 0\nfamily f 1 to 2"),
        ("to", "patch 1\nowner o 0\nfamily f 1 to 2"),
        ("line", "patch 1\nowner o 0\nfamily f 1 to 2"),
        ("name", "patch 1\nowner o 0\nfamily f 1 to 2\nadd D"),
        ("type", "patch 1\nowner o 0\nfamily f 1 to 2\nadd D.f u8 0"),
        ("value", "patch 1\nowner o 0\nfamily f 1 to 2\nadd D.f u8 0"),
        ("text", "patch 1\nowner o 0\nfamily f 1 to 2\nwritten D\nrefuses r \"word\""),
        ("record", "patch 1\nowner o 0\nfamily f 1 to 2\npayload D.v R -"),
        ("end", "patch 1\nowner o 0"),
    ];
    for (name, text) in rows {
        parse(text.as_bytes()).expect(name);
    }
}

#[test]
fn version_and_bound_overflows_name_their_tokens() {
    let rows = [
        ("from", "family f 65536 to 2", 27, 32),
        ("to", "family f 1 to 65536", 32, 37),
        ("type bound", "family f 1 to 2\nadd D.f text 4294967296 \"\"", 47, 57),
    ];
    for (name, block, start, end) in rows {
        let text = format!("patch 1\nowner o 0\n{block}");
        assert_eq!(parse(text.as_bytes()).expect_err(name).at, Span { start, end }, "{name}");
        assert_eq!(parse(text.as_bytes()).expect_err(name).wrong, PatchWrong::Number, "{name}");
    }
}

#[test]
fn a_refusal_cannot_cross_an_operation_or_block() {
    let rows = [
        ("operation", "written D\nadd E\nrefuses r \"reason\""),
        ("block", "written D\nfamily other 1 to 2\nrefuses r \"reason\""),
    ];
    for (name, lines) in rows {
        assert_eq!(parse(block(lines).as_bytes()).expect_err(name).wrong, PatchWrong::Stray, "{name}");
    }
}

#[test]
fn a_part_in_a_name_statement_refuses_the_dot() {
    let rows = [
        ("reshape D.f", Span { start: 43, end: 44 }, Expected::End),
        ("versioned D.f", Span { start: 45, end: 46 }, Expected::End),
        ("unversioned D.f", Span { start: 47, end: 48 }, Expected::End),
        ("order D.f a", Span { start: 41, end: 42 }, Expected::Name),
        ("refuses r.f \"reason\"", Span { start: 43, end: 44 }, Expected::Text),
    ];
    for (line, at, wanted) in rows {
        assert_eq!(
            parse(block(line).as_bytes()).expect_err(line),
            error(4, at, PatchWrong::Expected(wanted)),
            "{line}"
        );
    }
}

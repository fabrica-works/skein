#![expect(clippy::disallowed_types, reason = "ordinary test code uses collected text and link projections")]
#![expect(clippy::disallowed_macros, reason = "checked-in report passages are compile-time test fixtures")]
#![expect(clippy::disallowed_methods, reason = "ordinary test code collects semantic text for assertions")]

use crate::{Builder, NodeId, Patch, Role, Run, Tree, View, markdown};
use skein_lib::Queue;

fn read(input: &[u8], nesting: u32) -> Tree {
    let bytes = u32::try_from(input.len()).unwrap();
    let limits = crate::Limits {
        nodes: markdown::max_nodes(bytes).unwrap().checked_add(1).unwrap(),
        depth: nesting.checked_mul(2).unwrap().checked_add(2).unwrap().max(5).checked_add(1).unwrap(),
        text_per_node: bytes.checked_mul(2).unwrap(),
        text_bytes: markdown::max_text_bytes(bytes).unwrap(),
        markdown_depth: nesting,
        ..crate::tests::fixtures::LIMITS
    };
    let mut builder = Builder::new(&limits);
    builder.open(Role::Page);
    markdown::read(input, 2, &mut builder);
    builder.close();
    let tree = builder.finish();
    assert!(tree.len() <= limits.nodes);
    tree
}
fn text(tree: &Tree) -> Vec<u8> {
    tree.nodes().iter().flat_map(|node| node.text.as_slice()).flat_map(|run| run.text.iter().copied()).collect()
}

#[test]
fn paragraphs_blocks_runs_links_and_literal_fallbacks() {
    let input = b"# Title\nparagraph *soft* **strong** `code`\nnext\n\n- first\n  - nested\n- last\n\n1. numbered\n2. next\n\n> quoted\n\n```text\nraw *stars*\n```\n\n[visit](https://example.org/a) [plain](http://example.org)\n";
    let tree = read(input, 3);
    for role in
        [Role::Heading { level: 3 }, Role::Paragraph, Role::List, Role::Item, Role::Quote, Role::Code, Role::Link]
    {
        assert!(tree.nodes().iter().any(|node| node.role == role), "{role:?}");
    }
    for run in [Run::Plain, Run::Emphasis, Run::Strong, Run::Code] {
        assert!(tree.nodes().iter().any(|node| node.text.as_slice().iter().any(|text| text.run == run)));
    }
    let links: Vec<_> = tree.nodes().iter().filter_map(|node| node.external.as_deref()).collect();
    assert_eq!(links, [b"https://example.org/a".as_slice(), b"http://example.org".as_slice()]);
    assert!(text(&tree).windows(5).any(|bytes| bytes == b"\nnext"));
    for (literal, expected) in [
        (b"<b>*raw*</b>".as_slice(), b"<b>*raw*</b>".as_slice()),
        (b"| **table** | cell |", b"| **table** | cell |"),
        (b"**table** | cell", b"**table** | cell"),
        (b"| **table** | cell |\n| -- |\n", b"| **table** | cell |\n| -- |"),
        (b"before <b>*raw*</b>", b"before <b>*raw*</b>"),
        (b"![*image*](https://example.org)", b"![*image*](https://example.org)"),
        (b"[*unsafe*](javascript:alert)", b"[*unsafe*](javascript:alert)"),
        (b"[open](https://example.org", b"[open](https://example.org"),
        (b"*never closed", b"*never closed"),
        (b"`[inside](https://example.org)`", b"[inside](https://example.org)"),
        (b"```\nunclosed *fence*\n", b"```\nunclosed *fence*\n"),
        (b"        - **too deep**", b"        - **too deep**"),
        (b"[]()", b"[]()"),
        (b"[](https://example.org)", b"[](https://example.org)"),
        (b"[x](https://)", b"[x](https://)"),
    ] {
        let tree = read(literal, 2);
        assert_eq!(text(&tree), expected, "{literal:?}");
        assert!(tree.nodes().iter().all(|node| node.external.is_none()));
    }
    let mut view = View::new(&crate::Limits {
        nodes: 8,
        depth: 4,
        text_per_node: 64,
        text_bytes: 128,
        ..crate::tests::fixtures::LIMITS
    });
    let mut out = Queue::with_capacity(crate::tests::fixtures::LIMITS.patches);
    let mut link_id = NodeId(0);
    for (shown, input) in
        [(1, b"[label](https://example.org/a)".as_slice()), (2, b"[label](https://example.org/b)".as_slice())]
    {
        let mut builder = view.builder();
        builder.open(Role::Page);
        markdown::read(input, 0, &mut builder);
        builder.close();
        view.finish(builder, shown, &mut out);
        let id = view.tree().nodes().iter().find(|node| node.role == Role::Link).unwrap().id;
        if shown == 1 {
            link_id = id;
        } else {
            assert_eq!(id, link_id);
            assert_eq!(view.external(id), Some(b"https://example.org/b".as_slice()));
            if let Some(Patch::Attribute { node, attribute }) = out.pop() {
                assert_eq!(node, id);
                assert_eq!(attribute.external.as_deref(), Some(b"https://example.org/b".as_slice()));
            } else {
                panic!("changed target has an attribute patch");
            }
            assert!(out.is_empty());
        }
        for _patch in 0..out.len() {
            let _discarded = out.pop().unwrap();
        }
    }
    for unsafe_url in [
        b"https:///path".as_slice(),
        b"http://",
        b"file://example.org",
        b"https://x/<tag>",
        b"https://x/\"",
        b"https://x/ space",
    ] {
        assert!(!markdown::safe_url(unsafe_url));
    }
}

#[test]
fn agent_reports_and_malformed_inputs_fit_the_byte_bound() {
    // Verbatim passages from the extraction, module move and
    // smith-protocol-layer coordination logs under ~/src/plans/ (01 to 12),
    // and replies holding tables (13 to 16, ui.md, 10).
    let reports: [&[u8]; 16] = [
        include_bytes!("markdown/01.md"),
        include_bytes!("markdown/02.md"),
        include_bytes!("markdown/03.md"),
        include_bytes!("markdown/04.md"),
        include_bytes!("markdown/05.md"),
        include_bytes!("markdown/06.md"),
        include_bytes!("markdown/07.md"),
        include_bytes!("markdown/08.md"),
        include_bytes!("markdown/09.md"),
        include_bytes!("markdown/10.md"),
        include_bytes!("markdown/11.md"),
        include_bytes!("markdown/12.md"),
        include_bytes!("markdown/13.md"),
        include_bytes!("markdown/14.md"),
        include_bytes!("markdown/15.md"),
        include_bytes!("markdown/16.md"),
    ];
    for report in reports {
        let _tree = read(report, 4);
    }
    let alphabet = b"*`[]()!#> |-:\\a\n";
    for seed in 0..32_usize {
        let input: Vec<_> =
            (0..48_usize).map(|index| alphabet[(index * 7 + seed * 3 + index / 7) % alphabet.len()]).collect();
        let _tree = read(&input, 3);
    }
}

#[expect(clippy::use_debug, reason = "tree shape assertions show the role and run variants by name")]
fn shape(tree: &Tree) -> String {
    use core::fmt::Write;
    let mut shape = String::new();
    let mut ends = Vec::new();
    for (index, node) in tree.nodes().iter().enumerate() {
        while ends.last().is_some_and(|end| *end <= index) {
            ends.pop();
        }
        write!(shape, "{}{:?}", "  ".repeat(ends.len()), node.role).unwrap();
        for run in &node.text {
            write!(shape, " {:?} b\"{}\"", run.run, run.text.escape_ascii()).unwrap();
        }
        if let Some(target) = &node.external {
            write!(shape, " to b\"{}\"", target.escape_ascii()).unwrap();
        }
        shape.push('\n');
        ends.push(index.checked_add(usize::try_from(node.size).unwrap()).unwrap());
    }
    shape
}

#[test]
fn table_cells_keep_runs_and_their_own_counts() {
    assert_eq!(
        shape(&read(include_bytes!("markdown/13.md"), 1)),
        r#"Page
  Paragraph
    Text Plain b"The checks are ready for review."
  Table
    Row
      ColumnHeader
        Text Plain b"Check"
      ColumnHeader
        Text Plain b"Result"
      ColumnHeader
        Text Plain b"Reference"
    Row
      Cell
        Text Strong b"Ready"
      Cell
        Text Code b"pass"
      Cell
        Link to b"https://example.org/report"
          Text Plain b"report"
    Row
      Cell
        Text Plain b"Short"
      Cell
        Text Plain b"kept"
    Row
      Cell
        Text Plain b"Extra"
      Cell
        Text Plain b"cells"
      Cell
        Text Plain b"stay"
      Cell
        Text Plain b"here"
  Paragraph
    Text Plain b"I will keep the report with the change."
"#
    );
}

#[test]
fn quoted_tables_stay_text_and_indented_tables_follow_lists() {
    assert_eq!(
        shape(&read(include_bytes!("markdown/14.md"), 1)),
        r#"Page
  Paragraph
    Text Plain b"The quoted reply keeps its table as text."
  Quote
    Text Plain b"| a | b |"
  Quote
    Text Plain b"|---|---|"
  Quote
    Text Plain b"| 1 | 2 |"
  List
    Item
      Text Plain b"The indented summary follows this item."
  Table
    Row
      ColumnHeader
        Text Plain b"Check"
      ColumnHeader
        Text Plain b"Result"
    Row
      Cell
        Text Plain b"Build"
      Cell
        Text Plain b"passed"
  Paragraph
    Text Plain b"That summary is ready."
"#
    );
}

#[test]
fn broken_tables_stay_text_and_blocks_end_rows() {
    assert_eq!(
        shape(&read(include_bytes!("markdown/15.md"), 1)),
        r#"Page
  Paragraph
    Text Plain b"These incomplete summaries stay readable."
    Text Plain b"\n"
    Text Plain b"| **Mismatch** | cell |"
    Text Plain b"\n"
    Text Plain b"| -- |"
  Paragraph
    Text Plain b"| **No delimiter** | cell |"
    Text Plain b"\n"
    Text Plain b"The next line is prose."
  Paragraph
    Text Plain b"| Bad | cell |"
    Text Plain b"\n"
    Text Plain b"|--x|--|"
  Table
    Row
      ColumnHeader
        Text Plain b"End"
      ColumnHeader
        Text Plain b"Reason"
    Row
      Cell
        Text Plain b"one"
      Cell
        Text Plain b"text"
  Paragraph
    Text Plain b"This line ends the table."
  Table
    Row
      ColumnHeader
        Text Plain b"End"
      ColumnHeader
        Text Plain b"Reason"
    Row
      Cell
        Text Plain b"two"
      Cell
        Text Plain b"heading"
  Heading { level: 3 }
    Text Plain b"Details"
  Table
    Row
      ColumnHeader
        Text Plain b"End"
      ColumnHeader
        Text Plain b"Reason"
    Row
      Cell
        Text Plain b"three"
      Cell
        Text Plain b"list"
  List
    Item
      Text Plain b"The list ends the last table."
"#
    );
}

#[test]
fn escaped_and_code_pipes_keep_their_cells() {
    assert_eq!(
        shape(&read(include_bytes!("markdown/16.md"), 1)),
        r#"Page
  Paragraph
    Text Plain b"The scanner keeps pipes inside their cells."
  Table
    Row
      ColumnHeader
        Text Plain b"Form"
      ColumnHeader
        Text Plain b"Value"
    Row
      Cell
        Text Plain b"a "
        Text Plain b"| b"
      Cell
        Text Code b"x | y"
    Row
      Cell
        Text Code b"a"
        Text Code b"|b"
      Cell
        Text Plain b"escaped"
    Row
      Cell
        Text Plain b"loose `"
      Cell
        Text Plain b"tail"
    Row
      Cell
      Cell
      Cell
    Row
      Cell
        Text Plain b"<b>"
        Text Plain b"|**raw**</b>"
      Cell
        Text Plain b"final "
        Text Plain b"|"
  Paragraph
    Text Plain b"The plain reply keeps a \\| b as written."
"#
    );
}

#[test]
fn empty_table_rows_fit_the_declared_byte_bound() {
    // ui.md, 10: the row of three empty cells is the worst table row.
    const ROWS: usize = 128;
    let input = format!("|||\n|-|-|\n{}", "||||\n".repeat(ROWS));
    let tree = read(input.as_bytes(), 0);
    assert_eq!(tree.nodes().iter().filter(|node| node.role == Role::Row).count(), ROWS + 1);
    assert_eq!(tree.nodes().iter().filter(|node| node.role == Role::Cell).count(), ROWS * 3);
}

#![expect(clippy::disallowed_types, reason = "ordinary test code uses collected text and link projections")]
#![expect(clippy::disallowed_macros, reason = "checked-in report passages are compile-time test fixtures")]
#![expect(clippy::disallowed_methods, reason = "ordinary test code collects semantic text for assertions")]

use crate::{Builder, NodeId, Patch, Role, Run, Tree, View, markdown};
use skein_lib::Queue;

fn read(input: &[u8], nesting: u32) -> Tree {
    let bytes = u32::try_from(input.len()).unwrap();
    let limits = crate::Limits {
        nodes: markdown::max_nodes(bytes).unwrap().checked_add(1).unwrap(),
        depth: nesting.checked_mul(2).unwrap().checked_add(5).unwrap(),
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
    for literal in [
        b"<b>*raw*</b>".as_slice(),
        b"| **table** | cell |",
        b"**table** | cell",
        b"before <b>*raw*</b>",
        b"![*image*](https://example.org)",
        b"[*unsafe*](javascript:alert)",
        b"[open](https://example.org",
        b"*never closed",
        b"`[inside](https://example.org)`",
        b"```\nunclosed *fence*\n",
        b"        - **too deep**",
        b"[]()",
        b"[](https://example.org)",
        b"[x](https://)",
    ] {
        let tree = read(literal, 2);
        let expected = if literal.first() == Some(&b'`') && literal.last() == Some(&b'`') {
            literal.get(1..literal.len() - 1).unwrap()
        } else {
            literal
        };
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
fn a_dozen_agent_reports_and_malformed_inputs_fit_the_byte_bound() {
    // Verbatim passages from the extraction, module move and
    // smith-protocol-layer coordination logs under ~/src/plans/.
    let reports: [&[u8]; 12] = [
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
    ];
    for report in reports {
        let _tree = read(report, 4);
    }
    let alphabet = b"*`[]()!#>- |a\n";
    for seed in 0..32_usize {
        let input: Vec<_> =
            (0..48_usize).map(|index| alphabet[(index * 7 + seed * 3 + index / 7) % alphabet.len()]).collect();
        let _tree = read(&input, 3);
    }
}

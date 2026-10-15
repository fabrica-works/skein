//! A bounded Markdown subset (skein's ui.md, section 10).
//! Each line is visited once. Inline links and delimiter pairs are found by
//! bounded scans, then emitted without recursion. A fence retains only input
//! offsets until it closes; an unclosed fence is copied as literal text.
//! No platform markup is created from bytes. Unsupported constructs retain
//! their characters, and copied text is bounded by the builder's limits.

use crate::{Builder, Role, Run};
use skein_lib::{List, Stack};

#[derive(Clone, Copy, Debug)]
struct ListLevel {
    indent: usize,
    ordered: bool,
}
#[derive(Clone, Copy, Debug)]
struct Fence {
    start: usize,
    body: usize,
}
#[derive(Debug)]
struct Blocks {
    lists: Stack<ListLevel>,
    paragraph: bool,
    fence: Option<Fence>,
}

/// At most two nodes per input byte, plus two block nodes.
#[must_use]
pub fn max_nodes(bytes: u32) -> Option<u32> {
    bytes.checked_mul(2)?.checked_add(2)
}
/// Copied words and link names/targets together fit twice the input bytes.
#[must_use]
pub fn max_text_bytes(bytes: u32) -> Option<u32> {
    bytes.checked_mul(2)
}

/// Read beneath the page's own heading levels; bytes must fit the view's input
/// bound. Depth and tree counts include the reader's nodes and its parent.
pub fn read(input: &[u8], heading_base: u8, builder: &mut Builder) {
    assert!(heading_base <= 249, "six Markdown heading levels fit after the page levels");
    assert!(crate::tree::length(input) <= builder.limits.text_bytes, "Markdown input fits the declared byte bound");
    let mut blocks =
        Blocks { lists: Stack::with_capacity(builder.limits.markdown_depth), paragraph: false, fence: None };
    let mut start = 0_usize;
    for (end, byte) in input.iter().enumerate() {
        if *byte == b'\n' {
            let next = end.checked_add(1).expect("line offset fits usize");
            line(input, start, end, next, heading_base, &mut blocks, builder);
            start = next;
        }
    }
    if start < input.len() {
        line(input, start, input.len(), input.len(), heading_base, &mut blocks, builder);
    }
    if let Some(fence) = blocks.fence {
        builder.open(Role::Paragraph);
        builder.text(Run::Plain, input.get(fence.start..).expect("unclosed fence is in input"));
        builder.close();
    }
    close_paragraph(&mut blocks, builder);
    close_lists(&mut blocks, builder);
}

fn line(input: &[u8], start: usize, end: usize, next: usize, base: u8, blocks: &mut Blocks, builder: &mut Builder) {
    let raw = input.get(start..end).expect("line is in input");
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    if let Some(fence) = blocks.fence {
        if let Some(rest) = raw.strip_prefix(b"```") {
            let mut closed = true;
            for byte in rest {
                if *byte != b' ' && *byte != b'\t' {
                    closed = false;
                    break;
                }
            }
            if closed {
                builder.open(Role::Code);
                builder.text(Run::Code, input.get(fence.body..start).expect("code body is in input"));
                builder.close();
                blocks.fence = None;
            }
        }
        return;
    }
    if raw.is_empty() {
        close_paragraph(blocks, builder);
        close_lists(blocks, builder);
        return;
    }
    if raw.starts_with(b"```") {
        close_paragraph(blocks, builder);
        close_lists(blocks, builder);
        blocks.fence = Some(Fence { start, body: next });
        return;
    }
    if let Some(count) = heading(raw) {
        close_paragraph(blocks, builder);
        close_lists(blocks, builder);
        let level = base.checked_add(count).expect("heading base reserves six levels");
        builder.open(Role::Heading { level });
        let start = usize::from(count).checked_add(1).expect("heading prefix fits usize");
        inline(raw.get(start..).expect("heading body exists"), builder);
        builder.close();
        return;
    }
    if let Some(body) = raw.strip_prefix(b"> ") {
        close_paragraph(blocks, builder);
        close_lists(blocks, builder);
        builder.open(Role::Quote);
        inline(body, builder);
        builder.close();
        return;
    }
    if let Some(item) = list_item(raw) {
        close_paragraph(blocks, builder);
        if list_step(blocks, item.level, builder) {
            if item.level.ordered {
                builder.text(Run::Plain, raw.get(item.level.indent..item.body).expect("ordered marker exists"));
            }
            inline(raw.get(item.body..).expect("list body exists"), builder);
        } else {
            close_lists(blocks, builder);
            builder.open(Role::Paragraph);
            builder.text(Run::Plain, raw);
            builder.close();
        }
        return;
    }
    close_lists(blocks, builder);
    if blocks.paragraph {
        builder.text(Run::Plain, b"\n");
    } else {
        builder.open(Role::Paragraph);
        blocks.paragraph = true;
    }
    if raw.contains(&b'<') || raw.contains(&b'|') {
        builder.text(Run::Plain, raw);
    } else {
        inline(raw, builder);
    }
}

fn heading(line: &[u8]) -> Option<u8> {
    let mut count = 0_u8;
    for byte in line {
        if *byte != b'#' {
            break;
        }
        count = count.checked_add(1)?;
        if count > 6 {
            return None;
        }
    }
    if count > 0 && line.get(usize::from(count)) == Some(&b' ') { Some(count) } else { None }
}

#[derive(Debug)]
struct Item {
    level: ListLevel,
    body: usize,
}
fn list_item(line: &[u8]) -> Option<Item> {
    let mut indent = 0_usize;
    for byte in line {
        if *byte != b' ' {
            break;
        }
        indent = indent.checked_add(1)?;
    }
    let rest = line.get(indent..)?;
    for marker in [b"- ", b"* ", b"+ "] {
        if rest.starts_with(marker) {
            return Some(Item { level: ListLevel { indent, ordered: false }, body: indent.checked_add(2)? });
        }
    }
    let mut digits = 0_usize;
    for byte in rest {
        if !byte.is_ascii_digit() {
            break;
        }
        digits = digits.checked_add(1)?;
    }
    if digits > 0 && rest.get(digits..digits.checked_add(2)?) == Some(b". ") {
        Some(Item { level: ListLevel { indent, ordered: true }, body: indent.checked_add(digits)?.checked_add(2)? })
    } else {
        None
    }
}

fn list_step(blocks: &mut Blocks, level: ListLevel, builder: &mut Builder) -> bool {
    let limit = builder.limits.markdown_depth;
    if u32::try_from(level.indent.checked_div(2).expect("nonzero divisor")).expect("input indentation fits u32")
        >= limit
    {
        return false;
    }
    for _level in 0..blocks.lists.len() {
        if blocks.lists.top().expect("list level remains").indent <= level.indent {
            break;
        }
        builder.close();
        builder.close();
        let _closed = blocks.lists.pop().expect("list level remains");
    }
    if let Some(top) = blocks.lists.top() {
        if top.indent == level.indent && top.ordered == level.ordered {
            builder.close();
            builder.open(Role::Item);
            return true;
        }
        if top.indent == level.indent {
            builder.close();
            builder.close();
            let _closed = blocks.lists.pop().expect("list level remains");
        }
    }
    if blocks.lists.len() >= limit {
        return false;
    }
    builder.open(Role::List);
    blocks.lists.push(level).expect("list nesting fits its bound");
    builder.open(Role::Item);
    true
}
fn close_lists(blocks: &mut Blocks, builder: &mut Builder) {
    for _level in 0..blocks.lists.len() {
        let _closed = blocks.lists.pop().expect("list level remains");
        builder.close();
        builder.close();
    }
}
fn close_paragraph(blocks: &mut Blocks, builder: &mut Builder) {
    if blocks.paragraph {
        builder.close();
        blocks.paragraph = false;
    }
}

#[derive(Clone, Copy, Debug)]
enum RangeKind {
    Link,
    Literal,
}
#[derive(Clone, Copy, Debug)]
struct Range {
    start: usize,
    end: usize,
    label: usize,
    label_end: usize,
    target: usize,
    target_end: usize,
    kind: RangeKind,
}
#[derive(Debug)]
enum LinkScan {
    None,
    Label { start: usize, label: usize, image: bool },
    Target { start: usize, label: usize, label_end: usize, target: usize, image: bool },
}
fn ranges(line: &[u8]) -> List<Range> {
    let mut ranges = List::with_capacity(crate::tree::length(line));
    let mut state = LinkScan::None;
    let mut code = false;
    for (index, byte) in line.iter().enumerate() {
        let outside_link = match &state {
            LinkScan::None => true,
            LinkScan::Label { .. } | LinkScan::Target { .. } => false,
        };
        if outside_link && *byte == b'`' {
            code = !code;
            continue;
        }
        state = match state {
            LinkScan::None => {
                if *byte == b'[' && !code {
                    let image = match index.checked_sub(1) {
                        Some(previous) => line.get(previous) == Some(&b'!'),
                        None => false,
                    };
                    let start = if image { index.checked_sub(1).expect("image has a prefix") } else { index };
                    LinkScan::Label { start, label: index.checked_add(1).expect("label index fits"), image }
                } else {
                    LinkScan::None
                }
            }
            LinkScan::Label { start, label, image } => {
                if *byte == b']' && line.get(index.checked_add(1).expect("link index fits")) == Some(&b'(') {
                    LinkScan::Target {
                        start,
                        label,
                        label_end: index,
                        target: index.checked_add(2).expect("target index fits"),
                        image,
                    }
                } else {
                    LinkScan::Label { start, label, image }
                }
            }
            LinkScan::Target { start, label, label_end, target, image } => {
                if *byte == b')' && index >= target {
                    let safe = safe_url(line.get(target..index).expect("target is in line"));
                    let kind = if safe && !image && label < label_end { RangeKind::Link } else { RangeKind::Literal };
                    ranges
                        .push(Range {
                            start,
                            end: index.checked_add(1).expect("range end fits"),
                            label,
                            label_end,
                            target,
                            target_end: index,
                            kind,
                        })
                        .expect("ranges fit input bytes");
                    LinkScan::None
                } else {
                    LinkScan::Target { start, label, label_end, target, image }
                }
            }
        };
    }
    let unclosed = match state {
        LinkScan::None => None,
        LinkScan::Label { start, .. } | LinkScan::Target { start, .. } => Some(start),
    };
    if let Some(start) = unclosed {
        ranges
            .push(Range {
                start,
                end: line.len(),
                label: start,
                label_end: start,
                target: start,
                target_end: start,
                kind: RangeKind::Literal,
            })
            .expect("ranges fit input bytes");
    }
    ranges
}

/// Only whole http/https targets with a nonempty host and no markup characters.
#[must_use]
pub fn safe_url(url: &[u8]) -> bool {
    let host = if let Some(host) = url.strip_prefix(b"https://") {
        host
    } else if let Some(host) = url.strip_prefix(b"http://") {
        host
    } else {
        return false;
    };
    if host.is_empty() || host.first() == Some(&b'/') {
        return false;
    }
    for byte in url {
        if *byte <= b' ' || *byte == b'"' || *byte == b'<' || *byte == b'>' {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug)]
struct Marker {
    at: usize,
    len: usize,
    run: Run,
    partner: Option<u32>,
    opening: bool,
}
fn markers(line: &[u8], ranges: &List<Range>, depth: u32) -> List<Marker> {
    let mut markers: List<Marker> = List::with_capacity(crate::tree::length(line));
    let mut open: Stack<u32> = Stack::with_capacity(depth);
    let mut skip = 0_usize;
    let mut range = 0_u32;
    for (index, byte) in line.iter().enumerate() {
        if let Some(link) = ranges.get(range)
            && index == link.start
        {
            skip = link.end;
            range = range.checked_add(1).expect("range index fits");
        }
        if index < skip {
            continue;
        }
        let mut in_code = false;
        if let Some(top) = open.top() {
            in_code = markers.get(*top).expect("open marker exists").run == Run::Code;
        }
        let (len, run) = if *byte == b'`' {
            (1, Run::Code)
        } else if !in_code && *byte == b'*' {
            if line.get(index.checked_add(1).expect("marker index fits")) == Some(&b'*') {
                (2, Run::Strong)
            } else {
                (1, Run::Emphasis)
            }
        } else {
            continue;
        };
        skip = index.checked_add(len).expect("marker end fits");
        let mut matched = None;
        if let Some(top) = open.top()
            && markers.get(*top).expect("open marker exists").run == run
        {
            matched = Some(*top);
        }
        let position = markers.len();
        if let Some(partner) = matched {
            let previous = markers.get_mut(partner).expect("open marker exists");
            let nonempty = index > previous.at.checked_add(previous.len).expect("marker end fits");
            if nonempty {
                previous.partner = Some(position);
            }
            markers
                .push(Marker {
                    at: index,
                    len,
                    run,
                    partner: if nonempty { Some(partner) } else { None },
                    opening: false,
                })
                .expect("markers fit input bytes");
            let _closed = open.pop().expect("matching marker remains");
        } else if open.len() < depth {
            markers
                .push(Marker { at: index, len, run, partner: None, opening: true })
                .expect("markers fit input bytes");
            open.push(position).expect("emphasis fits its nesting bound");
        }
    }
    markers
}

fn inline(line: &[u8], builder: &mut Builder) {
    let ranges = ranges(line);
    let markers = markers(line, &ranges, builder.limits.markdown_depth);
    let mut active = Stack::with_capacity(builder.limits.markdown_depth);
    let mut range = 0_u32;
    let mut marker = 0_u32;
    let mut plain = 0_usize;
    let mut skip = 0_usize;
    for index in 0..line.len() {
        if index < skip {
            continue;
        }
        let run = active.top().copied().unwrap_or(Run::Plain);
        if let Some(link) = ranges.get(range)
            && index == link.start
        {
            flush(line, plain, index, run, builder);
            match link.kind {
                RangeKind::Link => {
                    if run == Run::Code {
                        flush(line, link.start, link.end, run, builder);
                    } else {
                        builder.open(Role::Link);
                        builder.external(line.get(link.target..link.target_end).expect("link target exists"));
                        let label = line.get(link.label..link.label_end).expect("link label exists");
                        builder.name(label);
                        builder.text(run, label);
                        builder.close();
                    }
                }
                RangeKind::Literal => flush(line, link.start, link.end, run, builder),
            }
            plain = link.end;
            skip = link.end;
            range = range.checked_add(1).expect("range index fits");
            continue;
        }
        if let Some(mark) = markers.get(marker)
            && index == mark.at
        {
            marker = marker.checked_add(1).expect("marker index fits");
            if mark.partner.is_some() {
                flush(line, plain, index, run, builder);
                if mark.opening {
                    active.push(mark.run).expect("emphasis fits its nesting bound");
                } else {
                    let _closed = active.pop().expect("paired emphasis remains");
                }
                plain = index.checked_add(mark.len).expect("marker end fits");
                skip = plain;
            }
        }
    }
    flush(line, plain, line.len(), active.top().copied().unwrap_or(Run::Plain), builder);
    assert!(active.is_empty(), "all emitted emphasis has a closing marker");
}
fn flush(line: &[u8], start: usize, end: usize, run: Run, builder: &mut Builder) {
    if start < end {
        builder.text(run, line.get(start..end).expect("text span is in line"));
    }
}

/// Scratch heap for one line's token tables and bounded block/emphasis stacks.
#[must_use]
pub fn worst_case(bytes: u32, depth: u32) -> Option<u64> {
    List::<Range>::worst_case(bytes)?
        .checked_add(List::<Marker>::worst_case(bytes)?)?
        .checked_add(Stack::<u32>::worst_case(depth)?)?
        .checked_add(Stack::<Run>::worst_case(depth)?)?
        .checked_add(Stack::<ListLevel>::worst_case(depth)?)
}

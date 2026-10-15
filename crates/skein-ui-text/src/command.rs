//! Last-print numbers interpreted against the current tree (ui-text.md, 6).
use alloc::boxed::Box;
use skein_lib::{Decimal, Writer, bytes};
use skein_ui::Act;
use skein_ui::{NodeId, Role, State, Tree};
/// What one typed line asks the host to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Apply `first`, then submit when requested, to the same node.
    Act {
        node: NodeId,
        first: Act,
        submit: bool,
    },
    Print,
    Close,
    Nothing,
    Refused {
        refusal: Refusal,
        number: u64,
    },
}
/// A refused line; `NoNumber` and `Gone` also request a fresh print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoNumber,
    Gone,
    TakesNoWords,
    NoPage,
    TooLong,
    Unknown,
}
/// Read a line without its line ending. Numbers belong to the last print.
#[must_use]
pub fn command(line: &[u8], numbers: Option<&[NodeId]>, tree: &Tree, longest: u32) -> Command {
    let line = trim(line);
    if line.len() > usize::try_from(longest).expect("line bound fits memory") {
        return refused(Refusal::TooLong, 0);
    }
    match line {
        b"" => return Command::Nothing,
        b"p" => return Command::Print,
        b"q" => return Command::Close,
        _ => {}
    }
    let split = bytes::find(line, b" ").unwrap_or(line.len());
    let digits = line.get(..split).expect("split is in line");
    let Some(number) = skein_ui::number(digits) else {
        return refused(Refusal::Unknown, 0);
    };
    let Some(numbers) = numbers else {
        return refused(Refusal::NoPage, number);
    };
    let Some(index) = number.checked_sub(1) else {
        return refused(Refusal::NoNumber, number);
    };
    let Ok(index) = usize::try_from(index) else {
        return refused(Refusal::NoNumber, number);
    };
    let Some(id) = numbers.get(index).copied() else {
        return refused(Refusal::NoNumber, number);
    };
    let Some(node) = tree.find(id) else {
        return refused(Refusal::Gone, number);
    };
    if tree.bound(id).is_none() || node.states.as_slice().contains(&State::Disabled) {
        return refused(Refusal::Gone, number);
    }
    let words = trim(line.get(split..).expect("words follow the number"));
    let field = node.role == Role::TextField || node.role == Role::TextArea;
    if !words.is_empty() && !field {
        return refused(Refusal::TakesNoWords, number);
    }
    let first = match node.role {
        Role::TextField | Role::TextArea => Act::Input { value: bytes::copy_of(words) },
        Role::Check | Role::Choice => Act::Toggle,
        Role::Page
        | Role::Banner
        | Role::Navigation
        | Role::Main
        | Role::Complementary
        | Role::ContentInfo
        | Role::Form
        | Role::Search
        | Role::Region
        | Role::Heading { .. }
        | Role::Paragraph
        | Role::Text
        | Role::List
        | Role::Item
        | Role::Table
        | Role::Row
        | Role::Cell
        | Role::Term
        | Role::Description
        | Role::Button
        | Role::Link
        | Role::ChoiceGroup
        | Role::Dialog
        | Role::Status
        | Role::Alert
        | Role::Log
        | Role::Tree
        | Role::TreeItem
        | Role::Tabs
        | Role::Tab
        | Role::Progress
        | Role::Time
        | Role::Code
        | Role::Quote
        | Role::Separator
        | Role::Menu
        | Role::MenuItem
        | Role::Disclosure
        | Role::Image
        | Role::Figure
        | Role::Slot
        | Role::Drawing => Act::Press,
    };
    Command::Act { node: id, first, submit: field && node.states.as_slice().contains(&State::SubmitsOnEnter) }
}
fn refused(refusal: Refusal, number: u64) -> Command {
    Command::Refused { refusal, number }
}
fn trim(line: &[u8]) -> &[u8] {
    let mut start = 0_usize;
    let mut end = line.len();
    for byte in line {
        if *byte != b' ' {
            break;
        }
        start = start.saturating_add(1);
    }
    for byte in line.iter().rev() {
        if end <= start || *byte != b' ' {
            break;
        }
        end = end.saturating_sub(1);
    }
    line.get(start..end).expect("trimmed spaces stay inside the line")
}
/// The shell's own refusal line, including its trailing newline.
#[must_use]
pub fn refusal_line(name: &[u8], refusal: Refusal, number: u64) -> Box<[u8]> {
    let digits = Decimal::of(number);
    let (before, after, numbered): (&[u8], &[u8], bool) = match refusal {
        Refusal::NoNumber => (b": no [", b"] on the page\n", true),
        Refusal::Gone => (b": [", b"] is gone\n", true),
        Refusal::TakesNoWords => (b": [", b"] takes no words\n", true),
        Refusal::NoPage => (b": no page yet\n", b"", false),
        Refusal::TooLong => (b": line too long\n", b"", false),
        Refusal::Unknown => (b": commands: n, n words, p, q\n", b"", false),
    };
    let number_bytes = if numbered { digits.as_bytes() } else { b"" };
    let length = name
        .len()
        .checked_add(before.len())
        .expect("refusal length fits memory")
        .checked_add(number_bytes.len())
        .expect("refusal length fits memory")
        .checked_add(after.len())
        .expect("refusal length fits memory");
    let mut out = Writer::new(length);
    out.put(name).expect("name fits computed refusal length");
    out.put(before).expect("prefix fits computed refusal length");
    out.put(number_bytes).expect("number fits computed refusal length");
    out.put(after).expect("suffix fits computed refusal length");
    out.finish()
}

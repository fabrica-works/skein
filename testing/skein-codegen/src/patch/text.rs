//! A patch's text read into its statements (codec.md, 11.1), leniently: the
//! command reads what it wrote and its author edited, and leaves the grammar's
//! refusals to skein-codec's `header` and `plan`.

/// A patch's statements, comments dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PatchText {
    pub owner: String,
    pub level: u32,
    /// Each upstream's name and level, in order.
    pub after: Vec<(String, u32)>,
    pub marks: Vec<String>,
    pub blocks: Vec<Block>,
}

/// A family's block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    pub family: String,
    pub from: u16,
    pub to: u16,
    pub lines: Vec<Line>,
}

/// A statement of a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// An operation, as written, its comment and outer spaces dropped.
    Op(String),
    /// `written D` or `written D.f`, and the refusals after it.
    Site { target: String, refusals: Vec<Refusal> },
}

/// `refuses <name> "<text>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub name: String,
    /// The text as written, quotes included.
    pub text: String,
}

impl PatchText {
    /// Reads a patch's statements; `None` when a header or block line does not
    /// read, or a refusal follows no site.
    pub(crate) fn read(source: &str) -> Option<Self> {
        let mut patch = Self::default();
        for line in source.lines().map(uncommented).filter(|line| !line.is_empty()) {
            let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
            match word {
                "patch" => {}
                "owner" => {
                    let mut words = rest.split_whitespace();
                    words.next()?.clone_into(&mut patch.owner);
                    patch.level = words.next()?.parse().ok()?;
                    if let Some(after) = words.next() {
                        if after != "after" {
                            return None;
                        }
                        while let Some(upstream) = words.next() {
                            patch.after.push((upstream.to_owned(), words.next()?.parse().ok()?));
                        }
                    }
                }
                "mark" => patch.marks.push(rest.trim().to_owned()),
                "family" => {
                    let words = rest.split_whitespace().collect::<Vec<_>>();
                    let [family, from, "to", to] = words.as_slice() else { return None };
                    patch.blocks.push(Block {
                        family: (*family).to_owned(),
                        from: from.parse().ok()?,
                        to: to.parse().ok()?,
                        lines: Vec::new(),
                    });
                }
                "written" => {
                    let target = rest.trim().to_owned();
                    patch.blocks.last_mut()?.lines.push(Line::Site { target, refusals: Vec::new() });
                }
                "refuses" => {
                    let (name, text) = rest.trim().split_once(' ')?;
                    let refusal = Refusal { name: name.to_owned(), text: text.trim().to_owned() };
                    match patch.blocks.last_mut()?.lines.last_mut()? {
                        Line::Site { refusals, .. } => refusals.push(refusal),
                        Line::Op(_) => return None,
                    }
                }
                _ => patch.blocks.last_mut()?.lines.push(Line::Op(line.to_owned())),
            }
        }
        Some(patch)
    }

    /// Each site in text order with its family's block: the site's index is
    /// its position (11.1).
    pub(crate) fn sites(&self) -> impl Iterator<Item = (&Block, &str, &[Refusal])> {
        self.blocks.iter().flat_map(|block| {
            block.lines.iter().filter_map(move |line| match line {
                Line::Site { target, refusals } => Some((block, target.as_str(), refusals.as_slice())),
                Line::Op(_) => None,
            })
        })
    }

    /// Each refusal's name in text order: its index is its position (11.1).
    pub(crate) fn refusals(&self) -> impl Iterator<Item = &str> {
        self.sites().flat_map(|(_, _, refusals)| refusals.iter().map(|refusal| refusal.name.as_str()))
    }
}

/// A statement without its comment: `#` outside a text starts one.
fn uncommented(line: &str) -> &str {
    let mut quoted = false;
    for (at, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'#' if !quoted => return line.get(..at).unwrap_or(line).trim(),
            _ => {}
        }
    }
    line.trim()
}

#[cfg(test)]
mod tests {
    use super::{Line, PatchText, Refusal};

    #[test]
    fn reads_header_blocks_sites_and_refusals() {
        let patch = PatchText::read(
            "patch 1\nowner jig 41 after smith 7 skein 3\nmark fresh-windows\n\n\
             family tasks 9 to 10 # a comment\nadd Task.note text 8 \"a # b\"\nwritten Task.score\n\
             refuses unscored \"no attempts\"\n",
        )
        .expect("reads");
        assert_eq!((patch.owner.as_str(), patch.level), ("jig", 41));
        assert_eq!(patch.after, [("smith".to_owned(), 7), ("skein".to_owned(), 3)]);
        assert_eq!(patch.marks, ["fresh-windows"]);
        assert_eq!(patch.blocks.len(), 1);
        let block = patch.blocks.first().expect("one block");
        assert_eq!((block.family.as_str(), block.from, block.to), ("tasks", 9, 10));
        assert_eq!(
            block.lines,
            [
                Line::Op("add Task.note text 8 \"a # b\"".to_owned()),
                Line::Site {
                    target: "Task.score".to_owned(),
                    refusals: vec![Refusal { name: "unscored".to_owned(), text: "\"no attempts\"".to_owned() }],
                },
            ]
        );
        assert_eq!(patch.refusals().collect::<Vec<_>>(), ["unscored"]);
    }

    #[test]
    fn refuses_a_refusal_after_no_site() {
        assert_eq!(PatchText::read("patch 1\nowner jig 1\nfamily tasks 1 to 2\nrefuses x \"y\"\n"), None);
    }
}

//! Ordered document data shared by the host and browser (ui-web.md, 6).

use std::fmt::Write;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Child {
    Element(Element),
    Text(String),
}

/// An empty tag is a fragment: its children print without a containing element.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Element {
    pub tag: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Child>,
}

impl Element {
    #[must_use]
    pub fn new(tag: &str) -> Self {
        Self { tag: tag.into(), attributes: Vec::new(), children: Vec::new() }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        if let Some((_, stored)) = self.attributes.iter_mut().find(|(key, _)| key == name) {
            value.clone_into(stored);
        } else {
            self.attributes.push((name.into(), value.into()));
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.attributes.retain(|(key, _)| key != name);
    }

    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    pub fn text(&mut self, bytes: &[u8]) {
        self.children.push(Child::Text(String::from_utf8_lossy(bytes).into_owned()));
    }

    pub fn append(&mut self, element: Self) {
        self.children.push(Child::Element(element));
    }
}

#[must_use]
pub fn print(element: &Element) -> String {
    let mut output = String::new();
    write_element(element, &mut output);
    output
}

fn escaped(text: &str, output: &mut String) {
    for c in text.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(c),
        }
    }
}

fn write_element(element: &Element, output: &mut String) {
    if !element.tag.is_empty() {
        write!(output, "<{}", element.tag).expect("writing a string succeeds");
        for (key, value) in &element.attributes {
            write!(output, " {key}=\"").expect("writing a string succeeds");
            escaped(value, output);
            output.push('"');
        }
        output.push('>');
    }
    for child in &element.children {
        match child {
            Child::Element(child) => write_element(child, output),
            Child::Text(text) => escaped(text, output),
        }
    }
    if !element.tag.is_empty()
        && ![
            "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track",
            "wbr",
        ]
        .contains(&element.tag.as_str())
    {
        write!(output, "</{}>", element.tag).expect("writing a string succeeds");
    }
}

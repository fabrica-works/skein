//! Typed calls and canonical answers for lab-browser.md, section 3.
use crate::json::{self, Value, number, object, string};
use skein_browser::boundary::{self as b, Rect};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug)]
pub enum Call {
    Serve { origin: String, dir: String },
    Open { page: String, width: u32, height: u32, mobile: bool },
    Go { page: String, url: String },
    Read { page: String, read: Read },
    Trouble { page: String },
    Close { page: String },
}
impl Call {
    pub fn page(&self) -> Option<&str> {
        match self {
            Self::Serve { .. } => None,
            Self::Open { page, .. }
            | Self::Go { page, .. }
            | Self::Read { page, .. }
            | Self::Trouble { page }
            | Self::Close { page } => Some(page),
        }
    }
}
/// The reads shared by the capture and the hand (lab-browser.md, 3 and 8.10).
#[derive(Debug)]
pub enum Read {
    Await(b::Query, u32),
    Find(b::Query),
    Tree,
    Select(String),
    Style(u64, Vec<String>),
    Shot(Option<Rect>, u8, String),
}
impl Read {
    pub fn request(
        self,
        page: skein_lib::Token,
        op: skein_lib::Token,
        properties_limit: u32,
    ) -> Result<b::Request, Value> {
        let look = match self {
            Self::Await(query, seconds) => {
                return Ok(b::Request::Await {
                    page,
                    op,
                    query,
                    expect: b::Expect::Present,
                    within: skein_lib::Duration::from_secs(u64::from(seconds)),
                });
            }
            Self::Find(query) => return Ok(b::Request::Find { page, op, query }),
            Self::Tree => b::Look::Tree,
            Self::Select(selector) => b::Look::Select { selector: selector.into_bytes().into() },
            Self::Style(node, properties) => {
                if properties.len() > usize::try_from(properties_limit).expect("property cap") {
                    return Err(refused("limit", "style property count"));
                }
                let mut names = skein_lib::List::with_capacity(properties_limit);
                for name in properties {
                    names.push(name.into_bytes().into()).expect("property admitted");
                }
                b::Look::Style { node, properties: names }
            }
            Self::Shot(clip, scale, _) => b::Look::Shot { clip, scale },
        };
        Ok(b::Request::Look { page, op, look })
    }
}
pub fn read_answer(event: &b::Event, op: skein_lib::Token) -> Option<Value> {
    match event {
        b::Event::Found { op: answer, seen: entries, more } if *answer == op => {
            Some(object([("found", seen(entries)), ("more", number(*more))]))
        }
        b::Event::Met { op: answer, seen: entries } if *answer == op => Some(object([("met", seen(entries))])),
        b::Event::Missed { op: answer, seen: entries, .. } if *answer == op => {
            Some(object([("missed", seen(entries))]))
        }
        b::Event::Looked { op: answer, looked: look } if *answer == op => looked(look),
        b::Event::Refused { op: answer, why } if *answer == op => Some(kit_refusal(*why)),
        b::Event::Ready { .. }
        | b::Event::Opened { .. }
        | b::Event::Found { .. }
        | b::Event::Met { .. }
        | b::Event::Missed { .. }
        | b::Event::Looked { .. }
        | b::Event::Refused { .. }
        | b::Event::Warned { .. }
        | b::Event::Done { .. }
        | b::Event::Address { .. }
        | b::Event::Attribute { .. }
        | b::Event::Snapshot { .. }
        | b::Event::Screenshot { .. }
        | b::Event::Trouble { .. }
        | b::Event::Closed { .. } => None,
    }
}
/// Parse section 3's read members without a page.
pub fn parse_read(kind: &str, members: BTreeMap<String, Value>) -> Result<Read, String> {
    let mut m = Members(members);
    let read = m.read(kind)?;
    m.end()?;
    Ok(read)
}
struct Members(BTreeMap<String, Value>);
impl Members {
    fn take(&mut self, key: &str) -> Result<Value, String> {
        self.0.remove(key).ok_or_else(|| format!("missing {key}"))
    }
    fn text(&mut self, key: &str) -> Result<String, String> {
        match self.take(key)? {
            Value::String(s) => Ok(s),
            other @ (Value::Object(_) | Value::Array(_) | Value::Number(_) | Value::Bool(_) | Value::Null) => {
                Err(format!("{key} must be a string: {other:?}"))
            }
        }
    }
    fn integer(&mut self, key: &str) -> Result<i64, String> {
        match self.take(key)? {
            Value::Number(n) => Ok(n),
            other @ (Value::Object(_) | Value::Array(_) | Value::String(_) | Value::Bool(_) | Value::Null) => {
                Err(format!("{key} must be an integer: {other:?}"))
            }
        }
    }
    fn unsigned(&mut self, key: &str) -> Result<u32, String> {
        self.integer(key)?.try_into().map_err(|_| format!("{key} outside u32"))
    }
    fn boolean(&mut self, key: &str) -> Result<bool, String> {
        match self.take(key)? {
            Value::Bool(b) => Ok(b),
            other @ (Value::Object(_) | Value::Array(_) | Value::String(_) | Value::Number(_) | Value::Null) => {
                Err(format!("{key} must be boolean: {other:?}"))
            }
        }
    }
    fn query(&mut self) -> Result<b::Query, String> {
        let matching = match self.text("match")?.as_str() {
            "exact" => b::Match::Exact,
            "words" => b::Match::Words,
            _ => return Err("match must be exact or words".into()),
        };
        Ok(b::Query {
            role: self.text("role")?.into_bytes().into(),
            name: self.text("name")?.into_bytes().into(),
            matching,
            within: None,
            boxes: true,
        })
    }
    fn read(&mut self, kind: &str) -> Result<Read, String> {
        let m = self;
        Ok(match kind {
            "await" => Read::Await(m.query()?, m.unsigned("seconds")?),
            "find" => Read::Find(m.query()?),
            "tree" => Read::Tree,
            "select" => Read::Select(m.text("selector")?),
            "style" => {
                let node = m.integer("node")?.try_into().map_err(|_| "node must be unsigned")?;
                let Value::Array(values) = m.take("properties")? else {
                    return Err("properties must be an array".into());
                };
                let properties = values
                    .into_iter()
                    .map(|value| match value {
                        Value::String(s) if s.is_ascii() => Ok(s),
                        Value::Object(_)
                        | Value::Array(_)
                        | Value::String(_)
                        | Value::Number(_)
                        | Value::Bool(_)
                        | Value::Null => Err("property must be an ASCII string"),
                    })
                    .collect::<Result<_, _>>()?;
                Read::Style(node, properties)
            }
            "shot" => {
                let clip = match m.take("clip")? {
                    Value::Null => None,
                    Value::Object(members) => {
                        let mut r = Members(members);
                        let rect = Rect {
                            x: r.integer("x")?,
                            y: r.integer("y")?,
                            width: r.integer("width")?.try_into().map_err(|_| "negative width")?,
                            height: r.integer("height")?.try_into().map_err(|_| "negative height")?,
                        };
                        r.end()?;
                        Some(rect)
                    }
                    Value::Array(_) | Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                        return Err("clip must be a rectangle or null".into());
                    }
                };
                let scale = match m.integer("scale")? {
                    1 => 1,
                    2 => 2,
                    _ => return Err("scale must be 1 or 2".into()),
                };
                let file = m.text("file")?;
                if !shot_file(&file) {
                    return Err("file must contain only normal relative components".into());
                }
                Read::Shot(clip, scale, file)
            }
            _ => return Err("unknown read".into()),
        })
    }
    fn end(self) -> Result<(), String> {
        if self.0.is_empty() { Ok(()) } else { Err(format!("unknown members: {:?}", self.0.keys())) }
    }
}
pub fn parse(line: &[u8]) -> Result<Call, String> {
    let value = json::parse(line)?;
    if json::encode(&value) != line {
        return Err("call must use canonical JSON".into());
    }
    let Value::Object(members) = value else {
        return Err("call must be an object".into());
    };
    let mut m = Members(members);
    let kind = m.text("call")?;
    let call = match kind.as_str() {
        "serve" => {
            let origin = m.text("origin")?;
            let dir = m.text("dir")?;
            if !Path::new(&dir).is_absolute() {
                return Err("dir must be absolute".into());
            }
            Call::Serve { origin, dir }
        }
        "open" => Call::Open {
            page: m.text("page")?,
            width: m.unsigned("width")?,
            height: m.unsigned("height")?,
            mobile: m.boolean("mobile")?,
        },
        "go" => Call::Go { page: m.text("page")?, url: m.text("url")? },
        "await" | "find" | "tree" | "select" | "style" | "shot" => {
            let page = m.text("page")?;
            Call::Read { page, read: m.read(&kind)? }
        }
        "trouble" => Call::Trouble { page: m.text("page")? },
        "close" => Call::Close { page: m.text("page")? },
        _ => return Err(format!("unknown call {kind}")),
    };
    m.end()?;
    Ok(call)
}
pub fn shot_file(file: &str) -> bool {
    !file.is_empty()
        && !file.contains('\0')
        && file.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
        && !Path::new(file).is_absolute()
}
pub fn on_origin(url: &str, addresses: impl Iterator<Item = impl AsRef<str>>) -> bool {
    addresses
        .into_iter()
        .any(|address| url.strip_prefix(address.as_ref()).is_some_and(|rest| rest.is_empty() || rest.starts_with('/')))
}
pub fn refused(why: &str, detail: impl Into<String>) -> Value {
    object([("detail", string(detail)), ("refused", string(why))])
}
pub fn kit_refusal(why: b::Refusal) -> Value {
    let why = match why {
        b::Refusal::Gone => "gone",
        b::Refusal::Hidden => "hidden",
        b::Refusal::Covered => "covered",
        b::Refusal::Disabled => "disabled",
        b::Refusal::Crashed => "crashed",
        b::Refusal::Limit => "limit",
        b::Refusal::Timeout => "timeout",
        b::Refusal::Protocol => "protocol",
    };
    refused(why, format!("browser refused: {why}"))
}
pub fn rect(rect: Option<Rect>) -> Value {
    rect.map_or(Value::Null, |r| {
        object([("height", number(r.height)), ("width", number(r.width)), ("x", number(r.x)), ("y", number(r.y))])
    })
}
pub fn seen(seen: &skein_lib::List<b::Seen>) -> Value {
    Value::Array(
        seen.iter()
            .map(|s| {
                object([
                    ("box", rect(s.rect)),
                    ("name", json::bytes(&s.name)),
                    ("node", number(s.node)),
                    ("role", json::bytes(&s.role)),
                ])
            })
            .collect(),
    )
}
pub fn looked(look: &b::Looked) -> Option<Value> {
    match look {
        b::Looked::Tree(entries) => Some(object([(
            "tree",
            Value::Array(
                entries
                    .iter()
                    .map(|e| {
                        object([
                            ("depth", number(e.depth)),
                            ("level", number(e.level)),
                            ("name", json::bytes(&e.name)),
                            ("node", e.node.map_or(Value::Null, number)),
                            ("role", json::bytes(&e.role)),
                        ])
                    })
                    .collect(),
            ),
        )])),
        b::Looked::Selected(node) => Some(object([("selected", node.map_or(Value::Null, number))])),
        b::Looked::Style { values, faces, rect: r } => Some(object([(
            "style",
            object([
                ("box", rect(*r)),
                (
                    "faces",
                    Value::Array(
                        faces
                            .iter()
                            .map(|f| {
                                object([
                                    ("custom", Value::Bool(f.custom)),
                                    ("family", json::bytes(&f.family)),
                                    ("glyphs", number(f.glyphs)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "values",
                    object(
                        values.iter().map(|p| (String::from_utf8_lossy(&p.name).into_owned(), json::bytes(&p.value))),
                    ),
                ),
            ]),
        )])),
        b::Looked::Viewport | b::Looked::Shot(_) => None,
    }
}
pub fn trouble(kind: &str, text: Value) -> Value {
    object([("kind", string(kind)), ("text", text)])
}
pub fn png_size(png: &[u8]) -> Result<(u32, u32), String> {
    if !png.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR") {
        return Err("browser returned invalid PNG IHDR".into());
    }
    let width = png.get(16..20).ok_or("truncated PNG width")?;
    let height = png.get(20..24).ok_or("truncated PNG height")?;
    Ok((
        u32::from_be_bytes(width.try_into().expect("four bytes")),
        u32::from_be_bytes(height.try_into().expect("four bytes")),
    ))
}
pub fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    skein_lib::sha256::digest(bytes).iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").expect("string write");
        s
    })
}

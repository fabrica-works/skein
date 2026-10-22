//! The hand's typed calls and answers (lab-browser.md, 8.3).
use crate::{
    calls::{self as capture, refused},
    json::{self, Value, object, string},
};
use skein_browser::boundary::{Go, Key, Rect, Seen};
use skein_fake_person::{Find, Target, role_name, role_named};
use skein_ui::Role;
use std::collections::BTreeMap;
#[derive(Debug)]
pub enum Call {
    Press(Part),
    Type(Part, String),
    Key(Key, bool),
    Choose(Part, Name),
    Go(String),
    History(Go),
    Address,
    Wait(Part, bool, u32),
    Shot,
    Trouble,
    SignIn(String),
    Outline(Vec<Target>),
    Look(LookCall),
}
impl Call {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Press(_) => "press",
            Self::Type(..) => "type",
            Self::Key(..) => "key",
            Self::Choose(..) => "choose",
            Self::Go(_) => "go",
            Self::History(_) => "history",
            Self::Address => "address",
            Self::Wait(..) => "wait",
            Self::Shot => "shot",
            Self::Trouble => "trouble",
            Self::SignIn(_) => "sign-in",
            Self::Outline(_) => "outline",
            Self::Look(look) => look.name(),
        }
    }
}
#[derive(Debug)]
pub enum LookCall {
    Viewport(Option<(u32, u32, bool)>),
    Await(skein_browser::boundary::Query, u32),
    Find(skein_browser::boundary::Query),
    Tree,
    Style(u64, Vec<String>),
    Shot(Option<Rect>, u8, String),
    Focus,
}
impl LookCall {
    fn name(&self) -> &'static str {
        match self {
            Self::Viewport(_) => "look-viewport",
            Self::Await(..) => "look-await",
            Self::Find(_) => "look-find",
            Self::Tree => "look-tree",
            Self::Style(..) => "look-style",
            Self::Shot(..) => "look-shot",
            Self::Focus => "look-focus",
        }
    }
}
fn look(kind: &str, members: BTreeMap<String, Value>) -> Result<LookCall, String> {
    if kind == "viewport" {
        if members.is_empty() {
            return Ok(LookCall::Viewport(None));
        }
        // The capture's open has the same three viewport members.
        if members.contains_key("page") {
            return Err("unknown member page".into());
        }
        let mut fields = members;
        fields.insert("call".into(), string("open"));
        fields.insert("page".into(), string("hand"));
        if let capture::Call::Open { width, height, mobile, .. } =
            capture::parse(&json::encode(&Value::Object(fields)))?
        {
            return Ok(LookCall::Viewport(Some((width, height, mobile))));
        }
        unreachable!("parsed open")
    }
    if kind == "focus" {
        Members(members).end()?;
        return Ok(LookCall::Focus);
    }
    Ok(match capture::parse_read(kind, members)? {
        capture::Read::Await(query, seconds) => LookCall::Await(query, seconds),
        capture::Read::Find(query) => LookCall::Find(query),
        capture::Read::Tree => LookCall::Tree,
        capture::Read::Style(node, properties) => LookCall::Style(node, properties),
        capture::Read::Shot(clip, scale, file) => {
            if !look_file(&file) {
                return Err("file must have at most eight lowercase path segments ending in .png".into());
            }
            LookCall::Shot(clip, scale, file)
        }
        capture::Read::Select(_) => return Err("unknown look call".into()),
    })
}
fn look_file(file: &str) -> bool {
    let Some(stem) = file.strip_suffix(".png") else { return false };
    stem.split('/').count() <= 8
        && stem.split('/').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
#[derive(Debug)]
pub struct Name {
    pub name: String,
    pub exact: bool,
}
#[derive(Debug)]
pub struct Part {
    pub role: Role,
    pub find: Find,
}
struct Members(BTreeMap<String, Value>);
impl Members {
    fn object(value: Value) -> Result<Self, String> {
        if let Value::Object(fields) = value { Ok(Self(fields)) } else { Err("expected an object".into()) }
    }
    fn take(&mut self, key: &str) -> Result<Value, String> {
        self.0.remove(key).ok_or_else(|| format!("missing {key}"))
    }
    fn text(&mut self, key: &str) -> Result<String, String> {
        if let Value::String(text) = self.take(key)? { Ok(text) } else { Err(format!("{key} must be a string")) }
    }
    fn boolean(&mut self, key: &str) -> Result<bool, String> {
        if let Value::Bool(value) = self.take(key)? { Ok(value) } else { Err(format!("{key} must be boolean")) }
    }
    fn name(&mut self) -> Result<Name, String> {
        let name = self.text("name")?;
        let exact = match self.text("match")?.as_str() {
            "exact" => true,
            "words" => false,
            _ => return Err("match must be exact or words".into()),
        };
        Ok(Name { name, exact })
    }
    fn target(&mut self) -> Result<(Role, Target), String> {
        let role = role_named(&self.text("role")?).ok_or("unknown person's role")?;
        let name = self.name()?;
        let find = if name.exact {
            Find::exactly_named(role, name.name.as_bytes())
        } else {
            Find::named(role, name.name.as_bytes())
        };
        Ok((role, find.target))
    }
    fn scope(&mut self) -> Result<Vec<Target>, String> {
        let Value::Array(values) = self.take("in")? else {
            return Err("in must be an array".into());
        };
        values
            .into_iter()
            .map(|value| {
                let mut m = Self::object(value)?;
                let (_, target) = m.target()?;
                m.end()?;
                Ok(target)
            })
            .collect()
    }
    fn part(&mut self) -> Result<Part, String> {
        let (role, target) = self.target()?;
        Ok(Part { role, find: Find { target, within: self.scope()?.into() } })
    }
    fn end(self) -> Result<(), String> {
        if self.0.is_empty() { Ok(()) } else { Err("unknown members".into()) }
    }
}
pub fn parse(line: &[u8]) -> Result<Call, Value> {
    parse_call(line).map_err(|detail| refused("call", detail))
}
fn parse_call(line: &[u8]) -> Result<Call, String> {
    let value = json::parse(line)?;
    if json::encode(&value) != line {
        return Err("call must use canonical JSON".into());
    }
    let mut m = Members::object(value)?;
    let kind = m.text("call")?;
    if let Some(kind) = kind.strip_prefix("look-") {
        return look(kind, m.0).map(Call::Look);
    }
    let call = match kind.as_str() {
        "press" => Call::Press(m.part()?),
        "type" => Call::Type(m.part()?, m.text("text")?),
        "key" => {
            let key = match m.text("key")?.as_str() {
                "Enter" => Key::Enter,
                "Escape" => Key::Escape,
                "Tab" => Key::Tab,
                _ => return Err("unknown key".into()),
            };
            Call::Key(key, m.boolean("shift")?)
        }
        "choose" => {
            let part = m.part()?;
            let mut option = Members::object(m.take("option")?)?;
            let name = option.name()?;
            option.end()?;
            Call::Choose(part, name)
        }
        "go" => Call::Go(m.text("path")?),
        "history" => Call::History(match m.text("to")?.as_str() {
            "reload" => Go::Reload,
            "back" => Go::Back,
            "forward" => Go::Forward,
            _ => return Err("unknown history destination".into()),
        }),
        "address" => Call::Address,
        "wait" => {
            let part = m.part()?;
            let gone = m.boolean("gone")?;
            let Value::Number(seconds) = m.take("seconds")? else {
                return Err("seconds must be an integer".into());
            };
            if !(1..=300).contains(&seconds) {
                return Err("seconds must be 1..=300".into());
            }
            Call::Wait(part, gone, u32::try_from(seconds).expect("bounded seconds"))
        }
        "shot" => Call::Shot,
        "trouble" => Call::Trouble,
        "sign-in" => Call::SignIn(m.text("link")?),
        "outline" => Call::Outline(m.scope()?),
        _ => return Err("unknown call".into()),
    };
    m.end()?;
    Ok(call)
}
pub fn path(path: &str) -> bool {
    path.starts_with('/') && !path.starts_with("//") && path.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}
pub fn link<'a>(origin: &str, link: &'a str, count: usize) -> Result<&'a str, Value> {
    let Some(rest) = link.strip_prefix(origin).filter(|rest| rest.starts_with('/') || rest.starts_with('?')) else {
        return Err(refused("origin", "sign-in link is outside the origin"));
    };
    // Reject browser URL spellings that could reinterpret the authority.
    if rest.contains('\\') || rest.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return Err(refused("origin", "invalid sign-in address"));
    }
    let Some((_, query)) = rest.split('#').next().expect("address before fragment").split_once('?') else {
        return Err(refused("call", "sign-in link needs a query"));
    };
    let query = query.split('#').next().expect("query segment");
    if query.len() < 16 || count >= 4 {
        return Err(refused("call", "sign-in query or link count outside bounds"));
    }
    Ok(query)
}
pub fn part(role: Role, seen: &Seen) -> Value {
    object([
        ("role", string(role_name(role))),
        ("name", json::bytes(&seen.name)),
        ("value", json::bytes(&seen.value)),
        ("states", Value::Array(crate::outline::states(seen.states).map(string).collect())),
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    fn canonical(line: &str) -> Vec<u8> {
        json::encode(&json::parse(line.as_bytes()).unwrap())
    }
    #[test]
    fn each_call_and_missing_unknown_mistyped_members() {
        let part = r#""role":"button","name":"Save","match":"words","in":[]"#;
        let cases = [
            format!(r#"{{"call":"press",{part}}}"#),
            format!(r#"{{"call":"type",{part},"text":"hello"}}"#),
            format!(r#"{{"call":"choose",{part},"option":{{"name":"Red","match":"exact"}}}}"#),
            format!(r#"{{"call":"wait",{part},"gone":false,"seconds":1}}"#),
            r#"{"call":"key","key":"Enter","shift":true}"#.into(),
            r#"{"call":"go","path":"/"}"#.into(),
            r#"{"call":"history","to":"back"}"#.into(),
            r#"{"call":"address"}"#.into(),
            r#"{"call":"shot"}"#.into(),
            r#"{"call":"trouble"}"#.into(),
            r#"{"call":"sign-in","link":"http://127.0.0.1:1/?abcdefghijklmnop"}"#.into(),
            r#"{"call":"outline","in":[{"role":"region","name":"Draft","match":"exact"}]}"#.into(),
        ];
        for case in cases {
            let bytes = canonical(&case);
            assert!(parse(&bytes).is_ok(), "{case}");
            let Value::Object(fields) = json::parse(&bytes).unwrap() else { panic!("object") };
            for key in fields.keys() {
                let mut missing = fields.clone();
                missing.remove(key);
                assert!(parse(&json::encode(&Value::Object(missing))).is_err(), "{case} missing {key}");
                let mut wrong = fields.clone();
                wrong.insert(key.clone(), Value::Null);
                assert!(parse(&json::encode(&Value::Object(wrong))).is_err());
            }
            let mut unknown = fields;
            unknown.insert("unknown".into(), Value::Null);
            assert!(parse(&json::encode(&Value::Object(unknown))).is_err());
        }
    }
    #[test]
    fn call_refusals_cover_nested_members_and_closed_vocabulary() {
        for case in [
            r#"{"call":"tree"}"#,
            r#"{"call":"key","key":"Backspace","shift":false}"#,
            r#"{"call":"history","to":"elsewhere"}"#,
            r#"{"call":"press","role":"textbox","name":"","match":"words","in":[]}"#,
            r#"{"call":"outline","in":[{"role":"main","name":"","match":"words","extra":0}]}"#,
            r#"{"call":"outline","in":[{"role":"main","name":"","match":"regexp"}]}"#,
            r#"{"call":"wait","role":"button","name":"","match":"words","in":[],"gone":true,"seconds":301}"#,
            r#"{"call":"wait","role":"button","name":"","match":"words","in":[],"gone":true,"seconds":0}"#,
        ] {
            assert!(parse(&canonical(case)).is_err(), "{case}");
        }
        assert!(parse(br#"{ "call":"address"}"#).is_err());
    }
    #[test]
    fn look_calls_parse_members_and_keep_their_wire_names() {
        for case in [
            r#"{"call":"look-viewport"}"#,
            r#"{"call":"look-viewport","width":390,"height":844,"mobile":true}"#,
            r#"{"call":"look-await","role":"textbox","name":"Message","match":"exact","seconds":1}"#,
            r#"{"call":"look-find","role":"textbox","name":"Message","match":"words"}"#,
            r#"{"call":"look-tree"}"#,
            r#"{"call":"look-style","node":42,"properties":["color"]}"#,
            r#"{"call":"look-shot","clip":null,"scale":1,"file":"viewport.png"}"#,
            r#"{"call":"look-shot","clip":{"x":-1,"y":2,"width":3,"height":4},"scale":2,"file":"a/b/c/d/e/f/g/h.png"}"#,
            r#"{"call":"look-focus"}"#,
        ] {
            let bytes = canonical(case);
            let call = parse(&bytes).unwrap();
            let Value::Object(fields) = json::parse(&bytes).unwrap() else { panic!("object") };
            assert_eq!(fields.get("call"), Some(&string(call.name())), "{case}");
            for key in fields.keys() {
                let mut wrong = fields.clone();
                wrong.insert(key.clone(), Value::Bool(false));
                // mobile's false value is valid; all other members reject a boolean.
                if key != "mobile" {
                    assert!(parse(&json::encode(&Value::Object(wrong))).is_err(), "{case} mistyped {key}");
                }
            }
            let mut unknown = fields;
            unknown.insert("page".into(), string("hand"));
            assert!(parse(&json::encode(&Value::Object(unknown))).is_err(), "{case}");
        }
    }
    #[test]
    fn look_calls_refuse_partial_viewports_bad_finders_and_bad_files() {
        for members in [
            r#""width":390"#,
            r#""height":844"#,
            r#""mobile":true"#,
            r#""width":390,"height":844"#,
            r#""width":390,"mobile":true"#,
            r#""height":844,"mobile":true"#,
            r#""width":-1,"height":844,"mobile":true"#,
            r#""width":390,"height":844,"mobile":null"#,
        ] {
            assert!(parse(&canonical(&format!(r#"{{"call":"look-viewport",{members}}}"#))).is_err(), "{members}");
        }
        for case in [
            r#"{"call":"look-find","role":"textbox","name":"Message","match":"regexp"}"#,
            r#"{"call":"look-find","role":1,"name":"Message","match":"exact"}"#,
            r#"{"call":"look-find","name":"Message","match":"exact"}"#,
            r#"{"call":"look-await","role":"textbox","name":"Message","match":"exact"}"#,
            r#"{"call":"look-await","role":"textbox","name":"Message","match":"exact","seconds":-1}"#,
            r#"{"call":"look-select","selector":"main"}"#,
            r#"{"call":"look-style","node":-1,"properties":[]}"#,
            r#"{"call":"look-style","node":1,"properties":[false]}"#,
            r#"{"call":"look-shot","clip":null,"scale":3,"file":"ok.png"}"#,
        ] {
            let answer = parse(&canonical(case)).unwrap_err();
            let Value::Object(fields) = answer else { panic!("refusal") };
            assert_eq!(fields.get("refused"), Some(&string("call")), "{case}");
        }
        for file in [
            "",
            "/a.png",
            "../a.png",
            "a/../b.png",
            "a//b.png",
            "./a.png",
            "a.PNG",
            "A.png",
            "a_b.png",
            "a.b.png",
            ".png",
            "a.png/b",
            "a/b/c/d/e/f/g/h/i.png",
            "a\\b.png",
            "a b.png",
        ] {
            let call = object([
                ("call", string("look-shot")),
                ("clip", Value::Null),
                ("scale", json::number(1)),
                ("file", string(file)),
            ]);
            assert!(parse(&json::encode(&call)).is_err(), "{file}");
        }
    }
    #[test]
    fn paths_and_sign_in_query_and_count_bounds() {
        for value in ["/", "/a?x=1", "/#fragment"] {
            assert!(path(value));
        }
        for value in ["", "//elsewhere", "a", "/space here", "/é", "/\n"] {
            assert!(!path(value));
        }
        let origin = "http://127.0.0.1:41";
        for count in 0..4 {
            assert!(link(origin, &format!("{origin}/?{}", "x".repeat(16)), count).is_ok());
        }
        for (value, count) in [
            (format!("{origin}/?{}", "x".repeat(15)), 0),
            (format!("{origin}/?{}", "x".repeat(16)), 4),
            (format!("{origin}/"), 0),
            (format!("{origin}/#fragment?abcdefghijklmnop"), 0),
        ] {
            assert!(link(origin, &value, count).is_err());
        }
        for value in [
            "https://127.0.0.1:41/?abcdefghijklmnop",
            "http://127.0.0.1:410/?abcdefghijklmnop",
            "http://127.0.0.1:41@elsewhere/?abcdefghijklmnop",
        ] {
            assert!(link(origin, value, 0).is_err());
        }
    }
}

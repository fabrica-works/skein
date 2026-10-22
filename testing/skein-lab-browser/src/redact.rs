//! The hand's sign-in queries, replaced longest first (lab-browser.md, 8.6).
use crate::json::Value;
#[derive(Default)]
pub struct Redact {
    queries: Vec<String>,
}
impl Redact {
    pub fn keep(&mut self, query: &str) {
        self.queries.push(query.to_owned());
        self.queries.sort_by_key(|query| std::cmp::Reverse(query.len()));
    }
    pub fn text(&self, text: &str) -> String {
        // Match the input once: replacement bytes are never scanned as credentials.
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            if let Some(query) = self.queries.iter().find(|query| !query.is_empty() && rest.starts_with(query.as_str()))
            {
                out.push_str("<link>");
                at += query.len();
            } else {
                let ch = rest.chars().next().expect("nonempty suffix");
                out.push(ch);
                at += ch.len_utf8();
            }
        }
        out
    }
    pub fn value(&self, value: &mut Value) {
        match value {
            Value::String(s) => *s = self.text(s),
            Value::Object(fields) => fields.values_mut().for_each(|value| self.value(value)),
            Value::Array(values) => values.iter_mut().for_each(|value| self.value(value)),
            Value::Number(_) | Value::Bool(_) | Value::Null => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queries_are_redacted_longest_first_in_every_string() {
        let mut redact = Redact::default();
        redact.keep("abcdefghijklmnop");
        redact.keep("abcdefghijklmnopqrstuv");
        redact.keep("é\"\\abcdefghijklmnop");
        assert_eq!(redact.text("url?abcdefghijklmnopqrstuv abcdefghijklmnop"), "url?<link> <link>");
        let mut value = crate::json::object([("address", crate::json::string("?é\"\\abcdefghijklmnop"))]);
        redact.value(&mut value);
        assert_eq!(crate::json::encode(&value), br#"{"address":"?<link>"}"#);
        assert_eq!(redact.text("unrelated"), "unrelated");
    }
}

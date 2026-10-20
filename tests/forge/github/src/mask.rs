//! GitHub's extra policy around the shared first-appearance mask.
use skein_forge_conformance::mask::Mask as Shared;
use std::collections::BTreeMap;
pub struct Mask {
    common: Shared,
    ids: BTreeMap<Vec<u8>, u64>,
    opaque: BTreeMap<Vec<u8>, usize>,
}
impl Default for Mask {
    fn default() -> Self {
        Self::new(0)
    }
}
impl Mask {
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self { common: Shared::new(port), ids: BTreeMap::new(), opaque: BTreeMap::new() }
    }
    fn id(&mut self, value: &[u8]) -> String {
        let next = u64::try_from(self.ids.len()).expect("bounded ids") + 1;
        self.ids.entry(value.to_vec()).or_insert(next).to_string()
    }
    fn opaque(&mut self, value: &[u8]) -> String {
        let next = self.opaque.len() + 1;
        let n = self.opaque.entry(value.to_vec()).or_insert(next);
        format!("opaque-{n}")
    }
    fn paths(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] == b'/' {
                out.push(b'/');
                at += 1;
                let start = at;
                while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
                if at > start && bytes.get(at).is_none_or(|b| b"/? #\"\r\n".contains(b)) {
                    out.extend_from_slice(self.id(&bytes[start..at]).as_bytes());
                } else {
                    out.extend_from_slice(&bytes[start..at]);
                }
            } else {
                out.push(bytes[at]);
                at += 1;
            }
        }
        out
    }
    /// Masks headers and signed blob addresses while retaining all other bytes for the leak scan.
    #[must_use]
    pub fn headers(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            let colon = line.iter().position(|b| *b == b':');
            if let Some(colon) = colon {
                let key = &line[..colon];
                if key.eq_ignore_ascii_case(b"authorization") {
                    out.extend_from_slice(key);
                    out.extend_from_slice(b": Bearer <masked>\r\n");
                    continue;
                }
                if key.eq_ignore_ascii_case(b"etag")
                    || key.eq_ignore_ascii_case(b"if-none-match")
                    || key.eq_ignore_ascii_case(b"x-github-request-id")
                {
                    out.extend_from_slice(key);
                    out.extend_from_slice(format!(": \"{}\"\r\n", self.opaque(&line[colon + 1..])).as_bytes());
                    continue;
                }
                if key.eq_ignore_ascii_case(b"location")
                    && (line.windows(6).any(|p| p == b"signed")
                        || line.windows(4).any(|p| p == b"sig=")
                        || line.windows(10).any(|p| p == b"signature=")
                        || line.windows(6).any(|p| p.eq_ignore_ascii_case(b"x-amz-")))
                {
                    out.extend_from_slice(b"Location: https://blob.example.test/<signed>\r\n");
                    continue;
                }
            }
            if line.starts_with(b"GET ")
                && (line.windows(10).any(|p| p == b"signature=")
                    || line.windows(4).any(|p| p == b"sig=")
                    || line.windows(6).any(|p| p.eq_ignore_ascii_case(b"x-amz-"))
                    || line.starts_with(b"GET /signed/"))
            {
                out.extend_from_slice(b"GET /<signed> HTTP/1.1\r\n");
                continue;
            }
            let mut framed = line.to_vec();
            framed.extend_from_slice(b"\r\n");
            let common = self.common.bytes(&framed);
            out.extend_from_slice(&self.paths(&common));
        }
        out
    }
    /// Masks numeric resource ids, node ids and the credentials returned by mint or conversion.
    #[must_use]
    pub fn body(&mut self, bytes: &[u8], credentials: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut at = 0;
        let mut key = Vec::new();
        while at < bytes.len() {
            if bytes[at] == b'"' {
                let start = at;
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == b'\\' {
                        at = at.saturating_add(2);
                        continue;
                    }
                    if bytes[at] == b'"' {
                        break;
                    }
                    at += 1;
                }
                if at >= bytes.len() {
                    out.extend_from_slice(&bytes[start..]);
                    break;
                }
                let value = &bytes[start + 1..at];
                at += 1;
                let mut next = at;
                while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                    next += 1;
                }
                if bytes.get(next) == Some(&b':') {
                    key = value.to_vec();
                    out.extend_from_slice(&bytes[start..at]);
                    continue;
                }
                if credentials
                    && [b"token".as_slice(), b"pem", b"webhook_secret", b"client_secret"].contains(&key.as_slice())
                {
                    out.extend_from_slice(b"\"<masked>\"");
                } else if key == b"id" && value.iter().all(u8::is_ascii_digit) {
                    out.extend_from_slice(format!("\"{}\"", self.id(value)).as_bytes());
                } else if key == b"node_id" || key == b"id" && !value.iter().all(u8::is_ascii_digit) {
                    out.extend_from_slice(format!("\"{}\"", self.opaque(value)).as_bytes());
                } else {
                    let common = self.common.bytes(&bytes[start..at]);
                    out.extend_from_slice(&self.paths(&common));
                }
            } else if bytes[at].is_ascii_digit()
                && (key == b"id"
                    || key == b"number"
                    || key == b"databaseId"
                    || key.ends_with(b"_id")
                    || key == b"repository_ids")
            {
                let start = at;
                while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
                out.extend_from_slice(self.id(&bytes[start..at]).as_bytes());
            } else {
                out.push(bytes[at]);
                at += 1;
            }
        }
        // GraphQL embeds a node id inside its query string after a preceding read named it.
        let mut known: Vec<_> = self.opaque.iter().filter(|(value, _)| !value.is_empty()).collect();
        known.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        for (value, number) in known {
            let mut replaced = Vec::new();
            let mut at = 0;
            while at < out.len() {
                if out[at..].starts_with(value) {
                    replaced.extend_from_slice(format!("opaque-{number}").as_bytes());
                    at += value.len();
                } else {
                    replaced.push(out[at]);
                    at += 1;
                }
            }
            out = replaced;
        }
        out
    }
}

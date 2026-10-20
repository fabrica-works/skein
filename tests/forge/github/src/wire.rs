use crate::Result;
use skein_forge::github::request::Method;
use skein_forge_conformance::json::Json;
#[derive(Clone)]
pub(crate) struct Request {
    pub method: Method,
    pub target: String,
    pub body: Vec<u8>,
    pub authorization: Option<String>,
    pub headers: Vec<(String, String)>,
    pub blob: bool,
}
impl Request {
    pub fn new(method: Method, target: impl Into<String>, token: Option<&str>, body: &[u8]) -> Self {
        Self {
            method,
            target: target.into(),
            body: body.to_vec(),
            authorization: token.map(|t| format!("Bearer {t}")),
            headers: vec![],
            blob: false,
        }
    }
    pub fn wire(&self, host: &str) -> Result<Vec<u8>> {
        let method = match self.method {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        };
        if self.target.bytes().any(|b| b <= 32 || b == 127) || host.contains(['\r', '\n']) {
            return Err("invalid request target".into());
        }
        let mut out = format!("{method} {} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: skein-conformance\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: {}\r\nConnection: close\r\n", self.target, skein_forge::github::pin::API_VERSION).into_bytes();
        if let Some(auth) = &self.authorization {
            if auth.contains(['\r', '\n']) {
                return Err("invalid credential bytes".into());
            }
            out.extend_from_slice(format!("Authorization: {auth}\r\n").as_bytes());
        }
        for (name, value) in &self.headers {
            if name.contains(['\r', '\n', ':']) || value.contains(['\r', '\n']) {
                return Err("invalid header field".into());
            }
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        if !self.body.is_empty() {
            let kind = if self.target.starts_with("/settings/") {
                "application/x-www-form-urlencoded"
            } else {
                "application/json"
            };
            out.extend_from_slice(format!("Content-Type: {kind}\r\n").as_bytes());
        }
        out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", self.body.len()).as_bytes());
        out.extend_from_slice(&self.body);
        Ok(out)
    }
}
#[derive(Clone)]
pub(crate) struct Response {
    pub status: u16,
    pub head: Vec<u8>,
    pub body: Vec<u8>,
}
impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let head = std::str::from_utf8(&self.head).ok()?;
        head.lines().skip(1).find_map(|line| {
            let (key, val) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| val.trim())
        })
    }
    pub fn json(&self) -> Result<Json> {
        Json::parse(&self.body)
    }
    pub fn logged_signature(&self, secret: &[u8]) -> Result<bool> {
        let json = self.json()?;
        let headers = json.root().get("request").get("headers");
        let signature = headers
            .get("X-Hub-Signature-256")
            .str()
            .or_else(|| headers.get("x-hub-signature-256").str())
            .ok_or("logged signature absent")?;
        let marker = b"\"payload\"";
        let at = self.body.windows(marker.len()).position(|v| v == marker).ok_or("logged payload absent")?;
        let rest = self.body[at + marker.len()..].trim_ascii_start();
        let rest = rest.strip_prefix(b":").ok_or("logged payload invalid")?.trim_ascii_start();
        if !rest.starts_with(b"{") {
            return Err("logged payload must be an object".into());
        }
        let mut depth = 0;
        let mut string = false;
        let mut escape = false;
        for (at, byte) in rest.iter().enumerate() {
            if string {
                if escape {
                    escape = false;
                } else if *byte == b'\\' {
                    escape = true;
                } else if *byte == b'"' {
                    string = false;
                }
            } else {
                match byte {
                    b'"' => string = true,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(skein_lib::sha256::equal(
                                signature.as_bytes(),
                                &skein_forge::github::delivery::sign(secret, &rest[..=at]),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        Err("logged payload incomplete".into())
    }
    pub fn is(&self, status: u16, message: &str) -> bool {
        self.status == status && self.json().is_ok_and(|j| j.root().get("message").is(message))
    }
}
pub(crate) fn parse(wire: &[u8]) -> Result<Option<Response>> {
    let Some(at) = wire.windows(4).position(|p| p == b"\r\n\r\n") else { return Ok(None) };
    let head = wire[..at].to_vec();
    let text = std::str::from_utf8(&head).map_err(|_| "invalid HTTP head")?;
    let status: u16 =
        text.split_whitespace().nth(1).ok_or("missing HTTP status")?.parse().map_err(|_| "invalid HTTP status")?;
    let mut response = Response { status, head, body: vec![] };
    let body = &wire[at + 4..];
    if matches!(status, 204 | 304) {
        return Ok(Some(response));
    }
    if response.header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        let mut rest = body;
        let mut unframed = Vec::new();
        loop {
            let Some(end) = rest.windows(2).position(|p| p == b"\r\n") else { return Ok(None) };
            let line = std::str::from_utf8(&rest[..end]).map_err(|_| "invalid chunk head")?;
            let count = usize::from_str_radix(line.split(';').next().ok_or("missing chunk length")?, 16)
                .map_err(|_| "invalid chunk length")?;
            rest = &rest[end + 2..];
            if count == 0 {
                if !rest.starts_with(b"\r\n") && !rest.windows(4).any(|p| p == b"\r\n\r\n") {
                    return Ok(None);
                }
                response.body = unframed;
                return Ok(Some(response));
            }
            let Some(chunk) = rest.get(..count) else { return Ok(None) };
            let Some(tail) = rest.get(count..) else { return Ok(None) };
            if tail.len() < 2 {
                return Ok(None);
            }
            if !tail.starts_with(b"\r\n") {
                return Err("invalid chunk end".into());
            }
            unframed.extend_from_slice(chunk);
            rest = &tail[2..];
        }
    }
    let count: usize = response
        .header("content-length")
        .ok_or("response has no framing")?
        .parse()
        .map_err(|_| "invalid response length")?;
    if count > 4_194_304 {
        return Err("response exceeds conformance bound".into());
    }
    if body.len() < count {
        return Ok(None);
    }
    response.body = body[..count].to_vec();
    Ok(Some(response))
}
pub(crate) fn framed(head: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in head.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let key = line.split(|b| *b == b':').next().unwrap_or_default();
        if line.is_empty()
            || key.eq_ignore_ascii_case(b"content-length")
            || key.eq_ignore_ascii_case(b"transfer-encoding")
        {
            continue;
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    out.extend_from_slice(body);
    out
}

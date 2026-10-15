//! Pure route answers and the policy on every answer (ui-web.md, 4.4).
use crate::{
    State,
    bundle::{self, Bundle},
};
use skein_http::server::{Body, Call, Response};
use skein_http::{Header, Method};
use std::path::Path;

const POLICY: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
#[derive(Debug)]
pub struct Answer {
    pub head: Response,
    pub body: Box<[u8]>,
}
fn header(name: &str, value: &str) -> Header {
    Header { name: Box::from(name.as_bytes()), value: Box::from(value.as_bytes()) }
}
fn answer(status: u16, body: Box<[u8]>, kind: &str, cache: Option<(&str, &str)>) -> Answer {
    let mut headers = vec![
        header("Content-Security-Policy", POLICY),
        header("X-Content-Type-Options", "nosniff"),
        header("Content-Type", kind),
    ];
    if let Some((name, value)) = cache {
        headers.push(header(name, value));
    }
    let length = u64::try_from(body.len()).expect("bounded answer fits u64");
    Answer {
        head: Response {
            status,
            headers: headers.into_boxed_slice(),
            body: if status == 304 { Body::None } else { Body::Length(length) },
            close: true,
        },
        body,
    }
}
#[must_use]
pub fn route(state: &mut State, bundle: &Bundle, call: &Call, body: &[u8]) -> Answer {
    let target = call.target.split(|byte| *byte == b'?').next().expect("split has first part");
    if call.method == Method::Post && target == b"/api/notes" {
        return match state.add(body) {
            Ok(note) => answer(200, note, "application/octet-stream", None),
            Err(status) => answer(status, Box::from(&b"add refused"[..]), "text/plain; charset=utf-8", None),
        };
    }
    if call.method != Method::Get {
        return answer(405, Box::from(&b"method not allowed"[..]), "text/plain; charset=utf-8", None);
    }
    if target == b"/api/notes" {
        return answer(200, state.list(), "application/octet-stream", None);
    }
    if let Some(path) = target.strip_prefix(b"/bundle/") {
        if let Ok(path) = core::str::from_utf8(path)
            && let Some((digest, name)) = path.split_once('/')
            && digest == bundle.digest
            && let Some(bytes) = bundle.files.get(name)
        {
            let kind = bundle::of(Path::new(name)).expect("startup checked the type");
            return answer(200, bytes.clone(), kind, Some(("Cache-Control", "public, max-age=31536000, immutable")));
        }
        return answer(404, Box::from(&b"not found"[..]), "text/plain; charset=utf-8", None);
    }
    let matched = call.headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case(b"if-none-match") && header.value.as_ref() == bundle.etag.as_bytes()
    });
    let mut result = answer(
        if matched { 304 } else { 200 },
        if matched { Box::from([]) } else { bundle.page.clone() },
        "text/html; charset=utf-8",
        Some(("Cache-Control", "no-cache")),
    );
    let mut headers = result.head.headers.into_vec();
    headers.push(header("ETag", &bundle.etag));
    result.head.headers = headers.into_boxed_slice();
    result
}

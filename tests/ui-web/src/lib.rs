//! Shared UI example fixtures, for host tests and the browser tier.
use skein_http::server::{Body, Call};
use skein_http::{Method, Version};
#[must_use]
pub fn get(path: &[u8]) -> Call {
    Call {
        method: Method::Get,
        target: Box::from(path),
        version: Version::Http11,
        headers: Box::from([]),
        body: Body::None,
    }
}

//! The bundle's closed list of extensions (ui-web.md, 4.4).
use std::path::Path;

/// The content type for a bundle file, or `None` outside the closed list.
#[must_use]
pub fn of(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "html" => Some("text/html; charset=utf-8"),
        "js" => Some("text/javascript"),
        "wasm" => Some("application/wasm"),
        "css" => Some("text/css"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "ico" => Some("image/x-icon"),
        "woff2" => Some("font/woff2"),
        "txt" => Some("text/plain; charset=utf-8"),
        "webmanifest" => Some("application/manifest+json"),
        _ => None,
    }
}

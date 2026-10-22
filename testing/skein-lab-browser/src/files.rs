//! Whole, bounded origin directories (lab-browser.md, 2.3).
use skein_fake_person_browser::origins::Page;
use std::{fs, io::Read, path::Path};
#[derive(Clone, Copy)]
pub struct Limits {
    pub file: usize,
    pub bytes: usize,
    pub count: usize,
}
pub const LIMITS: Limits = Limits { file: 8 << 20, bytes: 64 << 20, count: 4096 };
pub fn pages(dir: &Path, limits: Limits) -> Result<Vec<Page>, (&'static str, String)> {
    let mut pages = Vec::new();
    let mut bytes = 0;
    visit(dir, dir, limits, &mut pages, &mut bytes)?;
    pages.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(pages)
}
fn origin(e: impl std::fmt::Display) -> (&'static str, String) {
    ("origin", e.to_string())
}
fn visit(
    root: &Path,
    dir: &Path,
    limits: Limits,
    pages: &mut Vec<Page>,
    bytes: &mut usize,
) -> Result<(), (&'static str, String)> {
    if !fs::symlink_metadata(dir).map_err(origin)?.is_dir() {
        return Err(origin("origin directory is not a directory"));
    }
    for entry in fs::read_dir(dir).map_err(origin)? {
        let entry = entry.map_err(origin)?;
        let kind = entry.file_type().map_err(origin)?;
        let path = entry.path();
        if kind.is_dir() {
            visit(root, &path, limits, pages, bytes)?;
        } else if kind.is_file() {
            if pages.len() >= limits.count {
                return Err(("limit", "origin file count".into()));
            }
            let available = limits.file.min(limits.bytes.saturating_sub(*bytes));
            let mut body = Vec::new();
            fs::File::open(&path)
                .map_err(origin)?
                .take(u64::try_from(available).expect("limit fits") + 1)
                .read_to_end(&mut body)
                .map_err(origin)?;
            if body.len() > available {
                return Err(("limit", "origin or file bytes".into()));
            }
            *bytes += body.len();
            let relative = path
                .strip_prefix(root)
                .expect("walk stays in directory")
                .to_str()
                .ok_or_else(|| origin("origin path is not UTF-8"))?
                .to_owned();
            pages.push(Page {
                path: format!("/{relative}").into_bytes().into(),
                kind: mime(path.extension().and_then(|e| e.to_str()).unwrap_or("")).as_bytes().into(),
                body: body.into(),
                headers: Vec::new(),
            });
        } else {
            return Err(origin(format!("link or special file: {}", path.display())));
        }
    }
    Ok(())
}
pub fn mime(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "html" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

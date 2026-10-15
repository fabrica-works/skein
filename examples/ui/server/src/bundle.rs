//! Bounded startup reads and the server's independent type table (ui-web.md, 4.4).
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const FILE_BYTES: u32 = 4 << 20;
pub const BUNDLE_BYTES: u64 = 16 << 20;
pub const FILES: usize = 128;
#[derive(Debug)]
pub struct Bundle {
    pub page: Box<[u8]>,
    pub etag: String,
    pub digest: String,
    pub files: BTreeMap<String, Box<[u8]>>,
}
impl Bundle {
    pub fn new(page: Box<[u8]>, digest: String, files: BTreeMap<String, Box<[u8]>>) -> Result<Self, String> {
        if page.len() > usize::try_from(FILE_BYTES).expect("file bound fits usize")
            || digest.len() != 16
            || !digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || files.len() > FILES
        {
            return Err("bundle exceeds its limits or has an invalid digest".into());
        }
        let mut bytes = u64::try_from(page.len()).expect("page length fits");
        for (path, content) in &files {
            if path.len() > 1024
                || !Path::new(path).components().all(|part| matches!(part, Component::Normal(_)))
                || of(Path::new(path)).is_none()
                || content.len() > usize::try_from(FILE_BYTES).expect("file bound fits usize")
            {
                return Err("invalid bundle file".into());
            }
            bytes = bytes
                .checked_add(u64::try_from(content.len()).expect("file length fits"))
                .ok_or("bundle size overflow")?;
        }
        if bytes > BUNDLE_BYTES {
            return Err("bundle exceeds its byte limit".into());
        }
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in &page {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
        }
        let etag = format!("\"{hash:016x}\"");
        Ok(Self { page, etag, digest, files })
    }
    pub fn read(directory: &Path) -> Result<Self, String> {
        let page = skein_shell::read_file(&directory.join("index.html"), FILE_BYTES)
            .map_err(|why| format!("cannot read bundle page: {why:?}"))?;
        let mut digest = None;
        for entry in std::fs::read_dir(directory).map_err(|why| why.to_string())? {
            let entry = entry.map_err(|why| why.to_string())?;
            if entry.file_type().map_err(|why| why.to_string())?.is_dir() {
                if digest.is_some() {
                    return Err("bundle has more than one digest directory".into());
                }
                digest = Some(entry.file_name().into_string().map_err(|_| "bundle name is not UTF-8")?);
            }
        }
        let digest = digest.ok_or("bundle has no digest directory")?;
        let root = directory.join(&digest);
        let mut pending = vec![root.clone()];
        let mut files = BTreeMap::new();
        let mut bytes = u64::try_from(page.len()).expect("page fits u64");
        let mut entries = 0_usize;
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).map_err(|why| why.to_string())? {
                entries = entries.checked_add(1).ok_or("bundle entry count overflow")?;
                if entries > 256 {
                    return Err("bundle has too many entries".into());
                }
                let entry = entry.map_err(|why| why.to_string())?;
                let kind = entry.file_type().map_err(|why| why.to_string())?;
                let path = entry.path();
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() {
                    let content = skein_shell::read_file(&path, FILE_BYTES)
                        .map_err(|why| format!("cannot read bundle file: {why:?}"))?;
                    bytes = bytes
                        .checked_add(u64::try_from(content.len()).expect("file fits u64"))
                        .ok_or("bundle size overflow")?;
                    if bytes > BUNDLE_BYTES || files.len() >= FILES {
                        return Err("bundle exceeds its limits".into());
                    }
                    let name = path
                        .strip_prefix(&root)
                        .map_err(|why| why.to_string())?
                        .to_str()
                        .ok_or("bundle path is not UTF-8")?
                        .to_owned();
                    files.insert(name, content);
                } else {
                    return Err("bundle entries must be regular files or directories".into());
                }
            }
        }
        Self::new(page, digest, files)
    }
}
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

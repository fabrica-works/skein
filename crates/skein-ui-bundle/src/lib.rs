//! A reproducible browser bundle (ui-web.md, 4.2).
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

mod digest;
mod page;
mod sha256;
pub use digest::Digest;

/// What the application gives the bundler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inputs {
    pub name: String,
    /// One or more segments, with one leading and one trailing slash.
    pub prefix: String,
    pub stylesheets: Vec<PathBuf>,
    /// Each owner's prefix and its pictures' directory.
    pub pictures: Vec<(String, PathBuf)>,
    pub page: Option<PathBuf>,
    pub files: Option<PathBuf>,
    pub noscript: String,
}

/// The glue generated for one module, including its imported snippets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glue {
    pub js: Vec<u8>,
    pub wasm: Vec<u8>,
    pub snippets: Vec<(String, Vec<u8>)>,
}

/// An input that could not be bundled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoMarker { page: PathBuf },
    Io { path: PathBuf },
}

fn io<T>(path: &Path, result: std::io::Result<T>) -> Result<T, Refusal> {
    result.map_err(|_| Refusal::Io { path: path.to_owned() })
}

/// Replace `out` with the complete bundle, returning its directory's digest.
pub fn bundle(inputs: &Inputs, glue: &Glue, out: &Path) -> Result<Digest, Refusal> {
    let mut files = BTreeMap::new();
    files.insert("boot.js".into(), page::starter(&inputs.name));
    files.insert("skein-ui.css".into(), include_bytes!("../skein-ui.css").to_vec());
    files.insert(format!("{}.js", inputs.name), glue.js.clone());
    files.insert(format!("{}_bg.wasm", inputs.name), glue.wasm.clone());
    for (path, bytes) in &glue.snippets {
        files.insert(format!("snippets/{path}"), bytes.clone());
    }
    for path in &inputs.stylesheets {
        let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| Refusal::Io { path: path.clone() })?;
        files.insert(name.to_owned(), io(path, fs::read(path))?);
    }
    for (owner, directory) in &inputs.pictures {
        collect(directory, &format!("pictures/{owner}"), &mut files)?;
    }
    if let Some(directory) = &inputs.files {
        collect(directory, "files", &mut files)?;
    }
    let digest = digest::of(&files);
    let page = page::render(inputs, digest)?;
    if out.exists() {
        io(out, fs::remove_dir_all(out))?;
    }
    let directory = out.join(digest.to_string());
    io(&directory, fs::create_dir_all(&directory))?;
    for (name, bytes) in files {
        let path = directory.join(name);
        let parent = path.parent().expect("a bundled file has a directory");
        io(parent, fs::create_dir_all(parent))?;
        io(&path, fs::write(&path, bytes))?;
    }
    let index = out.join("index.html");
    io(&index, fs::write(&index, page))?;
    Ok(digest)
}

fn collect(directory: &Path, prefix: &str, files: &mut BTreeMap<String, Vec<u8>>) -> Result<(), Refusal> {
    for entry in io(directory, fs::read_dir(directory))? {
        let entry = io(directory, entry)?;
        let path = entry.path();
        let name = entry.file_name().into_string().map_err(|_| Refusal::Io { path: path.clone() })?;
        let name = format!("{prefix}/{name}");
        let kind = io(&path, entry.file_type())?;
        if kind.is_dir() {
            collect(&path, &name, files)?;
        } else if kind.is_file() {
            files.insert(name, io(&path, fs::read(&path))?);
        } else {
            return Err(Refusal::Io { path });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

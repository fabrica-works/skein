//! An owner crate's files, read (codec.md, 12.1).

use alloc::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::Error;

/// A schema text with its header's family and version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Text {
    /// Relative to the crate.
    pub path: PathBuf,
    pub family: String,
    pub version: u16,
    pub source: String,
}

/// What a run and the check read of a crate (codec.md, 12.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Crate {
    pub root: PathBuf,
    /// Each family's current text: its greatest version in `schema/`.
    pub current: BTreeMap<String, Text>,
    /// Each family's kept texts, by version.
    pub kept: BTreeMap<String, BTreeMap<u16, Text>>,
    /// Each family's waiting renames file, `schema/kept/<family>.renames`.
    pub renames: BTreeMap<String, String>,
    /// Each patch's text, `patches/NNNN.patch`, by number.
    pub patches: BTreeMap<u32, String>,
    /// The numbers of the patches with a `patches/pNNNN.rs`.
    pub skeletons: BTreeSet<u32>,
}

pub(crate) const SCHEMA: &str = "schema";
pub(crate) const KEPT: &str = "schema/kept";
pub(crate) const PATCHES: &str = "patches";

/// `schema/kept/<family>-v<N>.schema`.
pub(crate) fn kept_path(family: &str, version: u16) -> PathBuf {
    Path::new(KEPT).join(format!("{family}-v{version}.schema"))
}

/// `schema/kept/<family>.renames`.
pub(crate) fn renames_path(family: &str) -> PathBuf {
    Path::new(KEPT).join(format!("{family}.renames"))
}

/// `patches/NNNN.patch`.
pub(crate) fn patch_path(number: u32) -> PathBuf {
    Path::new(PATCHES).join(format!("{number:04}.patch"))
}

/// `patches/pNNNN.rs`.
pub(crate) fn skeleton_path(number: u32) -> PathBuf {
    Path::new(PATCHES).join(format!("p{number:04}.rs"))
}

/// `patches/golden/NNNN/`.
pub(crate) fn golden_path(number: u32) -> PathBuf {
    Path::new(PATCHES).join("golden").join(format!("{number:04}"))
}

impl Crate {
    /// Reads the base without consulting git or writing a temporary tree.
    pub(crate) fn base(root: &Path, landed: &super::Landed) -> Result<Self, Error> {
        let mut base = Self {
            root: root.to_path_buf(),
            current: BTreeMap::new(),
            kept: BTreeMap::new(),
            renames: BTreeMap::new(),
            patches: BTreeMap::new(),
            skeletons: BTreeSet::new(),
        };
        for (path, bytes) in &landed.files {
            if path.parent() == Some(Path::new(KEPT)) && path.extension().is_some_and(|ext| ext == "schema") {
                let source = String::from_utf8(bytes.clone())
                    .map_err(|error| Error::Schema { path: path.clone(), error: error.to_string() })?;
                let schema = crate::parse(&source)
                    .map_err(|error| Error::Schema { path: path.clone(), error: error.to_string() })?;
                if kept_path(&schema.family, schema.version) != *path {
                    return Err(Error::Schema {
                        path: path.clone(),
                        error: "kept filename differs from its header".into(),
                    });
                }
                let text = Text { path: path.clone(), family: schema.family, version: schema.version, source };
                base.kept.entry(text.family.clone()).or_default().insert(text.version, text);
            } else if path.parent() == Some(Path::new(PATCHES)) {
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
                if let Some(number) = name.strip_suffix(".patch").and_then(number) {
                    let source = String::from_utf8(bytes.clone())
                        .map_err(|error| Error::Schema { path: path.clone(), error: error.to_string() })?;
                    base.patches.insert(number, source);
                }
            }
        }
        Ok(base)
    }

    /// Reads the crate's current texts, kept texts, waiting renames and patches.
    pub(crate) fn read(root: &Path) -> Result<Self, Error> {
        let mut current: BTreeMap<String, Text> = BTreeMap::new();
        for path in listed(root, Path::new(SCHEMA))? {
            if path.extension().is_some_and(|extension| extension == "schema") {
                let text = schema_text(root, &path)?;
                let greater = current.get(&text.family).is_none_or(|held| held.version < text.version);
                if greater {
                    current.insert(text.family.clone(), text);
                }
            }
        }
        let mut kept: BTreeMap<String, BTreeMap<u16, Text>> = BTreeMap::new();
        let mut renames = BTreeMap::new();
        for path in listed(root, Path::new(KEPT))? {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            if let Some(family) = name.strip_suffix(".renames") {
                renames.insert(family.to_owned(), unconflicted(root, &path)?);
            } else if path.extension().is_some_and(|extension| extension == "schema") {
                let text = schema_text(root, &path)?;
                if kept_path(&text.family, text.version) != path {
                    return Err(Error::Schema {
                        path,
                        error: format!("a kept text is named {}-v{}.schema", text.family, text.version),
                    });
                }
                kept.entry(text.family.clone()).or_default().insert(text.version, text);
            }
        }
        let mut patches = BTreeMap::new();
        let mut skeletons = BTreeSet::new();
        for path in listed(root, Path::new(PATCHES))? {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            if let Some(number) = name.strip_suffix(".patch").and_then(number) {
                patches.insert(number, unconflicted(root, &path)?);
            } else if let Some(number) =
                name.strip_prefix('p').and_then(|rest| rest.strip_suffix(".rs")).and_then(number)
            {
                skeletons.insert(number);
            }
        }
        Ok(Self { root: root.to_path_buf(), current, kept, renames, patches, skeletons })
    }
}

/// Every file under the crate's `patches/` and `schema/kept/`, by path
/// relative to the crate: [`super::Landed::read`].
pub(crate) fn landed(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, Error> {
    let mut files = BTreeMap::new();
    let mut directories = vec![PathBuf::from(PATCHES), PathBuf::from(KEPT)];
    while let Some(directory) = directories.pop() {
        let full = root.join(&directory);
        if !full.is_dir() {
            continue;
        }
        let entries = fs::read_dir(&full).map_err(|error| Error::io(&full, &error))?;
        for entry in entries {
            let entry = entry.map_err(|error| Error::io(&full, &error))?;
            let path = directory.join(entry.file_name());
            if entry.path().is_dir() {
                directories.push(path);
            } else {
                let bytes = fs::read(entry.path()).map_err(|error| Error::io(&entry.path(), &error))?;
                files.insert(path, bytes);
            }
        }
    }
    Ok(files)
}

/// A four-digit patch number.
fn number(digits: &str) -> Option<u32> {
    if digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()) { digits.parse().ok() } else { None }
}

/// The files directly in a directory of the crate, relative to it, in order;
/// none when it is absent.
fn listed(root: &Path, directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let full = root.join(directory);
    if !full.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&full).map_err(|error| Error::io(&full, &error))? {
        let entry = entry.map_err(|error| Error::io(&full, &error))?;
        if entry.path().is_file() {
            paths.push(directory.join(entry.file_name()));
        }
    }
    paths.sort();
    Ok(paths)
}

/// A text file of the crate, refused when it holds conflict markers (12.6).
pub(crate) fn unconflicted(root: &Path, path: &Path) -> Result<String, Error> {
    let full = root.join(path);
    let text = fs::read_to_string(&full).map_err(|error| Error::io(&full, &error))?;
    if text.lines().any(|line| line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> ")) {
        return Err(Error::Conflict { path: path.to_path_buf() });
    }
    Ok(text)
}

fn schema_text(root: &Path, path: &Path) -> Result<Text, Error> {
    let source = unconflicted(root, path)?;
    let schema =
        crate::parse(&source).map_err(|error| Error::Schema { path: path.to_path_buf(), error: error.to_string() })?;
    Ok(Text { path: path.to_path_buf(), family: schema.family, version: schema.version, source })
}

//! Required paths and isolated font configuration (lab-browser.md, 2).
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};
pub const USAGE: &str =
    "usage: skein-lab-browser --chromium PATH --fonts DIR --scratch DIR --out DIR (absolute paths required)";
#[derive(Debug)]
pub struct Config {
    pub chromium: PathBuf,
    pub fonts: PathBuf,
    pub scratch: PathBuf,
    pub out: PathBuf,
}
impl Config {
    pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, &'static str> {
        let mut args = args;
        let mut values = BTreeMap::new();
        while let Some(key) = args.next() {
            if !["--chromium", "--fonts", "--scratch", "--out"].iter().any(|k| key == *k) {
                return Err(USAGE);
            }
            let value = PathBuf::from(args.next().ok_or(USAGE)?);
            if !value.is_absolute() || values.insert(key, value).is_some() {
                return Err(USAGE);
            }
        }
        Ok(Self {
            chromium: values.remove(&OsString::from("--chromium")).ok_or(USAGE)?,
            fonts: values.remove(&OsString::from("--fonts")).ok_or(USAGE)?,
            scratch: values.remove(&OsString::from("--scratch")).ok_or(USAGE)?,
            out: values.remove(&OsString::from("--out")).ok_or(USAGE)?,
        })
    }
}
pub fn fonts(fonts: &str, scratch: &str) -> String {
    let fonts = escape(fonts);
    let scratch = escape(scratch);
    format!(
        "<fontconfig><dir>{fonts}</dir><cachedir>{scratch}/fontconfig</cachedir><include ignore_missing=\"yes\">{fonts}/aliases.conf</include></fontconfig>\n"
    )
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

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
    pub hand: Option<Hand>,
}
impl Config {
    pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, &'static str> {
        let mut args = args.peekable();
        let hand = args.peek().is_some_and(|word| word == "hand");
        if hand {
            args.next();
        }
        let usage = if hand { HAND_USAGE } else { USAGE };
        let mut hand_values = BTreeMap::new();
        let mut mobile = false;
        let mut values = BTreeMap::new();
        while let Some(key) = args.next() {
            if hand && key == "--mobile" {
                if mobile {
                    return Err(HAND_USAGE);
                }
                mobile = true;
                continue;
            }
            if hand && ["--origin", "--size", "--listen"].iter().any(|k| key == *k) {
                let value = args.next().ok_or(HAND_USAGE)?.into_string().map_err(|_| HAND_USAGE)?;
                if hand_values.insert(key, value).is_some() {
                    return Err(HAND_USAGE);
                }
                continue;
            }
            if !["--chromium", "--fonts", "--scratch", "--out"].iter().any(|k| key == *k) {
                return Err(usage);
            }
            let value = PathBuf::from(args.next().ok_or(usage)?);
            if !value.is_absolute() || values.insert(key, value).is_some() {
                return Err(usage);
            }
        }
        Ok(Self {
            chromium: values.remove(&OsString::from("--chromium")).ok_or(usage)?,
            fonts: values.remove(&OsString::from("--fonts")).ok_or(usage)?,
            scratch: values.remove(&OsString::from("--scratch")).ok_or(usage)?,
            out: values.remove(&OsString::from("--out")).ok_or(usage)?,
            hand: if hand { Some(Hand::parse(hand_values, mobile)?) } else { None },
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

pub const HAND_USAGE: &str = "usage: skein-lab-browser hand --chromium PATH --fonts DIR --scratch DIR --out DIR --origin URL --size WIDTHxHEIGHT [--mobile] --listen ADDRESS";
#[derive(Debug)]
pub struct Hand {
    pub origin: String,
    pub authority: String,
    pub width: u32,
    pub height: u32,
    pub mobile: bool,
    pub listen: std::net::SocketAddr,
}
impl Hand {
    fn parse(mut values: BTreeMap<OsString, String>, mobile: bool) -> Result<Self, &'static str> {
        let origin = values.remove(&OsString::from("--origin")).ok_or(HAND_USAGE)?;
        let origin = origin.strip_suffix('/').unwrap_or(&origin).to_owned();
        let authority = origin.strip_prefix("http://").ok_or(HAND_USAGE)?.to_owned();
        let address = authority.parse::<std::net::SocketAddr>().map_err(|_| HAND_USAGE)?;
        if ![std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)]
            .contains(&address.ip())
            || address.port() == 0
            || authority != address.to_string()
        {
            return Err(HAND_USAGE);
        }
        let size = values.remove(&OsString::from("--size")).ok_or(HAND_USAGE)?;
        let (width, height) = size.split_once('x').ok_or(HAND_USAGE)?;
        if !width.bytes().all(|byte| byte.is_ascii_digit()) || !height.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(HAND_USAGE);
        }
        let width = width.parse::<u32>().map_err(|_| HAND_USAGE)?;
        let height = height.parse::<u32>().map_err(|_| HAND_USAGE)?;
        if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {
            return Err(HAND_USAGE);
        }
        let listen = values
            .remove(&OsString::from("--listen"))
            .ok_or(HAND_USAGE)?
            .parse::<std::net::SocketAddr>()
            .map_err(|_| HAND_USAGE)?;
        if !listen.ip().is_loopback() || listen.port() != 0 {
            return Err(HAND_USAGE);
        }
        Ok(Self { origin, authority, width, height, mobile, listen })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args() -> Vec<OsString> {
        [
            "hand",
            "--chromium",
            "/chromium",
            "--fonts",
            "/fonts",
            "--scratch",
            "/scratch",
            "--out",
            "/out",
            "--origin",
            "http://127.0.0.1:41/",
            "--size",
            "1x16384",
            "--listen",
            "127.0.0.1:0",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }
    #[test]
    fn hand_command_line_size_origin_listener_and_mobile() {
        let mut words = args();
        let config = Config::parse(words.clone().into_iter()).unwrap();
        let hand = config.hand.unwrap();
        assert_eq!((hand.width, hand.height, hand.mobile), (1, 16384, false));
        assert_eq!(hand.origin, "http://127.0.0.1:41");
        words.push("--mobile".into());
        words[10] = "http://[::1]:41".into();
        words[14] = "[::1]:0".into();
        assert!(Config::parse(words.into_iter()).unwrap().hand.unwrap().mobile);
        for (index, value) in [
            (10, "https://127.0.0.1:41"),
            (10, "http://localhost:41"),
            (10, "http://127.0.0.2:41"),
            (10, "http://127.0.0.1"),
            (10, "http://127.0.0.1:41/path"),
            (10, "http://127.0.0.1:41/?query"),
            (10, "http://127.0.0.1:41/#fragment"),
            (10, "http://127.0.0.1:0"),
            (12, "0x1"),
            (12, "1x0"),
            (12, "16385x1"),
            (12, "1x16385"),
            (12, "1x2x3"),
            (12, "-1x1"),
            (12, "+1x1"),
            (14, "0.0.0.0:0"),
            (14, "127.0.0.1:41"),
            (2, "relative"),
        ] {
            let mut words = args();
            words[index] = value.into();
            assert_eq!(Config::parse(words.into_iter()).unwrap_err(), HAND_USAGE, "{index}: {value}");
        }
    }
    #[test]
    fn hand_command_line_requires_each_argument_once() {
        let words = args();
        for index in (1..words.len()).step_by(2) {
            let mut missing = words.clone();
            missing.drain(index..index + 2);
            assert!(Config::parse(missing.into_iter()).is_err());
            let mut duplicate = words.clone();
            duplicate.extend_from_slice(&words[index..index + 2]);
            assert!(Config::parse(duplicate.into_iter()).is_err());
            let mut missing_value = words[..=index].to_vec();
            assert!(Config::parse(missing_value.drain(..)).is_err());
        }
        for extra in [vec!["--mobile", "--mobile"], vec!["--unknown"], vec!["capture"]] {
            let mut words = args();
            words.extend(extra.into_iter().map(OsString::from));
            assert!(Config::parse(words.into_iter()).is_err());
        }
    }
}

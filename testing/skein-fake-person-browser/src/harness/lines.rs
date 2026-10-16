use super::{Names, Opened, Output, Printed};
use skein_io::kernel::{Pipe, Way};
use std::{ffi::OsString, os::unix::ffi::OsStrExt};
/// Each complete matching line, as a URL and its authority.
#[must_use]
pub fn printed(output: &[u8], printed: &Printed<'_>) -> Vec<Opened> {
    output
        .split_inclusive(|b| *b == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .filter_map(|line| {
            let rest = line.strip_prefix(printed.prefix)?;
            let line = std::str::from_utf8(line).ok()?.trim();
            let address = std::str::from_utf8(rest).ok()?.trim();
            let (url, address) = match printed.names {
                Names::Address => (format!("http://{address}/"), address.to_owned()),
                Names::Url => (line.to_owned(), line.split_once("://")?.1.split(['/', '?', '#']).next()?.to_owned()),
            };
            Some(Opened { url, address })
        })
        .collect()
}
pub(crate) fn restart(args: &[OsString], address: &str) -> Vec<Box<[u8]>> {
    args.iter().map(|arg| replace_address(arg.as_bytes(), address)).collect()
}
pub(crate) fn replace_address(bytes: &[u8], address: &str) -> Box<[u8]> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(at) = rest.windows(9).position(|part| part == b"{address}") {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(address.as_bytes());
        rest = &rest[at + 9..];
    }
    out.extend_from_slice(rest);
    out.into()
}
pub(crate) fn server_pipes(on: Output) -> Box<[Pipe]> {
    let mut pipes = vec![Pipe { child: 2, way: Way::Out, parent: None }];
    if on == Output::Stdout {
        pipes.push(Pipe { child: 1, way: Way::Out, parent: None });
    }
    pipes.into()
}
pub(crate) fn opening(lines: &[Opened], names: Names, index: usize) -> Option<&Opened> {
    lines.get(if names == Names::Url { index } else { 0 })
}

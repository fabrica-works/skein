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
pub(crate) fn restart(args: &[OsString], address: &str, peers: &[(Box<[u8]>, String)]) -> Vec<Box<[u8]>> {
    args.iter().map(|arg| substitute(arg.as_bytes(), Some(address), peers)).collect()
}
/// Each placeholder is read from the original bytes exactly once. Inserted
/// addresses are never scanned, and unknown braces remain byte for byte.
pub(crate) fn substitute(bytes: &[u8], address: Option<&str>, peers: &[(Box<[u8]>, String)]) -> Box<[u8]> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some(at) = rest.iter().position(|byte| *byte == b'{') {
        out.extend_from_slice(&rest[..at]);
        rest = &rest[at..];
        let known = if rest.starts_with(b"{address}") {
            address.map(|address| (9, address))
        } else {
            peers.iter().find_map(|(name, value)| {
                let end = name.len().checked_add(1)?;
                if rest.get(1..end) == Some(name.as_ref()) && rest.get(end) == Some(&b'}') {
                    Some((end.checked_add(1)?, value.as_str()))
                } else {
                    None
                }
            })
        };
        if let Some((end, value)) = known {
            out.extend_from_slice(value.as_bytes());
            rest = &rest[end..];
        } else {
            out.push(b'{');
            rest = &rest[1..];
        }
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

#[cfg(test)]
mod tests {
    use super::substitute;
    #[test]
    fn repeated_peer_and_server_placeholders_preserve_other_bytes() {
        let peers = vec![(b"neighbor".as_slice().into(), "127.0.0.1:8".into())];
        assert_eq!(
            substitute(b"\xff{neighbor}/{address}/{neighbor}/{other}", Some("127.0.0.1:9"), &peers).as_ref(),
            b"\xff127.0.0.1:8/127.0.0.1:9/127.0.0.1:8/{other}"
        );
    }
    #[test]
    fn initial_arguments_preserve_the_unknown_server_address() {
        assert_eq!(
            substitute(b"{address}/{neighbor}", None, &[(b"neighbor".as_slice().into(), "127.0.0.1:8".into())])
                .as_ref(),
            b"{address}/127.0.0.1:8"
        );
    }
    #[test]
    fn inserted_values_are_not_rewritten_in_either_peer_order() {
        let mut peers =
            vec![(b"first".as_slice().into(), "{second}".into()), (b"second".as_slice().into(), "127.0.0.1:8".into())];
        for _ in 0..2 {
            assert_eq!(substitute(b"{first}/{second}", None, &peers).as_ref(), b"{second}/127.0.0.1:8");
            peers.reverse();
        }
    }
}

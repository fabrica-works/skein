//! Navigation targets admitted by the web platform (ui-web.md, section 10).

pub fn allowed(to: &[u8], loopback: bool) -> bool {
    if !to.iter().all(|byte| (0x21..=0x7e).contains(byte)) {
        return false;
    }
    if to.starts_with(b"/") {
        return !to.starts_with(b"//") && !to.contains(&b'\\');
    }
    if let Some(address) = to.strip_prefix(b"https://") {
        return !address.is_empty();
    }
    if !loopback {
        return false;
    }
    let Some(rest) = to.strip_prefix(b"http://127.0.0.1:").or_else(|| to.strip_prefix(b"http://[::1]:")) else {
        return false;
    };
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    (1..=5).contains(&digits) && (digits == rest.len() || matches!(rest.get(digits), Some(b'/' | b'?')))
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn navigation_targets_follow_the_origin_https_and_loopback_rules() {
        let rows: &[(&[u8], bool, bool)] = &[
            (b"/", false, true),
            (b"/page?next=https://example.com", false, true),
            (b"//example.com", false, false),
            (b"/\\example.com", false, false),
            (b"https://example.com", false, true),
            (b"https://", false, false),
            (b"javascript:alert(1)", false, false),
            (b"data:text/html,hello", false, false),
            (b"http://example.com", true, false),
            (b"http://127.0.0.1.evil/", true, false),
            (b"http://localhost:80/", true, false),
            (b"http://127.0.0.1:1", false, false),
            (b"http://127.0.0.1:1", true, true),
            (b"http://127.0.0.1:12345/path", true, true),
            (b"http://127.0.0.1:123456/", true, false),
            (b"http://127.0.0.1:/", true, false),
            (b"http://127.0.0.1:80.evil/", true, false),
            (b"http://127.0.0.1:80@evil/", true, false),
            (b"http://127.0.0.1:80#fragment", true, false),
            (b"http://127.0.0.1:80\\evil/", true, false),
            (b"http://[::1]:80/", false, false),
            (b"http://[::1]:80/", true, true),
            (b"http://[::1]:80?query", true, true),
            (b"http://[::1]:80", true, true),
            (b"http://[::1]:/", true, false),
            (b"http://[::1]:123456/", true, false),
            (b"http://[::1]:80.evil/", true, false),
            (b"http://[::1]:80@evil/", true, false),
            (b"https://example.com/space here", true, false),
            (b"/space here", true, false),
            (b"\thttps://example.com", true, false),
            (b"https://example.com\r\n", true, false),
            (b"/\x7f", false, false),
            (b"/\xff", false, false),
            (b"", true, false),
        ];
        for &(target, loopback, expected) in rows {
            assert_eq!(allowed(target, loopback), expected, "{target:?}, loopback={loopback}");
        }
    }
}

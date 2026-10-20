pub use skein_forge_conformance::mask::Mask;
use skein_throwaway_forgejo::Exchange;
#[must_use]
pub fn request(mask: &mut Mask, exchange: &Exchange) -> Vec<u8> {
    mask.bytes(&exchange.sent)
}
#[must_use]
pub fn response(mask: &mut Mask, exchange: &Exchange) -> Vec<u8> {
    let head = mask.bytes(&exchange.head);
    let body = sorted_units(&mask.bytes(&exchange.body));
    let mut out = Vec::new();
    for line in head.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let key = line.split(|b| *b == b':').next().unwrap_or_default();
        if key.eq_ignore_ascii_case(b"content-length") || key.eq_ignore_ascii_case(b"transfer-encoding") {
            continue;
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    out.extend_from_slice(&body);
    out
}

fn sorted_units(bytes: &[u8]) -> Vec<u8> {
    const KEY: &[u8] = b"\"units\":[";
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(start) = rest.windows(KEY.len()).position(|part| part == KEY) {
        let array = start + KEY.len() - 1;
        let Some(end) = rest[array..].iter().position(|byte| *byte == b']') else { break };
        let end = array + end + 1;
        let Ok(json) = crate::json::Json::parse(&rest[array..end]) else { break };
        let Some(mut units) = json.root().items().iter().map(|item| item.str()).collect::<Option<Vec<_>>>() else {
            break;
        };
        units.sort_unstable();
        out.extend_from_slice(&rest[..array]);
        out.extend_from_slice(
            &crate::json::write(|out| {
                out.array_start();
                for unit in &units {
                    out.string(unit.as_bytes());
                }
                out.array_end();
            })
            .expect("unit strings"),
        );
        rest = &rest[end..];
    }
    out.extend_from_slice(rest);
    out
}

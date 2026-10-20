use skein_throwaway_forgejo::Exchange;
use std::collections::BTreeMap;
#[derive(Debug)]
pub struct Mask {
    port: u16,
    listener: Option<u16>,
    deliveries: BTreeMap<Vec<u8>, usize>,
    ids: BTreeMap<Vec<u8>, usize>,
    times: BTreeMap<Vec<u8>, usize>,
}
impl Mask {
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self { port, listener: None, deliveries: BTreeMap::new(), ids: BTreeMap::new(), times: BTreeMap::new() }
    }
    pub(crate) fn listener(&mut self, port: u16) {
        self.listener = Some(port);
    }
    pub(crate) fn literal(&mut self, text: &str) {
        let next = self.ids.len() + 1;
        self.ids.entry(text.as_bytes().to_vec()).or_insert(next);
    }
    #[must_use]
    pub fn bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        let Some(packets) = git_packets(bytes) else { return self.plain(bytes) };
        let mut out = Vec::with_capacity(bytes.len());
        for packet in packets {
            out.extend_from_slice(&packet[..4]);
            out.extend_from_slice(&self.plain(&packet[4..]));
        }
        out
    }
    fn plain(&mut self, bytes: &[u8]) -> Vec<u8> {
        let listener = self.listener.map(|port| format!("127.0.0.1:{port}"));
        let needle = format!("127.0.0.1:{}", self.port).into_bytes();
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            if let Some(listener) = &listener
                && bytes[at..].starts_with(listener.as_bytes())
            {
                out.extend_from_slice(b"127.0.0.1:3001");
                at += listener.len();
                continue;
            }
            if bytes.len() - at >= 36
                && [8, 13, 18, 23].iter().all(|i| bytes[at + i] == b'-')
                && bytes[at..at + 36].iter().all(|b| b.is_ascii_hexdigit() || *b == b'-')
            {
                let next = self.deliveries.len() + 1;
                let n = *self.deliveries.entry(bytes[at..at + 36].to_vec()).or_insert(next);
                out.extend_from_slice(format!("00000000-0000-0000-0000-{n:012x}").as_bytes());
                at += 36;
                continue;
            }
            if bytes[at..].starts_with(&needle) {
                out.extend_from_slice(b"127.0.0.1:3000");
                at += needle.len();
                continue;
            }
            if (at == 0 || bytes[at - 1] == b'\n')
                && let Some(end) = bytes[at..].windows(2).position(|w| w == b"\r\n")
            {
                let line = &bytes[at..at + end];
                if let Some(colon) = line.iter().position(|b| *b == b':')
                    && (line[..colon].eq_ignore_ascii_case(b"date")
                        || line[..colon].eq_ignore_ascii_case(b"last-modified"))
                {
                    out.extend_from_slice(&line[..=colon]);
                    out.extend_from_slice(b" Wed, 01 Jan 2020 00:00:00 GMT\r\n");
                    at += end + 2;
                    continue;
                }
            }
            if bytes[at] == b'"'
                && let Some(end) = bytes[at + 1..].iter().position(|b| *b == b'"')
            {
                let time = &bytes[at + 1..at + 1 + end];
                if time.len() >= 20
                    && time[..4].iter().all(u8::is_ascii_digit)
                    && time[4] == b'-'
                    && time[10] == b'T'
                    && (&b"2000"[..]..=&b"2999"[..]).contains(&&time[..4])
                {
                    out.push(b'"');
                    out.extend_from_slice(&self.time(time, 0));
                    out.push(b'"');
                    at += end + 2;
                    continue;
                }
            }
            if (at == 0 || !bytes[at - 1].is_ascii_alphanumeric())
                && let Some((time, fraction)) = log_time(&bytes[at..])
            {
                out.extend_from_slice(&self.time(time, fraction));
                at += time.len();
                continue;
            }
            if bytes[at].is_ascii_digit() || (b'a'..=b'f').contains(&bytes[at]) {
                let end =
                    at + bytes[at..].iter().take_while(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)).count();
                let id = &bytes[at..end];
                if id.len() == 40 || id.len() == 64 || self.ids.contains_key(id) {
                    let next = self.ids.len() + 1;
                    let n = *self.ids.entry(id.to_vec()).or_insert(next);
                    out.extend_from_slice(format!("{n:0width$x}", width = id.len()).as_bytes());
                    at = end;
                    continue;
                }
                out.extend_from_slice(id);
                at = end;
                continue;
            }
            out.push(bytes[at]);
            at += 1;
        }
        out
    }
    fn time(&mut self, time: &[u8], fraction: usize) -> Vec<u8> {
        let next = self.times.len() + 1;
        let n = *self.times.entry(time.to_vec()).or_insert(next);
        let offset = if time.ends_with(b"Z") { &time[time.len() - 1..] } else { &time[time.len() - 6..] };
        let mut out = format!("2020-01-01T{:02}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60).into_bytes();
        if fraction != 0 {
            out.push(b'.');
            out.resize(out.len() + fraction, b'0');
        }
        out.extend_from_slice(offset);
        out
    }
}
// Only complete RFC3339 tokens are log times; resource numbers and pkt lengths are not.
fn log_time(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let date = bytes.get(..19)?;
    if ![4, 7].iter().all(|i| date[*i] == b'-')
        || date[10] != b'T'
        || ![13, 16].iter().all(|i| date[*i] == b':')
        || !date.iter().enumerate().all(|(i, b)| [4, 7, 10, 13, 16].contains(&i) || b.is_ascii_digit())
        || !(&b"2000"[..]..=&b"2999"[..]).contains(&&date[..4])
    {
        return None;
    }
    let mut end = date.len();
    let mut fraction = 0;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        fraction = bytes[end..].iter().take_while(|b| b.is_ascii_digit()).count();
        if fraction == 0 {
            return None;
        }
        end += fraction;
    }
    if bytes.get(end) == Some(&b'Z') {
        end += 1;
    } else {
        let offset = bytes.get(end..end + 6)?;
        if ![b'+', b'-'].contains(&offset[0])
            || offset[3] != b':'
            || ![1, 2, 4, 5].iter().all(|i| offset[*i].is_ascii_digit())
        {
            return None;
        }
        end += offset.len();
    }
    if bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    Some((&bytes[..end], fraction))
}
// Validate the complete pkt-line stream before touching the first-appearance maps.
// Flush and delimiter packets consist only of their four-byte length prefix.
fn git_packets(mut bytes: &[u8]) -> Option<Vec<&[u8]>> {
    if bytes.is_empty() {
        return None;
    }
    let mut packets = Vec::new();
    while !bytes.is_empty() {
        let prefix = std::str::from_utf8(bytes.get(..4)?).ok()?;
        if !prefix.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let length = usize::from_str_radix(prefix, 16).ok()?;
        let length = match length {
            0..=2 => 4,
            3 => return None,
            length => length,
        };
        packets.push(bytes.get(..length)?);
        bytes = &bytes[length..];
    }
    Some(packets)
}
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

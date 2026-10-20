//! A listener's hook URL (forge-setup.md, 4.5); task 12 gives the hook's steps back.
use alloc::boxed::Box;
use core::net::{IpAddr, SocketAddr};
use skein_forge::forgejo::delivery::hook_target;
use skein_lib::{Decimal, Writer};
/// A listener's URL, accepting the two loopback addresses only.
#[must_use]
pub fn hook_url(address: SocketAddr, forge: u16) -> Option<Box<[u8]>> {
    let host: Box<[u8]> = match address.ip() {
        IpAddr::V4(ip) if ip.is_loopback() => {
            let mut length = 3_usize;
            for octet in ip.octets() {
                length = length.saturating_add(Decimal::of(u64::from(octet)).as_bytes().len());
            }
            let mut host = Writer::new(length);
            for (index, octet) in ip.octets().iter().enumerate() {
                if index > 0 {
                    host.put(b".").expect("IPv4 dots");
                }
                host.put(Decimal::of(u64::from(*octet)).as_bytes()).expect("IPv4 octet");
            }
            host.finish()
        }
        IpAddr::V6(ip) if ip.is_loopback() => b"[::1]".as_slice().into(),
        IpAddr::V4(_) | IpAddr::V6(_) => return None,
    };
    let port = Decimal::of(u64::from(address.port()));
    let target = hook_target(forge);
    let mut out = Writer::new(
        8_usize.saturating_add(host.len()).saturating_add(port.as_bytes().len()).saturating_add(target.len()),
    );
    for part in [b"http://", host.as_ref(), b":", port.as_bytes(), &target] {
        out.put(part).expect("loopback URL fits");
    }
    Some(out.finish())
}

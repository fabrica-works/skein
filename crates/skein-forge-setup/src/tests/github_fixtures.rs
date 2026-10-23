use crate::github;
use skein_forge::github::{
    document::{App, Permission},
    setup,
};
use skein_lib::{
    Duration, List, Secret, Writer,
    base64::{self, Alphabet},
    bytes,
};
/// The small DER envelope used by the item decoder tests; no RSA operations.
pub const DER: &[u8] = b"\x30\x03\x02\x01\x07";

/// A hand-built App with every wanted grant; no key or signing operation.
pub fn app() -> App {
    let mut permissions = List::with_capacity(u32::try_from(setup::PERMISSIONS.len()).expect("wanted count"));
    for (name, access) in setup::PERMISSIONS {
        permissions.push(Permission { name: bytes::copy_of(name), access }).expect("wanted permission");
    }
    let mut events = List::with_capacity(u32::try_from(setup::EVENTS.len()).expect("wanted count"));
    for name in setup::EVENTS {
        events.push(bytes::copy_of(name)).expect("wanted event");
    }
    App {
        id: 19,
        slug: b"kit-app".as_slice().into(),
        permissions: permissions.into_boxed(),
        events: events.into_boxed(),
    }
}

pub fn pem(der: &[u8], ending: &[u8], surround: &[u8]) -> Secret {
    let size = base64::encoded_len(Alphabet::Standard, u32::try_from(der.len()).expect("fixture length"))
        .expect("fixture encoded length");
    let mut text = Writer::new(usize::try_from(size).expect("small encoding"));
    base64::encode(Alphabet::Standard, der, &mut text).expect("encoding fits");
    pem_text(&text.finish(), ending, surround)
}
pub fn pem_text(text: &[u8], ending: &[u8], surround: &[u8]) -> Secret {
    let parts = [
        surround,
        b"-----BEGIN RSA PRIVATE KEY-----",
        ending,
        text,
        ending,
        b"-----END RSA PRIVATE KEY-----",
        surround,
    ];
    let mut length = 0_usize;
    for part in parts {
        length = length.checked_add(part.len()).expect("fixture PEM length");
    }
    let mut out = Writer::new(length);
    for part in parts {
        out.put(part).expect("fixture PEM fits");
    }
    Secret::new(out.finish())
}
pub fn limits() -> github::limits::Limits {
    github::limits::Limits {
        key_bytes: 1196,
        name_bytes: 34,
        page_bytes: 4096,
        installations: 8,
        reachable: 4,
        pages: 4,
        state_life: Duration::from_secs(600),
        deadline: Duration::from_secs(30),
        backoff: Duration::from_millis(100),
        backoff_max: Duration::from_secs(2),
    }
}
pub fn given() -> github::given::Given {
    github::given::Given {
        forge: 4,
        read_only: false,
        word: b"kit".as_slice().into(),
        web: b"https://github.test".as_slice().into(),
        url: b"https://kit.test/".as_slice().into(),
        redirect_url: b"https://kit.test/connect/redirect".as_slice().into(),
        setup_url: b"https://kit.test/connect/setup".as_slice().into(),
        limits: limits(),
    }
}

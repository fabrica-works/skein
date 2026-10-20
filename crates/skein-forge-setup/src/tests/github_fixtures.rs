use crate::github;
use skein_lib::Duration;
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
        word: b"kit".as_slice().into(),
        web: b"https://github.test".as_slice().into(),
        url: b"https://kit.test/".as_slice().into(),
        redirect_url: b"https://kit.test/connect/redirect".as_slice().into(),
        setup_url: b"https://kit.test/connect/setup".as_slice().into(),
        limits: limits(),
    }
}

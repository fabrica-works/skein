//! A step's writes protect its requests and navigation (ui-web.md, 5.3).
use crate::{
    fetch::Exchange,
    platform::Platform,
    store::{Record, Store},
};

#[derive(Default)]
pub struct Requests {
    pub writes: Vec<Record>,
    pub removes: Vec<Box<[u8]>>,
    pub exchanges: Vec<Exchange>,
    pub stream: Option<StreamDuty>,
    pub navigate: Option<Box<[u8]>>,
}
pub type Header = (Box<[u8]>, Box<[u8]>);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamDuty {
    Open { path: Box<[u8]>, headers: Vec<Header>, bound: u32 },
    Close,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Carried {
    Written { key: Box<[u8]> },
    Refused { key: Box<[u8]> },
    Failed { exchange: u32 },
}
pub fn carry<P: Platform>(requests: Requests, store: &mut Store, platform: &mut P, reports: &mut Vec<Carried>) {
    for record in requests.writes {
        if store.write(&record.key, &record.value, platform).is_err() {
            reports.push(Carried::Refused { key: record.key });
            reports.extend(requests.exchanges.iter().map(|exchange| Carried::Failed { exchange: exchange.id }));
            return;
        }
        reports.push(Carried::Written { key: record.key });
    }
    for key in requests.removes {
        store.remove(&key, platform);
    }
    for exchange in requests.exchanges {
        platform.send(&exchange);
    }
    if let Some(stream) = requests.stream {
        platform.stream(stream);
    }
    if let Some(to) = requests.navigate {
        platform.navigate(&to);
    }
}

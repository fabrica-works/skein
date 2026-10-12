//! The client's heap in metered steps, apart from its harness (tls.md, 5).

use skein_heap::{Measured, Meter, Span};
use skein_tls::client::{self, Limits};
use std::fmt::Debug;

pub const SMALL: Limits = Limits { read: 1, send: 1, records: client::MAX_RECORD };
pub const LARGE: Limits = Limits { read: 40_000, send: 40_000, records: 3 * client::MAX_RECORD };

/// Metered steps of one case, with the peer's heap counted separately.
pub struct Steps {
    meter: Meter,
    outside: i64,
    bound: u64,
    most: u64,
}

impl Steps {
    #[must_use]
    pub fn new(bound: u64) -> Self {
        Self { meter: Meter::new(), outside: 0, bound, most: 0 }
    }

    #[must_use]
    pub fn bound(&self) -> u64 {
        self.bound
    }

    /// Reset the baseline once the harness has been reserved.
    pub fn reset(&mut self) {
        self.meter = Meter::new();
    }

    pub fn start(&self) {
        self.meter.start();
    }

    #[must_use]
    pub fn end(&self) -> Measured {
        self.meter.end()
    }

    pub fn check(&mut self, step: Measured, extra: u64, what: &dyn Debug) {
        let bound = i64::try_from(self.bound + extra).expect("finite") + self.outside;
        let own = self.meter.check(step, u64::try_from(bound).expect("positive"), what);
        let own = i64::try_from(own).expect("finite") - self.outside - i64::try_from(extra).expect("finite");
        self.most = self.most.max(u64::try_from(own).expect("positive"));
    }

    /// Count the peer and harness's net heap change between steps.
    pub fn outside(&mut self, net: i64) {
        self.outside += net;
    }

    /// Drop the client and check it frees what it held and the moved name.
    pub fn finish(&self, client: impl Sized, name: usize, what: &dyn Debug) -> u64 {
        let held = i64::try_from(self.meter.held()).expect("finite") - self.outside;
        let span = Span::start();
        drop(client);
        let freed = span.end().net;
        let name = i64::try_from(name).expect("finite name");
        assert_eq!(held + freed, -name, "{what:?}: the client frees what it held, and only that");
        self.most
    }
}

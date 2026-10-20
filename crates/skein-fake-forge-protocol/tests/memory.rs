//! The hosted git codec and two-parent walk under the counting allocator.
extern crate alloc;
use skein_fake_forge_domain as domain;
use skein_fake_forge_protocol::{
    Config, Event, Faults, Hosted, Limits, Repository, Server, Service, closed, delivery, down, resume_with_domain,
    start, up_with_domain,
};
#[cfg(test)]
#[path = "../src/tests/fixtures.rs"]
#[expect(dead_code, reason = "the protocol's shared fixture also serves route and document tests")]
mod fixtures;
#[cfg(test)]
mod tests {
    use super::*;
    use skein_fake_forge_protocol::{git, worst_case};
    use skein_heap::{Counting, Meter};
    use skein_lib::{List, Writer};
    #[global_allocator]
    static HEAP: Counting = Counting;

    fn bytes(count: u32, byte: u8) -> Box<[u8]> {
        let mut writer = Writer::new(count.try_into().expect("test bytes"));
        for _ in 0..count {
            writer.put(&[byte]).expect("sized bytes");
        }
        writer.finish()
    }
    fn tree(limits: &domain::Limits) -> Box<[domain::api::File]> {
        let mut files = List::with_capacity(limits.files);
        for index in 0..limits.files {
            files
                .push(domain::api::File {
                    path: bytes(limits.name_bytes, u8::try_from(index).expect("tiny files")),
                    content: bytes(limits.content_bytes, 0xff),
                })
                .expect("bounded tree");
        }
        files.into_boxed()
    }
    #[test]
    fn maximal_trees_and_merge_history_fit_the_protocol_envelope() {
        let meter = Meter::new();
        meter.start();
        let mut config = fixtures::config();
        let domain_limits = fixtures::fake_config().limits;
        config.max_response_items = domain_limits.commits;
        let bound = worst_case(&fixtures::limits(), &config)
            .expect("protocol bound")
            .checked_add(domain::worst_case(&domain_limits).expect("domain bound"))
            .expect("combined bound");
        // The driver buffers are counted too, with no deduction from the envelope.
        let mut h = fixtures::Harness::new(config);
        let mut tip = 1_u64;
        for _ in 1..domain_limits.commits {
            let call = git::Call::CommitTree {
                parent: tip,
                merging: Some(1),
                tree: tree(&domain_limits),
                message: bytes(
                    domain_limits.title_bytes.saturating_add(domain_limits.body_bytes).saturating_add(2),
                    0xfe,
                ),
            };
            let encoded = git::client::encode(&call, &h.env.limits).expect("largest request fits");
            h.ask(b"POST", &encoded.target, &encoded.body);
            assert_eq!(h.status(), 200);
            tip = tip.checked_add(1).expect("tiny history");
        }
        let call = git::Call::Objects { commits: Box::new([tip]) };
        let encoded = git::client::encode(&call, &h.env.limits).expect("objects request");
        h.ask(b"POST", &encoded.target, &encoded.body);
        assert_eq!(h.status(), 200);
        assert!(skein_lib::bytes::find(&h.body(), b"\"graph\":[{\"commit\":15,").is_some());
        let measured = meter.end();
        meter.check(measured, bound, &"maximal git trees and both-parent frontier");
    }
}

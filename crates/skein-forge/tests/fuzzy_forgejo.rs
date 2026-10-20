#![expect(
    clippy::default_numeric_fallback,
    reason = "the copied bounded sweeps retain their original counts and workloads"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the document test harness retains finite fixture rows and mutation buffers in ordinary Rust collections"
)]
#![expect(clippy::similar_names, reason = "the request and reply names follow the dialect vocabulary")]
#![cfg(test)]
pub mod common;
use common::{forgejo, mutated, mutated_into};
use skein_lib::Rng;
#[test]
fn forgejo_decoders_answer_every_mutated_transcript_without_panicking() {
    let mut rng = Rng::new(0x06_05);
    for (case, request) in forgejo::json_cases() {
        let body = forgejo::transcript(&case).body;
        let mut bytes = Vec::new();
        for _ in 0..32 {
            mutated_into(&mut rng, &body, &mut bytes);
            let decoded = match &request {
                Some(request) => forgejo::decode(request, &bytes),
                None => forgejo::decode_error(&bytes),
            };
            assert!(decoded.steps <= forgejo::STEP_BOUND, "{case}: decode terminates within its bound");
        }
    }
}
#[test]
fn forgejo_server_routes_answer_every_mutated_request_without_panicking() {
    let mut rng = Rng::new(0x06_05_02);
    let limits = forgejo::limits();
    for case in forgejo::transcript_cases() {
        let transcript = forgejo::transcript(&case);
        for _ in 0..200 {
            let target = mutated(&mut rng, &transcript.target);
            let body = mutated(&mut rng, &transcript.request_body);
            if let Ok((_, steps)) = forgejo::decode_route(transcript.method, &target, &body, &limits) {
                assert!(steps <= forgejo::STEP_BOUND, "{case}: route terminates within its bound");
            }
        }
    }
}
#[test]
fn forgejo_delivery_decoders_answer_every_mutated_delivery_without_panicking() {
    let mut rng = Rng::new(0x04_02);
    let limits = forgejo::profile_limits();
    for (case, _) in forgejo::delivery_cases() {
        let sent = forgejo::delivery(case);
        let mut bytes = Vec::new();
        for _ in 0..16 {
            mutated_into(&mut rng, &sent.body, &mut bytes);
            let _decoded = forgejo::decode_delivery(&sent.event, &bytes, &limits);
        }
    }
}

#[test]
fn generated_documents_and_their_mutations_end_within_the_step_bound() {
    use common::generated;
    use skein_forge::forgejo::{client, server};
    // The plan prescribes 32 seeds and eight mutations of each generated case.
    const SEEDS: core::ops::Range<u64> = 0..32;
    const MUTATIONS: u32 = 8;
    let limits = generated::limits();
    let mut rng = Rng::new(0x06_03);
    let mut bytes = Vec::new();
    for seed in SEEDS {
        for (name, request) in generated::request_documents() {
            let sent = client::encode(&request, &limits).expect("generated request");
            let Some(body) = sent.body.as_deref() else {
                continue;
            };
            assert_eq!(
                forgejo::decode_route(sent.method, &sent.target, body, &limits).expect("valid request").0,
                request,
                "{seed}/{name}"
            );
            for _ in 0..MUTATIONS {
                mutated_into(&mut rng, body, &mut bytes);
                if let Ok((_, steps)) = forgejo::decode_route(sent.method, &sent.target, &bytes, &limits) {
                    assert!(steps <= forgejo::STEP_BOUND, "{seed}/{name}: bounded request");
                }
            }
        }
        for reply in generated::documents(seed) {
            let body = server::encode(&reply.answer, &limits).expect("generated answer");
            let decoder = || match &reply.request {
                Some(request) => client::decoder(request, &limits).expect("answer decoder"),
                None => client::error_decoder(&limits),
            };
            let decoded = forgejo::decode_answer(decoder(), &body, &limits);
            assert_eq!(decoded.document, Ok(reply.document), "{seed}/{}", reply.name);
            assert_eq!(decoded.rows, reply.rows, "{seed}/{}", reply.name);
            for _ in 0..MUTATIONS {
                mutated_into(&mut rng, &body, &mut bytes);
                let decoded = forgejo::decode_answer(decoder(), &bytes, &limits);
                assert!(decoded.steps <= forgejo::STEP_BOUND, "{seed}/{}: bounded answer", reply.name);
            }
        }
        for (name, delivery) in generated::deliveries(seed) {
            let sent = server::delivery::encode(&delivery, &limits).expect("generated delivery");
            assert_eq!(forgejo::decode_delivery(sent.event, &sent.body, &limits), Ok(delivery), "{seed}/{name}");
            for _ in 0..MUTATIONS {
                mutated_into(&mut rng, &sent.body, &mut bytes);
                let (_, steps) = forgejo::decode_delivery_steps(sent.event, &bytes, &limits);
                assert!(steps <= forgejo::STEP_BOUND, "{seed}/{name}: bounded delivery");
            }
        }
    }
}

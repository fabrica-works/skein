#![cfg(test)]
pub mod common;
use common::forgejo;
use skein_forge::forgejo::{
    delivery::{Delivery, SUBSCRIBED},
    limits::Limits,
    server::delivery,
};
fn shipped() -> Limits {
    forgejo::profile_limits()
}
#[test]
fn recorded_deliveries_decode_to_the_fields_their_hints_read() {
    for (case, expected) in forgejo::delivery_cases() {
        let sent = forgejo::delivery(case);
        assert_eq!(forgejo::decode_delivery(&sent.event, &sent.body, &shipped()), Ok(expected), "{case}");
    }
}
#[test]
fn the_encoders_bodies_decode_unchanged_with_the_recordings_shape() {
    for (case, expected) in forgejo::delivery_cases() {
        let recorded = forgejo::delivery(case);
        let sent = delivery::encode(&expected, &shipped()).expect("encoded");
        assert_eq!(sent.event, recorded.event, "{case}");
        assert!(forgejo::shape(&sent.body).is_subset(&forgejo::shape(&recorded.body)), "{case}");
        assert_eq!(forgejo::decode_delivery(sent.event, &sent.body, &shipped()), Ok(expected), "{case}");
    }
}
#[test]
#[expect(clippy::disallowed_methods, reason = "recorded event headers name UTF-8 fixture directories")]
fn every_subscribed_event_has_a_recorded_delivery_the_decoder_knows() {
    for event in SUBSCRIBED {
        let case = core::str::from_utf8(event).expect("event text");
        let sent = forgejo::delivery(case);
        assert!(forgejo::decode_delivery(&sent.event, &sent.body, &shipped()).is_ok(), "{case}");
    }
}
#[test]
fn a_recorded_deletion_ref_names_its_branch() {
    let sent = forgejo::delivery("delete");
    let decoded: Delivery = forgejo::decode_delivery(&sent.event, &sent.body, &shipped()).expect("decoded");
    assert_eq!(decoded.branch(), Some(b"gone".as_slice()));
}

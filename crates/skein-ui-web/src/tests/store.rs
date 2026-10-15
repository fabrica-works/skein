use super::fixtures::{Call, FakePlatform};
use crate::store::{Record, Refused, Store};

#[test]
fn records_are_base64_under_the_prefix() {
    let mut platform = FakePlatform::default();
    Store::new(b"notes-", None).write(b"key", b"record", &mut platform).unwrap();
    assert_eq!(platform.calls, vec![Call::Write(b"notes-a2V5".to_vec(), b"cmVjb3Jk".to_vec())]);
}
#[test]
fn a_write_past_the_bound_is_refused() {
    let mut platform = FakePlatform::default();
    let result = Store::new(b"notes-", Some(17)).write(b"key", b"record", &mut platform);
    assert_eq!(result, Err(Refused));
}
#[test]
fn records_of_another_prefix_are_not_read() {
    let platform = FakePlatform {
        records: vec![Record { key: b"other-a2V5".as_slice().into(), value: b"cmVjb3Jk".as_slice().into() }],
        ..FakePlatform::default()
    };
    assert_eq!(Store::new(b"notes-", None).read_all(&platform), vec![]);
}
#[test]
fn records_are_decoded_at_start() {
    let platform = FakePlatform {
        records: vec![Record { key: b"notes-a2V5".as_slice().into(), value: b"cmVjb3Jk".as_slice().into() }],
        ..FakePlatform::default()
    };
    assert_eq!(
        Store::new(b"notes-", None).read_all(&platform),
        vec![Record { key: b"key".as_slice().into(), value: b"record".as_slice().into() }]
    );
}
#[test]
fn a_platform_quota_refuses_a_write() {
    let mut platform = FakePlatform { quota: Some(0), ..FakePlatform::default() };
    assert_eq!(Store::new(b"notes-", None).write(b"key", b"record", &mut platform), Err(Refused));
}
#[test]
fn removal_uses_the_encoded_key() {
    let mut platform = FakePlatform::default();
    Store::new(b"notes-", None).remove(b"key", &mut platform);
    assert_eq!(platform.calls, vec![Call::Remove(b"notes-a2V5".to_vec())]);
}

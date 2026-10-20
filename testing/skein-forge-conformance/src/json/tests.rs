use super::{Json, object, write};
const BODY: &[u8] =
    br#"{"id":7,"name":"specimen","private":false,"labels":["a","b"],"owner":{"id":3,"login":"fixture"}}"#;
#[test]
fn a_parsed_body_reads_members_items_and_scalars_by_name() {
    let json = Json::parse(BODY).expect("body");
    let root = json.root();
    assert_eq!(
        (root.get("id").u64(), root.get("name").str(), root.get("private").bool()),
        (Some(7), Some("specimen"), Some(false))
    );
    assert_eq!(root.get("owner").get("login").str(), Some("fixture"));
    assert_eq!((root.get("labels").items().len(), root.get("labels").nth(1).str()), (2, Some("b")));
    assert!(root.get("id").same(root.get("id")));
    assert!(!root.get("id").same(root.get("owner").get("id")));
}
#[test]
fn a_missing_member_or_the_wrong_kind_reads_as_none() {
    let json = Json::parse(BODY).expect("body");
    let root = json.root();
    assert_eq!(
        (root.get("absent").u64(), root.get("absent").get("deeper").u64(), root.get("name").u64()),
        (None, None, None)
    );
    assert_eq!((root.get("id").str(), root.get("id").bool(), root.get("name").items().len()), (None, None, 0));
}
#[test]
fn a_body_that_is_not_one_document_is_refused() {
    for input in ["", "{", r#"{"a":}"#, "[1,2", "{} {}"] {
        assert!(Json::parse(input.as_bytes()).is_err(), "{input:?}");
    }
}
#[test]
fn an_array_of_strings_is_written_compact_and_escaped() {
    let body = write(|out| {
        out.array_start();
        for text in ["a", "b\"c"] {
            out.string(text.as_bytes());
        }
        out.array_end();
    })
    .expect("strings");
    assert_eq!(&*body, br#"["a","b\"c"]"#);
}

#[test]
fn a_key_is_found_at_any_depth_and_a_value_is_not_a_key() {
    let json = Json::parse(br#"{"a":{"secret":1},"b":["x",{"c":"secret"}]}"#).expect("body");
    assert!(json.has_key("secret"));
    assert!(json.has_key("c"));
    assert!(!json.has_key("x"));
}

#[test]
fn an_object_is_written_compact_in_sorted_key_order() {
    let body = object(&[("a", &1u64), ("b", &"x"), ("c", &true)]).expect("sorted members");
    assert_eq!(&*body, br#"{"a":1,"b":"x","c":true}"#);
}

#[test]
fn members_out_of_order_or_repeated_are_refused() {
    for members in [[("b", &true), ("a", &true)], [("a", &true), ("a", &true)]] {
        assert!(object(&members.map(|(key, value)| (key, value as &dyn super::Put))).is_err(), "{members:?}");
    }
}

#[test]
fn a_string_is_escaped_to_the_bytes_the_base_wrote() {
    let body = write(|out| out.string("\"\\\n\t\u{1}é".as_bytes())).expect("string");
    assert_eq!(&*body, "\"\\\"\\\\\\n\\t\\u0001é\"".as_bytes());
}

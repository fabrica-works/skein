use super::fixtures;
use crate::forgejo::{
    client,
    decode::Malformed,
    document::{Document, Row},
    request::{Page, Request},
};
use alloc::boxed::Box;
fn make() -> Request {
    Request::CreateToken { login: Box::from(*b"bot"), name: Box::from(*b"kit-api"), scopes: Box::from([]) }
}
#[test]
fn a_made_token_holds_its_value_and_shows_only_its_length() {
    let limits = fixtures::limits();
    let body = br#"{"id":3,"name":"kit-api","scopes":["read:user"],"sha1":"0123456789abcdef0123456789abcdef01234567","token_last_eight":"01234567"}"#;
    let decoded = fixtures::decode(client::decoder(&make(), &limits).expect("token decoder"), body, &limits);
    let Ok(Document::Token(token)) = decoded.document else { panic!("a token") };
    assert_eq!(token.id, 3);
    assert_eq!(&*token.name, b"kit-api");
    assert_eq!(&*token.scopes, &[Box::<[u8]>::from(*b"read:user")]);
    let value = token.value.expect("new value");
    assert_eq!(value.expose(), b"0123456789abcdef0123456789abcdef01234567");
    assert_eq!(alloc::fmt::format(format_args!("{value:?}")), "Secret { len: 40 }");
}
#[test]
fn a_listed_token_has_no_value() {
    let limits = fixtures::limits();
    let request = Request::Tokens { login: Box::from(*b"bot"), page: Page { page: None, limit: None } };
    for body in [
        br#"[{"id":3,"name":"kit-api","scopes":[]}]"#.as_slice(),
        br#"[{"id":3,"name":"kit-api","scopes":[],"sha1":""}]"#,
    ] {
        let decoded = fixtures::decode(client::decoder(&request, &limits).expect("token list"), body, &limits);
        assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
        assert_eq!(decoded.rows.len(), 1);
        let Row::Token(token) = decoded.rows.iter().next().expect("one row") else { panic!("token row") };
        assert_eq!(token.value, None);
    }
}
#[test]
fn a_scope_list_past_inner_is_malformed_naming_scopes() {
    // Two elements here; the shipped small profile permits 32.
    let limits = crate::forgejo::limits::Limits { inner: 2, ..fixtures::limits() };
    let body = br#"{"id":3,"name":"kit-api","scopes":["read:user","write:issue","write:repository"]}"#;
    assert_eq!(
        fixtures::decode(client::decoder(&make(), &limits).expect("token decoder"), body, &limits).document,
        Err(Malformed::Bound { field: b"scopes" })
    );
}
#[test]
fn a_scope_list_encodes_in_the_order_given() {
    let request = Request::CreateToken {
        login: Box::from(*b"bot"),
        name: Box::from(*b"kit-git"),
        scopes: Box::new([Box::from(*b"write:repository"), Box::from(*b"read:user")]),
    };
    let encoded = client::encode(&request, &fixtures::limits()).expect("token encoding");
    assert_eq!(
        encoded.body.as_deref(),
        Some(br#"{"name":"kit-git","scopes":["write:repository","read:user"]}"#.as_slice())
    );
}
#[test]
fn a_delete_writes_the_id_as_one_segment() {
    let request = Request::DeleteToken { login: Box::from(*b"bot/other"), id: 42 };
    let encoded = client::encode(&request, &fixtures::limits()).expect("token deletion");
    assert_eq!(&*encoded.target, b"/api/v1/admin/users/bot%2Fother/tokens/42");
    assert!(client::decoder(&request, &fixtures::limits()).is_none());
}

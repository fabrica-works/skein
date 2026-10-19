use super::fixtures::{self, boxed};
use skein_lib::Secret;
#[test]
fn a_plain_user_is_refused_the_admin_routes() {
    let mut h = fixtures::setup_harness();
    h.service.setup_mut().token(2, b"plain", &[b"read:user"], Secret::new(boxed(b"plain-user-token")));
    h.ask_as(b"GET", b"/api/v1/admin/users/kit/tokens", b"", b"plain-user-token");
    assert_eq!(h.status(), 403);
    assert_eq!(h.body().as_ref(), b"{\"message\":\"user should be the site admin\"}");
}
#[test]
fn a_made_token_authenticates_as_its_user_and_a_deleted_one_does_not() {
    let mut h = fixtures::setup_harness();
    let made = fixtures::make_setup_token(&mut h);
    let secret = made.value.expect("new token value");
    h.ask_as(b"GET", b"/api/v1/user", b"", secret.expose());
    assert_eq!(h.status(), 200);
    assert_eq!(h.body().as_ref(), br#"{"id":2,"login":"kit","is_admin":false,"full_name":"kit:bot"}"#);
    h.ask(b"DELETE", b"/api/v1/admin/users/kit/tokens/2", b"");
    assert_eq!(h.status(), 204);
    h.ask_as(b"GET", b"/api/v1/user", b"", secret.expose());
    assert_eq!(h.status(), 401);
}
#[test]
fn a_second_token_of_one_name_is_refused() {
    let mut h = fixtures::setup_harness();
    fixtures::make_setup_token(&mut h);
    h.ask(b"POST", b"/api/v1/admin/users/kit/tokens", br#"{"name":"kit-api","scopes":["read:user"]}"#);
    assert_eq!(h.status(), 400);
    assert_eq!(h.body().as_ref(), b"{\"message\":\"access token name has been used already\"}");
}

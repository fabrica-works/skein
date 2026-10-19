use super::fixtures::{self, Run, secret};
use crate::{
    item::Item,
    reconcile::{Event, Failure, Need, Reply, Stage, Why},
};
use skein_forge::forgejo::{document::Document, request::Request};
fn user(user: skein_forge::forgejo::document::User) -> Event {
    Event::Reply(Reply::Document(Document::User(user)))
}
#[test]
fn an_admin_token_of_a_plain_user_stops_at_the_admin() {
    let mut run = Run::managed();
    run.step(Event::Begin);
    run.step(Event::Read(Some(secret(fixtures::ADMIN))));
    let need = run.step(user(fixtures::person(b"ada", b"Ada")));
    assert_eq!(
        need,
        Need::Stop(Failure {
            forge: 1,
            step: Stage::Admin,
            subject: b"kit-admin".as_slice().into(),
            why: Why::NotAdmin,
        })
    );
}
#[test]
fn an_absent_bot_is_made_with_a_drawn_password_and_the_marker() {
    let mut run = Run::managed();
    run.bot_lookup();
    assert_eq!(run.step(Event::Reply(Reply::Status(404))), Need::Draw(32));
    let need = run.step(Event::Drawn(secret(&[0x0a; 32])));
    assert_eq!(
        need,
        Need::Send {
            request: Request::CreateUser {
                login: b"kit".as_slice().into(),
                email: b"kit@example.invalid".as_slice().into(),
                full_name: b"kit:bot".as_slice().into(),
                password: secret(b"0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a"),
                must_change_password: false,
            },
            token: secret(fixtures::ADMIN),
        }
    );
    assert_eq!(run.step(user(fixtures::person(b"kit", b"kit:bot"))), Need::Read(Item::ApiToken));
    run.kept_tokens();
    assert_eq!(run.step(Event::Read(None)), Need::Write { item: Item::User, bytes: secret(b"42") });
    assert_eq!(run.step(Event::Written), Need::Send { request: Request::ApiSettings, token: secret(fixtures::ADMIN) });
}
#[test]
fn a_bot_with_the_marker_is_kept_and_nothing_is_written() {
    let mut run = Run::managed();
    run.bot_lookup();
    assert_eq!(run.step(user(fixtures::person(b"kit", b"note kit:bot account"))), Need::Read(Item::ApiToken));
    run.kept_tokens();
    assert_eq!(
        run.step(Event::Read(Some(secret(b"42")))),
        Need::Send { request: Request::ApiSettings, token: secret(fixtures::ADMIN) }
    );
}
#[test]
fn a_user_of_the_bots_login_without_the_marker_is_refused_naming_the_login() {
    let mut run = Run::managed();
    run.bot_lookup();
    let need = run.step(user(fixtures::person(&run.bot(), b"a note")));
    assert_eq!(
        need,
        Need::Stop(Failure { forge: run.forge(), step: Stage::Bot, subject: run.bot(), why: Why::Unmarked })
    );
}
#[test]
fn a_bot_that_is_a_site_admin_is_refused() {
    let mut run = Run::managed();
    run.bot_lookup();
    let mut bot = fixtures::person(b"kit", b"kit:bot");
    bot.is_admin = true;
    assert_eq!(
        run.step(user(bot)),
        Need::Stop(Failure { forge: 1, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::BotIsAdmin })
    );
}
#[test]
fn a_refused_request_stops_naming_its_step_and_subject() {
    let mut run = Run::managed();
    run.bot_lookup();
    run.step(Event::Reply(Reply::Status(404)));
    run.step(Event::Drawn(secret(&[0x0a; 32])));
    assert_eq!(
        run.step(Event::Reply(Reply::Status(403))),
        Need::Stop(Failure { forge: 1, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::Refused(403) })
    );
}
#[test]
fn a_forge_that_stops_answering_stops_the_run_at_its_step() {
    let mut run = Run::managed();
    run.bot_lookup();
    assert_eq!(
        run.step(Event::Reply(Reply::Down)),
        Need::Stop(Failure { forge: 1, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::Down })
    );
}
#[test]
fn the_bots_id_is_written_in_decimal_only_when_the_file_differs() {
    for old in [None, Some(secret(b"041")), Some(secret(b"42\n")), Some(secret(b"43"))] {
        let mut run = Run::owned();
        run.bot_lookup();
        run.step(user(fixtures::person(b"kit", b"kit:bot")));
        run.kept_tokens();
        assert_eq!(
            run.step(Event::Read(old.clone())),
            Need::Write { item: Item::User, bytes: secret(b"42") },
            "{old:?}"
        );
        assert_eq!(
            run.step(Event::Written),
            Need::Send { request: Request::ApiSettings, token: secret(fixtures::ADMIN) }
        );
    }
}
#[test]
fn a_missing_admin_token_stops_before_any_request() {
    let mut run = Run::managed();
    run.step(Event::Begin);
    assert_eq!(
        run.step(Event::Read(None)),
        Need::Stop(Failure {
            forge: 1,
            step: Stage::Admin,
            subject: b"kit-admin".as_slice().into(),
            why: Why::Missing,
        })
    );
}
#[test]
fn an_empty_admin_token_stops_before_any_request() {
    let mut run = Run::managed();
    run.step(Event::Begin);
    assert_eq!(
        run.step(Event::Read(Some(secret(b"")))),
        Need::Stop(Failure {
            forge: 1,
            step: Stage::Admin,
            subject: b"kit-admin".as_slice().into(),
            why: Why::Missing,
        })
    );
}
#[test]
fn a_draw_of_another_length_stops_before_creating_the_bot() {
    let mut run = Run::managed();
    run.bot_lookup();
    run.step(Event::Reply(Reply::Status(404)));
    assert_eq!(
        run.step(Event::Drawn(secret(&[0x0a; 31]))),
        Need::Stop(Failure { forge: 1, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::Unexpected })
    );
}
#[test]
fn hook_urls_are_written_for_loopback_v4_and_v6() {
    use crate::reconcile::hook_url;
    use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    for (address, forge, expected) in [
        (SocketAddr::from((Ipv4Addr::LOCALHOST, 9898)), 1, Some(b"http://127.0.0.1:9898/hooks/1".as_slice())),
        (SocketAddr::from((Ipv6Addr::LOCALHOST, 65535)), 65535, Some(b"http://[::1]:65535/hooks/65535".as_slice())),
        (SocketAddr::from((Ipv4Addr::new(127, 2, 3, 4), 80)), 2, Some(b"http://127.2.3.4:80/hooks/2".as_slice())),
        (SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 9898)), 1, None),
    ] {
        assert_eq!(hook_url(address, forge).as_deref(), expected, "{address}");
    }
}
#[test]
fn a_lower_hook_met_later_deletes_the_higher_one_kept() {
    use skein_forge::forgejo::request::{HookSpec, Page};
    let mut run = Run::hook_listing();
    assert_eq!(
        run.step(Event::Reply(Reply::Rows(Box::new([fixtures::hook_row(7), fixtures::hook_row(3)])))),
        Need::Send {
            request: Request::DeleteOrgHook { org: b"acme".as_slice().into(), id: 7 },
            token: secret(fixtures::ADMIN)
        }
    );
    assert_eq!(
        run.step(Event::Reply(Reply::Empty)),
        Need::Send {
            request: Request::OrgHooks { org: b"acme".as_slice().into(), page: Page { page: Some(1), limit: Some(2) } },
            token: secret(fixtures::ADMIN)
        }
    );
    assert_eq!(
        run.step(Event::Reply(Reply::Rows(Box::new([fixtures::hook_row(3)])))),
        Need::Send {
            request: Request::EditOrgHook {
                org: b"acme".as_slice().into(),
                id: 3,
                hook: HookSpec {
                    url: b"http://127.0.0.1:9898/hooks/1".as_slice().into(),
                    secret: secret(b"webhook-secret"),
                    events: fixtures::subscribed(),
                    active: true,
                }
            },
            token: secret(fixtures::ADMIN)
        }
    );
}

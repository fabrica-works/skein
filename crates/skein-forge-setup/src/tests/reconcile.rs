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
    assert_eq!(run.step(Event::Read(None)), Need::Write { item: Item::User, bytes: secret(&42u64.to_le_bytes()) });
    assert_eq!(run.step(Event::Written), Need::Done);
}
#[test]
fn a_bot_with_the_marker_is_kept_and_nothing_is_written() {
    let mut run = Run::managed();
    run.bot_lookup();
    assert_eq!(run.step(user(fixtures::person(b"kit", b"note kit:bot account"))), Need::Read(Item::ApiToken));
    run.kept_tokens();
    assert_eq!(run.step(Event::Read(Some(secret(&42u64.to_le_bytes())))), Need::Done);
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
fn the_bots_id_is_written_as_eight_bytes_only_when_the_item_differs() {
    for old in [
        None,
        Some(secret(b"42")),
        Some(secret(&43u64.to_le_bytes())),
        Some(secret(&42u64.to_le_bytes()[..7])),
        Some(secret(&[42, 0, 0, 0, 0, 0, 0, 0, 0])),
    ] {
        let mut run = Run::owned();
        run.bot_lookup();
        run.step(user(fixtures::person(b"kit", b"kit:bot")));
        run.kept_tokens();
        assert_eq!(
            run.step(Event::Read(old.clone())),
            Need::Write { item: Item::User, bytes: secret(&42u64.to_le_bytes()) },
            "{old:?}"
        );
        assert_eq!(run.step(Event::Written), fixtures::send(Request::Org { name: b"work".as_slice().into() }));
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
fn a_marker_at_a_word_boundary_is_accepted() {
    for name in [b"kit:bot".as_slice(), b"(kit:bot)", b"note kit:bot", b"xkit:bot kit:bot", b"kit:botx kit:bot"] {
        let mut run = Run::managed();
        fixtures::to_bot(&mut run);
        assert_eq!(run.step(user(fixtures::person(b"kit", name))), Need::Read(Item::ApiToken), "{name:?}");
    }
}
fn short_word_bot(name: &[u8]) -> Need {
    let mut given = fixtures::given();
    given.word = b"b".as_slice().into();
    let mut run = Run::with_given(given);
    fixtures::to_bot(&mut run);
    run.step(user(fixtures::person(b"kit", name)))
}
#[test]
fn a_marker_inside_another_word_is_refused() {
    for name in [b"ab:bot".as_slice(), b"0b:bot", b"-b:bot"] {
        assert_eq!(short_word_bot(name), fixtures::named_failure(Stage::Bot, b"kit", Why::Unmarked), "{name:?}");
    }
}
#[test]
fn a_marker_followed_by_word_bytes_is_refused() {
    for name in [b"b:botx".as_slice(), b"b:bot0", b"b:bot-"] {
        assert_eq!(short_word_bot(name), fixtures::named_failure(Stage::Bot, b"kit", Why::Unmarked), "{name:?}");
    }
}
#[test]
fn an_unmarked_admin_is_refused_as_unmarked() {
    let mut run = Run::managed();
    fixtures::to_bot(&mut run);
    let mut bot = fixtures::person(b"kit", b"a note");
    bot.is_admin = true;
    assert_eq!(run.step(user(bot)), fixtures::named_failure(Stage::Bot, b"kit", Why::Unmarked));
}
#[test]
fn an_empty_api_token_is_absent() {
    let mut run = fixtures::token_read(Item::ApiToken);
    assert_eq!(run.step(Event::Read(Some(secret(b"")))), fixtures::send(Request::ApiSettings));
}
#[test]
fn an_empty_git_token_is_absent() {
    let mut run = fixtures::token_read(Item::GitToken);
    assert_eq!(run.step(Event::Read(Some(secret(b"")))), fixtures::send(Request::ApiSettings));
}
#[test]
fn an_empty_bot_id_is_absent() {
    let mut run = fixtures::id_read();
    assert_eq!(
        run.step(Event::Read(Some(secret(b"")))),
        Need::Write { item: Item::User, bytes: secret(&42u64.to_le_bytes()) }
    );
    assert_eq!(run.step(Event::Written), Need::Done);
}
fn remake_script(status: u16) {
    use skein_forge::forgejo::request::Page;
    for item in [Item::ApiToken, Item::GitToken] {
        for deletion in [Reply::Empty, Reply::Status(404)] {
            let mut run = fixtures::token_read(item);
            run.step(Event::Read(Some(secret(b"old-token"))));
            assert_eq!(
                run.step(Event::Reply(Reply::Status(status))),
                fixtures::send(Request::ApiSettings),
                "{item:?} {status}"
            );
            assert_eq!(
                run.step(fixtures::settings_event(fixtures::PAGE_CAP)),
                fixtures::send(Request::Tokens {
                    login: b"kit".as_slice().into(),
                    page: Page { page: Some(1), limit: Some(fixtures::PAGE_CAP) },
                })
            );
            let row = fixtures::token_row(7, fixtures::token_name(item));
            assert_eq!(
                run.step(fixtures::rows(Box::new([row]))),
                fixtures::send(Request::DeleteToken { login: b"kit".as_slice().into(), id: 7 })
            );
            assert_eq!(run.step(Event::Reply(deletion)), fixtures::send(fixtures::create_token(item)));
            assert_eq!(run.step(fixtures::token_event()), Need::Write { item, bytes: secret(b"new-token") });
        }
    }
}
#[test]
fn a_revoked_token_is_deleted_made_and_written() {
    remake_script(401);
}
#[test]
fn a_forbidden_kept_token_is_remade() {
    remake_script(403);
}
#[test]
fn a_non_auth_status_checking_a_token_stops() {
    for item in [Item::ApiToken, Item::GitToken] {
        for status in [404, 429, 500, 502, 503] {
            let mut run = fixtures::token_read(item);
            run.step(Event::Read(Some(secret(b"bot-token"))));
            assert_eq!(
                run.step(Event::Reply(Reply::Status(status))),
                fixtures::named_failure(Stage::Token, fixtures::token_name(item), Why::Refused(status)),
                "{item:?} {status}"
            );
        }
    }
}
#[test]
fn a_down_checking_a_token_stops() {
    for item in [Item::ApiToken, Item::GitToken] {
        let mut run = fixtures::token_read(item);
        run.step(Event::Read(Some(secret(b"bot-token"))));
        assert_eq!(
            run.step(Event::Reply(Reply::Down)),
            fixtures::named_failure(Stage::Token, fixtures::token_name(item), Why::Down),
            "{item:?}"
        );
    }
}

#[test]
fn a_bot_of_id_zero_is_refused_as_unexpected() {
    let mut run = Run::owned();
    run.bot_lookup();
    let mut bot = fixtures::person(b"kit", b"kit:bot");
    bot.id = 0;
    assert_eq!(
        run.step(user(bot)),
        Need::Stop(Failure { forge: 2, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::Unexpected })
    );
}

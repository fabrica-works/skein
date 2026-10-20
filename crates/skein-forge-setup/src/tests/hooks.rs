use super::fixtures::{self, AdoptRun, secret};
use crate::call::Call;
use crate::{
    item::Item,
    reconcile::{Event, Need, Reply, Stage, Why},
};
use skein_forge::forgejo::{
    delivery::SUBSCRIBED,
    document::{Document, Permission, Row},
    request::{HookSpec, Repo, Request},
};

pub(super) fn url() -> Box<[u8]> {
    crate::reconcile::hook_url(fixtures::listener(), 1).expect("loopback")
}
fn at() -> Repo {
    Repo { owner: b"acme".as_slice().into(), name: b"specimen".as_slice().into() }
}
fn failure(why: Why) -> Need {
    fixtures::named_failure(Stage::Repository, b"acme/specimen", why)
}
fn permission(mut run: AdoptRun) -> AdoptRun {
    run.step(Event::Begin);
    run.step(Event::Read(Some(secret(fixtures::ADMIN))));
    run.step(fixtures::user_event(fixtures::admin()));
    run.step(fixtures::repository(7, b"acme", b"specimen"));
    run
}
// Prefixes leave each hook phase's effect outstanding.
pub(super) fn phase(phase: usize) -> AdoptRun {
    let mut run = permission(fixtures::adopt_listening());
    assert_eq!(run.step(fixtures::permission(Permission::Write)), Need::Read(Item::WebhookSecret));
    if phase == 0 {
        return run;
    }
    assert_eq!(run.step(Event::Read(None)), Need::Draw(32));
    if phase == 1 {
        return run;
    }
    run.step(fixtures::drawn());
    if phase == 2 {
        return run;
    }
    assert_eq!(run.step(Event::Written), fixtures::send(Request::ApiSettings));
    if phase == 3 {
        return run;
    }
    assert_eq!(run.step(fixtures::settings_event(2)), listing(1));
    if phase == 4 {
        return run;
    }
    if phase == 5 {
        run.step(fixtures::rows(Box::new([fixtures::hook(3, &url()), fixtures::hook(5, &url())])));
    } else {
        run.step(fixtures::rows(Box::new([])));
    }
    run
}
fn listing(page: u32) -> Need {
    fixtures::send(Request::RepoHooks { at: at(), page: fixtures::page(page) })
}
fn delete(id: u64) -> Need {
    fixtures::send(Request::DeleteRepoHook { at: at(), id })
}
fn save(need: Need, id: Option<u64>) -> HookSpec {
    match (need, id) {
        (Need::Send { request: Call::Forgejo(Request::CreateRepoHook { at: path, hook }), .. }, None) => {
            assert_eq!(path, at());
            hook
        }
        (Need::Send { request: Call::Forgejo(Request::EditRepoHook { at: path, id: found, hook }), .. }, Some(id)) => {
            assert_eq!(path, at());
            assert_eq!(found, id);
            hook
        }
        (other, id) => panic!("save {id:?}: {other:?}"),
    }
}
pub(super) fn saved() -> Event {
    let Row::Hook(hook) = fixtures::hook(3, &url()) else { unreachable!("hook fixture") };
    Event::Reply(Reply::Document(Document::Hook(hook)))
}
#[test]
fn a_bot_with_write_and_a_listener_asks_for_the_webhook_secret() {
    let mut run = permission(fixtures::adopt_listening());
    assert_eq!(run.step(fixtures::permission(Permission::Write)), Need::Read(Item::WebhookSecret));
}
#[test]
fn no_listener_ends_adopt_after_the_collaborator() {
    let mut run = permission(fixtures::adopt());
    run.step(fixtures::permission(Permission::None));
    assert_eq!(run.step(Event::Reply(Reply::Empty)), Need::Done(fixtures::done()));
}
#[test]
fn a_listener_off_loopback_ends_adopt_asking_no_secret() {
    let mut given = fixtures::given();
    given.listener = Some(core::net::SocketAddr::from(([192, 0, 2, 1], 8080)));
    let mut run = permission(AdoptRun { machine: crate::adopt::Adopt::new(&given, 7).expect("given") });
    assert_eq!(run.step(fixtures::permission(Permission::Write)), Need::Done(fixtures::done()));
}
#[test]
fn a_kept_secret_is_not_drawn() {
    let mut run = phase(0);
    assert_eq!(run.step(Event::Read(Some(secret(b"kept-secret")))), fixtures::send(Request::ApiSettings));
    run.step(fixtures::settings_event(2));
    assert_eq!(save(run.step(fixtures::rows(Box::new([]))), None).secret, secret(b"kept-secret"));
}
#[test]
fn a_missing_secret_is_drawn_then_written_as_64_lowercase_hex() {
    for value in [None, Some(secret(b""))] {
        let mut run = phase(0);
        assert_eq!(run.step(Event::Read(value)), Need::Draw(32));
        let mut bytes = skein_lib::List::with_capacity(32);
        for byte in 0_u8..32 {
            bytes.push(byte).expect("draw capacity");
        }
        let bytes = bytes.into_boxed();
        let expected = b"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        assert_eq!(
            run.step(Event::Drawn(secret(&bytes))),
            Need::Write { item: Item::WebhookSecret, bytes: secret(expected) }
        );
    }
}
#[test]
fn a_drawn_secret_of_another_length_is_unexpected() {
    for count in [0, 31, 33] {
        assert_eq!(
            phase(1).step(Event::Drawn(secret([0; 33].get(..count).expect("draw length")))),
            failure(Why::Unexpected)
        );
    }
}
#[test]
fn the_page_cap_is_read_once_before_the_first_listing() {
    let mut run = phase(3);
    assert_eq!(run.step(fixtures::settings_event(2)), listing(1));
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::hook(1, b"elsewhere"), fixtures::hook(2, b"elsewhere")]))),
        listing(2)
    );
}
#[test]
fn a_zero_page_cap_is_unexpected() {
    assert_eq!(phase(3).step(fixtures::settings_event(0)), failure(Why::Unexpected));
}
#[test]
fn hooks_are_listed_page_by_page_until_a_short_page() {
    let mut run = phase(4);
    for page in [2, 3] {
        assert_eq!(
            run.step(fixtures::rows(Box::new([fixtures::hook(1, b"other"), fixtures::hook(2, b"other")]))),
            listing(page)
        );
    }
    save(run.step(fixtures::rows(Box::new([fixtures::hook(7, b"other")]))), None);
}
#[test]
fn a_hook_of_another_url_is_left_alone() {
    save(phase(4).step(fixtures::rows(Box::new([fixtures::hook(8, b"other")]))), None);
}
#[test]
fn the_lowest_id_of_the_listeners_url_is_kept_and_each_other_deleted() {
    let mut run = phase(4);
    assert_eq!(run.step(fixtures::rows(Box::new([fixtures::hook(5, &url()), fixtures::hook(3, &url())]))), delete(5));
    assert_eq!(run.step(Event::Reply(Reply::Empty)), listing(1));
    assert_eq!(run.step(fixtures::rows(Box::new([fixtures::hook(3, &url()), fixtures::hook(9, &url())]))), delete(9));
    run.step(Event::Reply(Reply::Empty));
    save(run.step(fixtures::rows(Box::new([fixtures::hook(3, &url())]))), Some(3));
}
#[test]
fn a_deleted_duplicate_lists_the_same_page_again() {
    assert_eq!(phase(5).step(Event::Reply(Reply::Empty)), listing(1));
}
#[test]
fn a_404_on_a_duplicates_delete_is_as_good() {
    assert_eq!(phase(5).step(Event::Reply(Reply::Status(404))), listing(1));
}
#[test]
fn the_kept_id_is_remembered_across_pages() {
    let mut run = phase(4);
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::hook(3, &url()), fixtures::hook(8, b"other")]))),
        listing(2)
    );
    assert_eq!(run.step(fixtures::rows(Box::new([fixtures::hook(9, &url())]))), delete(9));
    assert_eq!(run.step(Event::Reply(Reply::Empty)), listing(2));
    save(run.step(fixtures::rows(Box::new([]))), Some(3));
}
#[test]
fn the_kept_hook_met_again_on_a_reread_page_is_not_deleted() {
    let mut run = phase(5);
    assert_eq!(run.step(Event::Reply(Reply::Empty)), listing(1));
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::hook(3, &url()), fixtures::hook(8, b"other")]))),
        listing(2)
    );
    save(run.step(fixtures::rows(Box::new([]))), Some(3));
}
#[test]
fn no_hook_of_the_listeners_url_makes_one() {
    let mut run = phase(4);
    save(run.step(fixtures::rows(Box::new([]))), None);
    assert_eq!(run.step(saved()), Need::Done(fixtures::done()));
}
#[test]
fn a_kept_hook_is_patched_with_the_url_secret_events_json_and_active() {
    let mut run = phase(4);
    let hook = save(run.step(fixtures::rows(Box::new([fixtures::hook(7, &url())]))), Some(7));
    assert_eq!(hook.url, url());
    assert_eq!(hook.secret.expose(), fixtures::DRAWN_HEX);
    assert!(hook.active);
    assert_eq!(hook.events.len(), SUBSCRIBED.len());
    for (event, expected) in hook.events.iter().zip(SUBSCRIBED) {
        assert_eq!(event.as_ref(), expected);
    }
    // HookSpec's encoder fixes the content type to JSON.
    assert_eq!(run.step(saved()), Need::Done(fixtures::done()));
}
#[test]
fn a_refused_hook_request_names_the_repositorys_path() {
    for p in 3..7 {
        assert_eq!(phase(p).step(Event::Reply(Reply::Status(403))), failure(Why::Refused(403)));
    }
}
#[test]
fn a_forge_that_stops_answering_stops_at_each_hook_request() {
    for p in 3..7 {
        assert_eq!(phase(p).step(Event::Reply(Reply::Down)), failure(Why::Down));
    }
}
#[test]
fn a_reply_of_another_kind_is_unexpected_at_each_hook_request() {
    for p in 3..7 {
        assert_eq!(phase(p).step(fixtures::bot_event()), failure(Why::Unexpected));
    }
}
#[test]
fn a_row_that_is_not_a_hook_is_unexpected() {
    assert_eq!(phase(4).step(fixtures::rows(Box::new([fixtures::token_row(1, b"wrong")]))), failure(Why::Unexpected));
}
#[test]
fn an_out_of_order_event_stops_each_hook_phase() {
    for p in 0..7 {
        for event in [Event::Begin, Event::Reply(Reply::Empty), Event::Read(None), fixtures::drawn(), Event::Written] {
            let expected = match &event {
                Event::Read(_) => p == 0,
                Event::Drawn(_) => p == 1,
                Event::Written => p == 2,
                Event::Reply(reply) => p == 5 && *reply == Reply::Empty,
                Event::Begin => false,
            };
            if expected {
                continue;
            }
            assert_eq!(phase(p).step(event), failure(Why::Unexpected), "phase {p}");
        }
    }
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the redaction test inspects the hook failure and machine Debug")]
fn no_secret_is_in_a_hook_failures_debug() {
    let mut run = phase(6);
    let stop = run.step(Event::Reply(Reply::Status(403)));
    let text = format!("{stop:?} {:?}", run.machine);
    for bytes in text.as_bytes().windows(64) {
        assert_ne!(bytes, fixtures::DRAWN_HEX);
    }
    for bytes in text.as_bytes().windows(32) {
        assert_ne!(bytes, [0x0a; 32]);
    }
    assert!(text.contains("secret: None"));
}

#[test]
fn a_learned_cap_above_inner_pages_hooks_at_inner() {
    use skein_forge::forgejo::request::Page;
    let cap = super::host::limits().inner;
    let mut run = phase(3);
    assert_eq!(
        run.step(fixtures::host_bounded_settings()),
        fixtures::send(Request::RepoHooks { at: at(), page: Page { page: Some(1), limit: Some(cap) } })
    );
    let mut rows = skein_lib::List::with_capacity(cap);
    for id in 1..=cap {
        rows.push(fixtures::hook(u64::from(id), b"elsewhere")).expect("page fits");
    }
    assert_eq!(
        run.step(fixtures::rows(rows.into_boxed())),
        fixtures::send(Request::RepoHooks { at: at(), page: Page { page: Some(2), limit: Some(cap) } })
    );
    save(run.step(fixtures::rows(Box::new([fixtures::hook(99, b"elsewhere")]))), None);
    assert_eq!(run.step(saved()), Need::Done(fixtures::done()));
}

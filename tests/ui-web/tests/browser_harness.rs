use skein_fake_person::{Find, Step};
use skein_fake_person_browser::{Outcome, Script, harness};
use skein_lib::Duration;
use skein_scratch::Scratch;
use skein_ui::Role;
use skein_ui_web_tests::{fixture_bundle, fixture_harness};
#[test]
fn the_harness_fails_a_page_whose_module_throws() {
    let scratch = Scratch::new("harness-throws");
    let bundle =
        fixture_bundle(&scratch, b"export default function init() { throw new Error('fixture module failed'); }");
    let mut person = Script::new(Box::new([harness::Step::Person(Step::See {
        find: Find::exactly_named(Role::Heading { level: 1 }, b"Ready"),
        within: Duration::from_secs(1),
    })]));
    let Outcome::Failed { report, .. } = fixture_harness(&bundle).run(&mut [&mut person]) else {
        panic!("a throwing module fails the story");
    };
    assert!(report.contains("unexpected Exception: Uncaught"), "{report}");
}
#[test]
fn the_harness_reads_the_listening_line() {
    let scratch = Scratch::new("harness-listens");
    let bundle = fixture_bundle(&scratch,
        b"export default function init() { const h = document.createElement('h1'); h.textContent = 'Ready'; document.querySelector('#app').append(h); }");
    let mut person = Script::new(Box::new([harness::Step::Person(Step::See {
        find: Find::exactly_named(Role::Heading { level: 1 }, b"Ready"),
        within: Duration::from_secs(1),
    })]));
    assert_eq!(fixture_harness(&bundle).run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn the_harness_searches_the_next_accessible_role() {
    let scratch = Scratch::new("harness-next-role");
    let bundle = fixture_bundle(&scratch, b"export default function init() { const e = document.createElement('input'); e.type = 'number'; e.setAttribute('aria-label', 'Count'); document.querySelector('#app').append(e); }");
    let mut person = Script::new(Box::new([harness::Step::Person(Step::See {
        find: Find::exactly_named(Role::TextField, b"Count"),
        within: Duration::from_secs(1),
    })]));
    assert_eq!(fixture_harness(&bundle).run(&mut [&mut person]), Outcome::Done);
}

const READY: &[u8] = b"export default function init() { const h = document.createElement('h1'); h.textContent = 'Ready'; document.querySelector('#app').append(h); }";
#[test]
fn a_page_of_another_origin_is_opened_by_its_step() {
    let scratch = Scratch::new("harness-origin");
    let bundle = fixture_bundle(&scratch, READY);
    let origin = harness::Origin {
        host: b"localhost".as_slice().into(),
        pages: vec![skein_ui_web_tests::page(b"/", b"<!doctype html><h1>Elsewhere</h1>")],
    };
    let mut person = Script::new(Box::new([
        harness::Step::Visit { origin: 0, path: b"/".as_slice().into() },
        harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Heading { level: 1 }, b"Elsewhere"),
            within: skein_ui_web_tests::SECOND,
        }),
    ]));
    assert_eq!(skein_ui_web_tests::fixture_harness_with(&bundle, vec![origin]).run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn a_framed_page_s_refusal_is_awaited_as_trouble() {
    let scratch = Scratch::new("harness-framed");
    let bundle = fixture_bundle(&scratch, READY);
    let framing = skein_ui_web_tests::page(b"/", b"<!doctype html><iframe src=\"http://{address}/\"></iframe>");
    let origin = harness::Origin { host: b"localhost".as_slice().into(), pages: vec![framing] };
    let mut person = Script::new(Box::new([
        harness::Step::Visit { origin: 0, path: b"/".as_slice().into() },
        harness::Step::Trouble { words: b"frame-ancestors".as_slice().into(), within: skein_ui_web_tests::SECOND },
    ]));
    assert_eq!(skein_ui_web_tests::fixture_harness_with(&bundle, vec![origin]).run(&mut [&mut person]), Outcome::Done);
}
#[test]
fn a_request_with_another_host_has_its_status_awaited() {
    let scratch = Scratch::new("harness-host");
    let bundle = fixture_bundle(&scratch, READY);
    let mut person = Script::new(Box::new([harness::Step::Request {
        method: skein_http::Method::Get,
        path: b"/".as_slice().into(),
        host: b"other.test".as_slice().into(),
        headers: vec![],
        body: Box::new([]),
        status: 200,
        within: skein_ui_web_tests::SECOND,
    }]));
    assert_eq!(fixture_harness(&bundle).run(&mut [&mut person]), Outcome::Done);
}

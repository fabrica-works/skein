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

fn ready_person() -> Script {
    Script::new(Box::new([harness::Step::Person(Step::See {
        find: Find::exactly_named(Role::Heading { level: 1 }, b"Ready"),
        within: skein_ui_web_tests::SECOND,
    })]))
}

#[test]
fn a_lent_peer_listens_before_the_server_requests_it() {
    let mut peer = skein_ui_web_tests::harness_peer::Peer::new();
    let mut person = ready_person();
    let outcome = skein_ui_web_tests::lending_harness(&["{neighbor}"], vec![])
        .run_lending(&mut [&mut person], &mut [harness::Lent { name: b"neighbor", peer: &mut peer }]);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(peer.observations, [b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n".to_vec()]);
}

#[test]
fn a_lent_peer_returns_empty_with_its_handle_usable() {
    use skein_world::Host;
    let mut peer = skein_ui_web_tests::harness_peer::Peer::new();
    let handle = peer.handle.clone();
    let mut person = ready_person();
    let outcome = skein_ui_web_tests::lending_harness(&["{neighbor}"], vec![])
        .run_lending(&mut [&mut person], &mut [harness::Lent { name: b"neighbor", peer: &mut peer }]);
    assert_eq!(outcome, Outcome::Done);
    assert!(peer.is_empty());
    assert_eq!(handle.get(), 1);
    assert_eq!(peer.observations.len(), 1);
}

#[test]
fn a_lent_peers_assertion_names_it_in_the_failure() {
    use skein_world::Host;
    let mut peer = skein_ui_web_tests::harness_peer::Peer::new();
    peer.assert_request = true;
    let mut person = ready_person();
    let outcome = skein_ui_web_tests::lending_harness(&["{neighbor}"], vec![])
        .run_lending(&mut [&mut person], &mut [harness::Lent { name: b"neighbor", peer: &mut peer }]);
    let Outcome::Failed { report, .. } = outcome else { panic!("asserting peer fails the run") };
    assert!(report.contains("lent peer neighbor: fixture request assertion"), "{report}");
    assert!(peer.is_empty());
}

#[test]
fn colliding_peer_tokens_reach_their_own_hosts() {
    use skein_world::Host;
    let mut first = skein_ui_web_tests::harness_peer::Peer::new();
    let mut second = skein_ui_web_tests::harness_peer::Peer::new();
    let mut person = ready_person();
    let outcome = skein_ui_web_tests::lending_harness(&["{first}", "{second}"], vec![]).run_lending(
        &mut [&mut person],
        &mut [harness::Lent { name: b"first", peer: &mut first }, harness::Lent { name: b"second", peer: &mut second }],
    );
    assert_eq!(outcome, Outcome::Done);
    assert_eq!((first.observations.len(), second.observations.len()), (1, 1));
    assert!(first.is_empty() && second.is_empty());
}

#[test]
fn a_peer_shutdown_assertion_preserves_a_failed_outcome() {
    let mut peer = skein_ui_web_tests::harness_peer::Peer::new();
    peer.assert_shutdown = true;
    let mut person = ready_person();
    let outcome = skein_ui_web_tests::lending_harness(&[], vec![])
        .run_lending(&mut [&mut person], &mut [harness::Lent { name: b"neighbor", peer: &mut peer }]);
    let Outcome::Failed { report, .. } = outcome else { panic!("shutdown assertion fails the run") };
    assert!(report.contains("lent peer neighbor: fixture shutdown assertion"), "{report}");
}

#[test]
fn repeated_peer_arguments_each_reach_the_server() {
    let mut peer = skein_ui_web_tests::harness_peer::Peer::new();
    let mut person = ready_person();
    let outcome = skein_ui_web_tests::lending_harness(&["{neighbor}", "{neighbor}"], vec![])
        .run_lending(&mut [&mut person], &mut [harness::Lent { name: b"neighbor", peer: &mut peer }]);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(peer.handle.get(), 2);
}

const KEEP_PAGE: &[u8] =
    b"<!doctype html><input type=hidden value=first data-note=named data-empty=''><input type=hidden value=second>";
fn keep(selector: &[u8], attribute: Option<&[u8]>, name: &[u8]) -> harness::Step {
    harness::Step::Keep { selector: selector.into(), attribute: attribute.map(Box::from), name: name.into() }
}
fn keep_harness() -> harness::Harness {
    skein_ui_web_tests::lending_harness(
        &[],
        vec![harness::Origin {
            host: b"localhost".as_slice().into(),
            pages: vec![skein_ui_web_tests::page(b"/", KEEP_PAGE)],
        }],
    )
}
fn keep_script(steps: Vec<harness::Step>) -> Script {
    let mut all = vec![harness::Step::Visit { origin: 0, path: b"/".as_slice().into() }];
    all.extend(steps);
    Script::new(all.into())
}
fn read_kept(selector: &[u8], attribute: Option<&[u8]>) -> Vec<u8> {
    let mut harness = keep_harness();
    let mut person = keep_script(vec![keep(selector, attribute, b"read")]);
    assert_eq!(harness.run(&mut [&mut person]), Outcome::Done);
    assert_eq!(harness.kept()[0].name.as_ref(), b"read");
    harness.kept()[0].value.to_vec()
}
#[test]
fn a_keep_reads_a_hidden_inputs_value_attribute() {
    assert_eq!(read_kept(b"input[type=hidden]", None), b"first");
}
#[test]
fn a_keep_reads_a_named_attribute() {
    assert_eq!(read_kept(b"input", Some(b"data-note")), b"named");
}
#[test]
fn a_keep_retains_an_empty_attribute() {
    assert_eq!(read_kept(b"input", Some(b"data-empty")), b"");
}
#[test]
fn a_keep_reads_the_first_matching_element() {
    assert_eq!(read_kept(b"input", None), b"first");
}
#[test]
fn a_keep_without_an_element_fails_its_step() {
    let mut harness = keep_harness();
    let mut person = keep_script(vec![keep(b"#missing", None, b"read")]);
    let Outcome::Failed { person: 0, step: 1, report } = harness.run(&mut [&mut person]) else {
        panic!("missing selector fails keep")
    };
    assert!(report.contains("keep selector #missing attribute value: no element"), "{report}");
}
#[test]
fn a_keep_without_an_attribute_fails_its_step() {
    let mut harness = keep_harness();
    let mut person = keep_script(vec![keep(b"input", Some(b"absent"), b"read")]);
    let Outcome::Failed { person: 0, step: 1, report } = harness.run(&mut [&mut person]) else {
        panic!("missing attribute fails keep")
    };
    assert!(report.contains("keep selector input attribute absent: no attribute"), "{report}");
}
#[test]
fn keeps_append_duplicate_names_in_completion_order_with_their_person() {
    let mut harness = keep_harness();
    let mut first = keep_script(vec![keep(b"input", None, b"same"), keep(b"input", Some(b"data-note"), b"same")]);
    let mut second = keep_script(vec![keep(b"input", Some(b"data-empty"), b"other")]);
    assert_eq!(harness.run(&mut [&mut first, &mut second]), Outcome::Done);
    let entries: Vec<_> = harness.kept().iter().map(|k| (k.person, k.name.as_ref(), k.value.as_ref())).collect();
    assert_eq!(
        entries,
        vec![
            (0, b"same".as_slice(), b"first".as_slice()),
            (0, b"same".as_slice(), b"named".as_slice()),
            (1, b"other".as_slice(), b"".as_slice())
        ]
    );
}
#[test]
fn a_new_run_clears_the_preceding_keeps() {
    let mut harness = keep_harness();
    let mut first = keep_script(vec![keep(b"input", None, b"old")]);
    assert_eq!(harness.run(&mut [&mut first]), Outcome::Done);
    let mut second = keep_script(vec![keep(b"input", Some(b"data-empty"), b"new")]);
    assert_eq!(harness.run(&mut [&mut second]), Outcome::Done);
    assert_eq!(harness.kept(), &[harness::Kept { person: 0, name: b"new".as_slice().into(), value: Box::new([]) }]);
}
#[test]
fn a_failed_run_retains_completed_keeps() {
    let mut harness = keep_harness();
    let mut person = keep_script(vec![keep(b"input", None, b"read"), keep(b"#missing", None, b"absent")]);
    assert!(matches!(harness.run(&mut [&mut person]), Outcome::Failed { step: 2, .. }));
    assert_eq!(harness.kept().len(), 1);
    assert_eq!(harness.kept()[0].value.as_ref(), b"first");
}

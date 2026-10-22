//! Chromium scenarios through the shared real-loop rig.
#[path = "support/real.rs"]
pub mod real;
use real::{Profile, Rig, Scenario, chromium};

#[test]
fn chromium_opens_finds_and_presses_a_button() {
    let chromium = chromium();
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium, &profile.0, Scenario::Button);
    rig.run();
    assert!(rig.root_closed && rig.exited && rig.child_closed);
    assert!(rig.version.starts_with(b"Chrome/"), "version: {}", String::from_utf8_lossy(&rig.version));
}

#[test]
fn chromium_selects_all_replaces_and_submits_a_form() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Form).run();
}

#[test]
fn chromium_rejects_a_covered_button() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Covered).run();
}

#[test]
fn chromium_scrolls_to_a_far_button() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Scroll).run();
}

#[test]
fn chromium_obeys_content_security_policy() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Csp).run();
}

#[test]
fn chromium_reports_a_script_exception() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Throw).run();
}

#[test]
fn chromium_finds_a_sentence_by_a_few_of_its_words() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Words).run();
}

#[test]
fn chromium_presses_an_svg_shape_through_its_figure_at_a_point() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Figure).run();
}

// A reload after either kind of exception must not race the kit's DOM/AX reads.
const RELOAD_SOURCES: &[(&str, &str)] = &[
    (
        "JavaScript",
        "Promise.resolve().then(() => { try { throw Error('reduction'); } finally { setTimeout(() => location.reload(), 0); } });",
    ),
    (
        "Wasm",
        "WebAssembly.instantiate(new Uint8Array([0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,116,114,97,112,0,0,10,5,1,3,0,0,11])).then(({instance}) => { try { instance.exports.trap(); } finally { setTimeout(() => location.reload(), 0); } });",
    ),
];
#[test]
fn chromium_survives_a_page_that_reloads_itself() {
    for (name, source) in RELOAD_SOURCES {
        let page = format!(
            "<!doctype html><html><body><h1>Reloaded</h1><script>if (!sessionStorage.getItem('reload-fact')) {{ sessionStorage.setItem('reload-fact', '1'); document.querySelector('h1').textContent='Before'; {source} }}</script></body></html>"
        );
        let profile = Profile::fresh();
        let mut rig = Rig::new(&chromium(), &profile.0, Scenario::Reduction);
        rig.fixed_page(page.into_bytes());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rig.run()));
        assert!(result.is_ok(), "reload case {name} failed: {result:?}");
    }
}

#[test]
fn chromium_accepts_the_closing_warning_on_a_reload() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Leave).run();
}
#[test]
fn chromium_dismisses_a_confirm_as_trouble() {
    let chromium = chromium();
    let profile = Profile::fresh();
    Rig::new(&chromium, &profile.0, Scenario::Confirm).run();
}

#[test]
fn chromium_refuses_a_scope_found_before_a_reload() {
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium(), &profile.0, Scenario::ScopeReload);
    rig.fixed_page(
        b"<!doctype html><html><body><section aria-label='Scope'><h1>Inside</h1></section></body></html>".to_vec(),
    );
    rig.run();
}

const ATTRIBUTE_PAGE: &[u8] =
    b"<!doctype html><input type=hidden value='first' data-note='named'><input type=hidden value='second'>";
fn attribute_rig(selector: &'static [u8], name: &'static [u8], expected: Option<&'static [u8]>) {
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium(), &profile.0, Scenario::Attribute { selector, name, expected });
    rig.fixed_page(ATTRIBUTE_PAGE.to_vec());
    rig.run();
}
#[test]
fn chromium_reads_the_first_hidden_inputs_value_attribute() {
    attribute_rig(b"input[type=hidden]", b"value", Some(b"first"));
}
#[test]
fn chromium_reads_a_named_markup_attribute() {
    attribute_rig(b"input", b"data-note", Some(b"named"));
}
#[test]
fn chromium_reports_a_selector_without_an_element() {
    attribute_rig(b".absent", b"value", None);
}

#[test]
fn chromium_reads_the_pages_look_and_captures_a_distant_part_at_double_scale() {
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium(), &profile.0, Scenario::Look);
    rig.look_fixture();
    rig.run();
    assert!(
        rig.version.starts_with(b"Chrome/147."),
        "look facts require Chromium 147: {}",
        String::from_utf8_lossy(&rig.version)
    );
}

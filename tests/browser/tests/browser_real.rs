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
fn chromium_types_submits_and_snapshots_a_form() {
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

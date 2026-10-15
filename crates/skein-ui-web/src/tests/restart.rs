use crate::restart::{Action, Restart};
use skein_lib::{Duration, Wall};

fn restart() -> Restart {
    Restart::new(3, Duration::from_secs(60), "skein-ui-restarts", "The application failed.")
}

#[test]
fn a_fourth_panic_in_a_minute_says_the_application_failed() {
    let restart = restart();
    let mut dates = Vec::new();
    let actions = [1, 2, 3, 4]
        .map(|seconds| restart.panicked(Wall::from_nanos(Duration::from_secs(seconds).as_nanos()), &mut dates));
    assert_eq!(actions, [Action::Reload, Action::Reload, Action::Reload, Action::Failed]);
}

#[test]
fn a_panic_past_the_minute_reloads_again() {
    let restart = restart();
    let mut dates = vec![Wall::EPOCH; 4];
    assert_eq!(restart.panicked(Wall::from_nanos(60_000_000_001), &mut dates), Action::Reload);
}

use super::Facts;
use skein_lib::{Time, Wall};

// Browser clocks have millisecond precision and finite nonnegative values.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "browser dates and clocks are nonnegative milliseconds; saturating cast bounds the result"
)]
fn nanos(millis: f64) -> u64 {
    (millis.max(0.0) * 1_000_000.0) as u64
}
#[must_use]
pub fn clocks() -> (Time, Wall) {
    let window = web_sys::window().expect("browser window");
    let now = window.performance().expect("monotonic browser clock").now();
    (Time::from_nanos(nanos(now)), Wall::from_nanos(nanos(js_sys::Date::now())))
}
#[must_use]
pub fn facts() -> Facts {
    let window = web_sys::window().expect("browser window");
    let navigator = window.navigator();
    let mut random = [0_u8; 8];
    window
        .crypto()
        .expect("browser cryptographic generator")
        .get_random_values_with_u8_array(&mut random)
        .expect("seed from cryptographic generator");
    let languages =
        navigator.languages().iter().filter_map(|language| language.as_string()).collect::<Vec<_>>().join(",");
    let (now, wall) = clocks();
    Facts {
        now,
        wall,
        seed: u64::from_le_bytes(random),
        offset: offset(),
        languages: languages.into_bytes().into(),
        primary: super::primary(navigator.platform().ok().as_deref()),
    }
}
#[expect(clippy::cast_possible_truncation, reason = "the browser timezone offset is integral minutes within i32")]
fn offset() -> i32 {
    -(js_sys::Date::new_0().get_timezone_offset() as i32)
}

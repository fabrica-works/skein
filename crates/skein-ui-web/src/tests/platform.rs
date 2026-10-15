use crate::{mapping::Primary, platform::primary};
const CASES: &[(Option<&str>, Primary)] = &[
    (Some("MacIntel"), Primary::Meta),
    (Some("iPhone"), Primary::Meta),
    (Some("iPad"), Primary::Meta),
    (Some("iPod"), Primary::Meta),
    (Some("Linux x86_64"), Primary::Control),
    (Some("Win32"), Primary::Control),
    (Some(""), Primary::Control),
    (None, Primary::Control),
];
#[test]
fn the_primary_modifier_is_meta_on_apples_platforms_alone() {
    for (platform, expected) in CASES {
        assert_eq!(primary(*platform), *expected, "{platform:?}");
    }
}

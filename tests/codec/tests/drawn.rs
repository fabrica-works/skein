//! Canonical draws agree with generated codecs (codec.md, section 8.3).

extern crate alloc;

mod support;

#[cfg(test)]
mod tests {
    use alloc::collections::{BTreeMap, BTreeSet};

    use skein_codegen::Declaration;
    use skein_lib::Rng;

    use super::support::generated::{Decoded, draw, families};

    // Thirty-two draws per root exercise the fixture's variants and option arms.
    const DRAWS: u64 = 32;

    fn seed(family: &str, root: &str, round: u64) -> u64 {
        family
            .bytes()
            .chain([0])
            .chain(root.bytes())
            .fold(round, |hash, byte| hash.wrapping_mul(0x100_0000_01b3) ^ u64::from(byte))
    }

    #[test]
    fn drawn_values_decode_by_the_generated_code_to_their_own_bytes() {
        let mut tags = BTreeMap::<String, BTreeSet<u8>>::new();
        let mut shown = BTreeMap::<String, String>::new();
        for family in families() {
            let schema = skein_codegen::parse(family.text).expect("valid schema");
            let names = schema
                .declarations
                .iter()
                .map(|declaration| match declaration {
                    Declaration::Record(record) => record.name.as_str(),
                    Declaration::Enum(enumeration) => enumeration.name.as_str(),
                })
                .collect::<Vec<_>>();
            assert_eq!(
                family.roots.iter().map(|root| root.name).collect::<Vec<_>>(),
                names,
                "every declaration has a decoder: {}",
                family.name
            );
            for root in family.roots {
                let key = format!("{}.{}", family.name, root.name);
                for round in 0..DRAWS {
                    let seed = seed(family.name, root.name, round);
                    let bytes = draw(&schema, root.name, &mut Rng::new(seed));
                    let context = format!("{key}, seed {seed:#018x}, bytes {bytes:?}");
                    let (debug, encoded) = match (root.decode)(&bytes) {
                        Decoded::Value { debug, bytes } => Ok((debug, bytes)),
                        Decoded::Problem { path, reason } => Err(format!("{context}: {path}: {reason:?}")),
                    }
                    .expect("draw decodes by the generated code");
                    assert_eq!(encoded, bytes, "{context}: {debug}");
                    let debug_shown = shown.entry(key.clone()).or_default();
                    debug_shown.push_str(&debug);
                    debug_shown.push('\n');
                    if let Some(tag) = bytes.first() {
                        tags.entry(key.clone()).or_default().insert(*tag);
                    }
                }
            }
        }

        let schema = skein_codegen::parse(include_str!("../schema/every-v1.schema")).expect("every schema");
        for declaration in &schema.declarations {
            if let Declaration::Enum(enumeration) = declaration {
                let key = format!("every.{}", enumeration.name);
                let reached = tags.get(&key).expect("enum root drawn");
                let expected = (0..enumeration.variants.len())
                    .map(|tag| u8::try_from(tag).expect("tag fits"))
                    .collect::<BTreeSet<_>>();
                assert_eq!(*reached, expected, "every variant reached: {key}");
            }
        }
        for (root, arms) in [
            ("Stamp", &["note: None", "note: Some("][..]),
            (
                "Nest",
                &[
                    "maybe_list: None",
                    "maybe_list: Some(",
                    "maybe_numbers: None",
                    "maybe_numbers: Some(",
                    "maybe_lights: List { items: [None",
                    "maybe_lights: List { items: [Some(",
                ][..],
            ),
        ] {
            let key = format!("every.{root}");
            let debug = shown.get(&key).expect("fixture root drawn");
            for arm in arms {
                assert!(debug.contains(arm), "option arm reached: {key}: {arm}\n{debug}");
            }
        }
    }
}

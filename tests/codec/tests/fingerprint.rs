//! All shipped schemas preserve their layouts through names and presentation.

#[path = "support/generated.rs"]
#[expect(dead_code, reason = "fingerprint tests use schema text, not generated decoders or draws")]
mod generated;
#[path = "support/reprint.rs"]
mod reprint;

#[cfg(test)]
mod tests {
    use super::{
        generated,
        reprint::{renamed, reprint},
    };
    use core::fmt::Write;
    use skein_codec::schema::{DeclIx, parse};

    #[test]
    fn every_schema_preserves_fingerprints_after_reprinting() {
        let mut families: Vec<_> = generated::families().into_iter().map(|family| (family.name, family.text)).collect();
        families.push(("notes", include_str!("../../../examples/ui/view/schema/notes-v1.schema")));
        for (name, text) in families {
            let schema = parse(text.as_bytes()).expect("shipped schema");
            let printed = reprint(&schema);
            let other = parse(printed.as_bytes()).expect("reprinted schema");
            let expected = schema.fingerprints();
            let fingerprints = other.fingerprints();
            assert_eq!(expected.family(), fingerprints.family(), "{name}");
            for (at, rename) in (0..schema.declarations()).zip(renamed(&schema)) {
                let at = DeclIx::new(at);
                let renamed_at = other.find(rename.new.as_bytes()).expect("renamed declaration");
                assert_eq!(expected.declaration(at), fingerprints.declaration(renamed_at), "{name}: {}", rename.old);
            }
        }
    }

    #[test]
    fn every_fixture_family_digest_is_golden() {
        let schema = parse(include_str!("../schema/every-v1.schema").as_bytes()).expect("every schema");
        let mut hex = String::new();
        for byte in schema.fingerprints().family().digest() {
            write!(hex, "{byte:02x}").expect("string write");
        }
        assert_eq!(hex, "8b13d594a02c726d494e5811152222d9f3d294c9f396773f71b517f000e786fb");
    }

    #[test]
    fn reprint_exposes_local_renames_and_reverses_payload_boxing() {
        let schema = parse(b"family f 2\n# comment\nrecord A { x: u8 }\nenum E { a, b: A, c: boxed A }\nrecord R { x: A, y: bytes 8 holds foreign A, z: bytes 8 pieces A }")
        .expect("reprint fixture");
        assert_eq!(
            reprint(&schema),
            concat!(
                "family f 2\n",
                "record AR { x_r: u8, }\n",
                "enum ER { a_r, b_r: boxed AR, c_r: AR, }\n",
                "record RR { x_r: AR, y_r: bytes 8 holds foreign A, z_r: bytes 8 pieces AR, }\n",
            )
        );
        let names =
            renamed(&schema).into_iter().map(|rename| (rename.old, rename.new, rename.parts)).collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                ("A".into(), "AR".into(), vec![("x".into(), "x_r".into())]),
                (
                    "E".into(),
                    "ER".into(),
                    vec![("a".into(), "a_r".into()), ("b".into(), "b_r".into()), ("c".into(), "c_r".into())]
                ),
                (
                    "R".into(),
                    "RR".into(),
                    vec![("x".into(), "x_r".into()), ("y".into(), "y_r".into()), ("z".into(), "z_r".into())]
                ),
            ]
        );
    }
}

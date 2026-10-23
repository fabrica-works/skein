//! Shipped schemas preserve fingerprints and diff through reprinting (codec.md, 10.6).

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
    use skein_codec::schema::{DeclIx, Schema, parse};
    use skein_codegen::diff::{Change, Kind, Op, Rename, Version, diff};

    fn shipped_schemas() -> Vec<(&'static str, &'static str)> {
        let mut families: Vec<_> = generated::families().into_iter().map(|family| (family.name, family.text)).collect();
        families.push(("notes", include_str!("../../../examples/ui/view/schema/notes-v1.schema")));
        families
    }

    fn shipped_schema(name: &str, text: &str) -> Schema {
        parse(text.as_bytes()).map_err(|error| format!("{name}: {error:?}")).expect("valid shipped schema")
    }

    fn reprinted_schemas(name: &str, text: &str) -> (Schema, Schema) {
        let schema = shipped_schema(name, text);
        let other = parse(reprint(&schema).as_bytes())
            .map_err(|error| format!("{name}: {error:?}"))
            .expect("valid reprinted schema");
        (schema, other)
    }

    fn reprint_renames(schema: &Schema) -> (Vec<Rename>, Vec<Change>) {
        let mut renames = Vec::new();
        let mut changes = Vec::new();
        for rename in renamed(schema) {
            renames.push(Rename::Declaration { old: rename.old.clone(), new: rename.new.clone() });
            changes.push(Change {
                kind: Kind::NoneNeeded,
                op: Op::RenameDeclaration { old: rename.old, new: rename.new.clone() },
            });
            for (old, new) in rename.parts {
                renames.push(Rename::Part { decl: rename.new.clone(), old: old.clone(), new: new.clone() });
                changes
                    .push(Change { kind: Kind::NoneNeeded, op: Op::RenamePart { decl: rename.new.clone(), old, new } });
            }
        }
        (renames, changes)
    }

    #[test]
    fn every_schema_preserves_fingerprints_after_reprinting() {
        for (name, text) in shipped_schemas() {
            let (schema, other) = reprinted_schemas(name, text);
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
    fn every_schema_diffs_empty_against_itself() {
        for (name, text) in shipped_schemas() {
            let schema = shipped_schema(name, text);
            let result = diff(&schema, &schema, &[])
                .map_err(|error| format!("{name}: {error:?}"))
                .expect("valid shipped schema and renames");
            assert!(result.changes.is_empty(), "{name}: {:?}", result.changes);
            assert!(result.proposed.is_empty(), "{name}: {:?}", result.proposed);
            assert_eq!(result.version(), Version::Kept, "{name}");
            assert_eq!(result.family.as_bytes(), schema.spelled(schema.family()), "{name}");
        }
    }

    #[test]
    fn every_schema_diffs_against_its_reprint_as_the_given_renames_alone() {
        for (name, text) in shipped_schemas() {
            let (schema, other) = reprinted_schemas(name, text);
            let (renames, expected) = reprint_renames(&schema);
            let result = diff(&schema, &other, &renames)
                .map_err(|error| format!("{name}: {error:?}"))
                .expect("valid shipped schema and renames");
            assert_eq!(result.changes, expected, "{name}");
            assert!(result.proposed.is_empty(), "{name}: {:?}", result.proposed);
            assert_eq!(result.version(), Version::Kept, "{name}");
        }
    }

    #[test]
    fn every_schema_reprint_proposals_converge_on_its_renames() {
        // At most one wave per declaration, then its parts and an empty round (codec.md, 10.1).
        const PARTS_AND_EMPTY_ROUNDS: u32 = 2;
        for (name, text) in shipped_schemas() {
            let (schema, other) = reprinted_schemas(name, text);
            let (renames, expected) = reprint_renames(&schema);
            let mut confirmed = Vec::new();
            let mut last = None;
            for round in 0..=schema.declarations() + PARTS_AND_EMPTY_ROUNDS {
                let result = diff(&schema, &other, &confirmed)
                    .map_err(|error| format!("{name}: round {round}: {error:?}"))
                    .expect("round accepts every proposal");
                if round == 0 {
                    let first = renames.first().ok_or(name).expect("shipped schema has a declaration");
                    assert!(result.proposed.contains(first), "{name}: first declaration: {:?}", result.proposed);
                    assert!(
                        result.proposed.iter().all(|rename| matches!(rename, Rename::Declaration { .. })),
                        "{name}: first round: {:?}",
                        result.proposed
                    );
                }
                if result.proposed.is_empty() {
                    last = Some(result);
                    break;
                }
                for proposal in result.proposed {
                    assert!(!confirmed.contains(&proposal), "{name}: round {round}: repeated {proposal:?}");
                    confirmed.push(proposal);
                }
            }
            let last = last.ok_or(name).expect("proposals converge within the round bound");
            assert!(last.proposed.is_empty(), "{name}: {:?}", last.proposed);
            assert_eq!(last.changes, expected, "{name}");
            assert!(
                confirmed.iter().all(|rename| renames.contains(rename))
                    && renames.iter().all(|rename| confirmed.contains(rename)),
                "{name}: confirmed {confirmed:?}, expected {renames:?}"
            );
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

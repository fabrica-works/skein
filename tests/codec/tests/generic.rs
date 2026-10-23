//! Every schema's goldens agree with the generic decoder (codec.md, 8.3).

extern crate alloc;

#[path = "support/generated.rs"]
#[expect(dead_code, reason = "goldens use the shared decoders; seeded draws run in fuzzy_generic")]
mod generated;
#[path = "support/generic.rs"]
pub mod generic;

#[cfg(test)]
mod tests {
    use super::{generated, generic};
    use skein_codec::value::Value;

    #[test]
    #[expect(clippy::panic, reason = "a refused golden reports the generated path and reason")]
    fn every_golden_decodes_generically_to_its_own_bytes_and_tree() {
        let mut paths = std::fs::read_dir("schema")
            .expect("schema directory")
            .map(|entry| entry.expect("schema entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "schema"))
            .collect::<Vec<_>>();
        paths.push("../../examples/ui/view/schema/notes-v1.schema".into());
        paths.sort();
        let mut value = Value::with_room(generic::ROOM);
        let mut count = 0_u32;
        let mut compared = 0_u32;
        for path in paths {
            let text = std::fs::read_to_string(&path).expect("schema readable");
            let schema = skein_codec::schema::parse(text.as_bytes()).expect("schema valid");
            let generated_schema = skein_codegen::parse(&text).expect("generator schema");
            let family = generated::families().into_iter().find(|family| family.text == text);
            for golden in skein_codegen::generate(&generated_schema).goldens {
                let root = generic::golden_root(&schema, &golden.name);
                let node =
                    value.decode(&schema, root, golden.bytes.clone().into_boxed_slice()).expect("golden decodes");
                assert_eq!(value.bytes(), golden.bytes, "{path:?}: {}", golden.name);
                if let Some(family) = &family {
                    let name = schema.spelled(schema.declaration(root).name);
                    let generated_root =
                        family.roots.iter().find(|root| root.name.as_bytes() == name).expect("generated decoder");
                    match (generated_root.decode)(&golden.bytes) {
                        generated::Decoded::Value { debug, bytes } => {
                            assert_eq!(
                                generic::render(&schema, &value, node, root),
                                debug,
                                "{path:?}: {}",
                                golden.name
                            );
                            assert_eq!(value.bytes(), bytes);
                        }
                        generated::Decoded::Problem { path, reason } => {
                            panic!("golden {}: {path}: {reason:?}", golden.name)
                        }
                    }
                    compared = compared.checked_add(1).expect("golden count");
                }
                count = count.checked_add(1).expect("golden count");
            }
        }
        // Five generated families contribute 84, and the UI schema contributes 12.
        assert_eq!(count, 96);
        assert_eq!(compared, 84);
    }
}

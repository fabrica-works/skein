//! Seeded agreement of generic and generated decoders (codec.md, 8.3).

extern crate alloc;

#[path = "support/generated.rs"]
mod generated;
#[path = "support/generic.rs"]
pub mod generic;

#[cfg(test)]
mod tests {
    use super::{generated, generic};
    use skein_codec::schema::{DeclIx, Schema};
    use skein_codec::value::{Refusal, Value};
    use skein_lib::Rng;

    // Per root: 1536 draws and their three mutations; every arbitrary length
    // 0..=48 sixteen times; every golden and its three mutations.
    const DRAWS: u64 = 1536;
    const ARBITRARY_ROUNDS: u32 = 16;
    const MAX_ARBITRARY: u32 = 48;

    fn seed(family: &str, root: &str, round: u64) -> u64 {
        family
            .bytes()
            .chain([0])
            .chain(root.bytes())
            .fold(round, |hash, byte| hash.wrapping_mul(0x100_0000_01b3) ^ u64::from(byte))
    }

    #[derive(Debug)]
    struct Oracle<'a> {
        schema: &'a Schema,
        decl: DeclIx,
        family: &'a generated::Family,
        root: &'a generated::Root,
        value: &'a mut Value,
    }

    impl Oracle<'_> {
        #[expect(clippy::panic, reason = "oracle disagreements report both results and the replay seed")]
        fn compare(&mut self, seed: u64, bytes: &[u8], valid: bool) {
            let Self { schema, decl, family, root, value } = self;
            let decl = *decl;
            let context = || format!("{}.{}, seed {seed:#018x}, bytes {bytes:?}", family.name, root.name);
            let checked = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
                let decoded = value.decode(schema, decl, bytes.into());
                match (decoded, (root.decode)(bytes)) {
                    (Ok(node), generated::Decoded::Value { debug, bytes: encoded }) => {
                        assert_eq!(value.bytes(), bytes, "{}", context());
                        assert_eq!(value.bytes(), encoded, "{}", context());
                        assert_eq!(generic::render(schema, value, node, decl), debug, "{}", context());
                    }
                    (Err(Refusal::Problem(problem)), generated::Decoded::Problem { path, reason }) => {
                        assert!(!valid, "draw or golden refused: {}: {problem:?}", context());
                        assert_eq!(generic::path_name(schema, problem.path), path, "{}", context());
                        assert_eq!(problem.reason, reason, "{}", context());
                        assert_eq!(value.bytes(), b"", "refusal empties the value: {}", context());
                    }
                    (generic, generated) => panic!("{}: generic {generic:?}, generated {generated:?}", context()),
                }
            }));
            if let Err(problem) = checked {
                panic!("{}: decoder comparison panicked: {problem:?}", context());
            }
        }
    }

    fn mutations(bytes: &[u8], rng: &mut Rng) -> [Vec<u8>; 3] {
        let length = u64::try_from(bytes.len()).expect("wire length");
        let mut changed = bytes.to_vec();
        if length > 0 {
            let at = usize::try_from(rng.below(length)).expect("wire index");
            // A nonzero XOR guarantees a changed byte.
            *changed.get_mut(at).expect("selected byte") ^= u8::try_from(rng.between(1, 255)).expect("byte");
        }
        let cut = usize::try_from(rng.between(0, length)).expect("cut index");
        let cut = bytes.get(..cut).expect("bounded cut").to_vec();
        let mut grown = bytes.to_vec();
        for _ in 0..rng.between(1, 4) {
            grown.push(u8::try_from(rng.below(256)).expect("byte"));
        }
        [changed, cut, grown]
    }

    #[test]
    fn generic_and_generated_decode_alike() {
        let mut value = Value::with_room(generic::ROOM);
        let mut cases = 0_u32;
        for family in generated::families() {
            let schema = skein_codec::schema::parse(family.text.as_bytes()).expect("schema");
            let drawn_schema = skein_codegen::parse(family.text).expect("generator schema");
            let goldens = skein_codegen::generate(&drawn_schema).goldens;
            for root in family.roots {
                let decl = schema.find(root.name.as_bytes()).expect("root");
                let mut oracle = Oracle { schema: &schema, decl, family: &family, root, value: &mut value };
                for round in 0..DRAWS {
                    let seed = seed(family.name, root.name, round);
                    let mut rng = Rng::new(seed);
                    let bytes = generated::draw(&drawn_schema, root.name, &mut rng);
                    oracle.compare(seed, &bytes, true);
                    cases = cases.checked_add(1).expect("case count");
                    for mutation in mutations(&bytes, &mut rng) {
                        oracle.compare(seed, &mutation, false);
                        cases = cases.checked_add(1).expect("case count");
                    }
                }
                for round in 0..ARBITRARY_ROUNDS {
                    let seed = seed(family.name, root.name, DRAWS.checked_add(u64::from(round)).expect("round"));
                    let mut rng = Rng::new(seed);
                    for length in 0..=MAX_ARBITRARY {
                        let bytes =
                            (0..length).map(|_| u8::try_from(rng.below(256)).expect("byte")).collect::<Vec<_>>();
                        oracle.compare(seed, &bytes, false);
                        cases = cases.checked_add(1).expect("case count");
                    }
                }
                for golden in &goldens {
                    if generic::golden_root(&schema, &golden.name) == decl {
                        let seed = seed(family.name, root.name, u64::MAX);
                        let mut rng = Rng::new(seed);
                        oracle.compare(seed, &golden.bytes, true);
                        cases = cases.checked_add(1).expect("case count");
                        for mutation in mutations(&golden.bytes, &mut rng) {
                            oracle.compare(seed, &mutation, false);
                            cases = cases.checked_add(1).expect("case count");
                        }
                    }
                }
            }
        }
        // 28 roots * (6144 drawn/mutated + 784 arbitrary), plus 4 per golden.
        assert_eq!(cases, 194_320);
    }
}

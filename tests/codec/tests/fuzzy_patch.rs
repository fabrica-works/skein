//! Seeded patches over every generic value form (codec.md, 11.8).

#[path = "support/generated.rs"]
mod generated;

#[cfg(test)]
mod tests {
    use super::generated;
    use skein_codec::patch::{Capacity, Patcher, PlanIx, Step};
    use skein_codec::schema::{DeclIx, Schema, Shape, Type};
    use skein_codec::value::{Node, Value};
    use skein_lib::Rng;

    const OLD: &str = include_str!("../schema/every-v1.schema");
    const NEW: &str = "family every 2
versioned record Blank {}
record Numbers { short: u16 small: u16 word: u32 long: u64 flag: bool wait: duration tag: fixed 3 }
record Strings { raw: bytes 8 key: secret 5 words: option text 7 ready: bool }
enum Light { off red amber green white }
versioned record Stamp { at: u64 note: option text 4 }
enum Shape { dot numbers: Numbers strings: boxed Strings stamp: Stamp dark }
record Nest {
    blanks: list 3 Blank lights: list 3 Light maybe_lights: list 2 option Light
    grid: list 2 list 2 u16 maybe_list: option list 3 u8 maybe_numbers: option Numbers
    shapes: list 2 Shape secrets: list 2 secret 3 stamps: list 2 Stamp pairs: list 2 fixed 2
}
versioned record Every {
    numbers: Numbers strings: Strings light: Light shape: Shape stamp: Stamp nest: Nest empty: Blank
    held: bytes 8 holds every-other Thing
}";
    // Numbers' pieces cannot be opened (11.3); dropping that field lets its
    // small integer widen. Appended variants need no line.
    const PATCH: &[u8] = b"patch 1\nowner o 1\nfamily every 1 to 2\nversioned Blank\nwiden Numbers.small u8 u16\norder Numbers short small word long flag wait tag\nwrap Strings.words text 7 option text 7\nadd Strings.ready bool true\nadd Light.off\ndrop Every.items\n";
    // Three schemas and plans: derived, identity, and version-only. The every
    // fixture's maximal tree is under 256 nodes and 4096 wire bytes.
    const CAPACITY: Capacity = Capacity { versions: 3, patches: 3, text: 4096, bytes: 4096, nodes: 256 };
    // 8 roots * 512 draws * 3 runs: a replayable property, well under a second.
    const DRAWS: u64 = 512;
    const VERSION_ONLY: &str = "family every 3
versioned record Blank {}
record Numbers { short: u16 small: u16 word: u32 long: u64 flag: bool wait: duration tag: fixed 3 }
record Strings { raw: bytes 8 key: secret 5 words: option text 7 ready: bool }
enum Light { off red amber green white }
versioned record Stamp { at: u64 note: option text 4 }
enum Shape { dot numbers: Numbers strings: boxed Strings stamp: Stamp dark }
record Nest {
    blanks: list 3 Blank lights: list 3 Light maybe_lights: list 2 option Light
    grid: list 2 list 2 u16 maybe_list: option list 3 u8 maybe_numbers: option Numbers
    shapes: list 2 Shape secrets: list 2 secret 3 stamps: list 2 Stamp pairs: list 2 fixed 2
}
versioned record Every {
    numbers: Numbers strings: Strings light: Light shape: Shape stamp: Stamp nest: Nest empty: Blank
    held: bytes 8 holds every-other Thing
}";

    fn run(patcher: &mut Patcher, plan: PlanIx, root: DeclIx, bytes: &[u8]) -> Vec<u8> {
        patcher.start(plan, root, bytes.into()).expect("draw decodes");
        // Each stop consumes one of the bounded old nodes.
        for _ in 0..=CAPACITY.nodes {
            match patcher.next().expect("derived patch") {
                Step::Held { at, .. } => {
                    let (old, new) = patcher.parts();
                    new.carry(old, at).expect("held bytes carried");
                }
                Step::Done => return patcher.finish().expect("finished").to_vec(),
                Step::Written { .. } => unreachable!("property uses derived plans"),
            }
        }
        unreachable!("stops fit the node capacity")
    }

    fn named_fields(schema: &Schema, decl: DeclIx) -> Vec<skein_codec::schema::FieldIx> {
        match schema.declaration(decl).shape {
            Shape::Record { first, count, .. } => (0..count)
                .map(|offset| skein_codec::schema::FieldIx::new(first.index().checked_add(offset).expect("field")))
                .collect(),
            Shape::Enum { .. } => unreachable!("record fields"),
        }
    }

    #[derive(Debug)]
    struct Pair {
        a: Node,
        b: Node,
        at: Type,
        bt: Type,
    }

    fn compare(old: &Schema, new: &Schema, a: &Value, an: Node, b: &Value, bn: Node, root: &str) {
        let mut pending = vec![Pair {
            a: an,
            b: bn,
            at: Type::Named(old.find(root.as_bytes()).expect("old root")),
            bt: Type::Named(new.find(root.as_bytes()).expect("new root")),
        }];
        while let Some(Pair { a: an, b: bn, at, bt }) = pending.pop() {
            match bt {
                Type::Option(bi) => match at {
                    Type::Option(ai) => {
                        let av = a.some(an).expect("old option");
                        let bv = b.some(bn).expect("new option");
                        assert_eq!(av.is_some(), bv.is_some(), "{root}");
                        if let (Some(an), Some(bn)) = (av, bv) {
                            pending.push(Pair { a: an, b: bn, at: old.ty(ai), bt: new.ty(bi) });
                        }
                    }
                    Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::U64
                    | Type::Bool
                    | Type::Duration
                    | Type::Fixed(_)
                    | Type::Secret(_)
                    | Type::Text(_)
                    | Type::Bytes { .. }
                    | Type::List { .. }
                    | Type::Named(_) => pending.push(Pair {
                        a: an,
                        b: b.some(bn).expect("wrapped").expect("present"),
                        at,
                        bt: new.ty(bi),
                    }),
                },
                Type::List { item: bi, .. } => {
                    let Type::List { item: ai, .. } = at else {
                        unreachable!("same list level");
                    };
                    assert_eq!(a.count(an), b.count(bn), "{root}");
                    let mut av = a.items(an).expect("old list");
                    let mut bv = b.items(bn).expect("new list");
                    for _ in 0..a.count(an).expect("count") {
                        pending.push(Pair {
                            a: a.next_item(&mut av).expect("old item"),
                            b: b.next_item(&mut bv).expect("new item"),
                            at: old.ty(ai),
                            bt: new.ty(bi),
                        });
                    }
                }
                Type::Named(bd) => {
                    let Type::Named(ad) = at else {
                        unreachable!("named match");
                    };
                    match new.declaration(bd).shape {
                        Shape::Record { .. } => {
                            let af = named_fields(old, ad);
                            for bf in named_fields(new, bd) {
                                let field = new.field(bf);
                                let name = new.spelled(field.name);
                                if let Some(af) = af.iter().find(|af| old.spelled(old.field(**af).name) == name) {
                                    pending.push(Pair {
                                        a: a.field(an, *af).expect("old field"),
                                        b: b.field(bn, bf).expect("new field"),
                                        at: old.ty(old.field(*af).ty),
                                        bt: new.ty(field.ty),
                                    });
                                } else {
                                    assert_eq!(name, b"ready", "only the added field has no old value");
                                    assert_eq!(b.boolean(b.field(bn, bf).expect("added")), Ok(true));
                                }
                            }
                        }
                        Shape::Enum { .. } => {
                            let av = old.variant(a.tag(an).expect("old tag"));
                            let bv = new.variant(b.tag(bn).expect("new tag"));
                            assert_eq!(old.spelled(av.name), new.spelled(bv.name), "{root}");
                            if let (Some(ad), Some(bd)) = (av.record, bv.record) {
                                pending.push(Pair {
                                    a: a.payload(an).expect("payload").expect("record"),
                                    b: b.payload(bn).expect("payload").expect("record"),
                                    at: Type::Named(ad),
                                    bt: Type::Named(bd),
                                });
                            }
                        }
                    }
                }
                Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Duration => {
                    assert_eq!(a.uint(an), b.uint(bn), "{root}");
                }
                Type::Bool => assert_eq!(a.boolean(an), b.boolean(bn), "{root}"),
                Type::Secret(_) => assert_eq!(a.expose(an), b.expose(bn), "{root}"),
                Type::Fixed(_) | Type::Bytes { .. } | Type::Text(_) => {
                    assert_eq!(a.content(an), b.content(bn), "{root}");
                }
            }
        }
    }

    #[test]
    fn seeded_every_values_survive_derived_identity_and_version_passes() {
        let mut patcher = Patcher::new(CAPACITY);
        patcher.keep(OLD.as_bytes()).expect("old");
        patcher.keep(NEW.as_bytes()).expect("new");
        patcher.keep(VERSION_ONLY.as_bytes()).expect("third");
        let derived = patcher.plan(PATCH).expect("plan").expect("family");
        let identity = patcher.identity(1).expect("identity");
        let version =
            patcher.plan(b"patch 1\nowner o 2\nfamily every 2 to 3\n").expect("version plan").expect("family");
        let old = skein_codec::schema::parse(OLD.as_bytes()).expect("old");
        let new = skein_codec::schema::parse(NEW.as_bytes()).expect("new");
        let third = skein_codec::schema::parse(VERSION_ONLY.as_bytes()).expect("third");
        let drawer = skein_codegen::parse(OLD).expect("draw schema");
        let mut av = Value::with_room(CAPACITY.nodes);
        let mut bv = Value::with_room(CAPACITY.nodes);
        let family = generated::families().into_iter().find(|family| family.name == "every").expect("every family");
        assert_eq!(family.text, OLD);
        for (index, root) in family.roots.iter().enumerate() {
            let name = root.name;
            for draw in 0..DRAWS {
                let seed = u64::try_from(index)
                    .expect("root index")
                    .checked_mul(DRAWS)
                    .expect("seeds")
                    .checked_add(draw)
                    .expect("seed");
                let mut rng = Rng::new(seed);
                let bytes = generated::draw(&drawer, name, &mut rng);
                match (root.decode)(&bytes) {
                    generated::Decoded::Value { debug, bytes: encoded } => {
                        assert!(!debug.is_empty(), "generated value is printable");
                        assert_eq!(encoded, bytes, "draw {name} seed {seed}");
                    }
                    generated::Decoded::Problem { path, reason } => {
                        unreachable!("draw {name} seed {seed}: {path} {reason:?}")
                    }
                }
                let ad = old.find(name.as_bytes()).expect("root");
                let bd = new.find(name.as_bytes()).expect("root");
                assert_eq!(run(&mut patcher, identity, ad, &bytes), bytes, "identity {name} seed {seed}");
                let converted = run(&mut patcher, derived, ad, &bytes);
                let an = av.decode(&old, ad, bytes.into()).expect("old value");
                let bn = bv.decode(&new, bd, converted.clone().into()).expect("converted value");
                compare(&old, &new, &av, an, &bv, bn, name);
                let revised = run(&mut patcher, version, bd, &converted);
                let mut expected = converted;
                rewrite_versions(&new, bd, &bv, bn, &mut expected);
                assert_eq!(revised, expected, "version-only {name} seed {seed}");
                let cd = third.find(name.as_bytes()).expect("third root");
                av.decode(&third, cd, revised.into()).expect("third value");
            }
        }
    }

    fn rewrite_versions(schema: &Schema, decl: DeclIx, value: &Value, root: Node, bytes: &mut [u8]) {
        let mut pending = vec![(root, Type::Named(decl))];
        while let Some((node, ty)) = pending.pop() {
            match ty {
                Type::Named(decl) => match schema.declaration(decl).shape {
                    Shape::Record { versioned, .. } => {
                        if versioned {
                            let start = usize::try_from(value.span(node).expect("span").start).expect("offset");
                            // The oracle edits only the decoded version prefixes.
                            *bytes.get_mut(start).expect("high version byte") = 0;
                            *bytes.get_mut(start.checked_add(1).expect("offset")).expect("low version byte") = 3;
                        }
                        for field in named_fields(schema, decl) {
                            pending.push((value.field(node, field).expect("field"), schema.ty(schema.field(field).ty)));
                        }
                    }
                    Shape::Enum { .. } => {
                        if let Some(record) = schema.variant(value.tag(node).expect("tag")).record {
                            pending.push((value.payload(node).expect("payload").expect("record"), Type::Named(record)));
                        }
                    }
                },
                Type::List { item, .. } => {
                    let mut items = value.items(node).expect("list");
                    for _ in 0..value.count(node).expect("count") {
                        pending.push((value.next_item(&mut items).expect("item"), schema.ty(item)));
                    }
                }
                Type::Option(item) => {
                    if let Some(node) = value.some(node).expect("option") {
                        pending.push((node, schema.ty(item)));
                    }
                }
                Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::Bool
                | Type::Duration
                | Type::Fixed(_)
                | Type::Secret(_)
                | Type::Text(_)
                | Type::Bytes { .. } => {}
            }
        }
    }
}

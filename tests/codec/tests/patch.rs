//! The every fixture's patch story (codec.md, 11.8).

extern crate alloc;

#[path = "support/generic.rs"]
pub mod generic;

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeSet;
    use std::fs;

    use super::generic;
    use skein_codec::patch::{self, Capacity, Halt, Missing, PatchWrong, Patcher, PlanIx, Step, Target};
    use skein_codec::schema::{self, Shape};
    use skein_codec_tests::{every as v1, every_v2 as v2, patches};
    use skein_codegen::diff::{Op, Rename, diff};
    use skein_lib::{Reader, Writer};
    use std::path::Path;

    #[test]
    fn patch_operations_spell_the_diff_in_order() {
        let kept = fs::read("schema/every-v1.schema").expect("version 1 readable");
        let current = fs::read("schema/every-v2.schema").expect("version 2 readable");
        let kept = schema::parse(&kept).expect("version 1 valid");
        let current = schema::parse(&current).expect("version 2 valid");
        let patch = fs::read_to_string("patches/0001.patch").expect("patch readable");
        let lines = patch
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter(|line| !line.starts_with("patch ") && !line.starts_with("owner ") && !line.starts_with("family "))
            .collect::<Vec<_>>();
        let renames = lines
            .iter()
            .filter_map(|line| line.strip_prefix("rename "))
            .map(|line| {
                let (old, new) = line.split_once(' ').expect("rename's old and new names");
                if let Some((decl, old)) = old.split_once('.') {
                    Rename::Part { decl: decl.into(), old: old.into(), new: new.into() }
                } else {
                    Rename::Declaration { old: old.into(), new: new.into() }
                }
            })
            .collect::<Vec<_>>();
        let result = diff(&kept, &current, &renames).expect("same family and valid renames");
        let (sites, operations): (Vec<_>, Vec<_>) = lines.into_iter().partition(|line| line.starts_with("written "));
        assert_eq!(sites, ["written Shape", "written Every.score"]);
        assert_eq!(operations.len(), result.changes.len());
        for (line, change) in operations.into_iter().zip(result.changes) {
            if let Op::AddField { decl, field, ty, .. } = &change.op {
                let prefix = format!("add {decl}.{field} {ty} ");
                let value = line.strip_prefix(&prefix).expect("added field spells the diff up to its value");
                assert!(!value.is_empty(), "{line}: added field has a value");
            } else {
                assert_eq!(line, change.to_string());
            }
        }
    }

    const V1: &str = include_str!("../schema/every-v1.schema");
    const V2: &str = include_str!("../schema/every-v2.schema");
    const PATCH: &str = patches::P0001;
    // Six records and seven enum variants, each with smallest and full goldens.
    const GOLDENS: usize = 26;
    // One held field and one written score in each of Every's two goldens.
    const EVERY_GOLDENS: u32 = 2;
    const GOLDEN_DIRECTORY: &str = "patches/golden/0001";

    fn kept() -> (Patcher, [u16; 2]) {
        // Two kept versions, one plan; text fits the longest input, records fit
        // either generated family's ceiling, nodes use the generic fixture's room.
        let capacity = Capacity {
            versions: 2,
            patches: 1,
            text: u32::try_from(V1.len().max(V2.len()).max(PATCH.len())).expect("fixture text fits"),
            bytes: u32::try_from(
                v1::worst_case_bytes(&v1::CEILINGS)
                    .expect("v1 bound")
                    .max(v2::worst_case_bytes(&v2::CEILINGS).expect("v2 bound")),
            )
            .expect("fixture bytes fit"),
            nodes: generic::ROOM,
        };
        let mut patcher = Patcher::new(capacity);
        let versions = [patcher.keep(V1.as_bytes()).expect("keep v1"), patcher.keep(V2.as_bytes()).expect("keep v2")];
        (patcher, versions)
    }

    fn planned() -> (Patcher, PlanIx) {
        let (mut patcher, _) = kept();
        let plan = patcher.plan(PATCH.as_bytes()).expect("0001.patch plans").expect("every block");
        (patcher, plan)
    }

    #[test]
    fn patch_plans_every_one_to_two_for_skein() {
        let (mut patcher, versions) = kept();
        assert_eq!(versions, [1, 2]);
        let plan = patcher.plan(PATCH.as_bytes()).expect("0001.patch plans").expect("every block");
        assert_eq!(patcher.versions(plan), patch::Versions { from: 1, to: 2 });
        let header = patch::header(PATCH.as_bytes()).expect("header");
        let owner = header.owner();
        assert_eq!(
            PATCH
                .as_bytes()
                .get(usize::try_from(owner.start).expect("start")..usize::try_from(owner.end).expect("end")),
            Some(b"skein".as_slice())
        );
        assert_eq!(header.level(), 1);
    }

    fn edited(statement: &str, replacement: Option<&str>) -> Vec<u8> {
        let mut changed = 0_u32;
        let mut lines = Vec::new();
        for line in PATCH.lines() {
            if line == statement {
                changed = changed.checked_add(1).expect("patch statement count fits");
                if let Some(replacement) = replacement {
                    lines.push(replacement);
                }
            } else {
                lines.push(line);
            }
        }
        assert_eq!(changed, 1, "edit exactly one patch statement");
        lines.join("\n").into_bytes()
    }

    #[test]
    fn missing_shape_site_is_refused_at_family_line() {
        let (mut patcher, _) = kept();
        let error = patcher.plan(&edited("written Shape", None)).expect_err("Shape needs its site");
        assert_eq!(error.line, 6);
        let shape = patcher.schema(2).expect("v2").find(b"Shape").expect("Shape");
        assert_eq!(
            error.wrong,
            PatchWrong::Missing(Missing { op: patch::Op::Written, target: Target::Declaration(shape) })
        );
    }

    #[test]
    fn asked_zone_value_is_refused_at_add_line() {
        let (mut patcher, _) = kept();
        let error = patcher
            .plan(&edited("add Stamp.zone text 8 \"utc\"", Some("add Stamp.zone text 8 ?")))
            .expect_err("asked value refused");
        assert_eq!(error.line, 12);
        assert_eq!(error.wrong, PatchWrong::Asked);
    }

    #[test]
    fn missing_order_is_refused_at_family_line() {
        let (mut patcher, _) = kept();
        let error = patcher
            .plan(&edited("order Every numbers strings light shape stamp empty nest held items", None))
            .expect_err("Every needs order");
        assert_eq!(error.line, 6);
        let every = patcher.schema(2).expect("v2").find(b"Every").expect("Every");
        assert_eq!(
            error.wrong,
            PatchWrong::Missing(Missing { op: patch::Op::Order, target: Target::Declaration(every) })
        );
    }

    #[test]
    fn kept_constants_name_their_own_declarations_parts() {
        use patches::kept::every::{v1, v2};
        let old = schema::parse(V1.as_bytes()).expect("v1");
        let new = schema::parse(V2.as_bytes()).expect("v2");
        for (decl, field, name) in [
            (b"Numbers".as_slice(), v1::numbers::SMALL, b"small".as_slice()),
            (b"Numbers".as_slice(), v1::numbers::SHORT, b"short".as_slice()),
            (b"Every".as_slice(), v1::every::NUMBERS, b"numbers".as_slice()),
        ] {
            let declaration = old.find(decl).expect("declaration");
            let Shape::Record { first, count, .. } = old.declaration(declaration).shape else {
                unreachable!("record constant")
            };
            assert!(
                (first.index()..first.index().checked_add(count).expect("field range")).contains(&field.index()),
                "field belongs to its declaration"
            );
            assert_eq!(old.spelled(old.field(field).name), name);
        }
        for (schema, variant, name) in
            [(&old, v1::shape::DOT, b"dot".as_slice()), (&new, v2::shape::BLANK, b"blank".as_slice())]
        {
            let declaration = schema.find(b"Shape").expect("Shape");
            let Shape::Enum { first, count } = schema.declaration(declaration).shape else {
                unreachable!("variant constant")
            };
            assert!(
                (first.index()..first.index().checked_add(count).expect("variant range")).contains(&variant.index()),
                "variant belongs to Shape"
            );
            assert_eq!(schema.spelled(schema.variant(variant).name), name);
        }
    }

    fn numbers(old: &v1::Numbers, new: &v2::Numbers) {
        assert_eq!(new.small(), old.small());
        assert_eq!(new.short(), old.short());
        assert_eq!(new.word(), old.word());
        assert_eq!(new.long(), old.long());
        assert_eq!(new.flag(), old.flag());
        assert_eq!(new.pause(), old.wait());
        assert_eq!(new.tag(), old.tag());
    }

    fn texts(old: &v1::Strings, new: &v2::Texts) {
        assert_eq!(new.raw(), old.raw());
        assert_eq!(new.key().expose(), old.key().expose());
        assert_eq!(new.words(), old.words());
    }

    fn light(old: &v1::Light, new: &v2::Light) {
        let expected = match old {
            v1::Light::Red => v2::Light::Red,
            v1::Light::Amber => v2::Light::Amber,
            v1::Light::Green => v2::Light::Green,
        };
        assert_eq!(*new, expected);
    }

    fn stamp(old: &v1::Stamp, new: &v2::Stamp) {
        assert_eq!(new.at(), old.at());
        assert_eq!(new.note(), old.note());
        assert_eq!(new.zone(), b"utc");
    }

    fn shape(old: &v1::Shape, new: &v2::Shape) {
        match (old, new) {
            (v1::Shape::Dot, v2::Shape::Blank(_)) => {}
            (v1::Shape::Numbers(old), v2::Shape::Numbers(new)) => numbers(old, new),
            (v1::Shape::Strings(old), v2::Shape::Strings(new)) => texts(old, new),
            (v1::Shape::Stamp(old), v2::Shape::Stamp(new)) => stamp(old, new),
            (
                v1::Shape::Dot | v1::Shape::Numbers(_) | v1::Shape::Strings(_) | v1::Shape::Stamp(_),
                v2::Shape::Blank(_) | v2::Shape::Numbers(_) | v2::Shape::Strings(_) | v2::Shape::Stamp(_),
            ) => unreachable!("shape keeps its variant except dot to blank"),
        }
    }

    fn nest(old: &v1::Nest, new: &v2::Nest) {
        assert_eq!(new.blanks().len(), old.blanks().len());
        assert_eq!(new.lights().len(), old.lights().len());
        for (old, new) in old.lights().iter().zip(new.lights().iter()) {
            light(old, new);
        }
        assert_eq!(new.maybe_lights().len(), old.maybe_lights().len());
        for (old, new) in old.maybe_lights().iter().zip(new.maybe_lights().iter()) {
            assert_eq!(new.is_some(), old.is_some());
            if let (Some(old), Some(new)) = (old, new) {
                light(old, new);
            }
        }
        assert_eq!(new.grid().len(), old.grid().len());
        for (old, new) in old.grid().iter().zip(new.grid().iter()) {
            assert_eq!(new.as_slice(), old.iter().copied().map(u32::from).collect::<Vec<_>>());
        }
        assert_eq!(new.maybe_list(), old.maybe_list());
        assert_eq!(new.maybe_numbers().is_some(), old.maybe_numbers().is_some());
        if let (Some(old), Some(new)) = (old.maybe_numbers(), new.maybe_numbers()) {
            numbers(old, new);
        }
        assert_eq!(new.shapes().len(), old.shapes().len());
        for (old, new) in old.shapes().iter().zip(new.shapes().iter()) {
            shape(old, new);
        }
        assert_eq!(new.secrets().len(), old.secrets().len());
        for (old, new) in old.secrets().iter().zip(new.secrets().iter()) {
            assert_eq!(new.expose(), old.expose());
        }
        assert_eq!(new.stamps().len(), old.stamps().len());
        for (old, new) in old.stamps().iter().zip(new.stamps().iter()) {
            stamp(old, new);
        }
        assert_eq!(new.label().as_deref(), Some(b"nest".as_slice()));
    }

    fn every(old: &v1::Every, new: &v2::Every) {
        numbers(old.numbers(), new.numbers());
        texts(old.strings(), new.strings());
        light(old.light(), new.light().as_ref().expect("light wrapped in Some"));
        shape(old.shape(), new.shape());
        stamp(old.stamp(), new.stamp());
        nest(old.nest(), new.nest());
        assert_eq!(new.empty(), &v2::Blank::new(&v2::CEILINGS, v2::BlankParts {}).expect("Blank"));
        assert_eq!(
            new.score(),
            u32::from(old.numbers().small())
                .checked_add(u32::from(old.numbers().short()))
                .expect("u8 plus u16 fits u32")
        );
        assert_eq!(new.held(), old.held());
        assert_eq!(new.items(), old.items());
    }

    fn generated_round_trip(name: &[u8], old: &[u8], bytes: &[u8]) {
        macro_rules! check {
            ($old:ident, $new:ident, $compare:expr) => {{
                let mut reader = Reader::new(bytes);
                let new = v2::$new::decode(&v2::CEILINGS, &mut reader).expect("v2 generated decode");
                assert_eq!(reader.remaining(), 0);
                let mut old_reader = Reader::new(old);
                let old = v1::$old::decode(&v1::CEILINGS, &mut old_reader).expect("v1 generated decode");
                assert_eq!(old_reader.remaining(), 0);
                $compare(&old, &new);
                let mut writer = Writer::new(usize::try_from(new.measure()).expect("measured bytes"));
                new.encode(&mut writer).expect("measured room");
                assert_eq!(writer.finish().as_ref(), bytes);
            }};
        }
        match name {
            b"Blank" => check!(Blank, Blank, |_: &v1::Blank, _: &v2::Blank| {}),
            b"Numbers" => check!(Numbers, Numbers, numbers),
            b"Texts" => check!(Strings, Texts, texts),
            b"Light" => check!(Light, Light, light),
            b"Stamp" => check!(Stamp, Stamp, stamp),
            b"Shape" => check!(Shape, Shape, shape),
            b"Nest" => check!(Nest, Nest, nest),
            b"Every" => check!(Every, Every, every),
            _ => unreachable!("every fixture's eight declarations"),
        }
    }

    #[test]
    #[expect(clippy::panic, reason = "a patch halt reports the golden and the record key")]
    fn patch_runs_every_v1_golden_into_v2_generated_code() {
        let (mut patcher, plan) = planned();
        let schema = schema::parse(V1.as_bytes()).expect("v1 schema");
        let goldens = skein_codegen::generate(&skein_codegen::parse(V1).expect("codegen v1")).goldens;
        let write = std::env::var_os("SKEIN_PATCH_WRITE").is_some_and(|value| value == "1");
        let directory = Path::new(GOLDEN_DIRECTORY);
        if write {
            fs::create_dir_all(directory).expect("create patched golden directory");
        }
        let mut names = BTreeSet::new();
        let mut held = 0_u32;
        let mut scores = 0_u32;
        for (key, golden) in goldens.into_iter().enumerate() {
            let mut cx = patches::Context { key: u32::try_from(key).expect("golden key fits") };
            let root = generic::golden_root(&schema, &golden.name);
            let new_root = patcher.counterpart(plan, root).expect("root survives");
            let new_schema = patcher.schema(2).expect("v2 schema");
            let name = new_schema.spelled(new_schema.declaration(new_root).name).to_vec();
            let bytes = fs::read(Path::new("golden/v1").join(&golden.name)).expect("committed v1 golden");
            assert_eq!(bytes, golden.bytes, "{}: v1 golden", golden.name);
            let result = (|| -> Result<Vec<u8>, Halt> {
                patcher.start(plan, root, bytes.clone().into_boxed_slice())?;
                loop {
                    match patcher.next()? {
                        Step::Written { site, at } => {
                            if site == patch::Site::new(1, 1) {
                                scores += 1;
                            }
                            let (old, new) = patcher.parts();
                            patches::written(site, old, new, at, &mut cx)?;
                        }
                        Step::Held { at, family, name } => {
                            assert_eq!(patcher.spelled(family), b"every-other");
                            assert_eq!(patcher.spelled(name), b"Thing");
                            held += 1;
                            let (old, new) = patcher.parts();
                            new.carry(old, at)?;
                        }
                        Step::Done => break,
                    }
                }
                Ok(patcher.finish()?.to_vec())
            })()
            .unwrap_or_else(|halt| panic!("{} (key {}): {halt:?}", golden.name, cx.key));
            generated_round_trip(&name, &bytes, &result);
            let path = directory.join(&golden.name);
            if write {
                fs::write(&path, &result).expect("write patched golden");
            } else {
                assert_eq!(fs::read(&path).expect("patched golden committed; run SKEIN_PATCH_WRITE=1 heavy cargo nextest run -p skein-codec-tests --test patch"), result, "{}", golden.name);
            }
            assert!(names.insert(golden.name), "golden name unique");
        }
        assert_eq!(names.len(), GOLDENS);
        assert_eq!(held, EVERY_GOLDENS);
        assert_eq!(scores, EVERY_GOLDENS);
        let committed = fs::read_dir(directory)
            .expect("patched goldens directory")
            .map(|entry| entry.expect("golden entry").file_name().into_string().expect("golden name"))
            .collect::<BTreeSet<_>>();
        assert_eq!(committed, names, "no stale patched goldens");
    }
}

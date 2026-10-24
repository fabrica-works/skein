//! A decoded value at every ceiling attains its declared heap worst case
//! (codec.md, sections 4 and 6; testing-strategy.md, section 6).

use skein_heap::Counting;

#[global_allocator]
static HEAP: Counting = Counting;

#[cfg(test)]
mod tests {
    use skein_codec_tests::v1::{CEILINGS, Effect, Field, FieldParts, Report, ReportParts};
    use skein_heap::Meter;
    use skein_lib::{Duration, List, Reader, Writer};

    fn full_field() -> Field {
        Field::new(&CEILINGS, FieldParts { name: Box::from(&b"abcd"[..]), value: Box::from(&[1, 2, 3, 4][..]) })
            .expect("field at ceilings")
    }

    fn full_report() -> Report {
        let mut fields = List::with_capacity(CEILINGS.report_fields);
        fields.push(full_field()).expect("first field room");
        fields.push(full_field()).expect("second field room");
        Report::new(
            &CEILINGS,
            ReportParts {
                title: Box::from(&b"abcdefgh"[..]),
                fields,
                effect: Effect::Write,
                present: Some(true),
                nonce: [0xff, 0xff],
                age: Duration::from_nanos(u64::MAX),
            },
        )
        .expect("report at ceilings")
    }

    #[test]
    fn full_record_decoder_attains_and_stays_within_heap_worst_case() {
        let value = full_report();
        assert_eq!(
            value.measure(),
            u32::try_from(Report::worst_case_bytes(&CEILINGS).expect("wire bound")).expect("u32 ceiling")
        );
        let mut writer = Writer::new(usize::try_from(value.measure()).expect("u32 fits usize"));
        value.encode(&mut writer).expect("measured room");
        let wire = writer.finish();
        drop(value);

        let bound = Report::worst_case_heap(&CEILINGS).expect("heap bound");
        let meter = Meter::new();
        meter.start();
        let decoded = Report::decode(&CEILINGS, &mut Reader::new(&wire)).expect("full record");
        let measured = meter.end();
        assert_eq!(measured.peak(), bound, "the full value attains the heap bound");
        assert_eq!(measured.held(), bound, "the decoded value owns all of its heap");
        drop(decoded);
        assert_eq!(meter.held(), 0, "decoded allocations released");
    }

    fn full_numbers() -> skein_codec_tests::every::Numbers {
        use skein_codec_tests::every::{CEILINGS, Numbers, NumbersParts};
        Numbers::new(
            &CEILINGS,
            NumbersParts {
                small: u8::MAX,
                short: u16::MAX,
                word: u32::MAX,
                long: u64::MAX,
                flag: true,
                wait: Duration::from_nanos(u64::MAX),
                tag: [0xff; 3],
            },
        )
        .expect("numbers at ceilings")
    }

    fn full_stamp() -> skein_codec_tests::every::Stamp {
        use skein_codec_tests::every::{CEILINGS, Stamp, StampParts};
        Stamp::new(
            &CEILINGS,
            StampParts {
                at: u64::MAX,
                note: Some(vec![b'a'; usize::try_from(CEILINGS.stamp_note_some).expect("bound")].into_boxed_slice()),
            },
        )
        .expect("stamp at ceilings")
    }

    fn full_strings() -> skein_codec_tests::every::Strings {
        use skein_codec_tests::every::{CEILINGS, Strings, StringsParts};
        let bytes = |bound| vec![b'a'; usize::try_from(bound).expect("bound")].into_boxed_slice();
        Strings::new(
            &CEILINGS,
            StringsParts {
                raw: bytes(CEILINGS.strings_raw),
                key: skein_lib::Secret::new(bytes(CEILINGS.strings_key)),
                words: bytes(CEILINGS.strings_words),
            },
        )
        .expect("strings at ceilings")
    }

    fn full_every() -> skein_codec_tests::every::Every {
        use skein_codec_tests::every::{Blank, BlankParts, CEILINGS, Every, EveryParts, Light, Nest, NestParts, Shape};
        let bytes = |bound| vec![b'a'; usize::try_from(bound).expect("bound")].into_boxed_slice();
        let mut blanks = List::with_capacity(CEILINGS.nest_blanks);
        for _ in 0..CEILINGS.nest_blanks {
            blanks.push(Blank::new(&CEILINGS, BlankParts {}).expect("blank")).expect("room");
        }
        let mut lights = List::with_capacity(CEILINGS.nest_lights);
        for _ in 0..CEILINGS.nest_lights {
            lights.push(Light::Green).expect("room");
        }
        let mut maybe_lights = List::with_capacity(CEILINGS.nest_maybe_lights);
        for _ in 0..CEILINGS.nest_maybe_lights {
            maybe_lights.push(Some(Light::Green)).expect("room");
        }
        let mut grid = List::with_capacity(CEILINGS.nest_grid);
        for _ in 0..CEILINGS.nest_grid {
            let mut row = List::with_capacity(CEILINGS.nest_grid_item);
            for _ in 0..CEILINGS.nest_grid_item {
                row.push(u16::MAX).expect("room");
            }
            grid.push(row).expect("room");
        }
        let mut maybe_list = List::with_capacity(CEILINGS.nest_maybe_list_some);
        for _ in 0..CEILINGS.nest_maybe_list_some {
            maybe_list.push(u8::MAX).expect("room");
        }
        let mut shapes = List::with_capacity(CEILINGS.nest_shapes);
        for _ in 0..CEILINGS.nest_shapes {
            shapes.push(Shape::Strings(Box::new(full_strings()))).expect("room");
        }
        let mut secrets = List::with_capacity(CEILINGS.nest_secrets);
        for _ in 0..CEILINGS.nest_secrets {
            secrets.push(skein_lib::Secret::new(bytes(CEILINGS.nest_secrets_item))).expect("room");
        }
        let mut stamps = List::with_capacity(CEILINGS.nest_stamps);
        for _ in 0..CEILINGS.nest_stamps {
            stamps.push(full_stamp()).expect("room");
        }
        let mut pairs = List::with_capacity(CEILINGS.nest_pairs);
        for _ in 0..CEILINGS.nest_pairs {
            pairs.push([0xff; 2]).expect("room");
        }
        let nest = Nest::new(
            &CEILINGS,
            NestParts {
                blanks,
                lights,
                maybe_lights,
                grid,
                maybe_list: Some(maybe_list),
                maybe_numbers: Some(full_numbers()),
                shapes,
                secrets,
                stamps,
                pairs,
            },
        )
        .expect("nest at ceilings");
        Every::new(
            &CEILINGS,
            EveryParts {
                numbers: full_numbers(),
                strings: full_strings(),
                light: Light::Green,
                shape: Shape::Strings(Box::new(full_strings())),
                stamp: full_stamp(),
                nest,
                empty: Blank::new(&CEILINGS, BlankParts {}).expect("blank"),
                held: bytes(CEILINGS.every_held),
                items: bytes(CEILINGS.every_items),
            },
        )
        .expect("every at ceilings")
    }

    #[test]
    fn a_value_allocates_its_room_once_and_decoding_allocates_nothing() {
        use skein_codec::value::Value;
        use skein_codec_tests::every::{CEILINGS, Every};
        // Every's largest tree has 99 nodes; room also admits every other fixture.
        const ROOM: u32 = 256;
        let schema = skein_codec::schema::parse(include_bytes!("../schema/every-v1.schema")).expect("schema");
        let root = schema.find(b"Every").expect("root");
        let full = full_every();
        assert_eq!(u64::from(full.measure()), Every::worst_case_bytes(&CEILINGS).expect("wire bound"));
        let mut writer = Writer::new(usize::try_from(full.measure()).expect("wire length"));
        full.encode(&mut writer).expect("measured room");
        let wire = writer.finish();
        drop(full);

        let meter = Meter::new();
        meter.start();
        let mut value = Value::with_room(ROOM);
        let measured = meter.end();
        let bound = Value::worst_case_heap(ROOM).expect("heap bound");
        assert_eq!(measured.peak(), bound, "room is allocated once");
        assert_eq!(measured.held(), bound, "only the entries are held");

        let meter = Meter::new();
        meter.start();
        value.decode(&schema, root, wire).expect("full every decodes");
        let measured = meter.end();
        assert_eq!(measured.peak(), 0, "decoding allocates nothing");
        assert_eq!(measured.held(), 0, "decoding holds no new allocation");

        // Reuse the allocated room after moving out its previous input.
        let previous = value.take();
        let wire = previous.expose().into();
        drop(previous);
        let meter = Meter::new();
        meter.start();
        value.decode(&schema, root, wire).expect("reused every decodes");
        let measured = meter.end();
        assert_eq!(measured.peak(), 0, "decoding again allocates nothing");
        assert_eq!(measured.held(), 0, "decoding again holds no new allocation");
    }
}

#[cfg(test)]
mod patch_memory {
    use skein_codec::patch::{Capacity, Patcher, Step};
    use skein_codec::schema::{DeclIx, FieldIx};
    use skein_heap::{Meter, Span};

    // Two short texts, one patch, and a root with two scalar fields.
    const CAPACITY: Capacity = Capacity { versions: 2, patches: 1, text: 256, bytes: 64, nodes: 8 };
    const OLD: &[u8] = b"family f 1\nversioned record D { f: u8 held: bytes 3 holds other R }";
    const NEW: &[u8] = b"family f 2\nversioned record D { f: u16 held: bytes 3 holds other R }";
    const PATCH: &[u8] = b"patch 1\nowner o 1\nfamily f 1 to 2\nwiden D.f u8 u16\nwritten D.f\n";
    const WIRE: &[u8] = &[0, 1, 7, 0, 0, 0, 1, 97];
    const EXPECTED: &[u8] = &[0, 2, 0, 7, 0, 0, 0, 1, 97];
    const RECORDS: u32 = 128;

    #[test]
    fn patcher_stays_within_its_heap_bound_after_every_call() {
        let bound = Patcher::worst_case_heap(CAPACITY).expect("bound");
        let meter = Meter::new();
        meter.start();
        let mut patcher = Patcher::new(CAPACITY);
        assert!(meter.held() <= bound, "new");
        patcher.keep(OLD).expect("old");
        assert!(meter.held() <= bound, "old schema");
        patcher.keep(NEW).expect("new");
        assert!(meter.held() <= bound, "new schema");
        let plan = patcher.plan(PATCH).expect("patch").expect("family");
        let measured = meter.end();
        assert!(measured.peak() <= bound, "planning scratch is counted");
        for _ in 0..RECORDS {
            let wire = Box::from(WIRE);
            let span = Span::start();
            patcher.start(plan, DeclIx::new(0), wire).expect("record");
            assert_eq!(span.end().peak, 0, "start allocates nothing beyond the input box");
            assert!(meter.held() <= bound, "start");
            let span = Span::start();
            let at = match patcher.next().expect("written") {
                Step::Written { at, .. } => at,
                Step::Held { .. } | Step::Done => unreachable!("written field first"),
            };
            assert_eq!(span.end().peak, 0, "next allocates nothing");
            assert!(meter.held() <= bound, "written stop");
            let span = Span::start();
            let (old, new) = patcher.parts();
            let field = old.field(at, FieldIx::new(0)).expect("f");
            new.put_uint(old.uint(field).expect("uint")).expect("written answer");
            assert_eq!(span.end().peak, 0, "answer allocates nothing");
            assert!(meter.held() <= bound, "written answer");
            let span = Span::start();
            let at = match patcher.next().expect("held") {
                Step::Held { at, .. } => at,
                Step::Written { .. } | Step::Done => unreachable!("held field second"),
            };
            assert_eq!(span.end().peak, 0, "held next allocates nothing");
            assert!(meter.held() <= bound, "held stop");
            let span = Span::start();
            let (old, new) = patcher.parts();
            new.carry(old, at).expect("held answer");
            assert_eq!(span.end().peak, 0, "carry allocates nothing");
            assert!(meter.held() <= bound, "held answer");
            let span = Span::start();
            assert_eq!(patcher.next(), Ok(Step::Done));
            assert_eq!(span.end().peak, 0, "done allocates nothing");
            assert!(meter.held() <= bound, "done");
            let span = Span::start();
            assert_eq!(patcher.finish().expect("finish"), EXPECTED);
            assert_eq!(span.end().peak, 0, "finish lends the output without allocation");
            assert!(meter.held() <= bound, "finish");
        }
        drop(patcher);
        assert_eq!(meter.held(), 0, "all machine storage released");
    }
}

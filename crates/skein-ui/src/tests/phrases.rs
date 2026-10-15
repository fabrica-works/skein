use crate::Owner;
use crate::phrases::{
    Form, Hole, HoleKind, HoleShape, Invalid, Language, Phrase, PhraseId, PhraseLimits, PhraseShape, Phrasebook,
    PluralClass, PluralRule, ReferenceSpan, Segment, SpanUnit, WallFormat,
};
use skein_lib::{Duration, Wall};

const ID: PhraseId = PhraseId { owner: Owner { number: 4 }, number: 1 };
const LIMITS: PhraseLimits =
    PhraseLimits { languages: 2, phrases: 2, holes: 6, segments: 60, text_bytes: 200, hole_bytes: 8 };
fn shape() -> Box<[PhraseShape]> {
    Box::new([PhraseShape {
        id: ID,
        holes: Box::new([
            HoleShape { kind: HoleKind::Number, bytes: 0 },
            HoleShape { kind: HoleKind::Name, bytes: 4 },
            HoleShape { kind: HoleKind::Amount, bytes: 0 },
            HoleShape { kind: HoleKind::Span, bytes: 0 },
            HoleShape { kind: HoleKind::Wall, bytes: 0 },
            HoleShape { kind: HoleKind::Reference, bytes: 8 },
        ]),
        plural: Some(0),
    }])
}
fn language(long: bool) -> Language {
    let forms = [PluralClass::One, PluralClass::Other].map(|class| {
        let prefix: &[u8] = if long {
            b"longer: "
        } else if class == PluralClass::One {
            b"one: "
        } else {
            b"many: "
        };
        let mut segments = skein_lib::List::with_capacity(13);
        segments.push(Segment::Text(prefix.into())).unwrap();
        let kinds =
            [HoleKind::Number, HoleKind::Name, HoleKind::Amount, HoleKind::Span, HoleKind::Wall, HoleKind::Reference];
        let indices = if long { [5, 4, 3, 2, 1, 0] } else { [0, 1, 2, 3, 4, 5] };
        for index in indices {
            segments.push(Segment::Hole { index, kind: kinds[usize::from(index)] }).unwrap();
            segments.push(Segment::Text(b" ".as_slice().into())).unwrap();
        }
        Form { class, segments: segments.into_boxed() }
    });
    Language {
        plural: PluralRule::English,
        span_unit: SpanUnit::Nanos,
        wall: WallFormat::Nanos,
        phrases: Box::new([Phrase { id: ID, forms: Box::new(forms) }]),
    }
}

#[test]
fn loading_refuses_missing_phrases_wrong_holes_and_incomplete_forms() {
    let mut missing = language(false);
    missing.phrases = Box::new([]);
    assert_eq!(Phrasebook::load(shape(), Box::new([missing]), &LIMITS).unwrap_err(), Invalid::Missing { id: ID });
    let mut wrong = language(false);
    wrong.phrases[0].forms[0].segments[1] = Segment::Hole { index: 0, kind: HoleKind::Amount };
    assert_eq!(Phrasebook::load(shape(), Box::new([wrong]), &LIMITS).unwrap_err(), Invalid::Holes { id: ID });
    let mut incomplete = language(false);
    incomplete.phrases[0].forms[0].class = PluralClass::Other;
    assert_eq!(Phrasebook::load(shape(), Box::new([incomplete]), &LIMITS).unwrap_err(), Invalid::Forms { id: ID });
    let mut duplicate = language(false);
    duplicate.phrases[0].forms[0].segments[1] = Segment::Hole { index: 1, kind: HoleKind::Name };
    assert_eq!(Phrasebook::load(shape(), Box::new([duplicate]), &LIMITS).unwrap_err(), Invalid::Holes { id: ID });
}

#[test]
fn a_language_with_a_foreign_phrase_is_refused_naming_it() {
    let foreign = PhraseId { owner: Owner { number: 4 }, number: 2 };
    let mut words = language(false);
    words.phrases[0].id = foreign;
    let shapes = shape();
    assert_eq!(words.phrases.len(), shapes.len());
    assert_eq!(Phrasebook::load(shapes, Box::new([words]), &LIMITS).unwrap_err(), Invalid::Unknown { id: foreign });
}

#[test]
fn plural_selection_hole_values_and_longest_language_capacity_are_checked() {
    let book = Phrasebook::load(shape(), Box::new([language(false), language(true)]), &LIMITS).unwrap();
    let mut values = [
        Hole::Number(1),
        Hole::Name(b"name".as_slice().into()),
        Hole::Amount(3),
        Hole::Span(Duration::from_nanos(4)),
        Hole::Wall(Wall::from_nanos(5)),
        Hole::Reference { number: 9, name: b"a *chip*".as_slice().into() },
    ];
    let singular = book.say(0, ID, &values).unwrap();
    assert_eq!(singular.text.as_ref(), b"one: 1 name 3 4 5 a *chip* ");
    assert_eq!(singular.references.as_ref(), [ReferenceSpan { start: 18, end: 26, number: 9 }]);
    values[0] = Hole::Number(0);
    assert!(book.say(0, ID, &values).unwrap().text.starts_with(b"many: 0"));
    assert!(book.say(1, ID, &values).unwrap().text.starts_with(b"longer: a *chip* 5 4 3 name 0"));
    values[0] = Hole::Number(u64::MAX);
    values[2] = Hole::Amount(u64::MAX);
    values[3] = Hole::Span(Duration::from_nanos(u64::MAX));
    values[4] = Hole::Wall(Wall::from_nanos(u64::MAX));
    let longest = book.say(1, ID, &values).unwrap();
    assert_eq!(u32::try_from(longest.text.len()).unwrap(), book.capacity(ID).unwrap());
    values[1] = Hole::Name(b"long name".as_slice().into());
    assert_eq!(book.say(0, ID, &values), Err(Invalid::Values { id: ID }));
    values[1] = Hole::Amount(5);
    assert_eq!(book.say(0, ID, &values), Err(Invalid::Values { id: ID }));
    assert_eq!(book.say(2, ID, &[]), Err(Invalid::Values { id: ID }));
    clock_words();
    plural_capacity();
    assert!(crate::phrases::worst_case(&LIMITS).unwrap() > 200);
}

fn clock_words() {
    let mut clock = language(false);
    clock.wall = WallFormat::Clock;
    clock.span_unit = SpanUnit::Seconds;
    let clock_book = Phrasebook::load(shape(), Box::new([clock]), &LIMITS).unwrap();
    let clock_values = [
        Hole::Number(1),
        Hole::Name(b"name".as_slice().into()),
        Hole::Amount(3),
        Hole::Span(Duration::from_secs(4)),
        Hole::Wall(Wall::from_nanos(3_720_000_000_000)),
        Hole::Reference { number: 9, name: b"a *chip*".as_slice().into() },
    ];
    assert_eq!(clock_book.say(0, ID, &clock_values).unwrap().text.as_ref(), b"one: 1 name 3 4 01:02 a *chip* ");
    assert_eq!(clock_book.say(1, ID, &clock_values), Err(Invalid::Language));
}

fn plural_capacity() {
    let count_shape = Box::new([PhraseShape {
        id: ID,
        holes: Box::new([HoleShape { kind: HoleKind::Number, bytes: 0 }]),
        plural: Some(0),
    }]);
    let count_language = Language {
        plural: PluralRule::English,
        span_unit: SpanUnit::Nanos,
        wall: WallFormat::Nanos,
        phrases: Box::new([Phrase {
            id: ID,
            forms: Box::new([
                Form {
                    class: PluralClass::One,
                    segments: Box::new([
                        Segment::Text(b"a very long singular prefix: ".as_slice().into()),
                        Segment::Hole { index: 0, kind: HoleKind::Number },
                    ]),
                },
                Form {
                    class: PluralClass::Other,
                    segments: Box::new([Segment::Hole { index: 0, kind: HoleKind::Number }]),
                },
            ]),
        }]),
    };
    let count_book = Phrasebook::load(count_shape, Box::new([count_language]), &LIMITS).unwrap();
    assert_eq!(
        count_book.capacity(ID),
        Some(u32::try_from(count_book.say(0, ID, &[Hole::Number(1)]).unwrap().text.len()).unwrap())
    );
}

#[test]
fn shuffled_languages_keep_owner_scoped_ids_capacities_and_whole_load_refusals() {
    let rows = ROWS;
    let shapes = shuffled_shapes();
    let limits = PhraseLimits { languages: 2, phrases: 6, holes: 0, segments: 12, text_bytes: 256, hole_bytes: 0 };
    let book = Phrasebook::load(shapes.clone(), Box::new([shuffled_language(false), shuffled_language(true)]), &limits)
        .unwrap();
    for (id, bytes) in rows {
        assert_eq!(book.say(0, id, &[]).unwrap().text.as_ref(), bytes);
        assert_eq!(book.say(1, id, &[]).unwrap().text.as_ref(), twice(bytes).as_ref());
        assert_eq!(book.capacity(id), Some(u32::try_from(bytes.len().checked_mul(2).unwrap()).unwrap()));
        assert_eq!(book.clone().say(1, id, &[]), book.say(1, id, &[]));
    }
    let absent = PhraseId { owner: Owner { number: 7 }, number: 1 };
    assert_eq!(book.capacity(absent), None);
    assert_eq!(book.say(0, absent, &[]), Err(Invalid::Unknown { id: absent }));
    let mut duplicate_shapes = shapes.clone();
    duplicate_shapes[4].id = duplicate_shapes[0].id;
    assert_eq!(
        Phrasebook::load(duplicate_shapes, Box::new([shuffled_language(false)]), &limits).unwrap_err(),
        Invalid::Duplicate { id: shapes[0].id },
    );
    let mut duplicate_language = shuffled_language(false);
    let duplicate = duplicate_language.phrases[0].id;
    duplicate_language.phrases[4].id = duplicate;
    assert_eq!(
        Phrasebook::load(shapes.clone(), Box::new([duplicate_language]), &limits).unwrap_err(),
        Invalid::Duplicate { id: duplicate },
    );
    let mut unknown_language = shuffled_language(false);
    unknown_language.phrases[4].id = absent;
    assert_eq!(
        Phrasebook::load(shapes, Box::new([unknown_language]), &limits).unwrap_err(),
        Invalid::Unknown { id: absent },
    );
}

fn twice(bytes: &[u8]) -> Box<[u8]> {
    let mut writer = skein_lib::Writer::new(bytes.len().checked_mul(2).unwrap());
    writer.put(bytes).unwrap();
    writer.put(bytes).unwrap();
    writer.finish()
}

const ROWS: [(PhraseId, &[u8]); 6] = [
    (PhraseId { owner: Owner { number: 65_535 }, number: 0 }, b"other maximum".as_slice()),
    (PhraseId { owner: Owner { number: 65_536 }, number: u16::MAX }, b"kit maximum".as_slice()),
    (PhraseId { owner: Owner { number: 5 }, number: 1 }, b"side five".as_slice()),
    (PhraseId { owner: Owner { number: 6 }, number: 1 }, b"other four".as_slice()),
    (PhraseId { owner: Owner { number: 65_536 }, number: 1 }, b"kit one".as_slice()),
    (PhraseId { owner: Owner { number: 4 }, number: 1 }, b"side four".as_slice()),
];

fn shuffled_shapes() -> Box<[PhraseShape]> {
    Box::new(ROWS.map(|(id, _)| PhraseShape { id, holes: Box::new([]), plural: None }))
}

fn shuffled_language(long: bool) -> Language {
    let mut reverse = ROWS;
    reverse.reverse();
    Language {
        plural: PluralRule::English,
        span_unit: SpanUnit::Seconds,
        wall: WallFormat::Clock,
        phrases: Box::new(reverse.map(|(id, bytes)| Phrase {
            id,
            forms: Box::new([Form {
                class: PluralClass::Other,
                segments: if long {
                    Box::new([Segment::Text(twice(bytes))])
                } else {
                    Box::new([Segment::Text(bytes.into())])
                },
            }]),
        })),
    }
}

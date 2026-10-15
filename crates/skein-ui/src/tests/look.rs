use crate::look::{Fault, Names, check};
use crate::{Owner, Region, Style};

const OWNER: Owner = Owner { number: 1 };
const OTHER: Owner = Owner { number: 2 };
const fn names(
    prefix: &'static [u8],
    styles: &'static [&'static [u8]],
    regions: &'static [&'static [u8]],
) -> Names<'static> {
    Names { owner: OWNER, prefix, styles, regions }
}
type Row = (&'static [Names<'static>], Option<Fault>);
const ROWS: &[Row] = &[
    (&[names(b"notes", &[b"notes-bar"], &[])], None),
    (&[names(b"notes", &[b"bar"], &[])], Some(Fault::OffPrefix { owner: OWNER, index: 0 })),
    (&[names(b"notes", &[b"notes-bar", b"notes-bar"], &[])], Some(Fault::Twice { owner: OWNER, index: 1 })),
    (&[names(b"Notes", &[], &[])], Some(Fault::Prefix { owner: OWNER })),
    (
        &[names(b"notes", &[], &[]), Names { owner: OTHER, prefix: b"notes", styles: &[], regions: &[] }],
        Some(Fault::SamePrefix { owner: OTHER }),
    ),
    (&[names(b"", &[], &[])], Some(Fault::Prefix { owner: OWNER })),
    (&[names(b"notes", &[b"notes-bar!"], &[])], Some(Fault::OffPrefix { owner: OWNER, index: 0 })),
    (&[names(b"notes", &[b"notes-bar"], &[b"notes-bar"])], Some(Fault::Twice { owner: OWNER, index: 1 })),
    (&[names(b"notes", &[b"notes-bar", b"notes-bar"], &[b"bad"])], Some(Fault::OffPrefix { owner: OWNER, index: 2 })),
    (&[names(b"notes", &[b"bad"], &[]), names(b"Notes", &[], &[])], Some(Fault::Prefix { owner: OWNER })),
];

#[test]
fn each_fault_of_the_look_is_named() {
    for (names, expected) in ROWS {
        assert_eq!(check(names), *expected, "{names:?}");
    }
}

#[test]
fn a_style_finds_its_name_by_number() {
    let names = names(b"notes", &[b"notes-first", b"notes-Second2"], &[]);
    let rows: [(Style, Option<&[u8]>); 4] = [
        (Style { owner: OWNER, token: 0 }, Some(b"notes-first")),
        (Style { owner: OWNER, token: 1 }, Some(b"notes-Second2")),
        (Style { owner: OTHER, token: 0 }, None),
        (Style { owner: OWNER, token: 2 }, None),
    ];
    for (style, expected) in rows {
        assert_eq!(names.style(style), expected, "{style:?}");
    }
}

#[test]
fn a_region_finds_its_name_by_number() {
    let names = names(b"notes", &[], &[b"notes-first", b"notes-second"]);
    let rows: [(Region, Option<&[u8]>); 4] = [
        (Region { owner: OWNER, number: 0 }, Some(b"notes-first")),
        (Region { owner: OWNER, number: 1 }, Some(b"notes-second")),
        (Region { owner: OTHER, number: 0 }, None),
        (Region { owner: OWNER, number: 2 }, None),
    ];
    for (region, expected) in rows {
        assert_eq!(names.region(region), expected, "{region:?}");
    }
}

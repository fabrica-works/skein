//! The notes-by-label chart, with words supplied by the view (ui.md, 11).
use crate::view::OWNER;
use skein_lib::{Decimal, List, Reader};
use skein_ui::{
    DrawingKind, Style,
    scene::{Anchor, Point, Scene},
};

pub const KIND: DrawingKind = DrawingKind { owner: OWNER, kind: 0 };
const BAR: Style = Style { owner: OWNER, token: 1 };

pub(crate) fn caption(label: &[u8], count: u8) -> List<u8> {
    let mut text = List::with_capacity(64);
    for bytes in [label, b": ", Decimal::of(u64::from(count)).as_bytes()] {
        for byte in bytes {
            text.push(*byte).expect("label and count fit 64 bytes");
        }
    }
    text
}

/// Draw three label counts, each encoded as a count, word length and word.
#[must_use]
pub fn draw(kind: DrawingKind, data: &[u8], width: u16, height: u16, scene: &mut Scene) -> bool {
    if kind != KIND {
        return false;
    }
    let Some(rows) = rows(data) else { return false };
    let mut most = 1_u8;
    for (count, _) in rows {
        most = most.max(count);
    }
    let row_height = height / 3;
    for (index, (count, label)) in (0_u16..).zip(rows) {
        let top = row_height.saturating_mul(index);
        let length = u32::from(width.saturating_sub(112))
            .checked_mul(u32::from(count))
            .expect("u16 width times 64")
            .checked_div(u32::from(most))
            .expect("at least one");
        if !scene.rect(
            Point { x: 112, y: top },
            Point { x: u16::try_from(length).expect("scaled within width"), y: row_height.saturating_sub(8) },
            0,
            &[BAR],
        ) || !scene.act(&[u8::try_from(index).expect("three labels")])
        {
            return false;
        }
        if !scene.text(
            Point { x: 0, y: top.saturating_add(20) },
            Anchor::Start,
            caption(label, count).as_slice(),
            &[BAR],
        ) {
            return false;
        }
    }
    true
}
fn rows(data: &[u8]) -> Option<[(u8, &[u8]); 3]> {
    let mut reader = Reader::new(data);
    let mut rows = [(0_u8, b"".as_slice()); 3];
    for row in &mut rows {
        let count = reader.u8()?;
        let length = reader.u8()?;
        if count > 64 || length > 32 {
            return None;
        }
        *row = (count, reader.bytes(u32::from(length))?);
    }
    if reader.is_empty() { Some(rows) } else { None }
}

//! Bounded drawing marks in preorder (ui.md, section 11).

use crate::{DrawingKind, Limits, Style};
use skein_lib::List;

/// Whole pixels from the box's top left.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}
/// Where a text mark's point is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Anchor {
    Start,
    Middle,
    End,
}
/// A path's commands.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    Move(Point),
    Line(Point),
    Quadratic(Point, Point),
    Cubic(Point, Point, Point),
    Close,
}
/// One mark, with its variable-length data held in the scene's store.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mark {
    Group { offset: Point, marks: u32 },
    Line { from: Point, to: Point },
    Polyline { points: u32 },
    Path { commands: u32 },
    Rectangle { corner: Point, size: Point, radius: u16 },
    Circle { centre: Point, radius: u16 },
    Text { at: Point, anchor: Anchor },
}
#[derive(Clone, Copy, Debug)]
struct Span {
    start: u32,
    end: u32,
}
#[derive(Debug)]
pub(crate) struct Properties {
    data: Span,
    styles: Span,
    hint: Option<Span>,
    act: Option<Span>,
}
/// A scene reserves all storage at construction; refusal removes every mark.
#[derive(Debug)]
pub struct Scene {
    marks: List<Mark>,
    properties: List<Properties>,
    bytes: List<u8>,
    styles: List<Style>,
    styles_per_mark: u32,
    background: Option<Span>,
    refused: bool,
}
impl Scene {
    #[must_use]
    pub fn new(limits: &Limits) -> Scene {
        let styles = limits.scene_marks.checked_mul(limits.styles_per_node).expect("scene tokens fit u32");
        Scene {
            marks: List::with_capacity(limits.scene_marks),
            properties: List::with_capacity(limits.scene_marks),
            bytes: List::with_capacity(limits.scene_bytes),
            styles: List::with_capacity(styles),
            styles_per_mark: limits.styles_per_node,
            background: None,
            refused: false,
        }
    }
    pub fn clear(&mut self) {
        self.marks.clear();
        self.properties.clear();
        self.bytes.clear();
        self.styles.clear();
        self.background = None;
        self.refused = false;
    }
    #[must_use]
    pub const fn refused(&self) -> bool {
        self.refused
    }
    #[must_use]
    pub fn marks(&self) -> &[Mark] {
        self.marks.as_slice()
    }
    /// Text bytes, polyline points (x then y, little-endian), or path commands.
    /// Commands use ASCII M/L/Q/C/Z followed by their points in the same form.
    #[must_use]
    pub fn data(&self, index: u32) -> &[u8] {
        match self.properties.get(index) {
            Some(p) => self.slice(p.data),
            None => &[],
        }
    }
    #[must_use]
    pub fn styles(&self, index: u32) -> &[Style] {
        let Some(p) = self.properties.get(index) else {
            return &[];
        };
        self.styles
            .as_slice()
            .get(
                usize::try_from(p.styles.start).expect("u32 fits usize")
                    ..usize::try_from(p.styles.end).expect("u32 fits usize"),
            )
            .expect("stored token span")
    }
    #[must_use]
    pub fn mark_hint(&self, index: u32) -> Option<&[u8]> {
        let span = self.properties.get(index)?.hint?;
        Some(self.slice(span))
    }
    #[must_use]
    pub fn mark_act(&self, index: u32) -> Option<&[u8]> {
        let span = self.properties.get(index)?.act?;
        Some(self.slice(span))
    }
    fn slice(&self, span: Span) -> &[u8] {
        self.bytes
            .as_slice()
            .get(
                usize::try_from(span.start).expect("u32 fits usize")
                    ..usize::try_from(span.end).expect("u32 fits usize"),
            )
            .expect("stored byte span")
    }
    fn refuse(&mut self) -> bool {
        self.clear();
        self.refused = true;
        false
    }
    fn add(&mut self, mark: Mark, styles: &[Style], bytes: u32) -> bool {
        if self.refused {
            return false;
        }
        let Ok(count) = u32::try_from(styles.len()) else {
            return self.refuse();
        };
        if self.marks.room() == 0 || count > self.styles_per_mark || bytes > self.bytes.room() {
            return self.refuse();
        }
        let data =
            Span { start: self.bytes.len(), end: self.bytes.len().checked_add(bytes).expect("within byte capacity") };
        let tokens = Span {
            start: self.styles.len(),
            end: self.styles.len().checked_add(count).expect("within token capacity"),
        };
        for style in styles {
            self.styles.push(*style).expect("reserved token capacity");
        }
        self.marks.push(mark).expect("checked mark capacity");
        self.properties
            .push(Properties { data, styles: tokens, hint: None, act: None })
            .expect("one property per mark");
        true
    }
    pub fn group(&mut self, offset: Point, marks: u32, styles: &[Style]) -> bool {
        self.add(Mark::Group { offset, marks }, styles, 0)
    }
    pub fn line(&mut self, from: Point, to: Point, styles: &[Style]) -> bool {
        self.add(Mark::Line { from, to }, styles, 0)
    }
    pub fn rect(&mut self, corner: Point, size: Point, radius: u16, styles: &[Style]) -> bool {
        self.add(Mark::Rectangle { corner, size, radius }, styles, 0)
    }
    pub fn circle(&mut self, centre: Point, radius: u16, styles: &[Style]) -> bool {
        self.add(Mark::Circle { centre, radius }, styles, 0)
    }
    pub fn text(&mut self, at: Point, anchor: Anchor, bytes: &[u8], styles: &[Style]) -> bool {
        let Ok(count) = u32::try_from(bytes.len()) else {
            return self.refuse();
        };
        if !self.add(Mark::Text { at, anchor }, styles, count) {
            return false;
        }
        self.write(bytes);
        true
    }
    pub fn polyline(&mut self, points: &[Point], styles: &[Style]) -> bool {
        let Ok(count) = u32::try_from(points.len()) else {
            return self.refuse();
        };
        let Some(bytes) = count.checked_mul(4) else {
            return self.refuse();
        };
        if !self.add(Mark::Polyline { points: count }, styles, bytes) {
            return false;
        }
        for point in points {
            self.write_point(*point);
        }
        true
    }
    pub fn path(&mut self, commands: &[Command], styles: &[Style]) -> bool {
        let Ok(count) = u32::try_from(commands.len()) else {
            return self.refuse();
        };
        let mut bytes = 0_u32;
        for command in commands {
            let size = match command {
                Command::Move(_) | Command::Line(_) => 5,
                Command::Quadratic(_, _) => 9,
                Command::Cubic(_, _, _) => 13,
                Command::Close => 1,
            };
            let Some(total) = bytes.checked_add(size) else {
                return self.refuse();
            };
            bytes = total;
        }
        if !self.add(Mark::Path { commands: count }, styles, bytes) {
            return false;
        }
        for command in commands {
            match command {
                Command::Move(p) => {
                    self.write(b"M");
                    self.write_point(*p);
                }
                Command::Line(p) => {
                    self.write(b"L");
                    self.write_point(*p);
                }
                Command::Quadratic(a, b) => {
                    self.write(b"Q");
                    self.write_point(*a);
                    self.write_point(*b);
                }
                Command::Cubic(a, b, c) => {
                    self.write(b"C");
                    self.write_point(*a);
                    self.write_point(*b);
                    self.write_point(*c);
                }
                Command::Close => self.write(b"Z"),
            }
        }
        true
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.bytes.push(*byte).expect("checked byte capacity");
        }
    }
    fn write_point(&mut self, point: Point) {
        self.write(&point.x.to_le_bytes());
        self.write(&point.y.to_le_bytes());
    }
    fn store(&mut self, bytes: &[u8]) -> Option<Span> {
        if self.refused {
            return None;
        }
        let Ok(count) = u32::try_from(bytes.len()) else {
            self.refuse();
            return None;
        };
        if count > self.bytes.room() {
            self.refuse();
            return None;
        }
        let start = self.bytes.len();
        self.write(bytes);
        Some(Span { start, end: self.bytes.len() })
    }
    pub fn hint(&mut self, bytes: &[u8]) -> bool {
        let Some(index) = self.marks.len().checked_sub(1) else {
            return self.refuse();
        };
        let Some(span) = self.store(bytes) else {
            return false;
        };
        self.properties.get_mut(index).expect("last mark's properties").hint = Some(span);
        true
    }
    pub fn act(&mut self, bytes: &[u8]) -> bool {
        let Some(index) = self.marks.len().checked_sub(1) else {
            return self.refuse();
        };
        let Some(span) = self.store(bytes) else {
            return false;
        };
        self.properties.get_mut(index).expect("last mark's properties").act = Some(span);
        true
    }
    pub fn scene_act(&mut self, bytes: &[u8]) -> bool {
        let Some(span) = self.store(bytes) else {
            return false;
        };
        self.background = Some(span);
        true
    }
    /// Resolve platform hit-test indices, topmost first (ui.md, 11.3).
    /// Replaces `out` whole, returning false when no act or insufficient room.
    pub fn act_at(&self, under: &[u32], point: Point, out: &mut List<u8>) -> bool {
        out.clear();
        for index in under {
            if let Some(bytes) = self.mark_act(*index) {
                return emit_act(bytes, None, out);
            }
        }
        if let Some(topmost) = under.first() {
            let mut nearest = None;
            for (index, mark) in self.marks.iter().enumerate() {
                let index = u32::try_from(index).expect("mark index fits u32");
                match mark {
                    Mark::Group { marks, offset: _ } => {
                        if index < *topmost
                            && *topmost <= index.saturating_add(*marks)
                            && *topmost < self.marks.len()
                            && let Some(bytes) = self.mark_act(index)
                        {
                            nearest = Some(bytes);
                        }
                    }
                    Mark::Line { .. }
                    | Mark::Polyline { .. }
                    | Mark::Path { .. }
                    | Mark::Rectangle { .. }
                    | Mark::Circle { .. }
                    | Mark::Text { .. } => {}
                }
            }
            if let Some(bytes) = nearest {
                return emit_act(bytes, None, out);
            }
        }
        match self.background {
            Some(span) => emit_act(self.slice(span), Some(point), out),
            None => false,
        }
    }
}
/// Drawing code, routed by the kind's owner (ui.md, 11.1).
pub type Draw = fn(kind: DrawingKind, data: &[u8], width: u16, height: u16, scene: &mut Scene) -> bool;

fn emit_act(bytes: &[u8], point: Option<Point>, out: &mut List<u8>) -> bool {
    let extra = match point {
        Some(_) => 4,
        None => 0,
    };
    let Ok(count) = u32::try_from(bytes.len()) else {
        return false;
    };
    let Some(total) = count.checked_add(extra) else {
        return false;
    };
    if total > out.room() {
        return false;
    }
    for byte in bytes {
        out.push(*byte).expect("checked act capacity");
    }
    if let Some(point) = point {
        for byte in point.x.to_le_bytes() {
            out.push(byte).expect("checked point capacity");
        }
        for byte in point.y.to_le_bytes() {
            out.push(byte).expect("checked point capacity");
        }
    }
    true
}

impl core::fmt::Display for Scene {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (index, mark) in self.marks.iter().enumerate() {
            let index = u32::try_from(index).expect("mark index fits u32");
            for (parent, ancestor) in
                self.marks.iter().enumerate().take(usize::try_from(index).expect("u32 fits usize"))
            {
                match ancestor {
                    Mark::Group { marks, offset: _ } => {
                        let parent = u32::try_from(parent).expect("mark index fits u32");
                        if index <= parent.saturating_add(*marks) {
                            f.write_str("  ")?;
                        }
                    }
                    Mark::Line { .. }
                    | Mark::Polyline { .. }
                    | Mark::Path { .. }
                    | Mark::Rectangle { .. }
                    | Mark::Circle { .. }
                    | Mark::Text { .. } => {}
                }
            }
            print_mark(*mark, self.data(index), f)?;
            let styles = self.styles(index);
            if !styles.is_empty() {
                f.write_str(" [")?;
                for (position, style) in styles.iter().enumerate() {
                    if position != 0 {
                        f.write_str(" ")?;
                    }
                    f.write_fmt(format_args!("{}:{}", style.owner.number, style.token))?;
                }
                f.write_str("]")?;
            }
            if let Some(hint) = self.mark_hint(index) {
                f.write_str(" hint=")?;
                print_bytes(hint, f)?;
            }
            if let Some(act) = self.mark_act(index) {
                f.write_str(" act=")?;
                for byte in act {
                    f.write_fmt(format_args!("{byte:02X}"))?;
                }
            }
            f.write_str("\n")?;
        }
        Ok(())
    }
}
fn print_mark(mark: Mark, data: &[u8], f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match mark {
        Mark::Group { offset, marks: _ } => f.write_fmt(format_args!("group +{},{}", offset.x, offset.y)),
        Mark::Line { from, to } => f.write_fmt(format_args!("line {},{} {},{}", from.x, from.y, to.x, to.y)),
        Mark::Polyline { points } => {
            f.write_str("polyline")?;
            let mut reader = skein_lib::Reader::new(data);
            for _ in 0..points {
                print_point(&mut reader, f)?;
            }
            Ok(())
        }
        Mark::Path { commands } => {
            f.write_str("path")?;
            let mut reader = skein_lib::Reader::new(data);
            for _ in 0..commands {
                let command = reader.u8().expect("stored command tag");
                f.write_fmt(format_args!(" {}", char::from(command)))?;
                let points = match command {
                    b'M' | b'L' => 1_u32,
                    b'Q' => 2,
                    b'C' => 3,
                    b'Z' => 0,
                    _ => unreachable!("stored command tag"),
                };
                for _ in 0..points {
                    print_point(&mut reader, f)?;
                }
            }
            Ok(())
        }
        Mark::Rectangle { corner, size, radius } => {
            f.write_fmt(format_args!("rect {},{} {}x{} r{}", corner.x, corner.y, size.x, size.y, radius))
        }
        Mark::Circle { centre, radius } => f.write_fmt(format_args!("circle {},{} r{}", centre.x, centre.y, radius)),
        Mark::Text { at, anchor } => {
            let anchor = match anchor {
                Anchor::Start => "start",
                Anchor::Middle => "middle",
                Anchor::End => "end",
            };
            f.write_fmt(format_args!("text {},{} {} ", at.x, at.y, anchor))?;
            print_bytes(data, f)
        }
    }
}
fn print_point(reader: &mut skein_lib::Reader<'_>, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let x = read_coordinate(reader);
    let y = read_coordinate(reader);
    f.write_fmt(format_args!(" {x},{y}"))
}
fn print_bytes(bytes: &[u8], f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str("\"")?;
    for byte in bytes {
        if (0x20..=0x7e).contains(byte) {
            f.write_fmt(format_args!("{}", char::from(*byte)))?;
        } else {
            f.write_fmt(format_args!("\\x{byte:02X}"))?;
        }
    }
    f.write_str("\"")
}

fn read_coordinate(reader: &mut skein_lib::Reader<'_>) -> u16 {
    let low = reader.u8().expect("stored coordinate low byte");
    let high = reader.u8().expect("stored coordinate high byte");
    u16::from_le_bytes([low, high])
}

/// Reservations shared by `Scene::new` and the UI heap bound.
pub(crate) fn worst_case(limits: &Limits) -> Option<u64> {
    let tokens = limits.scene_marks.checked_mul(limits.styles_per_node)?;
    List::<Mark>::worst_case(limits.scene_marks)?
        .checked_add(List::<Properties>::worst_case(limits.scene_marks)?)?
        .checked_add(List::<u8>::worst_case(limits.scene_bytes)?)?
        .checked_add(List::<Style>::worst_case(tokens)?)
}

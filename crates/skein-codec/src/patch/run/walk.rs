//! Bounded conversion frames shared by next and carry (codec.md, 11.4–11.5).

use super::{
    Build, DeclIx, Expected, FieldIx, Frame, Halt, Held, Node, Old, Part, Path, Shape, Site, Source, Span, State, Step,
    Stop, Type, add, plan, spelled, ty,
};
use skein_lib::Reader;

impl Build {
    pub(super) fn advance(&mut self, old: &Old) -> Result<Step, Halt> {
        match self.state {
            State::Stopped => {
                if !self.opened.is_empty() || self.pending.is_some() {
                    return Err(Halt::Order);
                }
                self.stop = None;
                self.state = State::Walking;
            }
            State::Walking => {}
            State::Idle | State::Done | State::Finished => return Err(Halt::Order),
        }
        for _ in 0..self.turns() {
            let Some(frame) = self.walk.pop() else {
                self.state = State::Done;
                return Ok(Step::Done);
            };
            if let Some(step) = self.turn(old, frame, false)? {
                return Ok(step);
            }
        }
        Err(Halt::Room)
    }
    pub(super) fn turn(&mut self, old: &Old, frame: Frame, carrying: bool) -> Result<Option<Step>, Halt> {
        match frame {
            Frame::Enter { at, new, ignore_site } => self.enter(old, at, new, ignore_site, carrying),
            Frame::Record { at, decl, next, end } => self.record(old, at, decl, next, end, carrying),
            Frame::List { mut items, item, path } => {
                if let Some(at) = old.next_item(&mut items) {
                    self.push(Frame::List { items, item, path })?;
                    self.push(Frame::Enter {
                        at,
                        new: Expected { source: Source::Type(item), path },
                        ignore_site: false,
                    })?;
                }
                Ok(None)
            }
        }
    }
    fn stopped(&mut self, event: Step, new: Expected, stop: Stop, carrying: bool) -> Result<Option<Step>, Halt> {
        if carrying {
            return Err(Halt::Misbuilt(new.path));
        }
        self.pending = Some(new);
        self.stop = Some(stop);
        self.state = State::Stopped;
        Ok(Some(event))
    }
    fn enter(
        &mut self,
        old: &Old,
        at: Node,
        new: Expected,
        ignore_site: bool,
        carrying: bool,
    ) -> Result<Option<Step>, Halt> {
        let old_ty = ty(self.old_schema(), old.source(at)?);
        let new_ty = ty(self.new_schema(), new.source);
        match new_ty {
            Type::Named(decl) => return self.declaration(old, at, decl, new, ignore_site, carrying),
            Type::Bytes { held: Held::Record { .. }, .. } => match old_ty {
                Type::Bytes { held: Held::Record { family, name }, .. } => {
                    if self.old_schema().spelled(family) != self.old_schema().spelled(self.old_schema().family())
                        && self.compatible(old.source(at)?, new.source)
                    {
                        return self.stopped(
                            Step::Held { at, family, name },
                            new,
                            Stop { at, site: None, held: true, declaration: false },
                            carrying,
                        );
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
                | Type::Bytes { held: Held::Opaque | Held::Pieces(_), .. }
                | Type::List { .. }
                | Type::Option(_)
                | Type::Named(_) => {}
            },
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Bytes { held: Held::Opaque | Held::Pieces(_), .. }
            | Type::List { .. }
            | Type::Option(_) => {}
        }
        self.count_nodes(1)?;
        match new_ty {
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Duration => self.write_uint(new, old.uint(at)?)?,
            Type::Bool => self.put(&[u8::from(old.boolean(at)?)])?,
            Type::Fixed(_) | Type::Secret(_) | Type::Text(_) | Type::Bytes { .. } => {
                let bytes = match old_ty {
                    Type::Secret(_) => old.expose(at)?,
                    Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::U64
                    | Type::Bool
                    | Type::Duration
                    | Type::Fixed(_)
                    | Type::Text(_)
                    | Type::Bytes { .. }
                    | Type::List { .. }
                    | Type::Option(_)
                    | Type::Named(_) => old.content(at)?,
                };
                self.write_content(new, bytes)?;
            }
            Type::List { item, .. } => {
                self.put(&old.count(at)?.to_be_bytes())?;
                self.push(Frame::List { items: old.items(at)?, item, path: new.path })?;
            }
            Type::Option(item) => {
                let child = match old_ty {
                    Type::Option(_) => old.some(at)?,
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
                    | Type::Named(_) => Some(at),
                };
                self.put(&[u8::from(child.is_some())])?;
                if let Some(at) = child {
                    self.push(Frame::Enter {
                        at,
                        new: Expected { source: Source::Type(item), path: new.path },
                        ignore_site,
                    })?;
                }
            }
            Type::Named(_) => unreachable!("named types enter their declaration before counting"),
        }
        Ok(None)
    }
    fn declaration(
        &mut self,
        old: &Old,
        at: Node,
        decl: DeclIx,
        new: Expected,
        ignore_site: bool,
        carrying: bool,
    ) -> Result<Option<Step>, Halt> {
        let old_decl = match ty(self.old_schema(), old.source(at)?) {
            Type::Named(decl) => decl,
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
            | Type::Option(_) => return Err(Halt::Misbuilt(new.path)),
        };
        let conversion = *self.current().declarations.get(old_decl.index()).expect("old declaration conversion");
        if let Some(index) = conversion.site
            && !ignore_site
        {
            return self.stopped(
                Step::Written { site: Site::new(self.current().patch, index), at },
                new,
                Stop { at, site: Some(index), held: false, declaration: true },
                carrying,
            );
        }
        if carrying && !ignore_site && conversion.stops {
            return Err(Halt::Misbuilt(new.path));
        }
        if conversion.copies {
            let count = match old.value.nodes(at) {
                Ok(count) => count,
                Err(error) => return Err(Halt::Misread(error.node)),
            };
            self.count_nodes(count)?;
            let span = match old.value.span(at) {
                Ok(span) => span,
                Err(error) => return Err(Halt::Misread(error.node)),
            };
            self.put(spelled(old.value.bytes(), span))?;
            return Ok(None);
        }
        self.count_nodes(1)?;
        match self.new_schema().declaration(decl).shape {
            Shape::Record { versioned, first, count } => {
                match self.old_schema().declaration(old_decl).shape {
                    Shape::Enum { .. } => return Err(Halt::Misbuilt(new.path)),
                    Shape::Record { .. } => {}
                }
                if versioned {
                    self.put(&self.new_schema().version().to_be_bytes())?;
                }
                self.push(Frame::Record { at, decl, next: first.index(), end: add(first.index(), count) })?;
            }
            Shape::Enum { first, count } => {
                match self.old_schema().declaration(old_decl).shape {
                    Shape::Record { .. } => return Err(Halt::Misbuilt(new.path)),
                    Shape::Enum { .. } => {}
                }
                let old_tag = old.tag(at)?;
                let tag_path = Path { decl, part: Part::Tag };
                if carrying && !*self.current().variant_derived.get(old_tag.index()).expect("old variant flag") {
                    return Err(Halt::Misbuilt(tag_path));
                }
                let variant =
                    self.current().variants.get(old_tag.index()).copied().flatten().ok_or(Halt::Misbuilt(tag_path))?;
                let tag = variant.index().checked_sub(first.index()).ok_or(Halt::Misbuilt(tag_path))?;
                if tag >= count {
                    return Err(Halt::Misbuilt(tag_path));
                }
                self.put(&[u8::try_from(tag).expect("schema variant at most 255")])?;
                if let Some(payload) = self.new_schema().variant(variant).record {
                    let at = old.payload(at)?.ok_or(Halt::Misbuilt(tag_path))?;
                    self.push(Frame::Enter {
                        at,
                        new: Expected { source: Source::Declaration(payload), path: tag_path },
                        ignore_site: false,
                    })?;
                }
            }
        }
        Ok(None)
    }
    fn record(
        &mut self,
        old: &Old,
        at: Node,
        decl: DeclIx,
        next: u32,
        end: u32,
        carrying: bool,
    ) -> Result<Option<Step>, Halt> {
        if next == end {
            return Ok(None);
        }
        self.push(Frame::Record { at, decl, next: add(next, 1), end })?;
        let field = FieldIx::new(next);
        let new = Expected {
            source: Source::Type(self.new_schema().field(field).ty),
            path: Path { decl, part: Part::Field(field) },
        };
        match *self.current().sources.get(next).expect("new field source") {
            plan::Source::Old(field) => {
                if carrying && !*self.current().fields.get(field.index()).expect("old field derived flag") {
                    return Err(Halt::Misbuilt(new.path));
                }
                self.push(Frame::Enter { at: old.field(at, field)?, new, ignore_site: false })?;
                Ok(None)
            }
            plan::Source::Site(index) => self.stopped(
                Step::Written { site: Site::new(self.current().patch, index), at },
                new,
                Stop { at, site: Some(index), held: false, declaration: false },
                carrying,
            ),
            plan::Source::Written => Err(Halt::Misbuilt(new.path)),
            plan::Source::Value { start, end } => {
                let span = Span { start, end };
                let nodes = self.value_nodes(new, span);
                self.count_nodes(nodes)?;
                let plan = self.tables.plans.get(self.plan.expect("a run has a plan").index()).expect("kept plan");
                let bytes = spelled(plan.pool.as_slice(), span);
                match self.out.put(bytes) {
                    Ok(()) => Ok(None),
                    Err(_) => Err(Halt::Room),
                }
            }
        }
    }
    fn value_nodes(&self, expected: Expected, span: Span) -> u32 {
        let mut reader = Reader::new(spelled(self.current().pool.as_slice(), span));
        let mut source = expected.source;
        let mut count = 0;
        // Added values are scalar leaves, empty lists and options.
        for _ in 0..self.tables.capacity.text {
            count = add(count, 1);
            match ty(self.new_schema(), source) {
                Type::Option(item) => {
                    if reader.u8() == Some(1) {
                        source = Source::Type(item);
                    } else {
                        return count;
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
                | Type::Named(_) => return count,
            }
        }
        unreachable!("an added value's options fit in its bounded text")
    }
}

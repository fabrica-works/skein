//! Checked values supplied by written functions (codec.md, 11.5).

use super::{
    Build, DeclIx, Expected, FieldIx, Frame, Halt, Held, Node, Old, Open, OpenKind, Part, Path, Reason, RefusalIx,
    Shape, Source, State, Type, VariantIx, add, built, plan, ty,
};

impl Build {
    fn expected(&self) -> Result<Expected, Halt> {
        if self.state != State::Stopped {
            return Err(Halt::Order);
        }
        let mut expected = self.pending.ok_or(Halt::Order)?;
        match ty(self.new_schema(), expected.source) {
            Type::Named(decl) => match self.new_schema().declaration(decl).shape {
                Shape::Enum { .. } => expected.path = Path { decl, part: Part::Tag },
                Shape::Record { .. } => {}
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
            | Type::Bytes { .. }
            | Type::List { .. }
            | Type::Option(_) => {}
        }
        Ok(expected)
    }
    fn begin(&mut self) -> Result<Expected, Halt> {
        let expected = self.expected()?;
        if let Some(open) = self.opened.top() {
            match open.kind {
                OpenKind::List { bound, .. } => {
                    if open.progress >= bound {
                        return Err(built(open.path, Reason::Bound));
                    }
                }
                OpenKind::Record { .. } | OpenKind::Option { .. } => {}
            }
        }
        self.count_nodes(1)?;
        Ok(expected)
    }
    fn complete(&mut self) {
        if let Some(open) = self.opened.top_mut() {
            open.progress = add(open.progress, 1);
        }
        self.pending = self.following();
    }
    fn following(&self) -> Option<Expected> {
        let open = self.opened.top()?;
        match open.kind {
            OpenKind::Record { decl, first, count } => {
                if open.progress >= count {
                    return None;
                }
                let field = FieldIx::new(add(first, open.progress));
                Some(Expected {
                    source: Source::Type(self.new_schema().field(field).ty),
                    path: Path { decl, part: Part::Field(field) },
                })
            }
            OpenKind::List { item, .. } => Some(Expected { source: Source::Type(item), path: open.path }),
            OpenKind::Option { item } => {
                if open.progress != 0 {
                    return None;
                }
                Some(Expected { source: Source::Type(item), path: open.path })
            }
        }
    }
    fn opened(&mut self, kind: OpenKind, path: Path) -> Result<(), Halt> {
        match self.opened.push(Open { kind, path, progress: 0 }) {
            Ok(()) => {
                self.pending = self.following();
                Ok(())
            }
            Err(_) => Err(Halt::Room),
        }
    }
    /// Puts a uint or duration, within the new integer's width.
    pub fn put_uint(&mut self, value: u64) -> Result<(), Halt> {
        let result = self.uint(value);
        self.settle(result)
    }
    fn uint(&mut self, value: u64) -> Result<(), Halt> {
        let expected = self.begin()?;
        self.write_uint(expected, value)?;
        self.complete();
        Ok(())
    }
    pub(super) fn write_uint(&mut self, expected: Expected, value: u64) -> Result<(), Halt> {
        let width = match ty(self.new_schema(), expected.source) {
            Type::U8 => 1,
            Type::U16 => 2,
            Type::U32 => 4,
            Type::U64 | Type::Duration => 8,
            Type::Bool
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Bytes { .. }
            | Type::List { .. }
            | Type::Option(_)
            | Type::Named(_) => return Err(Halt::Misbuilt(expected.path)),
        };
        let bytes = value.to_be_bytes();
        let start = 8_u32.checked_sub(width).expect("integer width at most eight");
        for at in 0..start {
            if bytes.get(usize::try_from(at).expect("u32 fits usize")) != Some(&0) {
                return Err(built(expected.path, Reason::Bound));
            }
        }
        self.put(bytes.get(usize::try_from(start).expect("u32 fits usize")..).expect("integer suffix"))
    }
    /// Puts a boolean byte.
    pub fn put_bool(&mut self, value: bool) -> Result<(), Halt> {
        let result = self.boolean(value);
        self.settle(result)
    }
    fn boolean(&mut self, value: bool) -> Result<(), Halt> {
        let expected = self.begin()?;
        if ty(self.new_schema(), expected.source) != Type::Bool {
            return Err(Halt::Misbuilt(expected.path));
        }
        self.put(&[u8::from(value)])?;
        self.complete();
        Ok(())
    }
    /// Puts fixed, bytes, text or secret content, checked by the new type.
    pub fn put_content(&mut self, bytes: &[u8]) -> Result<(), Halt> {
        let result = self.content(bytes);
        self.settle(result)
    }
    fn content(&mut self, bytes: &[u8]) -> Result<(), Halt> {
        let expected = self.begin()?;
        self.write_content(expected, bytes)?;
        self.complete();
        Ok(())
    }
    pub(super) fn write_content(&mut self, expected: Expected, bytes: &[u8]) -> Result<(), Halt> {
        let Ok(length) = u32::try_from(bytes.len()) else {
            return Err(built(expected.path, Reason::Bound));
        };
        match ty(self.new_schema(), expected.source) {
            Type::Fixed(width) => {
                if length < width {
                    return Err(built(expected.path, Reason::Short));
                }
                if length > width {
                    return Err(built(expected.path, Reason::Trailing));
                }
            }
            Type::Bytes { bound, .. } | Type::Text(bound) | Type::Secret(bound) => {
                if length > bound {
                    return Err(built(expected.path, Reason::Bound));
                }
                match ty(self.new_schema(), expected.source) {
                    Type::Text(_) => {
                        if !crate::text_is_valid(bytes) {
                            return Err(built(expected.path, Reason::Utf8));
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
                    | Type::Bytes { .. }
                    | Type::List { .. }
                    | Type::Option(_)
                    | Type::Named(_) => {}
                }
                self.put(&length.to_be_bytes())?;
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::List { .. }
            | Type::Option(_)
            | Type::Named(_) => return Err(Halt::Misbuilt(expected.path)),
        }
        self.put(bytes)
    }
    /// Copies an old content value, checking it against the new type.
    pub fn put_content_of(&mut self, old: &Old, node: Node) -> Result<(), Halt> {
        let result = self.content_of(old, node);
        self.settle(result)
    }
    fn content_of(&mut self, old: &Old, node: Node) -> Result<(), Halt> {
        let expected = self.expected()?;
        match ty(self.new_schema(), expected.source) {
            Type::Bytes { held: Held::Record { .. }, .. } => return Err(Halt::Misbuilt(expected.path)),
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
        }
        let bytes = match ty(self.old_schema(), old.source(node)?) {
            Type::Secret(_) => old.expose(node)?,
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
            | Type::Named(_) => old.content(node)?,
        };
        self.content(bytes)
    }
    /// Puts an absent option.
    pub fn put_none(&mut self) -> Result<(), Halt> {
        let result = self.none();
        self.settle(result)
    }
    fn none(&mut self) -> Result<(), Halt> {
        let expected = self.begin()?;
        match ty(self.new_schema(), expected.source) {
            Type::Option(_) => {}
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
            | Type::Named(_) => return Err(Halt::Misbuilt(expected.path)),
        }
        self.put(&[0])?;
        self.complete();
        Ok(())
    }
    /// Puts an enumeration variant without a payload.
    pub fn put_variant(&mut self, variant: VariantIx) -> Result<(), Halt> {
        let result = self.variant(variant, false);
        self.settle(result)
    }
    /// Opens an enumeration variant and its record together.
    pub fn open_variant(&mut self, variant: VariantIx) -> Result<(), Halt> {
        let result = self.variant(variant, true);
        self.settle(result)
    }
    fn variant(&mut self, variant: VariantIx, open: bool) -> Result<(), Halt> {
        let expected = self.begin()?;
        let decl = match ty(self.new_schema(), expected.source) {
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
            | Type::Option(_) => return Err(Halt::Misbuilt(expected.path)),
        };
        let tag_path = Path { decl, part: Part::Tag };
        let (first, count) = match self.new_schema().declaration(decl).shape {
            Shape::Enum { first, count } => (first.index(), count),
            Shape::Record { .. } => return Err(Halt::Misbuilt(expected.path)),
        };
        let Some(tag) = variant.index().checked_sub(first) else {
            return Err(Halt::Misbuilt(tag_path));
        };
        if tag >= count {
            return Err(Halt::Misbuilt(tag_path));
        }
        let payload = self.new_schema().variant(variant).record;
        if open != payload.is_some() {
            return Err(Halt::Misbuilt(tag_path));
        }
        self.put(&[u8::try_from(tag).expect("schema tag at most 255")])?;
        match payload {
            Some(record) => {
                self.count_nodes(1)?;
                self.open_record(record, tag_path)
            }
            None => {
                self.complete();
                Ok(())
            }
        }
    }
    /// Opens a record, a list or a present option.
    pub fn open(&mut self) -> Result<(), Halt> {
        let result = self.open_value();
        self.settle(result)
    }
    fn open_value(&mut self) -> Result<(), Halt> {
        let expected = self.begin()?;
        match ty(self.new_schema(), expected.source) {
            Type::Named(decl) => self.open_record(decl, expected.path),
            Type::List { bound, item } => {
                let at = self.out.len();
                self.put(&0_u32.to_be_bytes())?;
                self.opened(OpenKind::List { bound, item, at }, expected.path)
            }
            Type::Option(item) => {
                self.put(&[1])?;
                self.opened(OpenKind::Option { item }, expected.path)
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
            | Type::Bytes { .. } => Err(Halt::Misbuilt(expected.path)),
        }
    }
    fn open_record(&mut self, decl: DeclIx, path: Path) -> Result<(), Halt> {
        match self.new_schema().declaration(decl).shape {
            Shape::Record { versioned, first, count } => {
                if versioned {
                    self.put(&self.new_schema().version().to_be_bytes())?;
                }
                self.opened(OpenKind::Record { decl, first: first.index(), count }, path)
            }
            Shape::Enum { .. } => Err(Halt::Misbuilt(path)),
        }
    }
    /// Closes the innermost record, list or option after its last value.
    pub fn close(&mut self) -> Result<(), Halt> {
        let result = self.close_value();
        self.settle(result)
    }
    fn close_value(&mut self) -> Result<(), Halt> {
        if self.state != State::Stopped {
            return Err(Halt::Order);
        }
        let open = self.opened.top().copied().ok_or(Halt::Order)?;
        match open.kind {
            OpenKind::Record { count, .. } => {
                if open.progress != count {
                    return Err(Halt::Misbuilt(self.pending.expect("a record lacks a field").path));
                }
            }
            OpenKind::Option { .. } => {
                if open.progress != 1 {
                    return Err(Halt::Misbuilt(open.path));
                }
            }
            OpenKind::List { at, .. } => self.out.set_u32(at, open.progress),
        }
        self.opened.pop().expect("an open frame was checked");
        self.complete();
        Ok(())
    }
    /// Gives one old part its derived conversion in place, without stops.
    pub fn carry(&mut self, old: &Old, node: Node) -> Result<(), Halt> {
        let result = self.carry_value(old, node);
        self.settle(result)
    }
    fn carry_value(&mut self, old: &Old, node: Node) -> Result<(), Halt> {
        let expected = self.expected()?;
        let a = old.source(node)?;
        if !self.compatible(a, expected.source) {
            return Err(Halt::Misbuilt(expected.path));
        }
        let stop = self.stop.expect("stopped build has its stop");
        let own = self.opened.is_empty() && stop.at == node;
        if own && stop.held {
            return self.content(old.content(node)?);
        }
        // begin checks the enclosing list's bound; the walk counts the carried nodes.
        self.begin()?;
        self.nodes = self.nodes.checked_sub(1).expect("begin counted this node");
        let base = self.walk.len();
        self.push(Frame::Enter { at: node, new: expected, ignore_site: own && stop.declaration })?;
        for _ in 0..self.turns() {
            if self.walk.len() == base {
                self.complete();
                return Ok(());
            }
            let frame = self.walk.pop().expect("a carried frame remains");
            if self.turn(old, frame, true)?.is_some() {
                unreachable!("carry refuses every stop");
            }
        }
        Err(Halt::Room)
    }
    pub(super) fn compatible(&self, a: Source, b: Source) -> bool {
        match a {
            Source::Type(a) => match b {
                Source::Type(b) => {
                    plan::compare(&self.current().olds, self.old_schema(), self.new_schema(), a, b)
                        != plan::Comparison::Retype
                }
                Source::Declaration(b) => match ty(self.old_schema(), Source::Type(a)) {
                    Type::Named(a) => self.current().olds.get(b.index()) == Some(&Some(a)),
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
                    | Type::Option(_) => false,
                },
            },
            Source::Declaration(a) => {
                let mut source = b;
                for _ in 0..self.tables.capacity.text {
                    match ty(self.new_schema(), source) {
                        Type::Named(b) => return self.current().olds.get(b.index()) == Some(&Some(a)),
                        Type::Option(item) => source = Source::Type(item),
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
                        | Type::List { .. } => return false,
                    }
                }
                false
            }
        }
    }
    /// Refuses the record, checking the current site's declared refusal range.
    pub fn refuse(&mut self, refusal: RefusalIx) -> Halt {
        if self.state != State::Stopped {
            self.state = State::Idle;
            return Halt::Order;
        }
        let mut declared = false;
        if self.state == State::Stopped
            && let Some(stop) = self.stop
            && let Some(site) = stop.site
        {
            for entry in &self.current().sites {
                if entry.index == site {
                    declared = refusal.index() >= entry.refusals.0 && refusal.index() < entry.refusals.1;
                }
            }
        }
        self.state = State::Idle;
        if declared { Halt::Refused(refusal) } else { Halt::Undeclared(refusal) }
    }
}

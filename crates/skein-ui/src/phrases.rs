//! Typed language data (skein's ui.md, section 9).
//! Loading checks every owner's declared phrase, its forms and typed holes.
//! Each phrase's capacity is the longest form in any loaded language with
//! every hole at its bound. Saying allocates its measured final bytes, within
//! that capacity, and preserves reference spans for the owner's semantic builder.
//! No domain vocabulary, platform language or application is known here.

use alloc::boxed::Box;
use skein_lib::{Decimal, Duration, List, Wall, Writer};

/// A phrase in an owner's closed vocabulary.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PhraseId {
    pub owner: crate::tree::Owner,
    pub number: u16,
}
/// The declared type of a phrase's hole.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum HoleKind {
    /// An unsigned count.
    Number,
    /// An amount in the owner's units.
    Amount,
    /// A duration.
    Span,
    /// A wall-clock instant.
    Wall,
    /// A name copied unchanged.
    Name,
    /// A reference whose name is copied unchanged.
    Reference,
}
/// A typed fact given to a phrase.
#[derive(Debug)]
pub enum Hole {
    /// An unsigned count.
    Number(u64),
    /// An amount in the owner's units.
    Amount(u64),
    /// A duration.
    Span(Duration),
    /// A wall-clock instant.
    Wall(Wall),
    /// An owned name.
    Name(Box<[u8]>),
    /// An owner reference number and its name; the saying preserves its span.
    Reference { number: u64, name: Box<[u8]> },
}
impl Hole {
    fn kind(&self) -> HoleKind {
        match self {
            Hole::Number(_) => HoleKind::Number,
            Hole::Amount(_) => HoleKind::Amount,
            Hole::Span(_) => HoleKind::Span,
            Hole::Wall(_) => HoleKind::Wall,
            Hole::Name(_) => HoleKind::Name,
            Hole::Reference { .. } => HoleKind::Reference,
        }
    }
}
/// One declared hole; `bytes` bounds names and reference names only.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct HoleShape {
    pub kind: HoleKind,
    pub bytes: u32,
}
/// An owner's phrase contract, independent of any language's word order.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct PhraseShape {
    pub id: PhraseId,
    pub holes: Box<[HoleShape]>,
    /// The index of a number that selects a plural form, or one unnumbered form.
    pub plural: Option<u16>,
}
/// A language's closed plural rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PluralRule {
    /// One for one, other for every other count.
    English,
}
/// A plural form selected by the language's rule.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum PluralClass {
    /// A singular count.
    One,
    /// Any other count, or a phrase with no plural selector.
    Other,
}
/// Integer duration units selected by language data; fractions are truncated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpanUnit {
    /// Nanoseconds.
    Nanos,
    /// Milliseconds.
    Millis,
    /// Seconds.
    Seconds,
    /// Minutes.
    Minutes,
    /// Hours.
    Hours,
}
impl SpanUnit {
    fn divisor(self) -> u64 {
        match self {
            SpanUnit::Nanos => 1,
            SpanUnit::Millis => 1_000_000,
            SpanUnit::Seconds => 1_000_000_000,
            SpanUnit::Minutes => 60_000_000_000,
            SpanUnit::Hours => 3_600_000_000_000,
        }
    }
}
/// Wall-clock presentation selected by language data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WallFormat {
    /// Nanoseconds since the epoch.
    Nanos,
    /// Whole seconds since the epoch.
    Seconds,
    /// UTC hour and minute, with two digits each.
    Clock,
}
/// One segment, in the language's own order.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Segment {
    /// Literal words owned by the phrasebook.
    Text(Box<[u8]>),
    /// The declared hole at this index, with its type checked at load.
    Hole { index: u16, kind: HoleKind },
}
/// One plural form of a phrase.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Form {
    pub class: PluralClass,
    pub segments: Box<[Segment]>,
}
/// All forms for a phrase, in one language.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Phrase {
    pub id: PhraseId,
    pub forms: Box<[Form]>,
}
/// A language's words and number/time presentations, given at start.
#[derive(Clone, Debug)]
pub struct Language {
    pub plural: PluralRule,
    pub span_unit: SpanUnit,
    pub wall: WallFormat,
    pub phrases: Box<[Phrase]>,
}
/// Bounds on loaded data and on copied names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PhraseLimits {
    pub languages: u16,
    pub phrases: u32,
    pub holes: u16,
    /// Total segments across every form and language.
    pub segments: u32,
    /// Total literal bytes across every form and language.
    pub text_bytes: u32,
    pub hole_bytes: u32,
}
/// A load or saying refusal; no partial phrase is emitted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Invalid {
    /// Data exceeds its configured counts or bytes.
    Limit,
    /// A capacity calculation does not fit.
    Overflow,
    /// A declared phrase is absent.
    Missing { id: PhraseId },
    /// A phrase occurs twice.
    Duplicate { id: PhraseId },
    /// A language or caller names an undeclared phrase.
    Unknown { id: PhraseId },
    /// A form's holes do not match its owner's declaration.
    Holes { id: PhraseId },
    /// Required plural forms are absent or duplicated.
    Forms { id: PhraseId },
    /// The selected language was not loaded.
    Language,
    /// Typed values do not fit the phrase's shape.
    Values { id: PhraseId },
}
#[derive(Clone, Debug)]
struct Capacity {
    id: PhraseId,
    bytes: u32,
}
/// Checked language data and the capacities shared across its languages.
#[derive(Clone, Debug)]
pub struct Phrasebook {
    ids: Box<[PhraseId]>,
    shapes: Box<[PhraseShape]>,
    languages: Box<[Language]>,
    capacities: List<Capacity>,
    limits: PhraseLimits,
}
/// A reference's name span in the saying, for the caller's binding table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReferenceSpan {
    pub start: u32,
    pub end: u32,
    pub number: u64,
}
/// Words and their reference positions, with no action tokens assigned here.
#[derive(PartialEq, Eq, Debug)]
pub struct Saying {
    pub text: Box<[u8]>,
    pub references: Box<[ReferenceSpan]>,
}

impl Phrasebook {
    /// Take bounded language data only after its whole shape has been checked.
    pub fn load(
        mut shapes: Box<[PhraseShape]>,
        mut languages: Box<[Language]>,
        limits: &PhraseLimits,
    ) -> Result<Phrasebook, Invalid> {
        if shapes.len() > usize::try_from(limits.phrases).expect("u32 count fits usize")
            || languages.is_empty()
            || languages.len() > usize::from(limits.languages)
        {
            return Err(Invalid::Limit);
        }
        // Id is the first derived ordering field. Equal ids become adjacent,
        // regardless of the caller's order or the rest of their declarations.
        shapes.sort_unstable();
        for pair in shapes.windows(2) {
            let earlier = pair.first().expect("window has two shapes");
            let later = pair.last().expect("window has two shapes");
            if earlier.id == later.id {
                return Err(Invalid::Duplicate { id: later.id });
            }
        }
        let mut ids = List::with_capacity(limits.phrases);
        for shape in &shapes {
            ids.push(shape.id).expect("declared shapes fit their limit");
            if shape.holes.len() > usize::from(limits.holes) {
                return Err(Invalid::Limit);
            }
            for hole in &shape.holes {
                if hole.bytes > limits.hole_bytes {
                    return Err(Invalid::Limit);
                }
            }
            if let Some(index) = shape.plural {
                let hole = shape.holes.get(usize::from(index)).ok_or(Invalid::Holes { id: shape.id })?;
                if hole.kind != HoleKind::Number {
                    return Err(Invalid::Holes { id: shape.id });
                }
            }
        }
        let ids = ids.into_boxed();
        let mut segments = 0_u32;
        let mut text = 0_u32;
        for language in &mut languages {
            if language.phrases.len() > usize::try_from(limits.phrases).expect("phrase count fits usize") {
                return Err(Invalid::Limit);
            }
            language.phrases.sort_unstable();
            for pair in language.phrases.windows(2) {
                let earlier = pair.first().expect("window has two phrases");
                let later = pair.last().expect("window has two phrases");
                if earlier.id == later.id {
                    return Err(Invalid::Duplicate { id: later.id });
                }
            }
            if language.phrases.len() != shapes.len() {
                let mut supplied = List::with_capacity(limits.phrases);
                for words in &language.phrases {
                    supplied.push(words.id).expect("language phrases fit their limit");
                }
                for id in &ids {
                    if supplied.as_slice().binary_search(id).is_err() {
                        return Err(Invalid::Missing { id: *id });
                    }
                }
                return Err(Invalid::Limit);
            }
            for (position, words) in language.phrases.iter().enumerate() {
                let index = if ids.get(position) == Some(&words.id) {
                    position
                } else {
                    let Ok(index) = ids.binary_search(&words.id) else {
                        return Err(Invalid::Unknown { id: words.id });
                    };
                    index
                };
                let shape = shapes.get(index).expect("id table has the shapes' order");
                check_forms(shape, words)?;
                for form in &words.forms {
                    for segment in &form.segments {
                        segments = segments.checked_add(1).ok_or(Invalid::Overflow)?;
                        match segment {
                            Segment::Text(bytes) => {
                                text = text.checked_add(crate::tree::length(bytes)).ok_or(Invalid::Overflow)?;
                            }
                            Segment::Hole { .. } => {}
                        }
                    }
                }
            }
        }
        if segments > limits.segments || text > limits.text_bytes {
            return Err(Invalid::Limit);
        }
        let mut capacities = List::with_capacity(limits.phrases);
        for (index, shape) in shapes.iter().enumerate() {
            let mut bytes = 0_u32;
            for language in &languages {
                let words = language.phrases.get(index).expect("every sorted phrase was checked");
                for form in &words.forms {
                    bytes = bytes.max(form_capacity(shape, language, form)?);
                }
            }
            capacities.push(Capacity { id: shape.id, bytes }).expect("declared phrases fit their limit");
        }
        let book = Phrasebook { ids, shapes, languages, capacities, limits: *limits };
        Ok(book)
    }
    /// The checked loading envelope, for a composing root to price its words.
    #[must_use]
    pub const fn limits(&self) -> &PhraseLimits {
        &self.limits
    }
    /// The longest bound of any loaded language's form for this phrase.
    #[must_use]
    pub fn capacity(&self, id: PhraseId) -> Option<u32> {
        let index = self.ids.binary_search(&id).ok()?;
        let capacity = self.capacities.get(u32::try_from(index).expect("phrase index fits its u32 bound"))?;
        assert!(capacity.id == id, "capacity has the id table's order");
        Some(capacity.bytes)
    }
    /// Say typed facts in a selected loaded language, with measured bounded bytes.
    pub fn say(&self, language: u16, id: PhraseId, values: &[Hole]) -> Result<Saying, Invalid> {
        let Ok(index) = self.ids.binary_search(&id) else {
            return Err(Invalid::Unknown { id });
        };
        let shape = self.shapes.get(index).expect("id table has the shapes' order");
        if values.len() != shape.holes.len() {
            return Err(Invalid::Values { id });
        }
        for (index, value) in values.iter().enumerate() {
            let declared = shape.holes.get(index).expect("value count equals shape count");
            if value.kind() != declared.kind {
                return Err(Invalid::Values { id });
            }
            match value {
                Hole::Name(bytes) | Hole::Reference { name: bytes, .. } => {
                    if crate::tree::length(bytes) > declared.bytes {
                        return Err(Invalid::Values { id });
                    }
                }
                Hole::Number(_) | Hole::Amount(_) | Hole::Span(_) | Hole::Wall(_) => {}
            }
        }
        let language = self.languages.get(usize::from(language)).ok_or(Invalid::Language)?;
        let class = match shape.plural {
            Some(index) => {
                let count = match values.get(usize::from(index)).expect("plural index was checked") {
                    Hole::Number(count) => *count,
                    Hole::Amount(_) | Hole::Span(_) | Hole::Wall(_) | Hole::Name(_) | Hole::Reference { .. } => {
                        return Err(Invalid::Values { id });
                    }
                };
                match language.plural {
                    PluralRule::English => {
                        if count == 1 {
                            PluralClass::One
                        } else {
                            PluralClass::Other
                        }
                    }
                }
            }
            None => PluralClass::Other,
        };
        let words = language.phrases.get(index).expect("every sorted phrase was checked");
        let form = form(words, class).expect("every required form was checked");
        let mut size = 0_u32;
        for segment in &form.segments {
            let bytes = match segment {
                Segment::Text(bytes) => crate::tree::length(bytes),
                Segment::Hole { index, .. } => {
                    measure(values.get(usize::from(*index)).expect("hole was checked"), language)
                }
            };
            size = size.checked_add(bytes).ok_or(Invalid::Overflow)?;
        }
        assert!(size <= self.capacity(id).expect("phrase capacity was checked"), "saying fits its loaded capacity");
        let mut writer = Writer::new(usize::try_from(size).expect("saying bytes fit usize"));
        let mut references = List::with_capacity(u32::try_from(values.len()).expect("hole count fits u32"));
        for segment in &form.segments {
            match segment {
                Segment::Text(bytes) => writer.put(bytes).expect("literal fits the measured saying"),
                Segment::Hole { index, .. } => {
                    let value = values.get(usize::from(*index)).expect("hole was checked");
                    let start = u32::try_from(writer.written()).expect("saying size fits u32");
                    write(value, language, &mut writer);
                    match value {
                        Hole::Reference { number, .. } => {
                            let end = u32::try_from(writer.written()).expect("saying size fits u32");
                            references
                                .push(ReferenceSpan { start, end, number: *number })
                                .expect("references fit the declared holes");
                        }
                        Hole::Number(_) | Hole::Amount(_) | Hole::Span(_) | Hole::Wall(_) | Hole::Name(_) => {}
                    }
                }
            }
        }
        let text = writer.finish();
        Ok(Saying { text, references: references.into_boxed() })
    }
}

#[expect(clippy::manual_find, reason = "step code uses bounded loops, not iterator closures")]
fn form(phrase: &Phrase, class: PluralClass) -> Option<&Form> {
    for form in &phrase.forms {
        if form.class == class {
            return Some(form);
        }
    }
    None
}
fn check_forms(shape: &PhraseShape, words: &Phrase) -> Result<(), Invalid> {
    let needed = if shape.plural.is_some() { 2 } else { 1 };
    if words.forms.len() != needed
        || form(words, PluralClass::Other).is_none()
        || (shape.plural.is_some() && form(words, PluralClass::One).is_none())
    {
        return Err(Invalid::Forms { id: shape.id });
    }
    for form in &words.forms {
        for segment in &form.segments {
            match segment {
                Segment::Text(_) => {}
                Segment::Hole { index, kind } => {
                    let hole = shape.holes.get(usize::from(*index)).ok_or(Invalid::Holes { id: shape.id })?;
                    if hole.kind != *kind {
                        return Err(Invalid::Holes { id: shape.id });
                    }
                }
            }
        }
        for (index, _hole) in shape.holes.iter().enumerate() {
            let mut seen = 0_u32;
            for segment in &form.segments {
                match segment {
                    Segment::Text(_) => {}
                    Segment::Hole { index: hole, .. } => {
                        if usize::from(*hole) == index {
                            seen = seen.checked_add(1).ok_or(Invalid::Overflow)?;
                        }
                    }
                }
            }
            if seen != 1 {
                return Err(Invalid::Holes { id: shape.id });
            }
        }
    }
    Ok(())
}
fn width(value: u64) -> u32 {
    crate::tree::length(Decimal::of(value).as_bytes())
}
fn form_capacity(shape: &PhraseShape, language: &Language, form: &Form) -> Result<u32, Invalid> {
    let mut capacity = 0_u32;
    for segment in &form.segments {
        let bytes = match segment {
            Segment::Text(bytes) => crate::tree::length(bytes),
            Segment::Hole { index, .. } => {
                let hole = shape.holes.get(usize::from(*index)).expect("hole was checked");
                match hole.kind {
                    HoleKind::Number => {
                        if shape.plural == Some(*index) && form.class == PluralClass::One {
                            1
                        } else {
                            20
                        }
                    }
                    HoleKind::Amount => 20,
                    HoleKind::Name | HoleKind::Reference => hole.bytes,
                    HoleKind::Span => {
                        width(u64::MAX.checked_div(language.span_unit.divisor()).expect("duration unit is nonzero"))
                    }
                    HoleKind::Wall => match language.wall {
                        WallFormat::Nanos => 20,
                        WallFormat::Seconds => width(Wall::from_nanos(u64::MAX).as_secs()),
                        WallFormat::Clock => 5,
                    },
                }
            }
        };
        capacity = capacity.checked_add(bytes).ok_or(Invalid::Overflow)?;
    }
    Ok(capacity)
}
fn measure(hole: &Hole, language: &Language) -> u32 {
    match hole {
        Hole::Number(value) | Hole::Amount(value) => width(*value),
        Hole::Span(value) => {
            width(value.as_nanos().checked_div(language.span_unit.divisor()).expect("duration unit is nonzero"))
        }
        Hole::Wall(value) => match language.wall {
            WallFormat::Nanos => width(value.as_nanos()),
            WallFormat::Seconds => width(value.as_secs()),
            WallFormat::Clock => 5,
        },
        Hole::Name(bytes) | Hole::Reference { name: bytes, .. } => crate::tree::length(bytes),
    }
}
fn decimal(value: u64, writer: &mut Writer) {
    writer.put(Decimal::of(value).as_bytes()).expect("decimal fits the measured saying");
}
fn write(hole: &Hole, language: &Language, writer: &mut Writer) {
    match hole {
        Hole::Number(value) | Hole::Amount(value) => decimal(*value, writer),
        Hole::Span(value) => decimal(
            value.as_nanos().checked_div(language.span_unit.divisor()).expect("duration unit is nonzero"),
            writer,
        ),
        Hole::Wall(value) => match language.wall {
            WallFormat::Nanos => decimal(value.as_nanos(), writer),
            WallFormat::Seconds => decimal(value.as_secs(), writer),
            WallFormat::Clock => {
                let hours = value
                    .as_secs()
                    .checked_div(3600)
                    .expect("hour divisor is nonzero")
                    .checked_rem(24)
                    .expect("day divisor is nonzero");
                let minutes = value
                    .as_secs()
                    .checked_div(60)
                    .expect("minute divisor is nonzero")
                    .checked_rem(60)
                    .expect("hour divisor is nonzero");
                if hours < 10 {
                    writer.put(b"0").expect("hour fits");
                }
                decimal(hours, writer);
                writer.put(b":").expect("separator fits");
                if minutes < 10 {
                    writer.put(b"0").expect("minute fits");
                }
                decimal(minutes, writer);
            }
        },
        Hole::Name(bytes) | Hole::Reference { name: bytes, .. } => {
            writer.put(bytes).expect("name fits the measured saying");
        }
    }
}

/// Loaded metadata, literal bytes, capacities and the largest saying's heap.
#[must_use]
pub fn worst_case(limits: &PhraseLimits) -> Option<u64> {
    let phrases = u64::from(limits.phrases);
    let languages = u64::from(limits.languages);
    let shapes = List::<PhraseShape>::worst_case(limits.phrases)?
        .checked_add(List::<HoleShape>::worst_case(u32::from(limits.holes))?.checked_mul(phrases)?)?;
    let words = List::<Language>::worst_case(u32::from(limits.languages))?
        .checked_add(List::<Phrase>::worst_case(limits.phrases)?.checked_mul(languages)?)?
        .checked_add(List::<Form>::worst_case(limits.phrases.checked_mul(2)?)?.checked_mul(languages)?)?
        .checked_add(List::<Segment>::worst_case(limits.segments)?)?
        .checked_add(u64::from(limits.text_bytes))?;
    let saying = u64::from(limits.hole_bytes.max(20))
        .checked_mul(u64::from(limits.holes))?
        .checked_add(u64::from(limits.text_bytes))?
        .checked_add(List::<ReferenceSpan>::worst_case(u32::from(limits.holes))?)?;
    shapes
        .checked_add(words)?
        .checked_add(List::<PhraseId>::worst_case(limits.phrases)?)?
        .checked_add(List::<Capacity>::worst_case(limits.phrases)?)?
        .checked_add(saying)
}

//! The list page and its English phrasebook (examples.md, 9.1).
use crate::{Place, State, records::Label};
use alloc::boxed::Box;
use skein_lib::{List, Token};
use skein_ui::phrases::{
    Form, Language, Phrase, PhraseId, PhraseLimits, PhraseShape, Phrasebook, PluralClass, PluralRule, Segment,
    SpanUnit, WallFormat,
};
use skein_ui::{Binding, Builder, Key, Owner, Role, Run, Style};
pub(crate) const OWNER: Owner = Owner { number: 1 };
pub(crate) const TITLE: Binding = Binding { owner: OWNER, token: Token::new(u64::MAX) };
pub(crate) const ADD: Binding = Binding { owner: OWNER, token: Token::new(u64::MAX - 1) };
pub(crate) const BACK: Binding = Binding { owner: OWNER, token: Token::new(u64::MAX - 2) };
pub(crate) const CARD: Style = Style { owner: OWNER, token: 0 };
const WORDS: [&[u8]; 11] = [
    b"Notes",
    b"Live",
    b"Offline",
    b"No notes yet.",
    b"idea",
    b"task",
    b"reference",
    b"No such note.",
    b"Title",
    b"Add",
    b"Back to notes",
];
const WORD_LIMITS: PhraseLimits =
    PhraseLimits { languages: 1, phrases: 11, holes: 0, segments: 11, text_bytes: 128, hole_bytes: 0 };

pub(crate) fn words() -> Phrasebook {
    let mut shapes = List::with_capacity(11);
    let mut phrases = List::with_capacity(11);
    for (number, text) in WORDS.iter().enumerate() {
        let id = PhraseId { owner: OWNER, number: u16::try_from(number).expect("eight phrases fit") };
        shapes.push(PhraseShape { id, holes: Box::from([]), plural: None }).expect("eight shapes fit");
        let segment = Segment::Text(Box::from(*text));
        let form = Form { class: PluralClass::Other, segments: Box::from([segment]) };
        phrases.push(Phrase { id, forms: Box::from([form]) }).expect("eight phrases fit");
    }
    let language = Language {
        plural: PluralRule::English,
        span_unit: SpanUnit::Minutes,
        wall: WallFormat::Seconds,
        phrases: phrases.into_boxed(),
    };
    Phrasebook::load(shapes.into_boxed(), Box::from([language]), &WORD_LIMITS)
        .expect("English phrases cover every shape")
}
pub(crate) fn words_worst_case() -> Option<u64> {
    skein_ui::phrases::worst_case(&WORD_LIMITS)
}
fn phrase(state: &State, number: u16) -> Box<[u8]> {
    state.words.say(0, PhraseId { owner: OWNER, number }, &[]).expect("declared English phrase").text
}
fn text(builder: &mut Builder, role: Role, bytes: &[u8]) {
    builder.open(role);
    builder.text(Run::Plain, bytes);
    builder.close();
}
pub(crate) fn page(state: &State, builder: &mut Builder) {
    builder.open(Role::Page);
    builder.open(Role::Banner);
    text(builder, Role::Text, &phrase(state, 0));
    text(builder, Role::Status, &phrase(state, if state.live { 1 } else { 2 }));
    builder.close();
    builder.open(Role::Main);
    match state.place {
        Place::List => {
            text(builder, Role::Heading { level: 1 }, &phrase(state, 0));
            list(state, builder);
            form(state, builder);
        }
        #[cfg(debug_assertions)]
        Place::Panic => text(builder, Role::Paragraph, &phrase(state, 7)),
        Place::Note { number } => note(state, number, builder),
        Place::Missing => text(builder, Role::Heading { level: 1 }, &phrase(state, 7)),
    }
    builder.close();
    builder.close();
}
fn list(state: &State, builder: &mut Builder) {
    if state.notes.notes().is_empty() {
        text(builder, Role::Paragraph, &phrase(state, 3));
        return;
    }
    builder.open(Role::List);
    for note in state.notes.notes() {
        builder.open(Role::Item);
        builder.key(Key { owner: OWNER, kind: 0, a: note.number(), b: 0 });
        builder.open(Role::Link);
        builder.text(Run::Plain, note.title());
        builder.link(Binding { owner: OWNER, token: Token::new(note.number()) });
        builder.close();
        text(builder, Role::Time, note.due());
        let label = match note.label() {
            Label::Idea => 4,
            Label::Task => 5,
            Label::Reference => 6,
        };
        text(builder, Role::Text, &phrase(state, label));
        builder.close();
    }
    builder.close();
}

fn form(state: &State, builder: &mut Builder) {
    builder.open(Role::Form);
    builder.name(&phrase(state, 9));
    builder.open(Role::TextField);
    builder.name(&phrase(state, 8));
    builder.value(&state.title, state.title_written);
    builder.state(skein_ui::State::SubmitsOnEnter);
    builder.bind(TITLE);
    builder.close();
    builder.open(Role::Button);
    builder.name(&phrase(state, 9));
    builder.bind(ADD);
    builder.close();
    builder.close();
}

fn note(state: &State, number: u64, builder: &mut Builder) {
    let mut found = false;
    for note in state.notes.notes() {
        if note.number() != number {
            continue;
        }
        found = true;
        builder.open(Role::Region);
        builder.name(note.title());
        builder.style(CARD);
        text(builder, Role::Heading { level: 1 }, note.title());
        builder.open(Role::Paragraph);
        let mut start = 0;
        for (index, byte) in note.body().iter().enumerate() {
            if index.saturating_sub(start) >= 252 && byte & 0xc0 != 0x80 {
                builder.text(Run::Plain, note.body().get(start..index).expect("body chunk"));
                start = index;
            }
        }
        builder.text(Run::Plain, note.body().get(start..).expect("last body chunk"));
        builder.close();
        text(builder, Role::Time, note.due());
        let minutes = skein_lib::Decimal::of(u64::from(note.minutes()));
        text(builder, Role::Text, minutes.as_bytes());
        let label = match note.label() {
            Label::Idea => 4,
            Label::Task => 5,
            Label::Reference => 6,
        };
        text(builder, Role::Text, &phrase(state, label));
        builder.close();
    }
    if !found {
        text(builder, Role::Heading { level: 1 }, &phrase(state, 7));
    }
    builder.open(Role::Link);
    builder.text(Run::Plain, &phrase(state, 10));
    builder.link(BACK);
    builder.close();
}

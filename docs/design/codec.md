# Codecs from schemas

Provisional, 2026-10-06, revised 2026-10-23. How an application protocol
built on skein encodes its records, and how its kept records follow its
schemas from one version to the next. A schema says each record's fields
and their bounds. skein's generator turns it into plain Rust codecs,
limits, worst cases and golden bytes, and `skein-codec` holds the little
the generated code shares, and reads schemas and their records at run
time. Codecs carry the bodies of a framed channel (channel.md), and
records kept in files or stores.

## 1. In one page

- **The schema is the specification.** A small file per family and version
  gives each record's fields, their types and their bounds. Design
  documents name records and bounds; the schema gives the bytes.
- **Generated, not written.** A generator, ordinary Rust in skein, turns a
  schema into:
  - the records' types, with their bounds sealed by constructors;
  - limits, whose ceilings are the schema's bounds;
  - measured encoding;
  - bounded decoding with typed problems;
  - each record's worst case;
  - golden bytes.
- **Plain Rust of the subset** (programming-model.md, section 10):
  structs, enums, functions and `match`; no traits beyond derives, no
  generics, closures or macros. The output is kept in the consumer's
  repository and reviewed like any other code. A test fails when the code
  and the schema drift.
- **Sized** (programming-model.md, section 8). Every length comes before
  its bytes, and is checked against its limit and what is left before
  anything is set aside. Types form a tree of fixed depth, so decoding
  never recurses on a peer's choice.
- **Canonical.** One value has one encoding, and decoding refuses any
  other, so what decodes encodes back to the same bytes.
- **Versioned.** A schema is one version of a family, and a family has
  one current version. A record kept beyond a channel carries its
  version. A reader reads the current version and refuses another. From
  the baseline on (section 11), kept versions are read as generic values
  and patched to the current one; until it, nothing is translated.
- **Read at run time too.** `skein-codec` reads a schema's text as step
  code (section 7), and decodes the bytes of any of its records into a
  generic value, an index over those bytes, with the generated code's
  checks (section 8). The generator reads schemas through the same
  parser, so there is one language.
- **Upgrades by patches.** A layout's fingerprint (section 9), a diff of
  two versions into the kinds of change (section 10), patches over
  generic values, run at run time without generated code for old
  versions (section 11), and the command that writes them, whose check a
  gate calls (section 12).

## 2. In skein

- **`skein-codec`** depends on lib only, and builds for
  `wasm32-unknown-unknown` as lib does (lib.md, section 2), with the code
  generated for it. It holds what the generated code shares:
  - the reasons a decoding fails;
  - bounded reads of lengths, counts and text;
  - reading a versioned record's version;
  - writing a piece of items and splitting one (section 3).

  And, as step code, what reads schemas and records at run time:
  - `schema`: the parser and the layout checks (section 7);
  - `value`: generic values (section 8);
  - `patch`: patches and the machine that runs them (section 11).
- **`skein-codegen`** is the generator, and the tooling of upgrades: the
  diff (section 10) and the command (section 12). Ordinary Rust, not
  step code. It reads schemas through `skein-codec`'s parser and adds
  the checks only Rust needs (section 7). A consumer depends on it in its
  tests and tools only.
- **A consumer's codec crate** holds its schemas, the generated code, its
  golden bytes and its drift test, and depends on lib and `skein-codec`.
  smith's `smith-charter`, `smith-channel` and `smith-transcript` are such
  crates.

## 3. The schema

```
family smith-charter 1

record Section {
    title: text 256
    body: text 65536
}

enum Effect { read, write }

record HostTool {
    name: text 64
    description: text 4096
    input: bytes 65536
    effect: Effect
    deadline: duration
}

record Report {
    fields: list 32 Field
}

enum Form {
    report: Report
    failure
}
```

| Type | Encoded as | In Rust |
|---|---|---|
| `u8`, `u16`, `u32`, `u64` | fixed width, big-endian | the same |
| `bool` | a byte, 0 or 1 | `bool` |
| `duration` | a `u64` of nanoseconds | `lib::Duration` |
| `fixed N` | N bytes | `[u8; N]` |
| `bytes N` | a `u32` length, then at most N bytes | `Box<[u8]>` |
| `secret N` | as `bytes N` | `lib::Secret` |
| `text N` | a `u32` length, then at most N bytes of UTF-8 | `Box<[u8]>`, checked UTF-8 |
| `list N T` | a `u32` count, then at most N items | `lib::List<T>` |
| `option T` | a byte, 0 for none or 1 for some, then the value | `Option<T>` |
| a record | its fields, in order | a struct |
| an enumeration | a byte tag, the variants numbered in order from 0, then the variant's record, if it has one | an enum; a `boxed` variant's record in a `Box` |

- **Durations, never times.** A time means something only on its writer's
  clock. A peer turns a duration into a deadline on its own clock
  (programming-model.md, section 9).
- **Every field is present.** An absent value is an explicit `option`;
  there are no defaults.
- **A variant holds one record, or nothing.** The record seals its own
  bounds, which an enum variant's fields could not.
- **A variant marked `boxed`** (`report: boxed Report`) holds its record
  in a `Box`, and its bytes are the record's as they would be. Its owner
  marks a variant whose record is much larger than its siblings', where
  the workspace's lints report it (section 6): only the compiler knows a
  type's size (section 4), so the generator never guesses one, and every
  other variant's record stays inline, costing no allocation. Like
  `secret`, the word changes the Rust type and not the bytes, so marking
  a variant or unmarking it moves no golden bytes and no version
  (section 5).
- **A sum crosses as one piece.** A schema bounds each field and each
  count, so a list's worst case is its count times its item's, and lists
  of items that hold lists multiply. Where an owner bounds a collection
  by its bytes in all, not by its count times its largest item, the
  collection is one `bytes N` field, N the owner's sum: its items each
  after a `u32` length, written and split with `skein-codec`'s piece
  helpers, each item decoded by its own type's code as the reader splits
  the piece. A piece whose lengths do not end exactly at its end is
  refused as malformed. Its worst case is its bound, and its items'
  decoded size is the owner's to count against the same sum.
- **A type uses only types declared before it.** The types form a tree, and
  decoding nests to a fixed depth.
- **No imports.** A record of another family, such as a charter carried in
  a channel's body, travels as a `bytes N` field and is decoded by its own
  family's code.
- **A `bytes` field says what it holds,** when it holds a record or
  pieces: `refusal: bytes 517 holds jig-store-people Refusal` holds a
  record of another family, named by its family and type;
  `segments: bytes 65536 pieces Segment` holds pieces (above) whose items
  are a type of this schema, declared before. The words change neither
  the bytes nor the Rust: the generated code reads the field as any
  `bytes N`, and its owner decodes what it holds as before. They let a
  patch of the held type reach the field wherever it is kept (section
  11). From the baseline on (section 11), a field of a stored family
  whose bytes are a record or pieces says so.
- **A record marked `versioned`** starts with its family's version, a
  `u16`. A reader of bytes kept elsewhere learns from it which code reads
  them.
- **A credential is a `secret N`,** never `bytes N`: a token, a key, a
  secret, a grant's value. Its bytes cross as `bytes N` would; its type,
  lib's `Secret` (lib.md, section 6), shows only its length in `Debug`, so
  a record that holds one is printed, logged or traced without it. Its
  owner reads the bytes with `expose`, to use or send them.

## 4. What is generated

For each family:

- **Its limits.** A plain struct with one limit per bound in the schema,
  and a constant holding the schema's bounds as ceilings. A side's limits
  may be smaller than the ceilings, never larger (programming-model.md,
  section 7). One generated function checks a family's limits against
  its ceilings, naming the first past one; the family's owner calls it
  once, where it makes its limits. Constructors and decoders check
  values only, never the limits: each value against its limit no higher
  than its ceiling, so no value past a ceiling exists or decodes, whatever
  limits they are given, and a call costs what its value holds, not the
  family's size.
- **Its problems.** An enumeration of the family's fields, and a problem
  that names one of them with `skein-codec`'s reason.

For each record and enumeration:

- **Its type.** Fields are private:
  - a constructor checks every field against the limits it is given and
    refuses with the field that broke one, so any value that exists fits
    them;
  - accessors read the fields, and a function that consumes the value
    hands its fields back, so bytes move and are not copied;
  - the derives are `Debug`, `Clone`, equality and `Hash`; a `secret`
    field's `Debug` is its length (section 3).
- **Measuring:** its encoded length. This cannot overflow, since the
  ceilings bound it.
- **Encoding** into a `lib::Writer` with that much room left: the rest of a
  frame's writer (channel.md, section 7), or a box of its own.
- **Decoding** from a `lib::Reader`, against the limits it is given. It
  refuses with a problem naming the field:
  - too short;
  - beyond a limit;
  - an unknown tag;
  - a `bool` that is neither 0 nor 1;
  - text that is not UTF-8;
  - another version;
  - bytes left over at the end.

  Every box is allocated at its final length, once that length has passed
  its limit and what is left.
- **Its worst case,** for given limits:
  - the most bytes it encodes to, which must fit a `u32` at the ceilings or
    the schema is refused;
  - the most heap its decoded value holds, as a function over the limits
    and the sizes of the generated types, which only the compiler knows,
    a `boxed` variant's record counting its own size.

  Each is `None` for limits past a ceiling. It checks the family's
  limits once, where it is asked, then reaches the types its fields name
  without checking them again: a worst case costs the types it reaches,
  not their number times the family's limits.

## 5. Versions

- **Any change a reader would notice is a new version:** a new schema,
  whose generated code replaces the old. There is one version at a time,
  and no promise of stability to anyone outside the family's owner.
- **A reader reads one version.** It reads a versioned record's version
  first, with `skein-codec`, and refuses another with the problem
  *another version*. Its generated code reads the current version only.
- **Kept versions are patched, from the baseline on.** The baseline is
  the first landing under section 11's rules. From it, a family's owner
  keeps each version's schema as text, and a kept record of an older
  version is read as a generic value (section 8) and patched to the
  current version (section 11) before the generated code reads it. A
  version then names one layout. Until the baseline, nothing is
  translated: what an owner does with a record of another version, such
  as starting afresh, is its own policy.
- **A writer writes its current version only.**
- **On a channel,** the agreed version picks the code for its bodies
  (channel.md, section 5.2). Bodies carry no version of their own.
- **A version's golden bytes stand for it.** A change that moves them is
  a new version.

## 6. Golden bytes and tests

- **Golden values follow a fixed rule,** so the generator can change
  without moving them. For each record, and each variant of each
  enumeration, there are two values:
  - **the smallest:** integers zero, bytes, text and lists empty, options
    absent, each enumeration at its first variant;
  - **a full one:** integers at their maximum, bytes and text a fixed
    pattern of a few bytes, each list with one full item, options present.

  Their encodings sit in a file beside the code, and each type's values
  must encode and decode as recorded.
- **Bounds:** generated tests decode a field at its limit, and one past it,
  without stored files. The limit is a test's own, a few bytes or items,
  with every other limit at its ceiling: decoding checks the limit it is
  given, so a small one checks the same code as the ceiling, which the
  worst case (section 4) and the memory test check.
- **Table tests:** each generated test checks a group of cases, up to
  1,024 golden values or 64 bounds, and names every case that fails. A
  test process costs more to start than one case costs to check
  (testing-strategy.md, section 8).
- **Drift:** a test runs the generator on the schema and compares the
  result with the committed code and golden bytes. On a difference it
  fails, saying how to write them again, which the same test does when an
  environment variable asks it to.
- **The generator's cost grows with its schema,** not with its square:
  what it needs of a type or a field (its declaration by name, whether a
  type holds bounds, a field's path) is found once per generation. The
  limits are checked against the ceilings once per family, where their
  owner makes them, and once by each worst case asked of them; never in
  each type, nor in a constructor or a decoder (section 4).
- **Generated files** say so in their first line. They are left out of
  formatting, and pass the workspace's lints with no allowances: the
  generator writes what the lints accept (functions short enough, no
  nesting to collapse), and an owner marks `boxed` an enum's variant the
  lints find too large (section 3).
- **Fuzzing,** in the fuzzy suite: each top-level record's decoder over
  arbitrary bytes. It never panics, and what decodes encodes back to the
  same bytes.
- **Memory:** decoding a full value at the ceilings, measured with the
  counting allocator against its heap's worst case.
- **The generator's own tests:**
  - schemas it refuses: a type used before it is declared, a name used
    twice, a worst case too large;
  - small schemas' encodings, compared with byte strings written by hand.

## 7. Schemas at run time

- **One grammar.** `skein-codec`'s `schema` module reads a schema's text
  into tables and checks its layout, as step code: bounded, no
  recursion, nothing allocated past the text's ceilings. skein-codegen's
  `parse` calls it, converts the tables into the generator's own tree,
  and adds the checks only Rust needs: names that are Rust identifiers
  of the right case and no keyword, fields not named as the generated
  methods, and each generated name (types, paths, limits) used once.
- **Refusals as before.** A text with one fault is refused at the same
  line with the same message as before the parser moved; a text with
  several gives the first by pass: characters, then the grammar, then
  the layout, then Rust's names.
- **Every version is read so.** The same parser reads a kept version's
  text at run time (section 11) and the current one in the generator,
  so a kept text means what it meant when it was current.

### 7.1 The grammar

```
schema      = "family" family version declaration*
declaration = ["versioned"] "record" name "{" field* "}"
            | "enum" name "{" variant* "}"
field       = name ":" type [","]
variant     = name [":" ["boxed"] name] [","]
type        = "u8" | "u16" | "u32" | "u64" | "bool" | "duration"
            | "fixed" bound | "secret" bound | "text" bound
            | "bytes" bound [held]
            | "list" bound type | "option" type
            | word                        # a declaration declared before
held        = "holds" family word | "pieces" word
```

- **Tokens:** words of ASCII letters, digits, `_` and `-`; the marks
  `{`, `}`, `:` and `,`; whitespace between them. `#` starts a comment
  that runs to the end of its line. Any other byte outside a comment is
  refused, before the grammar is read.
- **Words:** a `name` starts with a letter or `_` and holds letters,
  digits and `_`; a `family` holds letters, digits, `_` and `-`; a
  `bound` is a decimal `u32` and a `version` a decimal `u16`.
- **`holds` and `pieces`** after a `bytes` bound start its `held` clause,
  unless a `:` follows them: then they name the next field, as
  `holds: list 8 ChangeHold` does.
- **Lines** count from 1, one more after each `\n`. Each declaration,
  field and variant keeps the line of its name. A refusal of the
  grammar has the line of its token, or the text's last line at its
  end, which a final `\n` ends without starting another; a refusal of
  the layout, the line of the field, variant or declaration it names.
- **Bounded:** a text of at most `MAX_TEXT` bytes and `MAX_DECLARATIONS`
  declarations, refused past either. Each table is allocated once, at a
  size the text's length bounds. A type's chain of `list` and `option`
  is read in a loop, outermost first, and a named type is looked up in
  the declarations already read, so neither the parser nor the checks
  recurse.

### 7.2 The layout checks

Each declaration, in order, as the generator checked them (section 6):

- a record: for each field in order, its name not used before in the
  record, each name its type reaches declared before the record, and its
  worst case, and the record's up to it, within `u64`;
- an enumeration: 1 to 256 variants; for each in order, its name not
  used before in the enumeration, and its payload, if any, a record
  declared before; then its worst case within `u64`;
- the declaration's name not used before in the schema;
- its encoded worst case (section 4) within `u32`.

`pieces T` names a declaration declared before; `holds` names another
family's type, which the schema cannot check.

### 7.3 The tables

```rust
pub const MAX_TEXT: u32 = 1_048_576;
pub const MAX_DECLARATIONS: u32 = 4_096;

pub fn parse(text: &[u8]) -> Result<Schema, SchemaError>; // copies the text

pub struct Schema { /* the text, the family, the version, the tables */ }
pub struct DeclIx(u32);    // and FieldIx, VariantIx, TypeIx: indexes in
                           // the schema's tables, in the text's order
impl DeclIx {
    pub const fn new(index: u32) -> DeclIx;
    pub const fn index(self) -> u32;
}
pub struct Span { pub start: u32, pub end: u32 }  // a range of the text

pub struct Declaration { pub name: Span, pub line: u32, pub shape: Shape }
pub enum Shape {
    Record { versioned: bool, first: FieldIx, count: u32 },
    Enum { first: VariantIx, count: u32 },
}
pub struct Field { pub name: Span, pub line: u32, pub ty: TypeIx }
pub struct Variant { pub name: Span, pub line: u32, pub record: Option<DeclIx>, pub boxed: bool }
pub enum Type {
    U8, U16, U32, U64, Bool, Duration,
    Fixed(u32), Secret(u32), Text(u32),
    Bytes { bound: u32, held: Held },
    List { bound: u32, item: TypeIx },
    Option(TypeIx),
    Named(DeclIx),
}
pub enum Held { Opaque, Record { family: Span, name: Span }, Pieces(DeclIx) }

impl Schema {
    pub fn text(&self) -> &[u8];
    pub fn spelled(&self, span: Span) -> &[u8];
    pub fn family(&self) -> Span;
    pub fn version(&self) -> u16;
    pub fn declarations(&self) -> u32;
    pub fn declaration(&self, at: DeclIx) -> Declaration;
    pub fn field(&self, at: FieldIx) -> Field;
    pub fn variant(&self, at: VariantIx) -> Variant;
    pub fn ty(&self, at: TypeIx) -> Type;
    pub fn find(&self, name: &[u8]) -> Option<DeclIx>;
    pub fn fewest_bytes(&self, at: TypeIx) -> u32;
    pub fn most_bytes(&self, at: DeclIx) -> u32;  // the encoded worst case
}

pub struct SchemaError { pub line: u32, pub at: Span, pub wrong: Wrong }
pub enum Wrong {
    Long, Many,                                       // past a ceiling
    Character, End, Expected(Expected), Name, Bound, Family, Version,
    Undeclared, NotRecord, NoVariants, ManyVariants,  // the layout
    DuplicateType, DuplicateField, DuplicateVariant, Overflow, Large,
}
pub enum Expected { Family, Open, Colon, Record, Type, Declaration }
```

The plain types derive `Clone`, `Copy` where small, `Debug`, equality
and `Hash`; `Schema` derives `Debug` alone, as lib's `Map` holding its
names has neither `Clone` nor equality. A refusal names its token by `at`; skein-codegen writes the
message from it and the text. A type's fewest bytes are the generator's
(a list's precheck, section 8.2): a scalar's width, `fixed N`'s N, 4 for
a length or a count, 1 for an `option`, a record's fields' sum (2 more
if `versioned`), and an enumeration's 1 more than its smallest
variant's.

## 8. Generic values

- **An index over the bytes.** A `Value` holds a record's bytes, moved
  in, and an index of entries, one per node, in preorder: the record,
  each field's value, each item of a list, an option's value, a
  variant's record. An entry names its declaration or type, where its
  bytes start and end, its count, tag or presence, its parent, and the
  entry after its subtree. Nothing is copied out of the bytes.
- **Encoding is the bytes.** The encoding is canonical (section 1), so
  a decoded value's encoding is the bytes it was decoded from, two
  values of one type are equal when their bytes are, and a subtree's
  encoding is its range, which a patch carries by copying it (section
  11).
- **Decoded at the schema's ceilings,** with the generated code's
  checks in its order (8.2), so a value within its room decodes
  generically exactly when its generated decoder, given the ceilings as
  its limits, decodes it, and a refusal names the same field with the
  same reason. Limits below
  the ceilings stay the generated decoders'.
- **Its room.** A value's entries are allocated once, for the room it
  is made with, and reused for every record it decodes: decoding
  allocates nothing. A record that needs more nodes than the room is
  refused with `Refusal::Room`, since a node may take no bytes (an empty
  record), so the bytes alone do not bound the count.
- **Held fields stay opaque.** A `bytes` field that holds a record or
  pieces is read as its bytes, as the generated code reads it; a patch
  opens it (section 11).
- **Printed without content.** A value's bytes are a `lib::Secret`, so
  its `Debug` shows their length and the index (types, offsets, counts,
  tags), never a byte of a field, a secret's or any other. They are
  given back as that `Secret`, moved, not copied.
- **Built by a patch.** A new value is built only by a patch's machine
  (section 11), checked against its schema as it goes.

### 8.1 The value

```rust
pub struct Value { /* the bytes, as a Secret, and the entries */ }
pub struct Node(u32);                          // an entry, in preorder
pub struct Items { /* the next item and how many are left */ }
pub struct Path { pub decl: DeclIx, pub part: Part }
pub enum Part { Field(FieldIx), Tag, Tail, Version }
pub struct Problem { pub path: Path, pub reason: Reason }
pub enum Refusal { Problem(Problem), Room }
pub struct Misread { pub node: Node }          // the node is not of that kind

impl Value {
    pub fn with_room(nodes: u32) -> Value;
    pub fn worst_case_heap(nodes: u32) -> Option<u64>; // what with_room allocates
    pub fn decode(&mut self, schema: &Schema, root: DeclIx, bytes: Box<[u8]>)
        -> Result<Node, Refusal>;              // the root; the bytes are dropped on a refusal
    pub fn bytes(&self) -> &[u8];              // the encoding
    pub fn take(&mut self) -> Secret;          // the bytes back, moved; the value is empty
    pub fn span(&self, node: Node) -> Result<Span, Misread>;
    pub fn field(&self, record: Node, field: FieldIx) -> Result<Node, Misread>;
    pub fn items(&self, list: Node) -> Result<Items, Misread>;
    pub fn next_item(&self, items: &mut Items) -> Option<Node>;
    pub fn some(&self, option: Node) -> Result<Option<Node>, Misread>;
    pub fn tag(&self, enumeration: Node) -> Result<VariantIx, Misread>;
    pub fn payload(&self, enumeration: Node) -> Result<Option<Node>, Misread>;
    pub fn uint(&self, node: Node) -> Result<u64, Misread>;     // u8 to u64; a duration's nanoseconds
    pub fn boolean(&self, node: Node) -> Result<bool, Misread>;
    pub fn content(&self, node: Node) -> Result<&[u8], Misread>; // fixed, bytes, text
    pub fn expose(&self, node: Node) -> Result<&[u8], Misread>;  // a secret's
}
```

A `Path` is the generated `Path` (section 4) by its parts:
`ReportTitle` is `Report`'s field `title`, `EffectTag` the tag of
`Effect`, `FieldTail` the bytes after `Field`, `ReportVersion` the version
of `Report`. Reading a node as what it is not is a `Misread`, a value
the caller handles, never a panic.

### 8.2 The order of the checks

As the generated decoder's, with each bound at its ceiling:

- **A versioned record** reads its version first: too short is `Short`
  at its `Version`, another than the schema's is `Version` there. Then
  its fields in order.
- **An enumeration** reads its tag: too short is `Short` at its `Tag`,
  one past its variants `Tag` there. Then the variant's record, if it
  has one.
- **A field's value** is refused at the field's path, whatever list or
  option holds it; a named type's own checks name its own paths:
  - an integer, a duration or `fixed N`: too short is `Short`;
  - `bool`: too short is `Short`, a byte past 1 is `Bool`;
  - `bytes N`, `secret N` and `text N`: a length too short to read is
    `Short`, past N `Bound`, past the bytes left `Short`; text not UTF-8
    is `Utf8`;
  - `list N T`: a count too short to read is `Short`, past N `Bound`,
    and past the bytes left divided by T's fewest bytes `Short`, when
    those are at least 1; then each item;
  - `option T`: its byte too short is `Short`, 0 is none, 1 is T, any
    other `Tag`.
- **The root** refuses bytes left over with `Trailing`: at its `Tail` if
  a record, at its `Tag` if an enumeration.
- **The room** is checked as each node is entered, before its bytes are
  read.

### 8.3 Tests

- **Step tests** of the parser and the checks: each refusal from both
  sides, a text that passes beside one that does not; `holds` and
  `pieces`, and the fields so named; the ceilings at and past their
  bound.
- **The old parser as an oracle** until it goes: both give the same
  tree, or the same line and message, over every schema in skein and the
  generator's fixtures. The generated code stays byte-identical, which
  the drift tests hold.
- **Generic against generated,** in the fuzzy suite, over skein's
  schemas and a fixture that holds every type of the language: values
  drawn from a schema by a seeded generator decode by both to the same
  tree and encode back to the same bytes; arbitrary bytes, and goldens
  with a byte changed, cut or grown, are refused by both with the same
  path and reason, or decode by both alike. The trees are compared
  without changing the generated code: the generic one is printed in the
  form the generated types' `Debug` takes, and compared with it.
- **Goldens:** every golden of every skein schema decodes generically
  to its own bytes.
- **Memory:** `with_room` allocates `worst_case_heap`, and decoding
  allocates nothing, counted by the counting allocator.

## 9. Fingerprints

- **Of the layout, not the names.** A declaration's fingerprint is a
  SHA-256 (lib's) over a canonical encoding of what its bytes are
  (9.1): record or enumeration, `versioned` and the family's version,
  each field's type in order (its kind and width, a `fixed` length, a
  bound, `list` and `option`, text, bytes or secret, what a `bytes`
  holds), the variants' payloads in order, and each named type by its
  own fingerprint. Names, comments, the text's layout and `boxed` are
  not in it. A record held from another family is named by its family
  and type, since its layout is that family's.
- **A family's fingerprint** is over all its declarations', in the
  text's order, including those no other declaration names: the kept
  text is the unit, and any declaration may be a stored root, which only its
  owner's keys say. A declaration added, dropped or moved moves it, as
  a declaration's own change does.
- **Step code,** in `schema`: one pass over the declarations in order,
  each named type's fingerprint read from the entries already made, so
  nothing recurses. It allocates that table alone, a fingerprint per
  declaration, at most `MAX_DECLARATIONS`.
- **A rename moves no fingerprint,** and the check (section 12) also
  asks the latest kept text to follow the current names, so that a
  later diff never mistakes a rename for a field dropped and another
  added.
- **A family's fingerprint moved** means its kept text is replaced,
  not that a patch is owed: the check decides by the diff (section
  12), never by the family's fingerprint alone, and nothing at run
  time takes its move for a change of layout. What must decide one
  uses the declarations' fingerprints.

### 9.1 The encoding

A fingerprint is the SHA-256 of a byte string that starts with the
encoding's version, 1, so that a later encoding is never mistaken for
this one. Integers are big-endian; a fingerprint inside another is its
32 bytes.

```
fingerprint = SHA-256(0x01 · layout)
layout      = record | enumeration | family
record      = 0x01 · (0x00 | 0x01 · u16 version) · u32 count · type*
enumeration = 0x02 · u32 count · (0x00 | 0x01 · fingerprint)*
family      = 0x03 · u32 count · fingerprint*
type        = 0x01 u8 | 0x02 u16 | 0x03 u32 | 0x04 u64
            | 0x05 bool | 0x06 duration
            | 0x07 · u32 N               fixed N
            | 0x08 · u32 N               secret N
            | 0x09 · u32 N               text N
            | 0x0a · u32 N · held        bytes N
            | 0x0b · u32 N · type        list N, then its item
            | 0x0c · type                option, then its value
            | 0x0d · fingerprint         a named declaration
held        = 0x00                       nothing said
            | 0x01 · u32 n · family · u32 n · name     holds
            | 0x02 · fingerprint         pieces of a declaration
```

- A record's types are its fields', in order; an enumeration's entries
  its variants', 0 for none and 1 with the record's fingerprint; a
  family's its declarations', in order.
- The `version` is the family's, in a `versioned` record only: a family
  with no versioned record has the same fingerprints at every version.
- Each production starts with a byte of its own and ends where its
  grammar says, so two layouts' bytes are equal only when the layouts
  are.

### 9.2 The interface

```rust
pub struct Fingerprint(/* 32 bytes */);  // Clone, Copy, Debug, equality, order, Hash
pub struct Fingerprints { /* a fingerprint per declaration, and the family's */ }

impl Fingerprint {
    pub const fn digest(self) -> [u8; 32];
}
impl Schema {
    pub fn fingerprints(&self) -> Fingerprints;
}
impl Fingerprints {
    pub fn declaration(&self, at: DeclIx) -> Fingerprint;
    pub fn family(&self) -> Fingerprint;
}
```

`Fingerprints` derives `Debug` alone, as `Schema` does.

### 9.3 Tests

- **Step tests,** from both sides: each element the encoding covers
  moves a fingerprint, beside the nearest change that moves none (a
  rename, a comment, the text's layout, `boxed`); a named type's change
  moves its referrers', and its rename does not; a declaration nothing
  names moves the family's alone.
- **Golden digests:** a small schema's, its encoded bytes written out
  beside it, and the fixture that holds every type's, so that a change
  of the encoding is a reviewed diff.
- **Every skein schema,** at run time: its fingerprints equal those of
  its text with every name changed, the comments gone, the layout
  changed and `boxed` turned over.

## 10. The diff

- **Two versions of a family into their changes,** in skein-codegen's
  `diff`, ordinary Rust: the latest kept text and the current one, both
  read by `schema` (section 7). Each change names its declaration and
  its field or variant, has a kind (10.3), and is one line of text
  (10.2), the operation a patch spells (section 11) and the command
  writes (section 12).
- **Matched by name** (10.1): declarations, then the fields of matched
  records and the variants of matched enumerations, the renames its
  author confirmed first. A rename is proposed where one leaves and
  another of the same layout takes its place; until its author confirms
  it, the diff holds it for a field dropped and another added.
- **Changes through a type are carried.** A declaration whose only
  change is in a type it names, in a field, a list, an option, a
  variant or pieces, has no change of its own: the named type's
  conversion reaches it wherever it is held (section 11).
- **Every change is listed,** of every kind, so that a patch's
  operations turn the old layout into exactly the new one (section 11),
  and the command learns from the kinds whether a patch is needed at
  all (section 12).
- **Not changes:** `boxed`, comments and the text's layout, which are
  not in the layout, and the declarations' order, which no byte depends
  on. A text whose declarations only moved has another family
  fingerprint and no change: the command refreshes the kept text, as
  for a rename.

### 10.1 Matching

- **Confirmed renames first,** each pairing a kept declaration, field
  or variant with a current one; then what is left pairs by name.
  A rename names the old declaration and the new; a field's or a
  variant's, its declaration by its current name, its old name and its
  new. A rename is refused that names what its text lacks, a name a
  second time, or a part of a declaration not matched.
- **Unmatched are added and dropped:** a current declaration, field or
  variant matched to none is added, and a kept one matched to none is
  dropped.
- **Proposals:** the unmatched kept and the unmatched current, each in
  its text's order, pair one to one while both have one left: a pair of
  the same layout is a proposed rename, for its author to confirm. Two
  fields have the same layout when their types compare equal (10.3),
  two variants when their records match or both have none, two
  declarations when they are of one shape, `versioned` alike, with as
  many fields or variants, pair for pair of the same layout. A field's
  or a variant's rename is proposed in a matched declaration only, so
  declarations' renames are proposed, and confirmed, first.

### 10.2 The changes

| Line | The change | Kind |
|---|---|---|
| `add D` | declaration `D` added | none needed |
| `drop D` | declaration `D` dropped | none needed |
| `rename D E` | declaration `D` renamed `E` | none needed |
| `reshape D` | `D` a record in one text and an enumeration in the other | written |
| `versioned D`, `unversioned D` | record `D` made `versioned`, or no longer | derived |
| `add D.f T v` | field `f` of type `T` added, every old record taking `v` | derived |
| `drop D.f` | field `f` dropped | derived |
| `rename D.a b` | field or variant `a` renamed `b` | none needed |
| `widen D.f T U` | field `f`'s type `T` widened to `U` | none needed, or derived when an integer is wider |
| `wrap D.f T U` | field `f`'s type `T` wrapped in `option` to `U` | derived |
| `retype D.f T U` | field `f`'s type changed otherwise | written |
| `order D a b …` | the fields or variants kept, now in this order | derived |
| `add D.v` | variant `v` added | none needed at the end, derived before a kept one |
| `drop D.v` | variant `v` dropped | written |
| `payload D.v P Q` | variant `v`'s record `P` made `Q`, `-` for none | written |

- **Names:** each line names a declaration, field or variant by its
  current name; a rename's first name and a drop's are the kept ones.
  `D.f` is a field or variant `f` of declaration `D`.
- **Types** are spelled as the schema spells them (7.1), outermost
  first, a named type and a held record by their names: `T` with the
  kept text's names and `U` with the current's. A type ends where the
  grammar says, so a line needs no other marks.
- **An added field's value** is `none` for an `option`, which needs no
  more; otherwise `?`, asked of its author: no value is invented
  (temper's `draft/upgrades.md`, 4.3). The other forms of a value are
  the patch text's (section 11).
- **Order:** the changes of each declaration in the current text's
  order, then the declarations dropped, in the kept text's. Within a
  declaration: its rename; `reshape`, after which its parts are not
  compared, or `versioned`; then each current field or variant in
  order, with its rename, its change of type or payload, or its `add`;
  then the kept ones dropped, in order; then `order`. Proposals follow
  the same order.
- **`order`** lists the fields, or the variants, matched in both texts,
  by their current names in the current order, when their order differs
  from the kept text's; an added one takes its place from the current
  text, and needs no `order`.

### 10.3 The kinds

- **None needed:** every old value is a new value, byte for byte, and
  means what it did. A declaration added, dropped or renamed; a field
  or variant renamed; a variant added after every kept one; a `widen`
  with no integer wider. A declaration dropped this way leaves its old
  records to their store: a root's records are the store's to remove, a
  change of keys, which its owner writes (temper's `draft/upgrades.md`,
  4.3). The family keeps its version, and the command refreshes the
  kept text.
- **Derived:** every old value becomes exactly one new value from the
  change alone, its author giving at most an added field's value: a
  field added or dropped, fields or variants reordered, a variant added
  before a kept one (later tags move), a `widen` with an integer wider
  (its bytes move), a `wrap`, a record made `versioned` (the family's
  version put before it) or no longer (that version taken away).
- **Written:** anything else; a written function, its author's, builds
  the new value (section 11). A declaration with a written change is a
  written site, `written D` in a patch: its function builds the whole
  of its new value, and its other changes are listed beside it for that
  function's author.
- **Types compare level by level,** outermost first, a named type equal
  to the one it matches (10.1) whatever that one's own changes:
  - **equal,** no change: the same levels, bounds and end;
  - **`widen`:** `U` holds every value of `T` once its integers are
    written at their new widths. The levels are alike (a `list` for a
    `list`, an `option` for an `option`), each bound the same or
    larger, and at the end an integer the same or wider, `text` made
    `bytes` or `secret`, `bytes` and `secret` made one another, a
    `bytes` clause (`holds` or `pieces`) added or taken away, or a
    named type matched;
  - **`wrap`:** `U` is `T` with an `option` at one or more levels where
    `T` has none, and is otherwise a `widen` of it. An `option` of `T`
    stands for the outermost `option` of `U` it can, so the one added
    is the one `T` has no level for;
  - **`retype`,** anything else, written: a bound narrowed, an `option`
    taken away, a kind changed (an integer made a `duration` or a
    `bool`, or back), a `fixed` length changed, `bytes` made `text`
    (old bytes need not be UTF-8), a `bytes` clause naming another
    family, type or declaration, a named type matched to another.

### 10.4 The version

The kinds decide the family's version, and the diff says whether the
current text's header follows:

- **No patch needed,** when every change is none needed: the family
  keeps the kept text's version.
- **A patch needed,** when a change is derived or written: the family
  moves to the kept version plus one, once per landing (temper's
  `draft/upgrades.md`, 4.2).
- **The verdict:** `Kept` (no patch, the version kept), `Moved` (a
  patch, the next version), `Unmoved` (a patch, the version kept),
  `Needless` (no patch, the next version), `Wrong` (any other). The
  command acts on it (section 12).

A `versioned` record's layouts are compared without the version, which
is the family's: a version moved changes no record's line.

### 10.5 The interface

```rust
pub mod diff;  // skein_codegen::diff

pub fn diff(kept: &Schema, current: &Schema, renames: &[Rename])
    -> Result<Diff, DiffError>;          // skein_codec::schema::Schema

pub struct Diff {
    pub family: String,
    pub kept: u16,                       // the kept text's version
    pub current: u16,                    // the current text's version
    pub changes: Vec<Change>,            // in 10.2's order
    pub proposed: Vec<Rename>,           // proposed, not confirmed
}
pub struct Change { pub kind: Kind, pub op: Op }
pub enum Kind { NoneNeeded, Derived, Written }  // ordered so
pub enum Op {
    AddDeclaration { decl: String },
    DropDeclaration { decl: String },
    RenameDeclaration { old: String, new: String },
    Reshape { decl: String },
    Versioned { decl: String, versioned: bool },
    AddField { decl: String, field: String, ty: String, value: Fill },
    DropField { decl: String, field: String },
    RenamePart { decl: String, old: String, new: String },
    Widen { decl: String, field: String, kept: String, current: String },
    Wrap { decl: String, field: String, kept: String, current: String },
    Retype { decl: String, field: String, kept: String, current: String },
    Order { decl: String, parts: Vec<String> },
    AddVariant { decl: String, variant: String },
    DropVariant { decl: String, variant: String },
    Payload { decl: String, variant: String,
              kept: Option<String>, current: Option<String> },
}
pub enum Fill { Absent, Asked }          // `none`, `?`
pub enum Rename {
    Declaration { old: String, new: String },
    Part { decl: String, old: String, new: String },
}
pub enum Version { Kept, Moved, Unmoved, Needless, Wrong }
pub enum DiffError { Family, Rename(usize) }  // which rename

impl Diff {
    pub fn kind(&self) -> Option<Kind>;  // the most a change needs; None with none
    pub fn version(&self) -> Version;
    pub fn written(&self) -> Vec<String>; // the written sites, in order
}
```

- `Change`, `Kind`, `Fill` and `Rename` display as their lines and
  words: a change as its line (10.2), a kind as `none`, `derived` or
  `written`, a rename as its `rename` line.
- `Diff` displays as `family <family> <kept> to <current>`, then a
  line `<kind> <change>` per change and `proposed <rename>` per
  proposal.
- The types derive `Clone`, `Debug` and equality, and `Kind` its
  order. `DiffError::Family` is two texts of different families.

### 10.6 Tests

- **Step tests,** in skein-codegen, from both sides: for each line of
  10.2 and each kind it may have, a small pair of texts with that
  change beside the nearest that is not it (a variant appended and one
  inserted, a bound widened and one narrowed, a `wrap` and a `retype`),
  the whole output compared as text, so each line's form is pinned;
  proposals made and confirmed; each refused rename.
- **The story,** in `tests/codec`: a pair of fixture texts per kind,
  each holding every change of its kind and none of a higher one, and a
  pair with all three, their diffs kept as text beside them and naming
  each field.
- **Against the fingerprints:** every skein schema's diff against
  itself lists nothing; against its text with every name changed, given
  those renames, the renames alone, and without them, proposals for its
  declarations.

## 11. Patches

- **From the baseline on,** a landing that changes the layout of a
  stored family brings a patch beside it, and a store behind its
  binary applies the patches between, in order. Until the baseline,
  nothing asks for one.
- **Files, in the owner's crate beside its schemas:**
  - `schema/kept/<family>-v<N>.schema`: every version's text from the
    baseline on, never edited once landed but by the command's refresh
    of names (section 12); the latest has the current layout;
  - `patches/NNNN.patch`: the patch's text, numbered per owner from the
    baseline;
  - `patches/pNNNN.rs`: its author's written functions, if any;
  - `patches/mod.rs`, generated and drift-tested: the patches' texts,
    the dispatch of their written sites, and constants naming each kept
    version's fields and variants, so that a written patch that names a
    field wrongly does not build;
  - `patches/golden/NNNN/`: the old version's goldens through the patch,
    reviewed once and kept.
- **The text is data,** read at run time as step code, so jig runs
  temper's derived patches from temper's texts without naming temper:
  its patch language's version; the owner's level and its upstreams'
  levels when written (`owner jig 41 after smith 7 skein 3`); then for
  each family, its versions (`family jig-store-tasks 9 to 10`) and its
  operations per declaration (`add`, `drop`, `widen`, `wrap`,
  `rename`, `order`), its written sites with the refusals each may give,
  and marks for the caller (smith's windows that start fresh). Planning
  a patch checks that its operations turn the old layout into exactly
  the new one, or names the line that does not.
- **A machine that stops, never calls back.** A family's `Patcher`
  holds its kept schemas and planned patches, decodes an old record into
  a generic value, and walks it, building the new value in preorder in
  a reusable bounded buffer (lib's). It stops where its caller must act,
  at a written site or a held field, and resumes on the caller's
  answer: no function pointers, and jig runs temper's patches without
  naming temper.
- **Written sites are types,** not only roots, so a patch of a type
  reaches it in lists, variants and pieces. A written function reads
  the old value by section 8.1's accessors and builds the new by
  putting values, opening and closing nested ones, and carrying an
  untouched subtree through its derived conversion. It may refuse a
  record with a reason its patch's text declares; its caller names the
  record by its key, and the start stops.
- **Held fields:** at a `bytes` field that holds another family's
  record, the machine stops for its caller, which patches the held bytes
  by that family's machine, in the order the levels give, and hands the
  result back. A plan that changes nothing of its own family reaches
  held fields alone.
- **Versions inside:** every versioned record a patch reaches,
  root or nested, has its version rewritten.
- **Bounded:** a machine's heap is a worst case over its capacity (its
  versions, patches, a record's bytes and nodes), allocated once.

## 12. The command and its check

- **The command** (`skein-codegen patch <crate>`, aliased by each
  repository) diffs each current schema against its latest kept text.
  A change that needs no patch refreshes the kept text. Otherwise it
  keeps the current text as the next version, writes the next patch with
  the derived operations, a skeleton for the written ones and the
  values it asks of its author, regenerates `patches/mod.rs`, and writes
  the patch's golden test.
- **Renumbering:** the same command moves a patch whose number `land`
  found taken to the next free one, deriving again against the new
  latest kept text where the same family moved meanwhile, and keeping
  its author's values and written code.
- **The check** (`skein_codegen::patch::check`) finds a current schema
  whose layout differs from its latest kept text with no patch, a kept
  text whose names lag the current ones, and a landed patch or kept text
  changed, and names the command. The command calls it; a gate calls it
  from the baseline on.

## 13. Open questions

- **Left to the milestone that builds each:** the patch text's
  grammar, a value's forms among them, the checks of planning and the
  `Patcher`'s interface, with lib's reusable buffer (section 11); the
  command's options and the check's findings (section 12).
- **A held type renamed in its own family** is a `retype` in its
  holder's diff, written: the holder's diff reads its own family's two
  texts alone, so it cannot see the held family's renames. jig's
  mission, which patches held fields across families, gives the diff
  those renames.
- **Composing patches:** a record k versions behind is patched in k
  passes; plans may compose later, behind the same interface.
- **Maps and sets:** sorted lists whose keys are checked unique, when a
  record needs one.
- **Smaller integers:** variable-width encodings, if a payload shows that
  fixed widths cost too much.
- **JSON from schemas:** the same generator writing JSON codecs over
  `skein-json` (json.md, section 8).
- **Other languages:** a generator for a peer not built on skein, if one
  appears. The schema is plain enough for it.

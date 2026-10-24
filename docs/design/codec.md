# Codecs from schemas

Provisional, 2026-10-06, revised 2026-10-24. How an application protocol
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
  version. A reader reads the current version and refuses another. The
  baseline is the first release (section 5): until it, no version moves
  and nothing is translated; from it on, kept versions are read as
  generic values and patched to the current one (section 11).
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

- **The baseline is the first release.** Until it, a family's version
  keeps its number: a change a reader would notice changes the schema,
  its generated code and its golden bytes in place. A record an older
  build kept is then not told apart by its version, so nothing is kept
  across builds that change its family: a store from an older build is
  started afresh. Versions move at a release, not at a landing.
- **From the baseline on, any change a reader would notice is a new
  version:** a new schema, whose generated code replaces the old. There
  is one version at a time, and no promise of stability to anyone
  outside the family's owner.
- **A reader reads one version.** It reads a versioned record's version
  first, with `skein-codec`, and refuses another with the problem
  *another version*. Its generated code reads the current version only.
- **Kept versions are patched, from the baseline on.** From it, a
  family's owner keeps each version's schema as text, and a kept record
  of an older version is read as a generic value (section 8) and patched
  to the current version (section 11) before the generated code reads
  it. A version then names one layout. Until the baseline, nothing is
  translated. Rolling upgrades, a release running beside the one before
  it, are designed with the first release: what a channel's sides speak
  across it (channel.md, 5.2) and the order a store's readers move in.
- **A writer writes its current version only.**
- **On a channel,** the agreed version picks the code for its bodies
  (channel.md, section 5.2). Bodies carry no version of their own.
- **Golden bytes:** until the baseline, a family has one set, its
  current schema's, which a change moves in place. From it on, a
  version's golden bytes stand for it, and a change that moves them is a
  new version.

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
  kept text (section 12).
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
  function's author. A `retype` may instead be its field's site,
  `written D.f` (11.3).
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
  command acts on it (section 12), deciding whether a patch is needed
  by 12.2's third step: `version()` alone holds where the kept text is
  not frozen, since under a frozen one a change none needed may need
  a patch.

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
    of names, while no landed patch names it (section 12); the latest
    has the current layout;
  - `patches/NNNN.patch`: the patch's text (11.1), numbered per owner
    from the baseline, its number the owner's level once it applies;
  - `patches/pNNNN.rs`: its author's written functions, if any (11.5);
  - `patches/context.rs`: the owner's `Context`, what its driver passes
    each written function (11.5), written once by the command, empty,
    and its owner's to fill and keep from then on;
  - `patches/mod.rs`, generated and drift-tested: the patches' texts,
    the dispatch of their written sites, and constants naming each kept
    version's fields and variants and each patch's refusals, so that a
    written patch that names one wrongly does not build. Its drift test
    regenerates it alone from the kept texts and the patches: the
    author's `pNNNN.rs` and `context.rs` are outside it;
  - `patches/golden/NNNN/`: the old version's goldens through the patch,
    reviewed once and kept.
- **The text is data,** read at run time as step code, so jig runs
  temper's derived patches from temper's texts without naming temper:
  its language's version; the owner's level and its upstreams' levels
  when written; marks for the caller; then for each family it changes,
  its versions, section 10.2's lines as the diff prints them, the
  values its author gives added fields, and its written sites with the
  refusals each may give (11.1, 11.2).
- **Planned before it runs.** `plan` checks that a family's lines turn
  its old layout into exactly the new one, or names the line that does
  not (11.3), and keeps what the machine needs: which old declaration
  becomes which, each field's source, each value's encoding.
- **A machine that stops, never calls back.** A family's `Patcher`
  holds its kept schemas and planned patches, decodes an old record into
  a generic value, and walks it, building the new value in preorder in
  lib's reusable `Buffer` (lib.md, section 6). It stops where its caller
  must act, at a written site or a held field, and resumes on the
  caller's answer: no function pointers, and jig runs temper's patches
  without naming temper (11.4).
- **Written sites are types,** not only roots, so a patch of a type
  reaches it in lists, variants and options; or one field of a record.
  A written function reads the old value by section 8.1's accessors and
  builds the new by putting values, opening and closing nested ones, and
  carrying an untouched part through its derived conversion. It may
  refuse a record with a reason its patch's text declares; its caller
  names the record by its key, and the start stops (11.5).
- **Held fields:** at a `bytes` field that holds another family's
  record, the machine stops for its caller, which patches the held bytes
  by that family's machine, in the order the levels give, and hands the
  result back, or carries them as they are. A plan that changes nothing
  of its own family, an identity plan, reaches held fields alone.
- **Versions inside:** every versioned record a patch reaches,
  root or nested, has its version rewritten.
- **Bounded:** a machine's heap is a worst case over its capacity (its
  versions, patches, texts, a record's bytes and nodes), allocated once;
  a record one version behind allocates nothing (11.7).

### 11.1 The text

```
0041.patch:

patch 1                                   # the language's version
owner jig 41 after smith 7 skein 3        # the owner's level, its upstreams'
mark fresh-windows                        # for the caller

family jig-store-tasks 9 to 10
rename Task.owner assignee
widen Task.attempts u8 u32
add Task.priority u8 0
add Task.note option text 64 none
add Task.score u32 written                # a site gives it
order Task id title assignee state attempts
written Task.score                        # site 0
refuses unscored "a done task without attempts"   # refusal 0
add State.paused
```

```
patch     = "patch" language NL owner NL mark* block*
owner     = "owner" word level ["after" (word level)+]
mark      = "mark" word NL
block     = "family" family version "to" version NL (line NL)*
line      = operation | site | refusal
operation = "add" name | "drop" name | "rename" name name
          | "reshape" name | "versioned" name | "unversioned" name
          | "add" part [type value] | "drop" part | "rename" part name
          | "widen" part type type | "wrap" part type type
          | "retype" part type type
          | "order" name name+ | "payload" part record record
site      = "written" name | "written" part
refusal   = "refuses" name text
part      = name "." name
record    = name | "-"
value     = "?" | "written" | "none" | "some" value | "[]"
          | word | text
```

- **Types** are section 7.1's `type`, ending where its grammar says.
- **Tokens:** schema words (7.1), and `.` `?` `[` `]` as marks; a
  text, `"` then bytes of UTF-8 other than `"`, `\` and control bytes,
  then `"`, a `#` among them one of its bytes; `#` elsewhere starts a
  comment to the end of the line. Spaces and tabs separate tokens; a
  line's end ends a statement, and blank lines are none. Any other byte
  outside a comment is refused, before the grammar is read. `[]` is `[`
  then `]`.
- **Numbers:** a `language`, `version` and `level` are decimal words,
  a `u16`, `u16` and `u32`. The language is 1.
- **Lines** count from 1 as in a schema (7.1); a refusal names the line
  of its token, and a change a block lacks (11.3) names the block's
  `family` line.
- **The header:** the owner's level is the patch's number; `after`
  lists each upstream's level when the patch was written (temper's
  `draft/upgrades.md`, 4.4), none for an owner without upstreams. A
  mark is a word the language gives no meaning: its caller's, such as
  smith's `fresh-windows` (temper's `draft/upgrades.md`, 7.2).
  `header` reads it alone (11.6), so that a caller reads the levels and
  marks of a patch that changes none of its families.
- **A block per family the patch changes,** once each, its versions
  kept ones, the second the first plus one. Its operations are section
  10.2's lines as the diff prints them without their kind, and a type
  is spelled as there: with the old text's names in `widen`, `wrap` and
  `retype`'s first type and `payload`'s first record, the new text's
  elsewhere, an added field's among them. `add part` and `drop part`
  are a field's when the part is a record's, a variant's when an
  enumeration's.
- **Sites and refusals are numbered** each in text order across the
  whole text, from 0, so a patch's code names them by its number and
  theirs (11.5). A `refuses` line belongs to the site line it follows;
  a site may give several, and each refusal's name is the patch's once.
- **Other families' blocks** are read for their grammar alone: each
  family's machine plans its own block (11.3).

### 11.2 Values

An added field's value is the diff's `?` answered by its author, or an
option's `none` kept or overridden. Each value is a form its type
takes, checked as the new schema's decoder checks it, and encoded once,
when the patch is planned:

| Type | Its value |
|---|---|
| `u8`, `u16`, `u32`, `u64`, `duration` | a decimal word within its width; a duration's nanoseconds |
| `bool` | `true` or `false` |
| `fixed N` | `0x` and exactly 2N lowercase hex digits |
| `bytes N` | `0x` and at most 2N lowercase hex digits, in pairs; `0x` alone is empty |
| `bytes N pieces T` | `0x` alone, no pieces |
| `bytes N holds F T` | none: a written site |
| `text N` | a text of at most N bytes |
| `secret N` | none: a written site |
| `list N T` | `[]`, the empty list |
| `option T` | `none`, or `some` and a value of `T` |
| an enumeration | a variant without a record, by its name |
| a record, or a variant with one | none: a written site |

- **`?` is refused** when planned: no value is invented (temper's
  `draft/upgrades.md`, 4.3). A field old records hold no evidence for
  is an `option`, and `none` for them.
- **`written`** says a site gives the value: `written D.f`, or
  `written D` when the whole declaration is written. So a value read
  per record from other data, or an option whose `none` would be wrong,
  is written.
- **The words `?`, `written`, `none` and `some`** are read as such
  whatever the type, so a variant so named is given by a site.
- **No secret in a text:** a secret's value is always written, so no
  credential is kept in a patch.

### 11.3 Planning

`plan` reads the text and plans its block for the machine's family,
against the two kept texts its versions name.

- **Matching,** as the diff's (10.1), with the block's renames as the
  confirmed ones: declarations, then the fields of matched records and
  the variants of matched enumerations.
- **Every change has its line,** and every line its change, so the
  lines turn the old layout into exactly the new one:

  | The change | Its line |
  |---|---|
  | a declaration, field or variant unmatched in the new text, or in the old | `add`, or `drop` |
  | a variant added after every kept one | `add D.v`, or none |
  | a matched declaration of the other shape | `reshape D`, with `written D`; its parts are not compared |
  | a record made versioned, or no longer | `versioned D`, or `unversioned D` |
  | a matched field's type, by 10.3 | none when equal; `widen`, or none when no integer is wider; `wrap`; `retype`, with `written D` or `written D.f` |
  | a variant's record not the match of its old one | `payload D.v P Q`, with `written D` |
  | a variant dropped | `drop D.v`, with `written D` |
  | matched parts in another order | `order D`, naming them in the new order |

  A line's names and types must spell what the two texts hold. The two
  implicit changes, a variant added after every kept one and a `widen`
  with no integer wider, may have their lines or not; every other
  change needs its own, renames included.
- **A site** names a declaration of the new text matched to an old one
  (`written D`), or a field of such a record (`written D.f`): an added
  field whose value is `written`, or a matched field, whose derived
  conversion the site replaces. A declaration that is a site has no
  field site; its function gives each field, an added one with a value
  among them (11.5). The diff names a declaration's site (10.3); a
  `retype` may have its field's instead.
- **What a plan keeps:**
  - each old declaration's new one, and each new one's old, if any;
  - per new field, its old field, its encoded value or its site; per
    old variant, its new one;
  - per old variant and field, whether it converts derived: not a
    variant dropped or whose `payload` changed, a `retype`'s field, a
    part of a reshaped declaration, nor a record with an added field
    whose value is `written`;
  - per declaration, whether a site or a held field is in its reach,
    and whether its conversion copies its bytes as they are: every part
    matched in place (none added, dropped or moved, no tag moved), every
    type equal or widened with no integer wider, and no versioned
    record, held field or site in reach;
  - each site's refusals.
- **Pieces are not opened:** a `pieces` field's items are copied, so
  pieces of a type whose conversion is not a copy are refused
  (`Pieces`). Opening them is jig's mission's, which patches nested
  values (section 13).
- **An identity plan** is a kept version planned to itself: every
  declaration its own, nothing written, a versioned record's version
  unchanged. It copies a record but stops at its held fields, and
  refuses pieces whose type reaches one (`Pieces`).

`PatchError` names the line and the token (`at`, a span of the patch's
text) and what is wrong:

| `PatchWrong` | Refused |
|---|---|
| `Long` | a text past the capacity's `text` |
| `Character` | a byte outside a token, a text or a comment |
| `Expected(Expected)` | a token the grammar does not take next: the language, owner, level, family, version, `to`, a line, a name, a type, a value, a text, a record, the line's end |
| `Number` | a decimal past its width |
| `Language` | a language other than 1 |
| `Twice` | a block, the same operation on the same target, a site, or a refusal's name given again; a plan from a version already planned from |
| `Unkept` | a version the machine does not keep |
| `Version` | a second version other than the first plus one |
| `Unknown` | a name its text lacks: the old text's for `drop` and a rename's first name, the new text's otherwise |
| `Mismatch` | a line whose change the texts do not hold: types or records spelled otherwise, a `widen` that is not one by 10.3, an `order` other than the new one, a shape or `versioned` unchanged, a part renamed in a declaration not matched |
| `Missing(Missing)` | a change without its line, or a written change without its site: the operation and the declaration, field or variant (in the old text for a drop, the new otherwise) |
| `Asked` | a `?` |
| `Value` | a value its type does not take (11.2), past its width or bound, or not UTF-8 |
| `Site` | a site naming an added declaration or a field of one, a field site in a written declaration, a `written` value without its site |
| `Stray` | a `refuses` line after no site |
| `Pieces` | pieces of a type whose conversion is not a copy, or, in an identity plan, that reaches a held field |
| `Room` | past the capacity's patches |

### 11.4 The machine

```
start(plan, root, record)       decode the old record by the plan's old text
next()                          walk the old value, building the new one
  -> Written { site, at }       the caller puts the site's one value, then next()
  -> Held { at, family, name }  the caller puts the field's one value, then next()
  -> Done                       the new record is whole
finish()                        its bytes, until the next start
next_pass(plan, root)           the next plan, over those bytes
```

- **The walk** builds the new value in preorder, each new part from its
  source in the old value, by its plan:
  - a record's fields in the new order, each from its matched old field
    by the conversion of its type, or its added value; the new version
    first when it is versioned, the old version passed over;
  - an enumeration's new tag, the match of its old variant, then the
    conversion of its record;
  - a list's count, then each item's conversion; an option's presence,
    then its value's;
  - a type's conversion by 10.3, level by level: equal and widened
    levels as they are, a wrapped level's presence put before it, an
    integer at its new width, a named type by its declaration's
    conversion;
  - a declaration whose conversion copies, as its range of the old
    bytes, at once.
- **It stops** at each old value of a written declaration (`at` is
  that value), at each written field (`at` is the old record it
  belongs to), and at each held field, a `bytes` that holds the same
  type of another family in both texts (`at` is its old bytes; the
  family and type are spans of the old text, spelled by
  `Patcher::spelled`).
- **The caller's answer is one value,** of the new type the walk
  stands at, put before `next()`: a written function's (11.5); for a
  held field, the held record patched by its family's machine and put
  with `put_content`, or the old bytes carried, `carry(old, at)`, where
  that family does not move.
- **Halts end the record.** A `Halt` is the record's end, the start's
  to report by the record's key: refused by its old schema (`Decode`),
  by a written function with a declared reason (`Refused`) or another
  (`Undeclared`), an old node misread (`Misread`), a build the new type
  refuses (`Misbuilt`, `Built`), past the capacity (`Room`), a call out
  of turn (`Order`), a site its caller has no function for (`NoSite`).
  The next `start` begins afresh.

### 11.5 Written functions

```rust
// patches/p0041.rs: the command writes the skeleton, the author the body
use super::Context;
use super::kept::jig_store_tasks::{v9, v10};
use super::refusal::p0041::UNSCORED;
use skein_codec::patch::{Build, Halt, Old};
use skein_codec::value::Node;

/// Task.score, 9 to 10, a field (`written Task.score`): ten a try; a
/// done task without attempts refused.
pub fn task_score(old: &Old, new: &mut Build, at: Node, _cx: &mut Context) -> Result<(), Halt> {
    let attempts = old.uint(old.field(at, v9::task::ATTEMPTS)?)?;
    let state = old.field(at, v9::task::STATE)?;
    if attempts == 0 && old.tag(state)? == v9::state::DONE {
        return Err(new.refuse(UNSCORED));
    }
    new.put_uint(attempts.saturating_mul(10))
}

/// Shape, 9 to 10, a declaration (`drop Shape.square`, `written Shape`):
/// a square made a rectangle, the other variants carried.
pub fn shape(old: &Old, new: &mut Build, at: Node, _cx: &mut Context) -> Result<(), Halt> {
    if old.tag(at)? != v9::shape::SQUARE {
        return new.carry(old, at);
    }
    let square = old.payload(at)?.ok_or(Halt::Misread(at))?;
    let side = old.field(square, v9::square::SIDE)?;
    new.open_variant(v10::shape::RECTANGLE)?;
    new.carry(old, side)?;
    new.carry(old, side)?;
    new.close()
}
```

- **Two halves:** at a stop, `Patcher::parts()` gives the old value to
  read and the new one to build, `(&Old, &mut Build)`, borrowed apart so
  that a function reads one while it builds the other. Its owner's
  dispatch passes them with the stop's `at` and its `Context`. A
  function's own state is its arguments.
- **Beyond its site,** a function reads two ways:
  - **the same record,** outside the site's part, through `Old`: `root()`
    is the record's root, and `parent(node)` the node holding a node,
    none for the root, each a link the decoded value keeps. So a site
    nested in a list reads the record around it, and an enclosing
    declaration needs no site of its own for it;
  - **all else,** through `Context`, its owner's type: the record's key,
    which its driver sets before each `start`, and whatever other data
    the driver reads for it, such as another record's value. It holds
    no references, and a function changes nothing in it that a later
    record reads.
- **Reading the old:** section 8.1's accessors on `Old`, each misread a
  `Halt::Misread`: `field`, `items` and `next_item`, `count` (a list's
  items, so that a function walks them in a `for`), `some`, `tag`,
  `payload`, `uint`, `boolean`, `content`, `expose`; and `root` and
  `parent`.
- **Building the new,** in preorder, each step checked against the
  new type the build stands at, as the new schema's decoder checks it:
  - `put_uint`, `put_bool`; `put_content` for `fixed`, `bytes`, `text`
    and `secret` bytes, and `put_content_of` for an old node's own
    (checked as the new type's: width, bound, UTF-8), never a held
    field's (`Misbuilt`: `carry` it at its stop);
  - `put_none`; `put_variant`, a variant without a record;
  - `open`: a record (its version put when versioned), a list (its
    count put at its `close`), an option's value; `open_variant`: a
    variant and its record at once; `close`: the innermost, with every
    field given, a list's count, an option's one value;
  - `carry(old, node)`: one value, the derived conversion of an old
    part, when its type and the new one compare equal, widened or
    wrapped (10.3), named types matched by the plan.
- **What carries:** a part that converts derived (11.3), as the
  variant each record holds decides; `carry` of one that does not halts
  `Misbuilt` with the path of its first such part. A carried part
  converts in place, at once, so one whose type has a site or a held
  field in reach, through any variant, halts `Misbuilt` too, at its own
  path and before it puts anything, whichever variant the record holds:
  its author learns it from any record, and builds that part, until
  carried parts stop in their turn (section 13). A part that does not
  convert derived is a site or lies in a site's declaration (11.3), so
  the first rule meets one only where a declaration's site carries its
  own value. At a stop, `carry(old, at)` is the stop's own: at a
  declaration's site, the derived conversion of the variant at hand,
  never the site again; at a held field, its bytes as they are. At a
  field's site `at` is its record, and the old field `old.field(at,
  …)`.
- **One value per stop:** a field site's type, or the declaration's;
  `next()` refuses a stop answered with more or less (`Order`).
- **Refusals** are a site's declared reasons: `new.refuse(r)` gives the
  `Halt::Refused(r)` to return, or `Undeclared(r)` for a reason its site
  does not declare, and ends the record; `Patcher::refusal(r)` spells
  its text for the caller's report.
- **Names by constants,** never by text: `patches/mod.rs` (section 12)
  generates, per family and kept version, a module per declaration of
  its `FieldIx` and `VariantIx` constants, its parts' names in upper
  case (`kept::jig_store_tasks::v9::task::ATTEMPTS`), and per patch its
  refusals' (`refusal::p0041::UNSCORED`), so a misspelt name fails to
  build; the generator refuses two names that spell one constant. The
  dispatch matches a site's patch, then its index, in nested `match`es,
  each with a last arm `Err(Halt::NoSite(site))`:

  ```rust
  match site.patch() {
      41 => match site.index() {
          0 => p0041::task_score(old, new, at, cx),
          _ => Err(Halt::NoSite(site)),
      },
      _ => Err(Halt::NoSite(site)),
  }
  ```

### 11.6 The interface

```rust
// skein-lib (lib.md, section 6)
pub struct Buffer { /* bytes, reused, bounded by a capacity */ }

// skein_codec::value, beside 8.1's readers
impl Value {
    pub fn count(&self, list: Node) -> Result<u32, Misread>;           // a list's items
    pub fn parent(&self, node: Node) -> Result<Option<Node>, Misread>; // none for the root
}

// skein_codec::patch
pub const LANGUAGE: u16 = 1;

pub struct Capacity {
    pub versions: u32, // kept texts
    pub patches: u32,  // plans, identity plans among them
    pub text: u32,     // the longest kept or patch text, at most MAX_TEXT
    pub bytes: u32,    // a record's bytes, old or new
    pub nodes: u32,    // a record's nodes, old or new
}
pub struct Patcher { /* an Old and a Build */ }
pub struct Old { /* the old value */ }
pub struct Build { /* the kept schemas, the plans, the new value */ }
pub struct PlanIx(u32);
pub struct Versions { pub from: u16, pub to: u16 }
pub struct Site { /* the patch's number, the site's index */ }
impl Site {
    pub const fn new(patch: u32, index: u32) -> Site;
    pub const fn patch(self) -> u32;
    pub const fn index(self) -> u32;
}
pub struct RefusalIx(u32);           // in the patch's text order
impl RefusalIx {
    pub const fn new(index: u32) -> RefusalIx;
    pub const fn index(self) -> u32;
}
pub struct Header { /* spans of its text */ }

pub enum Step {
    Written { site: Site, at: Node },
    Held { at: Node, family: Span, name: Span },
    Done,
}
pub enum Halt {
    Decode(Refusal),                 // the old record, by its schema (8.1)
    Refused(RefusalIx),              // a written function's declared reason
    Undeclared(RefusalIx),           // a reason its site does not declare
    Misread(Node),                   // an old node read as what it is not
    Misbuilt(Path),                  // a build step the new value there is not
    Built(Problem),                  // a value its new type's decoder refuses
    Room,                            // past the capacity's bytes or nodes
    Order,                           // out of turn: no run, the stop's one value not put
    NoSite(Site),                    // the caller has no function for the site
}
pub enum KeepError { Schema(SchemaError), Family, Twice, Room }
pub struct PatchError { pub line: u32, pub at: Span, pub wrong: PatchWrong }
pub enum PatchWrong { /* 11.3's table */ }
pub enum Expected {
    Language, Owner, Level, Family, Version, To, Line,
    Name, Type, Value, Text, Record, End,
}
pub struct Missing { pub op: Op, pub target: Target }
pub enum Op {
    Add, Drop, Reshape, Versioned, Unversioned, Widen, Wrap, Retype,
    Order, Payload, Written,
}
pub enum Target { Declaration(DeclIx), Field(FieldIx), Variant(VariantIx) }

pub fn header(patch: &[u8]) -> Result<Header, PatchError>; // its header lines alone
impl Header {
    pub fn owner(&self) -> Span;
    pub fn level(&self) -> u32;
    pub fn after(&self, patch: &[u8], upstream: &[u8]) -> Option<u32>;
    pub fn marked(&self, patch: &[u8], mark: &[u8]) -> bool;
}

impl Patcher {
    pub fn new(capacity: Capacity) -> Patcher;
    pub fn worst_case_heap(capacity: Capacity) -> Option<u64>;
    pub fn keep(&mut self, text: &[u8]) -> Result<u16, KeepError>; // its version
    pub fn schema(&self, version: u16) -> Option<&Schema>;
    pub fn plan(&mut self, patch: &[u8]) -> Result<Option<PlanIx>, PatchError>; // none: no block of its family
    pub fn identity(&mut self, version: u16) -> Result<PlanIx, PatchWrong>; // Unkept, Twice, Pieces, Room
    pub fn plan_from(&self, version: u16) -> Option<PlanIx>; // a patch's, not an identity
    pub fn versions(&self, plan: PlanIx) -> Versions;
    pub fn counterpart(&self, plan: PlanIx, old: DeclIx) -> Option<DeclIx>;
    pub fn source(&self, plan: PlanIx, new: DeclIx) -> Option<DeclIx>;

    pub fn start(&mut self, plan: PlanIx, root: DeclIx, record: Box<[u8]>) -> Result<(), Halt>;
    pub fn next(&mut self) -> Result<Step, Halt>;
    pub fn parts(&mut self) -> (&Old, &mut Build);
    pub fn finish(&mut self) -> Result<&[u8], Halt>;   // until the next start
    pub fn next_pass(&mut self, plan: PlanIx, root: DeclIx) -> Result<(), Halt>;
    pub fn spelled(&self, span: Span) -> &[u8];        // the running plan's old text
    pub fn refusal(&self, refusal: RefusalIx) -> Option<&[u8]>; // its text, in the running plan
}

impl Old {                           // 8.1's readers, each Misread a Halt::Misread
    pub fn root(&self) -> Node;
    pub fn parent(&self, node: Node) -> Result<Option<Node>, Halt>;
    pub fn field(&self, record: Node, field: FieldIx) -> Result<Node, Halt>;
    pub fn items(&self, list: Node) -> Result<Items, Halt>;
    pub fn next_item(&self, items: &mut Items) -> Option<Node>;
    pub fn count(&self, list: Node) -> Result<u32, Halt>;
    pub fn some(&self, option: Node) -> Result<Option<Node>, Halt>;
    pub fn tag(&self, enumeration: Node) -> Result<VariantIx, Halt>;
    pub fn payload(&self, enumeration: Node) -> Result<Option<Node>, Halt>;
    pub fn uint(&self, node: Node) -> Result<u64, Halt>;
    pub fn boolean(&self, node: Node) -> Result<bool, Halt>;
    pub fn content(&self, node: Node) -> Result<&[u8], Halt>;
    pub fn expose(&self, node: Node) -> Result<&[u8], Halt>;
}

impl Build {                         // the new value, in preorder
    pub fn put_uint(&mut self, value: u64) -> Result<(), Halt>;
    pub fn put_bool(&mut self, value: bool) -> Result<(), Halt>;
    pub fn put_content(&mut self, bytes: &[u8]) -> Result<(), Halt>;
    pub fn put_content_of(&mut self, old: &Old, node: Node) -> Result<(), Halt>;
    pub fn put_none(&mut self) -> Result<(), Halt>;
    pub fn put_variant(&mut self, variant: VariantIx) -> Result<(), Halt>;
    pub fn open(&mut self) -> Result<(), Halt>;
    pub fn open_variant(&mut self, variant: VariantIx) -> Result<(), Halt>;
    pub fn close(&mut self) -> Result<(), Halt>;
    pub fn carry(&mut self, old: &Old, node: Node) -> Result<(), Halt>;
    pub fn refuse(&mut self, refusal: RefusalIx) -> Halt;
}
```

- **Indexes** are each version's own: `start`'s root, `Step`'s `at`
  and `Old`'s fields and variants are the plan's old text's; `Build`'s
  variants its new text's. A record k versions behind runs k plans:
  `plan_from` finds each, `source` names the root in the one before
  (and `counterpart` in the one after), and `next_pass` starts each
  after the first over the last `finish`'s bytes.
- **A machine per family.** `keep` refuses another family's text
  (`Family`), a version kept already (`Twice`), and past the
  capacity's versions or text (`Room`). Plans name kept versions, so
  texts are kept before the patches that use them are planned. `plan`
  reads the whole text's grammar and plans its family's block, if the
  text has one.
- The plain types derive as section 7.3's; `Patcher`, `Old` and `Build`
  derive `Debug` alone, and print no record's bytes.

### 11.7 Bounds

- **Allocated once:** `new` allocates the run's tables at the capacity
  (the old value's entries, the output, the walk's and the build's
  stacks); `keep` a schema's tables, `plan` a plan's, each once.
- **Per record:** the old record's box is its caller's, moved in by
  `start` and freed by the next. A record one version behind allocates
  nothing; each further pass allocates its record's box, the last
  `finish`'s bytes at their exact length, and a held field's bytes are
  its caller's box for its family's machine.
- **The worst case:** `worst_case_heap(capacity)` is the run's tables,
  two records' bytes, and the capacity's versions of kept texts and
  patches of plans, each at the capacity's `text`, parsing's and
  planning's scratch included, in checked arithmetic. jig's budget for
  patching is its map's twice plus the machine of each family it keeps:
  a held field runs its family's machine while its holder's waits.
- **Bounded walks:** each `next` turns at most a bound the capacity's
  nodes give, and no step recurses: the walk and the build, `carry`'s
  among them, are stacks bounded by the record's nodes. A record past
  the capacity's bytes or nodes, old or new, halts with `Room`, or with
  `Decode(Refusal::Room)` for the old value's nodes.

### 11.8 Tests

Step tests, in skein-lib and skein-codec, and the story in
`tests/codec`; no world.

- **The buffer** (lib.md, section 10): back-patching, reuse after
  `clear` and `truncate`, a put past the capacity refused whole, the
  exact copy out.
- **The text:** each line's form pinned, and each refusal from both
  sides, naming its line and token; the header read alone.
- **Planning:** each change's check from both sides, the line that
  turns the layout beside the nearest that does not; each value form
  and its refusals; no invention (an added field that is not an option
  needs a value or a site; `?` refused), sites required by written
  changes, the implicit changes with and without their lines.
- **The machine:** each derived operation over generic values; a
  written declaration and a written field with their functions (reads,
  puts, carry, a carry refused for a part that does not convert derived
  and for one with a site in reach, a declared refusal and an
  undeclared one); versions rewritten in nested records; held fields
  answered both ways; an identity plan; a record two versions behind;
  each `Halt`. Every result decodes by the new schema as a generic
  value and equals its expected bytes; numbers are carried as they are.
- **A property,** in the fuzzy suite: values drawn from the `every`
  fixture's old schema (8.3's seeded generator) through a derived patch
  decode by the new schema, each carried field equal to its old one;
  an identity plan, and a patch without lines between layouts equal but
  for the version, give back the bytes, but for the versions.
- **The story:** a fixture patch under `tests/codec` with every derived
  operation and a written site, run over the `every` fixture's goldens,
  its results kept as goldens beside it, the shape of `patches/golden/`,
  and decoded by the new schema's generated code.
- **Memory:** the heap the machine holds stays within
  `worst_case_heap`, counted by the counting allocator through keeping,
  planning and many records.

## 12. The command and its check

- **One command keeps a crate's patches,** `skein-codegen patch
  <crate>`, ordinary Rust in skein-codegen's `patch` module, which each
  repository may alias (`cargo xtask patch`, in its own mission). It
  diffs each family's current text against its latest landed kept text
  (section 10), and per family refreshes the kept text, keeps renames
  waiting, or writes the branch's patch: its lines, the values it asks,
  a skeleton for its written sites, the next kept text, `patches/mod.rs`
  and the patch's goldens. It ends by calling the check.
- **The base is the branch's own,** so that a branch not yet rebased
  sees main as it was when it branched, never main's newer patches
  without their schemas. Once rebased, its base is main's tip, so a
  renumbering (12.6) and a gate after a rebase see what main holds.
- **Landed is what the base holds.** The files under the crate's
  `patches/` and `schema/kept/` as they are at the commit the branch is
  built on, `git merge-base HEAD origin/main` unless `--base` names
  another, are landed, read by the binary and passed to the library; whatever
  else the tree holds there is the branch's. A landed patch, its
  goldens and a kept text a landed patch names never change; the
  branch's patch is derived again on every run, so that one command
  serves a first write, a schema edited again, and a renumbering.
- **One patch per landing.** A landing moves each family's version at
  most once (temper's `draft/upgrades.md`, 4.2), so a branch holds at
  most one patch of its own, numbered after the last landed one, with a
  block per family it moves.
- **The check** (`check`) finds, without writing, what the command would
  change or refuse, each finding naming the command or the fix. The
  command calls it; a gate calls it from the baseline on, none before.

### 12.1 The crate's files

| Path | What | Whose |
|---|---|---|
| `schema/<family>-v<N>.schema` | the current texts: of each family, the greatest version there | the author's |
| `schema/kept/<family>-v<N>.schema` | every kept version's text from the family's first (11) | the command's |
| `schema/kept/<family>.renames` | the renames waiting for the family's next patch (12.4), `rename` lines | the command's |
| `patches/NNNN.patch` | the patch's text (11.1) | the command's lines, the author's values, sites and refusals |
| `patches/pNNNN.rs` | its written functions, for a patch with sites (11.5) | the command's skeleton, then the author's |
| `patches/context.rs` | the `Context`, written once, empty, deriving `Default` | the author's |
| `patches/mod.rs` | generated from the kept texts and the patches alone (12.3) | the command's |
| `patches/golden/NNNN/<family>/` | each old golden through the patch (12.3) | the command's or the golden test's, reviewed |

- **The owner's crate** includes `patches/mod.rs` as a module of its
  library (`#[path = "../patches/mod.rs"] pub mod patches;`), and
  depends on skein-codec, and on skein-codegen in its tests.
- **The baseline is the first release** (section 5). A crate with no
  kept text, landed or its own, is before its baseline, and a run there
  is refused unless given `--baseline`, which its owner passes at that
  release: the guard is the crate's files, and the command knows no
  release. That first run keeps each family's current text as its first
  kept version and writes no patch. A family new to a crate past its
  baseline, or whose kept texts are all the branch's, is kept afresh in
  the same way, with no flag.
- **Fixtures stand for families past their baseline:** `tests/codec`'s
  `every` texts and `tests/codec-patches`' families belong to no build
  that stores, sends or embeds them, and move their versions as their
  stories need. Section 5's rule binds the families a build stores,
  sends or embeds.

### 12.2 A run

For each family, in name order, with `K` its latest landed kept text at
version `k` (none for a family new to the base) and `C` its current
text at version `c`:

1. **Matching** by section 10.1, the renames confirmed being those
   waiting (12.4), those given (`--rename`), and the `rename` lines of
   the branch's patch for the family.
2. **Proposals in rounds:** the diff is run again with its proposals
   taken as confirmed until none is new, at most once per declaration,
   so that renames a level of references down are proposed in the same
   run. Proposals are reported for the author to confirm; only
   confirmed renames reach a file.
3. **A patch is needed** when a change is derived or written (10.3), or
   when `K` is frozen and a change other than a rename or a declaration
   dropped is none needed: a frozen text cannot take a declaration, a
   variant or a bound that records of its version may now hold. `K` is
   frozen when a landed patch names its version. This departs from
   temper's `draft/upgrades.md`, 4.3, where such a change keeps its
   version; the alternative, a frozen text refreshed where every landed
   patch naming it still plans and keeps its goldens, would serve a
   variant appended and a bound widened alone, at the cost of landed
   kept texts that never change.
4. **The verdict,** 10.4's, from whether a patch is needed:
   - `Kept`: the version kept, no patch needed. A kept text not frozen
     is refreshed (made a copy of `C`); a frozen one keeps renames
     waiting (12.4);
   - `Moved`: the version moved by one, a patch needed. The family's
     block is written in the branch's patch, and `C` kept as
     `kept/<family>-v<c>.schema`;
   - `Unmoved`, `Needless` and `Wrong`: refused for the family, which
     keeps its files as they are, with the fix: move the version to
     `k + 1`, keep it at `k`, or say which (12.5).
5. **The branch's patch:** with at least one block, it is written (12.3)
   under the number after the last landed patch's; with none, its text
   and goldens go, and its `pNNNN.rs`, if any, is reported for the
   author to remove.
6. **Landed files** the tree lacks or holds otherwise, a renumbering's
   among them (12.6), are written back from the base, and the report
   names each: an author's edit of a landed file is undone by name, as
   the check's `Changed` names it.
7. **`mod.rs`** is generated again; `context.rs` written if absent.
8. **The check** runs, and its findings are reported (12.7).

### 12.3 The patch it writes

- **The text:** `patch 1`; `owner <owner> <n>`, the owner given once by
  `--owner` and then read from the latest patch, with `after` and each
  upstream's level as given by `--after`; the branch's patch's marks
  kept; then a block per family, in name order, `family <family> <k> to
  <c>`, holding the diff's lines in their order (10.2) without their
  kind, an added field's value `?` or `none` as the diff asks, and a
  `written D` line after the last line of each declaration the diff
  names as a site (10.3). No comment is written.
- **Derived again, the author's part kept:** when the branch's patch
  exists, each added field's value is kept where the new diff adds the
  same field with the same type; each site line, the author's and the
  diff's alike, and the `refuses` lines after it, kept where its
  declaration or field is still in the new text; the marks kept. A
  text whose lines come out the same is left as it is, comments and
  all; otherwise it is written whole, without them.
- **The skeleton,** `patches/pNNNN.rs`, written for a patch with sites,
  and for a site the file lacks a function appended: one function per
  site in 11.5's signature, named by its target in snake case (`Shape`
  is `shape`, `Every.score` is `every_score`), its doc comment naming
  the site, its family and versions, and the block's lines for its
  declaration, its body `Err(Halt::NoSite(Site::new(n, i)))`, never a
  panic, until its author writes it. Two sites that spell one name are
  refused.
- **`patches/mod.rs`,** generated from the kept texts and the patches
  alone, its first line saying so:
  - `mod context; pub use context::Context;` and `mod pNNNN;` per
    skeleton;
  - `PATCHES: &[&str]`, each patch's text by `include_str!` in number
    order, and per family `kept::<family>::TEXTS: &[&str]`, its kept
    texts in version order;
  - `kept::<family>::v<N>::<declaration>::<PART>`, the `FieldIx` and
    `VariantIx` of every part of every kept version, by its index in
    that version's schema; `refusal::pNNNN::<NAME>`, each refusal's
    `RefusalIx`; names in snake case, parts in upper case, a keyword
    raw, two names spelling one constant refused;
  - `written(site, old, new, at, cx)`, the dispatch of 11.5; with no
    site, `Err(Halt::NoSite(site))` alone;
  - its tests: `current`, which fails when `mod.rs` is not as generated,
    naming the command, and a golden test per patch.
- **The golden test** of patch `n` (`patch::goldens(crate, n,
  written)`), for each family it moves: each golden of the old version,
  made from the kept text by section 6's rule, its declaration's
  counterpart its root (a declaration dropped has none, and is passed
  over), run through the patch, each stop answered by the dispatch with
  a `Context::default()` and each held field carried; the result decodes
  as a generic value of the new kept text, and equals its file
  `patches/golden/NNNN/<family>/<golden>`, or, refused with a declared
  reason, holds `refuses <name>` in `<golden>.refused`. A file missing,
  different or stale fails it, naming every golden that does;
  `SKEIN_PATCH_WRITE=1` writes them for review. The command writes them
  itself for a patch with no site once it plans; a patch with sites
  waits for its author's functions and the test.

### 12.4 Renames

- **Confirmed by its author,** `--rename <family> <old> <new>`, a
  declaration's `Old New` or a part's `Decl.old new` with its
  declaration's current name, as the patch's `rename` line spells it.
- **Where a confirmed rename goes:** into the branch's patch, as its
  `rename` line, when the family moves; else into the kept text by its
  refresh, when not frozen; else into `schema/kept/<family>.renames`, one
  `rename` line each, where it waits for the family's next patch, its
  lines the diff's confirmed renames from then on. That patch takes them
  and the file goes.
- **Unconfirmed,** a rename is a drop and an add: a refresh takes it
  alike, and under a frozen text it asks for a patch (12.2, 3), whose
  report shows the proposal.

### 12.5 Versions

- **`Kept`, `Moved`:** acted on (12.2).
- **`Unmoved`:** a patch needed and the version kept: the author moves
  it, the current schema's header and file, then runs the command.
- **`Needless`:** the version moved with no patch needed, against 10.4;
  the author keeps it. Before the baseline no version moves (section 5),
  and no gate runs the check.
- **`Wrong`:** the current version neither `k` nor `k + 1`. A version
  without its kept text, such as a current `v9` over a latest kept `v7`,
  is `Wrong`: kept versions run without a gap from a family's first
  (12.7).

### 12.6 Renumbering

- **A taken number shows at the rebase:** two branches that each write
  the owner's next patch add the same `patches/NNNN.patch` and change
  `patches/mod.rs`, so the second's rebase on main stops on their
  conflict; a change to a landed file that rebases cleanly fails the
  check.
- **The author rebases on main,** by hand or by continuing the rebase
  `land` stopped, and resolves: the branch's side of the files the
  command owns (`git checkout --theirs -- <crate>/patches
  <crate>/schema/kept`), the current schemas merged as any conflict, a
  family both branches moved taking the version after the landed one;
  then runs the command. A file holding conflict markers is refused,
  naming this step.
- **The command moves the branch's patch** to the number after the last
  landed one, and writes the taken number's landed files back from the
  base, now main's tip. It derives each block again against the latest
  landed kept text (12.3), so a family that moved meanwhile has its
  block from the new version, keeping its author's values, sites and
  refusals; its `pNNNN.rs` and `patches/golden/NNNN/` move with it, the
  file's own names rewritten: the module `pNNNN`, `Site::new(n,` and,
  for a family moved meanwhile, its versions in `<family>::v<N>` paths.
  Then the rebase goes on, and the branch lands.

### 12.7 The check

`check(crate, landed)` reads the crate's files and the landed ones, and
returns every finding, each displayed with its fix; a crate it cannot
read, such as a text that does not parse, is its error, as the run's:

| `Finding` | Found |
|---|---|
| `Unkept { family }` | a current text with no kept text |
| `KeptGap { family, missing }` | a kept version missing between a family's first and its latest |
| `Unpatched { family, version }` | a kept version after a family's first that no patch moves it to, or a current text that needs a patch (12.2, 3) none moves it to |
| `Lagging { family, version }` | a latest kept text not frozen that differs from the current with no patch needed: the command refreshes it |
| `Version { family, verdict, kept, current }` | `Unmoved`, `Needless` or `Wrong` (12.5) |
| `Changed { path }` | a landed patch's text or goldens, or a landed kept text, changed or gone; but a kept text no landed patch names, refreshed to its family's current text |
| `PatchGap { missing }` | a patch number missing between 1 and the last |
| `Misnumbered { patch, level }` | a patch whose header's level is not its number |
| `Unplanned { patch, family, error }` | a block that does not plan against its kept texts (11.3): a `?` left, a site missing, a line wrong |

A frozen kept text's differences from the current that need no patch,
renames waiting or not, declarations dropped, comments and order, are
no finding. The golden tests and `current` (12.3) are the owner's own
tests beside it.

### 12.8 The interface

```rust
pub mod patch;  // skein_codegen::patch

pub struct Options {
    pub owner: Option<String>,             // --owner, for a crate's first patch
    pub after: Vec<(String, u32)>,         // --after <upstream> <level>
    pub renames: Vec<(String, Rename)>,    // --rename <family> <old> <new>
    pub baseline: bool,                    // --baseline, the crate's first run (12.1)
}
pub struct Landed { /* the base's files under patches/ and schema/kept/, by path */ }
impl Landed {
    pub fn new(files: BTreeMap<PathBuf, Vec<u8>>) -> Landed;  // paths relative to the crate
    pub fn read(owner: &Path) -> Result<Landed, Error>;       // the tree's own, as if landed
    pub fn none() -> Landed;                                  // nothing landed
}
pub fn run(owner: &Path, landed: &Landed, options: &Options) -> Result<Report, Error>;
pub fn check(owner: &Path, landed: &Landed) -> Result<Vec<Finding>, Error>;
pub fn module(owner: &Path) -> Result<String, Error>;  // patches/mod.rs's text
pub fn current(owner: &Path) -> Result<(), Error>;     // mod.rs as generated
pub fn goldens<C: Default>(owner: &Path, patch: u32, written: Written<C>) -> Result<(), Error>;
pub type Written<C> = fn(Site, &Old, &mut Build, Node, &mut C) -> Result<(), Halt>;

pub struct Report {
    pub outcomes: Vec<Outcome>,            // a family's each, in name order
    pub patch: Option<u32>,                // the branch's patch
    pub moved: Option<u32>,                // the number it was moved from
    pub proposed: Vec<(String, Rename)>,   // proposals not confirmed
    pub asked: Vec<String>,                // what its author must give: `?` lines, sites
    pub restored: Vec<PathBuf>,            // landed files written back (12.2, 6)
    pub findings: Vec<Finding>,            // the check's, after the run
}
pub enum Outcome {
    Kept { family: String, version: u16 },        // its first kept text
    Current { family: String },                   // nothing to do
    Refreshed { family: String, version: u16 },
    Waiting { family: String, renames: Vec<Rename> },
    Patched { family: String, from: u16, to: u16 },
    Refused { family: String, verdict: Version },
}
pub enum Finding { /* 12.7's table */ }
pub enum Error {
    Io { path: PathBuf, error: String },
    Schema { path: PathBuf, error: String },  // a text that does not parse
    Diff { family: String, error: DiffError },
    Baseline,                                 // before the baseline, without --baseline
    Owner,                                    // a first patch without --owner
    Unlanded { patches: Vec<u32> },           // two patches of the branch's own
    Conflict { path: PathBuf },               // conflict markers (12.6)
    Spelled { first: String, second: String },// two names, one constant or function
    Golden(Vec<String>),                      // each golden that fails
    Stale,                                    // mod.rs not as generated
}
```

- **The library reads no git:** what is landed is its input, so that
  its tests and the story pass it in memory.
- **The binary:** `skein-codegen patch <crate> [--baseline]
  [--owner <name>] [--after <upstream> <level>]… [--rename <family>
  <old> <new>]… [--base <ref>] [--check]`. It reads the landed files with `git
  ls-tree` and `git show <base>:<path>`, the base `git merge-base HEAD
  origin/main` unless `--base` names another, prints the report, its proposals as the
  `--rename` that confirms each, and exits 1 while a finding is left.
  `--check` runs the check alone. `skein-codegen <schema> <out-dir>`
  is as before; a schema is never named `patch`.
- `Report`, `Outcome`, `Finding` and `Error` derive `Clone`, `Debug` and
  equality, and display as the command prints them.

### 12.9 Tests

Step tests in skein-codegen, the story in `tests/codec-patches`; no
world, no cargo inside a test.

- **Step tests,** each over a crate in a scratch directory, its files
  compared as whole text: each outcome's files (a run before the
  baseline refused, the baseline, a refresh, a rename waiting, a
  derived patch with its `?`, a written one with its skeleton), the
  author's values and sites kept when derived again, each verdict of
  10.4, the gap, renumbering with and without the family moved
  meanwhile, `mod.rs` for a crate of two families and patches, and each
  finding of 12.7 and its absence.
- **The story,** `tests/codec-patches`, an owner crate whose history the
  command makes: the baseline, by `--baseline`; a field added, its patch
  asking `?`, refused when planned, then filled, planned and its goldens
  written; a rename refreshed in a family no patch names, and one
  waiting in a frozen one; a written change, its skeleton's golden test
  failing with `NoSite` naming the site, then its function written;
  another branch's patch landing first, and the story's renumbered. The
  check at each step. Each step is checked as text against its expected
  files; the crate's final files are the last step's, built and tested
  by the gate as any member: `current` and the golden tests.

## 13. Open questions

- **Upstreams' levels:** the command writes a patch's `after` as its
  author gives it (12.3); jig's mission, which orders patches across
  owners, reads them from the pins.
- **Proposals to a fixpoint:** the command runs the diff in rounds to
  propose renames a level of references down (12.2); the diff itself
  could take candidate pairs as matched until none fails, and propose
  them all in one pass.
- **Pieces of a converted type:** a patch whose conversion of a
  `pieces` field's items is not a copy, or an identity plan whose
  pieces reach a held field, is refused when planned (11.3). jig's
  mission, which patches values nested across families, opens them:
  each piece converted as its type, its length back-patched.
- **Carried parts that stop:** a written function's `carry` converts in
  place, so a part with a site or a held field in reach is refused
  (11.5). Drafts would lift it behind the same calls: the function
  writes into a draft, a stack of them in one arena, and a carried
  part is a slot the machine fills as it emits the draft, stopping at
  each site and held field in its turn.
- **Richer builders:** carrying a record's fields up to one, or to its
  end, should written functions that build records by hand be common.
- **Richer values:** a record's value, or a list's with items, in a
  patch's text, should authors write the same function often (11.2).
- **A held type renamed in its own family** is a `retype` in its
  holder's diff, written: the holder's diff reads its own family's two
  texts alone, so it cannot see the held family's renames. jig's
  mission, which patches held fields across families, gives the diff
  those renames.
- **Composing patches:** a record k versions behind is patched in k
  passes; plans may compose later, behind the same interface, and so
  remove the box each pass after the first allocates (11.7).
- **Maps and sets:** sorted lists whose keys are checked unique, when a
  record needs one.
- **Smaller integers:** variable-width encodings, if a payload shows that
  fixed widths cost too much.
- **JSON from schemas:** the same generator writing JSON codecs over
  `skein-json` (json.md, section 8).
- **Other languages:** a generator for a peer not built on skein, if one
  appears. The schema is plain enough for it.

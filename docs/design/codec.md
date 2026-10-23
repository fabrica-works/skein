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
  SHA-256 (lib's) over a canonical encoding of what its bytes are:
  record or enumeration, `versioned` and the family's version, each
  field's type in order (its kind and width, a `fixed` length, a bound,
  `list` and `option`, text, bytes or secret, what a `bytes` holds), the
  variants' payloads in order, and each named type by its own
  fingerprint. Names, comments, the text's layout and `boxed` are not
  in it.
- **A family's fingerprint** is over its declarations' in order.
- **Step code,** in `schema`: a kept text's fingerprints are computed
  at run time as the current one's are.
- **A rename moves no fingerprint,** and the check (section 12) also
  asks the latest kept text to follow the current names, so that a
  later diff never mistakes a rename for a field dropped and another
  added.

## 10. The diff

- **Two versions' layouts into the kinds of change,** each naming its
  declaration and field or variant, in skein-codegen. Declarations,
  fields and variants are matched by name, from the latest kept text to
  the current:
  - **none needed,** every old value surviving byte for byte: a field or
    variant renamed, a variant appended, a bound widened, `boxed` marked
    or unmarked, a `bytes` made a `secret` or back, a `text` made a
    `bytes` (every old text is valid bytes);
  - **derived:** a field added, with the value every old record takes
    (an `option` none), a field dropped, fields reordered, an integer
    widened, a type wrapped in `option`, variants reordered or
    inserted;
  - **written:** anything else, such as a bound narrowed, an `option`
    unwrapped, a type changed, a variant removed, `bytes` made `text`
    (old bytes need not be UTF-8, which text is checked to be).
- **A rename is proposed,** where a field or variant leaves and another
  of the same layout takes its place, and its author confirms it.

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

- **Left to the milestone that builds each:** the fingerprint's exact
  encoding (section 9); the diff's interface (section 10); the patch
  text's grammar, the checks of planning and the `Patcher`'s interface,
  with lib's reusable buffer (section 11); the command's options and
  the check's findings (section 12).
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

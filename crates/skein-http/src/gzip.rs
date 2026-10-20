//! A gzip encoder (http.md, section 3.1): a whole request body to its gzip
//! member (RFC 1952), DEFLATE inside (RFC 1951), for a caller whose server
//! takes a compressed body (RFC 9110, 8.4.1; llm.md, section 4.8).
//!
//! [`bound`] is the worst case of the output from the body's length: the
//! caller sets that much room aside, and [`encode`] writes the member into
//! its front. Matches are found through a table of 2^15 heads, one
//! candidate each, by a hash of four bytes, within a window of 32 KiB:
//! ratio traded for speed, chosen for JSON that repeats itself. Each block
//! of the body is parsed once and written in the shortest of three ways: in
//! Huffman codes built from its own counts (RFC 1951, 3.2.7), in the fixed
//! codes (3.2.6), or stored (3.2.4). A block is never longer than stored,
//! so the output never passes the bound. One body, one output.

/// The length of every block but the last: the most a stored block holds
/// (RFC 1951, 3.2.4).
const BLOCK: usize = 65_535;

/// How far back a match reaches (RFC 1951, 3.2.5).
const WINDOW: usize = 32_768;

/// The shortest match taken, the bytes the hash covers. DEFLATE's own
/// shortest is 3, which saves little over its literals.
const SHORTEST: usize = 4;

/// The longest match (RFC 1951, 3.2.5).
const LONGEST: usize = 258;

/// The bits of the hash: the table has 2^15 heads, 64 KiB on the stack.
const HASH: u32 = 15;

/// The table's heads.
const HEADS: usize = 1 << HASH;

/// The most matches one block holds, each of [`SHORTEST`] bytes at least.
const MATCHES: usize = BLOCK / SHORTEST + 1;

/// The symbols of the literal/length code (RFC 1951, 3.2.6); 286 and 287
/// never occur.
const SYMBOLS: usize = 288;

/// The symbol that ends a block.
const END: usize = 256;

/// The symbols of the distance code (RFC 1951, 3.2.6) that occur.
const DISTANCES: usize = 30;

/// The symbols of the code that sends the other codes' widths (RFC 1951,
/// 3.2.7): a width of 0 to 15, or 16, 17 and 18, which repeat one.
const WIDTHS: usize = 19;

/// The order in which that code's own widths are sent (RFC 1951, 3.2.7).
const ORDER: [usize; WIDTHS] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The widest code of a literal, a length or a distance (RFC 1951, 3.2.7).
const WIDEST: u32 = 15;

/// The widest code of a width: its own width is sent in 3 bits.
const WIDEST_WIDTH: u32 = 7;

/// The most widths a block's header sends, literal/length and distance.
const SENT: usize = SYMBOLS + DISTANCES;

/// The member's header (RFC 1952, 2.3): gzip, DEFLATE, no flags, no time,
/// no extra flags, an unknown system.
const HEADER: [u8; 10] = [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];

/// What the member adds to its blocks: the header, and the trailer's CRC-32
/// and ISIZE (RFC 1952, 2.2).
const FRAME: usize = 18;

/// What a stored block adds to its bytes: its three header bits padded to a
/// byte, and LEN and NLEN (RFC 1951, 3.2.4).
const STORED: usize = 5;

/// A code with no bits: a symbol that does not occur.
const NONE: Code = Code { bits: 0, width: 0 };

/// The worst case of [`encode`]'s output for a body of `length` bytes, or
/// `None` past `usize`: the header and trailer, and the body in stored
/// blocks of at most 65535 bytes, at least one of them.
#[must_use]
pub fn bound(length: usize) -> Option<usize> {
    blocks(length).checked_mul(STORED)?.checked_add(length)?.checked_add(FRAME)
}

/// Writes `body`'s gzip member into the front of `out` and returns its
/// length, at most [`bound`] of the body's; or `None`, writing nothing that
/// counts, when `out` is shorter than that bound.
///
/// The same body gives the same bytes: no time, no name, and the codes
/// chosen by the body alone.
#[must_use]
#[expect(
    clippy::large_stack_arrays,
    reason = "the match table and one block's matches, 144 KiB, live for one call: on the stack, not in the heap the limits account"
)]
pub fn encode(body: &[u8], out: &mut [u8]) -> Option<usize> {
    if out.len() < bound(body.len())? {
        return None;
    }
    let mut encoder = Encoder {
        heads: [0; HEADS],
        parse: Parse {
            count: 0,
            starts: [0; MATCHES],
            lengths: [0; MATCHES],
            distances: [0; MATCHES],
            counts: Counts { literals: [0; SYMBOLS], distances: [0; DISTANCES], extra: 0 },
        },
        fixed: fixed()?,
        own: Codes { literals: [NONE; SYMBOLS], distances: [NONE; DISTANCES] },
        header: Header {
            literals: 0,
            distances: 0,
            widths: 0,
            code: [NONE; WIDTHS],
            runs: [Run { symbol: 0, extra: 0 }; SENT],
            count: 0,
            bits: 0,
        },
        bits: Bits { at: copy(out, 0, &HEADER)?, held: 0, count: 0 },
    };
    let blocks = blocks(body.len());
    for index in 0..blocks {
        let start = index.checked_mul(BLOCK)?;
        let end = start.checked_add(BLOCK)?.min(body.len());
        let block = Block { start, end, last: index.checked_add(1)? == blocks };
        write(&mut encoder, body, block, out)?;
    }
    align(out, &mut encoder.bits)?;
    let at = copy(out, encoder.bits.at, &crc(body)?.to_le_bytes())?;
    let [a, b, c, d, ..] = u64::try_from(body.len()).ok()?.to_le_bytes();
    copy(out, at, &[a, b, c, d])
}

/// How many blocks a body of `length` bytes takes: one at least.
fn blocks(length: usize) -> usize {
    length.div_ceil(BLOCK).max(1)
}

/// The encoder's state through one body: the match table, the block being
/// written, its codes, and where the output stands.
#[derive(Debug)]
struct Encoder {
    /// By the hash of four bytes, the low 16 bits of where they last began.
    /// A head names a candidate whose bytes are compared before it is taken,
    /// so a stale or empty head costs a match, never a wrong one.
    heads: [u16; HEADS],
    parse: Parse,
    /// The fixed codes (RFC 1951, 3.2.6).
    fixed: Codes,
    /// The codes built for the block from its counts.
    own: Codes,
    /// What sends the block's own codes.
    header: Header,
    bits: Bits,
}

/// One block of the body: its bytes from `start` to `end`, and whether it is
/// the last.
#[derive(Clone, Copy, Debug)]
struct Block {
    start: usize,
    end: usize,
    last: bool,
}

/// One block parsed: its first `count` matches, in order, each where it
/// starts from the block's start, its length less 3 and its distance less
/// 1; the bytes between them are literals.
#[derive(Debug)]
struct Parse {
    count: usize,
    starts: [u16; MATCHES],
    lengths: [u8; MATCHES],
    distances: [u16; MATCHES],
    counts: Counts,
}

/// How often each literal/length and each distance symbol occurs in a block,
/// the end once, and the extra bits of its lengths and distances.
#[derive(Clone, Copy, Debug)]
struct Counts {
    literals: [u32; SYMBOLS],
    distances: [u32; DISTANCES],
    extra: usize,
}

/// A literal/length code and a distance code.
#[derive(Clone, Copy, Debug)]
struct Codes {
    literals: [Code; SYMBOLS],
    distances: [Code; DISTANCES],
}

/// A block's header in its own codes (RFC 1951, 3.2.7): how many
/// literal/length, distance and width-code widths it sends, the code of the
/// widths, the `count` runs that send the widths in it, and how many bits
/// the header takes after the block's first three.
#[derive(Debug)]
struct Header {
    literals: usize,
    distances: usize,
    widths: usize,
    code: [Code; WIDTHS],
    runs: [Run; SENT],
    count: usize,
    bits: usize,
}

/// A symbol of the width code: a width, or a repeat of `extra` more.
#[derive(Clone, Copy, Debug)]
struct Run {
    symbol: u8,
    extra: u8,
}

/// Where the output stands: whole bytes written before `at`, then `count`
/// bits held in `held`, the first in its lowest bit (RFC 1951, 3.1.1).
#[derive(Clone, Copy, Debug)]
struct Bits {
    at: usize,
    held: u64,
    count: u32,
}

/// A code as it is written: its bits in the order they go out, the first
/// in the lowest bit, and how many there are; none for a symbol that does
/// not occur.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Code {
    pub(crate) bits: u32,
    pub(crate) width: u32,
}

/// A match: `length` bytes that occurred `distance` bytes before.
#[derive(Clone, Copy, Debug)]
struct Match {
    length: usize,
    distance: usize,
}

/// How a block is written.
#[derive(Clone, Copy, Debug)]
enum Way {
    Stored,
    Fixed,
    Own,
}

/// Parses `body[block.start..block.end]` and writes it in the shortest of
/// the three ways, the bit at which each would end reckoned first.
fn write(encoder: &mut Encoder, body: &[u8], block: Block, out: &mut [u8]) -> Option<()> {
    let data = body.get(block.start..block.end)?;
    parse(encoder, body, block)?;
    let from = encoder.bits;
    let coded = from.position()?.checked_add(3)?;
    let mut way = Way::Stored;
    let mut end = stored_end(from, data.len())?;
    let fixed = coded.checked_add(cost(&encoder.parse.counts, &encoder.fixed)?)?;
    if fixed < end {
        way = Way::Fixed;
        end = fixed;
    }
    // Codes that cannot be built cost the block its own codes, nothing more.
    if let Some(taken) = own(encoder)
        && let Some(ends) = coded.checked_add(taken)
        && ends < end
    {
        way = Way::Own;
        end = ends;
    }
    let last = u32::from(block.last);
    match way {
        Way::Stored => stored(out, &mut encoder.bits, data, block.last)?,
        Way::Fixed => {
            put(out, &mut encoder.bits, last | 2, 3)?;
            emit(out, &mut encoder.bits, body, block, &encoder.parse, &encoder.fixed)?;
        }
        Way::Own => {
            put(out, &mut encoder.bits, last | 4, 3)?;
            header(out, &mut encoder.bits, &encoder.header)?;
            emit(out, &mut encoder.bits, body, block, &encoder.parse, &encoder.own)?;
        }
    }
    // Where the block was reckoned to end is where it ends.
    (encoder.bits.position()? == end).then_some(())
}

/// Finds the block's matches and counts its symbols into `encoder.parse`,
/// the matches reaching back into the bytes before it. Each turn takes a
/// literal or a match, a byte at least, so the block's length bounds the
/// turns; the bytes inside a match become heads as they are passed.
fn parse(encoder: &mut Encoder, body: &[u8], block: Block) -> Option<()> {
    let parse = &mut encoder.parse;
    parse.count = 0;
    parse.counts = Counts { literals: [0; SYMBOLS], distances: [0; DISTANCES], extra: 0 };
    let mut at = block.start;
    for _ in block.start..block.end {
        if at >= block.end {
            break;
        }
        let found = match word(body, at) {
            Some(bytes) => {
                let head = encoder.heads.get_mut(hash(bytes)?)?;
                let earlier = *head;
                *head = low(at);
                longest(body, at, block.end, earlier, bytes)
            }
            None => None,
        };
        let Some(found) = found else {
            tally(&mut parse.counts.literals, usize::from(*body.get(at)?))?;
            at = at.checked_add(1)?;
            continue;
        };
        record(parse, block, at, found)?;
        let next = at.checked_add(found.length)?;
        for inside in at.checked_add(1)?..next {
            let Some(bytes) = word(body, inside) else {
                break;
            };
            *encoder.heads.get_mut(hash(bytes)?)? = low(inside);
        }
        at = next;
    }
    tally(&mut parse.counts.literals, END)
}

/// Keeps a match found at `at`, and counts its symbols and extra bits.
fn record(parse: &mut Parse, block: Block, at: usize, found: Match) -> Option<()> {
    let index = parse.count;
    *parse.starts.get_mut(index)? = u16::try_from(at.checked_sub(block.start)?).ok()?;
    *parse.lengths.get_mut(index)? = u8::try_from(found.length.checked_sub(3)?).ok()?;
    *parse.distances.get_mut(index)? = u16::try_from(found.distance.checked_sub(1)?).ok()?;
    parse.count = index.checked_add(1)?;
    let length = length_code(found.length)?;
    let distance = distance_code(found.distance)?;
    tally(&mut parse.counts.literals, usize::try_from(length.symbol).ok()?)?;
    tally(&mut parse.counts.distances, usize::try_from(distance.symbol).ok()?)?;
    let extra = usize::try_from(length.width.checked_add(distance.width)?).ok()?;
    parse.counts.extra = parse.counts.extra.checked_add(extra)?;
    Some(())
}

/// Counts one more of `symbol`.
fn tally(counts: &mut [u32], symbol: usize) -> Option<()> {
    let count = counts.get_mut(symbol)?;
    *count = count.checked_add(1)?;
    Some(())
}

/// The bits a block's literals, lengths, distances and end take in `codes`,
/// its header aside; `None` if a symbol that occurs has no code.
fn cost(counts: &Counts, codes: &Codes) -> Option<usize> {
    let mut bits = counts.extra;
    for (count, code) in counts.literals.iter().zip(&codes.literals) {
        bits = bits.checked_add(weighed(*count, *code)?)?;
    }
    for (count, code) in counts.distances.iter().zip(&codes.distances) {
        bits = bits.checked_add(weighed(*count, *code)?)?;
    }
    Some(bits)
}

/// The bits `count` symbols take in `code`.
fn weighed(count: u32, code: Code) -> Option<usize> {
    if count > 0 && code.width == 0 {
        return None;
    }
    usize::try_from(count).ok()?.checked_mul(usize::try_from(code.width).ok()?)
}

/// Builds the block's own codes and the header that sends them, and returns
/// the bits both take after the block's first three.
fn own(encoder: &mut Encoder) -> Option<usize> {
    let counts = &encoder.parse.counts;
    huffman(&counts.literals, WIDEST, &mut encoder.own.literals)?;
    huffman(&counts.distances, WIDEST, &mut encoder.own.distances)?;
    describe(&encoder.own, &mut encoder.header)?;
    encoder.header.bits.checked_add(cost(counts, &encoder.own)?)
}

/// Fills `header` with what sends `codes` (RFC 1951, 3.2.7): their widths
/// in one sequence, a run of zeros as 17 or 18 and a run of another width
/// after its first as 16, in a code of widths built from those runs.
fn describe(codes: &Codes, header: &mut Header) -> Option<()> {
    let literals = sent(&codes.literals, 257)?;
    let distances = sent(&codes.distances, 1)?;
    let mut all = [0_u8; SENT];
    let each = codes.literals.get(..literals)?.iter().chain(codes.distances.get(..distances)?);
    for (slot, code) in all.iter_mut().zip(each) {
        *slot = u8::try_from(code.width).ok()?;
    }
    let sequence = all.get(..literals.checked_add(distances)?)?;
    let mut counts = [0_u32; WIDTHS];
    let mut count = 0_usize;
    let mut next = 0_usize;
    for at in 0..sequence.len() {
        if at < next {
            continue;
        }
        let width = *sequence.get(at)?;
        let run = run(sequence, at)?;
        let previous = match at.checked_sub(1) {
            Some(before) => sequence.get(before).copied(),
            None => None,
        };
        let (symbol, extra, length) = if width == 0 && run >= 11 {
            let length = run.min(138);
            (18, length.checked_sub(11)?, length)
        } else if width == 0 && run >= 3 {
            (17, run.checked_sub(3)?, run)
        } else if previous == Some(width) && run >= 3 {
            let length = run.min(6);
            (16, length.checked_sub(3)?, length)
        } else {
            (width, 0, 1)
        };
        *header.runs.get_mut(count)? = Run { symbol, extra: u8::try_from(extra).ok()? };
        count = count.checked_add(1)?;
        tally(&mut counts, usize::from(symbol))?;
        next = at.checked_add(length)?;
    }
    huffman(&counts, WIDEST_WIDTH, &mut header.code)?;
    let mut widths = 4_usize;
    for (index, symbol) in ORDER.iter().enumerate() {
        if header.code.get(*symbol)?.width > 0 {
            widths = widths.max(index.checked_add(1)?);
        }
    }
    let mut bits = widths.checked_mul(3)?.checked_add(14)?;
    for run in header.runs.get(..count)? {
        let width = header.code.get(usize::from(run.symbol))?.width.checked_add(repeat(run.symbol))?;
        bits = bits.checked_add(usize::try_from(width).ok()?)?;
    }
    header.literals = literals;
    header.distances = distances;
    header.widths = widths;
    header.count = count;
    header.bits = bits;
    Some(())
}

/// How many of `codes`' widths a header sends: to the last code, `least` at
/// least.
fn sent(codes: &[Code], least: usize) -> Option<usize> {
    let mut sent = least;
    for (index, code) in codes.iter().enumerate() {
        if code.width > 0 {
            sent = sent.max(index.checked_add(1)?);
        }
    }
    Some(sent)
}

/// How many widths from `at` on are the same, 138 at most.
fn run(sequence: &[u8], at: usize) -> Option<usize> {
    let first = *sequence.get(at)?;
    let mut length = 0_usize;
    for width in sequence.get(at..)?.iter().take(138) {
        if *width != first {
            break;
        }
        length = length.checked_add(1)?;
    }
    Some(length)
}

/// The extra bits after a width-code symbol: how many more repeats.
fn repeat(symbol: u8) -> u32 {
    match symbol {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

/// Writes the block's header in its own codes.
fn header(out: &mut [u8], bits: &mut Bits, header: &Header) -> Option<()> {
    put(out, bits, u32::try_from(header.literals.checked_sub(257)?).ok()?, 5)?;
    put(out, bits, u32::try_from(header.distances.checked_sub(1)?).ok()?, 5)?;
    put(out, bits, u32::try_from(header.widths.checked_sub(4)?).ok()?, 4)?;
    for symbol in ORDER.get(..header.widths)? {
        put(out, bits, header.code.get(*symbol)?.width, 3)?;
    }
    for run in header.runs.get(..header.count)? {
        let code = header.code.get(usize::from(run.symbol))?;
        put(out, bits, code.bits, code.width)?;
        put(out, bits, u32::from(run.extra), repeat(run.symbol))?;
    }
    Some(())
}

/// Writes the block's literals, matches and end in `codes`.
fn emit(out: &mut [u8], bits: &mut Bits, body: &[u8], block: Block, parse: &Parse, codes: &Codes) -> Option<()> {
    let mut at = block.start;
    for index in 0..parse.count {
        let start = block.start.checked_add(usize::from(*parse.starts.get(index)?))?;
        literals(out, bits, body.get(at..start)?, codes)?;
        let length = usize::from(*parse.lengths.get(index)?).checked_add(3)?;
        let distance = usize::from(*parse.distances.get(index)?).checked_add(1)?;
        put_coded(out, bits, length_code(length)?, &codes.literals)?;
        put_coded(out, bits, distance_code(distance)?, &codes.distances)?;
        at = start.checked_add(length)?;
    }
    literals(out, bits, body.get(at..block.end)?, codes)?;
    let end = codes.literals.get(END)?;
    put(out, bits, end.bits, end.width)
}

/// Writes `bytes` as literals.
fn literals(out: &mut [u8], bits: &mut Bits, bytes: &[u8], codes: &Codes) -> Option<()> {
    for byte in bytes {
        let code = codes.literals.get(usize::from(*byte))?;
        put(out, bits, code.bits, code.width)?;
    }
    Some(())
}

/// Writes a length or distance symbol in `codes`, then its extra bits.
fn put_coded(out: &mut [u8], bits: &mut Bits, coded: Coded, table: &[Code]) -> Option<()> {
    let code = table.get(usize::try_from(coded.symbol).ok()?)?;
    let value = coded.extra.checked_shl(code.width)? | code.bits;
    put(out, bits, value, code.width.checked_add(coded.width)?)
}

/// The longest match at `at`, whose four bytes are `bytes`, ending by
/// `end`, with the candidate whose low 16 bits are `earlier`; `None` when it
/// is out of the window or shorter than [`SHORTEST`].
fn longest(body: &[u8], at: usize, end: usize, earlier: u16, bytes: u32) -> Option<Match> {
    let distance = usize::from(low(at).wrapping_sub(earlier));
    if distance == 0 || distance > WINDOW || distance > at {
        return None;
    }
    let from = at.checked_sub(distance)?;
    // Most candidates differ in the four bytes hashed, a stale head or
    // another four with the same hash: refused before the rest are compared.
    if word(body, from)? != bytes {
        return None;
    }
    let most = end.checked_sub(at)?.min(LONGEST);
    // Eight bytes at a time, the first that differs found in their XOR,
    // then the last few one by one.
    let mut length = 0_usize;
    for _ in 0..most.checked_div(8)? {
        let old = u64::from_le_bytes(*body.get(from.checked_add(length)?..)?.first_chunk::<8>()?);
        let new = u64::from_le_bytes(*body.get(at.checked_add(length)?..)?.first_chunk::<8>()?);
        let differ = old ^ new;
        if differ != 0 {
            let same = usize::try_from(differ.trailing_zeros().checked_div(8)?).ok()?;
            return matched(length.checked_add(same)?, distance);
        }
        length = length.checked_add(8)?;
    }
    let ahead = body.get(at.checked_add(length)?..at.checked_add(most)?)?;
    for (old, new) in body.get(from.checked_add(length)?..)?.iter().zip(ahead) {
        if old != new {
            break;
        }
        length = length.checked_add(1)?;
    }
    matched(length, distance)
}

/// A match of `length` bytes, if it is no shorter than [`SHORTEST`].
fn matched(length: usize, distance: usize) -> Option<Match> {
    (length >= SHORTEST).then_some(Match { length, distance })
}

/// The four bytes at `at`, if the body holds four there.
fn word(body: &[u8], at: usize) -> Option<u32> {
    let bytes = body.get(at..)?.first_chunk::<4>()?;
    Some(u32::from_le_bytes(*bytes))
}

/// The table's head for four bytes (Fibonacci hashing: the top bits of
/// their product with 2^32 over the golden ratio).
fn hash(word: u32) -> Option<usize> {
    usize::try_from(word.wrapping_mul(0x9E37_79B1).checked_shr(32_u32.checked_sub(HASH)?)?).ok()
}

/// The low 16 bits of a position, what a head keeps.
fn low(at: usize) -> u16 {
    let [a, b, ..] = at.to_le_bytes();
    u16::from_le_bytes([a, b])
}

/// A length or distance symbol and its extra bits (RFC 1951, 3.2.5).
#[derive(Clone, Copy, Debug)]
struct Coded {
    symbol: u32,
    extra: u32,
    width: u32,
}

/// The length symbol (257 to 285) and extra bits of a match of `length`
/// bytes, 3 to 258.
fn length_code(length: usize) -> Option<Coded> {
    let n = u32::try_from(length.checked_sub(3)?).ok()?;
    if n < 8 {
        return Some(Coded { symbol: n.checked_add(257)?, extra: 0, width: 0 });
    }
    if n == 255 {
        return Some(Coded { symbol: 285, extra: 0, width: 0 });
    }
    // n is 8 to 254: 2 bits under its top bit pick one of four symbols, and
    // the bits below them are the extra bits.
    let width = n.checked_ilog2()?.checked_sub(2)?;
    let group = width.checked_add(1)?.checked_mul(4)?;
    let symbol = (n.checked_shr(width)? & 3).checked_add(group)?.checked_add(257)?;
    Some(Coded { symbol, extra: n & mask(width)?, width })
}

/// The distance symbol (0 to 29) and extra bits of a match `distance`
/// bytes back, 1 to 32768.
fn distance_code(distance: usize) -> Option<Coded> {
    let n = u32::try_from(distance.checked_sub(1)?).ok()?;
    if n < 4 {
        return Some(Coded { symbol: n, extra: 0, width: 0 });
    }
    // n is 4 to 32767: the bit under its top bit picks one of two symbols,
    // and the bits below it are the extra bits.
    let width = n.checked_ilog2()?.checked_sub(1)?;
    let symbol = (n.checked_shr(width)? & 1).checked_add(width.checked_add(1)?.checked_mul(2)?)?;
    Some(Coded { symbol, extra: n & mask(width)?, width })
}

/// The low `width` bits set.
fn mask(width: u32) -> Option<u32> {
    1_u32.checked_shl(width)?.checked_sub(1)
}

/// The fixed codes (RFC 1951, 3.2.6): literals and lengths of 7 to 9 bits,
/// distances of 5.
fn fixed() -> Option<Codes> {
    let mut codes = Codes { literals: [NONE; SYMBOLS], distances: [Code { bits: 0, width: 5 }; DISTANCES] };
    for (symbol, code) in codes.literals.iter_mut().enumerate() {
        // 0 to 143 and 280 to 287 of 8 bits.
        code.width = match symbol {
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    canonical(&mut codes.literals)?;
    canonical(&mut codes.distances)?;
    Some(codes)
}

/// Gives each symbol `counts` weighs a code of at most `widest` bits in
/// `codes`, its width and then its bits: the more often a symbol occurs, the
/// shorter its code, and none for a symbol that does not. Two symbols get
/// one at least, 0 or 1 added for none, so that the code is complete.
///
/// Huffman's construction by two queues, the leaves sorted and the nodes
/// made from them, then widths past `widest` cut to it and the code made
/// whole again, as miniz does: a width taken from the widest and one
/// shorter code split in two.
pub(crate) fn huffman(counts: &[u32], widest: u32, codes: &mut [Code]) -> Option<()> {
    if counts.len() > SYMBOLS || codes.len() != counts.len() || widest == 0 || widest > WIDEST {
        return None;
    }
    // The leaves, the rarest first, a symbol's count and the symbol.
    let mut leaves = [(0_u32, 0_u16); SYMBOLS];
    let mut used = 0_usize;
    for (symbol, count) in (0_u16..).zip(counts) {
        if *count > 0 {
            *leaves.get_mut(used)? = (*count, symbol);
            used = used.checked_add(1)?;
        }
    }
    for symbol in 0_u16..2 {
        if used < 2 && *counts.get(usize::from(symbol))? == 0 {
            *leaves.get_mut(used)? = (0, symbol);
            used = used.checked_add(1)?;
        }
    }
    let leaves = leaves.get_mut(..used)?;
    leaves.sort_unstable();
    // Each node's weight and parent: the leaves first, then each node made
    // of the two lightest left, never lighter than the node made before it.
    let mut weights = [0_u32; 2 * SYMBOLS];
    let mut parents = [0_u16; 2 * SYMBOLS];
    for (weight, leaf) in weights.iter_mut().zip(leaves.iter()) {
        *weight = leaf.0;
    }
    let root = used.checked_mul(2)?.checked_sub(2)?;
    let mut leaf = 0_usize;
    let mut node = used;
    for made in used..=root {
        let mut weight = 0_u32;
        for _ in 0_u32..2 {
            let taken = if leaf < used && (node == made || weights.get(leaf)? <= weights.get(node)?) {
                leaf = leaf.checked_add(1)?;
                leaf
            } else if node < made {
                node = node.checked_add(1)?;
                node
            } else {
                return None;
            };
            let taken = taken.checked_sub(1)?;
            weight = weight.checked_add(*weights.get(taken)?)?;
            *parents.get_mut(taken)? = u16::try_from(made).ok()?;
        }
        *weights.get_mut(made)? = weight;
    }
    // Each node's depth, from the root down: a parent is made after its
    // children.
    let mut depths = [0_u32; 2 * SYMBOLS];
    for at in (0..root).rev() {
        let parent = usize::from(*parents.get(at)?);
        *depths.get_mut(at)? = depths.get(parent)?.checked_add(1)?;
    }
    // How many leaves have each width, the deeper cut to `widest`.
    let widest = usize::try_from(widest).ok()?;
    let mut widths = [0_u32; 16];
    for depth in depths.get(..used)? {
        tally(&mut widths, usize::try_from(*depth).ok()?.min(widest))?;
    }
    let full = 1_u32.checked_shl(u32::try_from(widest).ok()?)?;
    let mut total = 0_u32;
    for (width, count) in widths.iter().enumerate().take(widest.checked_add(1)?) {
        let share = 1_u32.checked_shl(u32::try_from(widest.checked_sub(width.min(widest))?).ok()?)?;
        total = total.checked_add(count.checked_mul(share)?)?;
    }
    // The cut leaves overfill the code by less than one each.
    for _ in 0..used {
        if total <= full {
            break;
        }
        let deepest = widths.get_mut(widest)?;
        *deepest = deepest.checked_sub(1)?;
        for width in (1..widest).rev() {
            if *widths.get(width)? > 0 {
                let count = widths.get_mut(width)?;
                *count = count.checked_sub(1)?;
                let longer = widths.get_mut(width.checked_add(1)?)?;
                *longer = longer.checked_add(2)?;
                break;
            }
        }
        total = total.checked_sub(1)?;
    }
    // Complete, and a code for every leaf: more leaves than `widest` bits
    // tell apart are refused.
    let mut given = 0_u32;
    for count in &widths {
        given = given.checked_add(*count)?;
    }
    if total != full || usize::try_from(given).ok()? != used {
        return None;
    }
    // The commonest leaves get the shortest widths.
    for code in codes.iter_mut() {
        *code = NONE;
    }
    let mut commonest = leaves.iter().rev();
    for (width, count) in (0_u32..).zip(widths.iter()).skip(1) {
        for _ in 0..*count {
            let (_, symbol) = commonest.next()?;
            codes.get_mut(usize::from(*symbol))?.width = width;
        }
    }
    canonical(codes)
}

/// Gives each code its bits from the widths alone (RFC 1951, 3.2.2): codes
/// of one width consecutive in the symbols' order, after the shorter.
fn canonical(codes: &mut [Code]) -> Option<()> {
    let mut counts = [0_u32; 16];
    for code in codes.iter() {
        if code.width > 0 {
            tally(&mut counts, usize::try_from(code.width).ok()?)?;
        }
    }
    let mut next = [0_u32; 16];
    let mut value = 0_u32;
    for width in 1..next.len() {
        value = value.checked_add(*counts.get(width.checked_sub(1)?)?)?.checked_shl(1)?;
        *next.get_mut(width)? = value;
    }
    for code in codes.iter_mut() {
        if code.width > 0 {
            let slot = next.get_mut(usize::try_from(code.width).ok()?)?;
            code.bits = reversed(*slot, code.width)?;
            *slot = slot.checked_add(1)?;
        }
    }
    Some(())
}

/// A Huffman code of `width` bits in the order it goes out: its first,
/// most significant, bit lowest (RFC 1951, 3.1.1).
fn reversed(value: u32, width: u32) -> Option<u32> {
    value.reverse_bits().checked_shr(32_u32.checked_sub(width)?)
}

/// Writes `data` as a stored block (RFC 1951, 3.2.4): its header bits, to
/// the byte's end, then LEN, NLEN and the bytes.
fn stored(out: &mut [u8], bits: &mut Bits, data: &[u8], last: bool) -> Option<()> {
    put(out, bits, u32::from(last), 3)?;
    align(out, bits)?;
    let length = u16::try_from(data.len()).ok()?;
    let [a, b] = length.to_le_bytes();
    let [c, d] = (!length).to_le_bytes();
    let at = copy(out, bits.at, &[a, b, c, d])?;
    bits.at = copy(out, at, data)?;
    Some(())
}

/// The bit at which a stored block of `length` bytes would end, written
/// from `bits`.
fn stored_end(bits: Bits, length: usize) -> Option<usize> {
    let header = bits.position()?.checked_add(3)?;
    header.div_ceil(8).checked_add(4)?.checked_add(length)?.checked_mul(8)
}

impl Bits {
    /// The bits written so far.
    fn position(self) -> Option<usize> {
        self.at.checked_mul(8)?.checked_add(usize::try_from(self.count).ok()?)
    }
}

/// Writes the low `width` bits of `value`, at most 32, whose other bits are
/// clear.
fn put(out: &mut [u8], bits: &mut Bits, value: u32, width: u32) -> Option<()> {
    bits.held |= u64::from(value).checked_shl(bits.count)?;
    bits.count = bits.count.checked_add(width)?;
    if bits.count >= 32 {
        let [a, b, c, d, ..] = bits.held.to_le_bytes();
        bits.at = copy(out, bits.at, &[a, b, c, d])?;
        bits.held >>= 32_u32;
        bits.count = bits.count.checked_sub(32)?;
    }
    Some(())
}

/// Pads what is held to a byte's end with clear bits, and writes it.
fn align(out: &mut [u8], bits: &mut Bits) -> Option<()> {
    let bytes = usize::try_from(bits.count.div_ceil(8)).ok()?;
    let held = bits.held.to_le_bytes();
    bits.at = copy(out, bits.at, held.get(..bytes)?)?;
    bits.held = 0;
    bits.count = 0;
    Some(())
}

/// Writes `bytes` into `out` at `at`, and returns where they end.
fn copy(out: &mut [u8], at: usize, bytes: &[u8]) -> Option<usize> {
    let end = at.checked_add(bytes.len())?;
    for (slot, byte) in out.get_mut(at..end)?.iter_mut().zip(bytes) {
        *slot = *byte;
    }
    Some(end)
}

/// The body's CRC-32 (RFC 1952, 8; ISO 3309).
pub(crate) fn crc(body: &[u8]) -> Option<u32> {
    let mut crc = u32::MAX;
    for byte in body {
        let [low, ..] = crc.to_le_bytes();
        crc = CRC.get(usize::from(low ^ *byte))? ^ (crc >> 8_u32);
    }
    Some(!crc)
}

/// The CRC-32 of each byte alone: for `n`, `n` shifted right eight times,
/// each time a set low bit shifted out xored with the reflected polynomial
/// 0xEDB88320. The tests build it again.
pub(crate) const CRC: [u32; 256] = [
    0x0000_0000,
    0x7707_3096,
    0xEE0E_612C,
    0x9909_51BA,
    0x076D_C419,
    0x706A_F48F,
    0xE963_A535,
    0x9E64_95A3,
    0x0EDB_8832,
    0x79DC_B8A4,
    0xE0D5_E91E,
    0x97D2_D988,
    0x09B6_4C2B,
    0x7EB1_7CBD,
    0xE7B8_2D07,
    0x90BF_1D91,
    0x1DB7_1064,
    0x6AB0_20F2,
    0xF3B9_7148,
    0x84BE_41DE,
    0x1ADA_D47D,
    0x6DDD_E4EB,
    0xF4D4_B551,
    0x83D3_85C7,
    0x136C_9856,
    0x646B_A8C0,
    0xFD62_F97A,
    0x8A65_C9EC,
    0x1401_5C4F,
    0x6306_6CD9,
    0xFA0F_3D63,
    0x8D08_0DF5,
    0x3B6E_20C8,
    0x4C69_105E,
    0xD560_41E4,
    0xA267_7172,
    0x3C03_E4D1,
    0x4B04_D447,
    0xD20D_85FD,
    0xA50A_B56B,
    0x35B5_A8FA,
    0x42B2_986C,
    0xDBBB_C9D6,
    0xACBC_F940,
    0x32D8_6CE3,
    0x45DF_5C75,
    0xDCD6_0DCF,
    0xABD1_3D59,
    0x26D9_30AC,
    0x51DE_003A,
    0xC8D7_5180,
    0xBFD0_6116,
    0x21B4_F4B5,
    0x56B3_C423,
    0xCFBA_9599,
    0xB8BD_A50F,
    0x2802_B89E,
    0x5F05_8808,
    0xC60C_D9B2,
    0xB10B_E924,
    0x2F6F_7C87,
    0x5868_4C11,
    0xC161_1DAB,
    0xB666_2D3D,
    0x76DC_4190,
    0x01DB_7106,
    0x98D2_20BC,
    0xEFD5_102A,
    0x71B1_8589,
    0x06B6_B51F,
    0x9FBF_E4A5,
    0xE8B8_D433,
    0x7807_C9A2,
    0x0F00_F934,
    0x9609_A88E,
    0xE10E_9818,
    0x7F6A_0DBB,
    0x086D_3D2D,
    0x9164_6C97,
    0xE663_5C01,
    0x6B6B_51F4,
    0x1C6C_6162,
    0x8565_30D8,
    0xF262_004E,
    0x6C06_95ED,
    0x1B01_A57B,
    0x8208_F4C1,
    0xF50F_C457,
    0x65B0_D9C6,
    0x12B7_E950,
    0x8BBE_B8EA,
    0xFCB9_887C,
    0x62DD_1DDF,
    0x15DA_2D49,
    0x8CD3_7CF3,
    0xFBD4_4C65,
    0x4DB2_6158,
    0x3AB5_51CE,
    0xA3BC_0074,
    0xD4BB_30E2,
    0x4ADF_A541,
    0x3DD8_95D7,
    0xA4D1_C46D,
    0xD3D6_F4FB,
    0x4369_E96A,
    0x346E_D9FC,
    0xAD67_8846,
    0xDA60_B8D0,
    0x4404_2D73,
    0x3303_1DE5,
    0xAA0A_4C5F,
    0xDD0D_7CC9,
    0x5005_713C,
    0x2702_41AA,
    0xBE0B_1010,
    0xC90C_2086,
    0x5768_B525,
    0x206F_85B3,
    0xB966_D409,
    0xCE61_E49F,
    0x5EDE_F90E,
    0x29D9_C998,
    0xB0D0_9822,
    0xC7D7_A8B4,
    0x59B3_3D17,
    0x2EB4_0D81,
    0xB7BD_5C3B,
    0xC0BA_6CAD,
    0xEDB8_8320,
    0x9ABF_B3B6,
    0x03B6_E20C,
    0x74B1_D29A,
    0xEAD5_4739,
    0x9DD2_77AF,
    0x04DB_2615,
    0x73DC_1683,
    0xE363_0B12,
    0x9464_3B84,
    0x0D6D_6A3E,
    0x7A6A_5AA8,
    0xE40E_CF0B,
    0x9309_FF9D,
    0x0A00_AE27,
    0x7D07_9EB1,
    0xF00F_9344,
    0x8708_A3D2,
    0x1E01_F268,
    0x6906_C2FE,
    0xF762_575D,
    0x8065_67CB,
    0x196C_3671,
    0x6E6B_06E7,
    0xFED4_1B76,
    0x89D3_2BE0,
    0x10DA_7A5A,
    0x67DD_4ACC,
    0xF9B9_DF6F,
    0x8EBE_EFF9,
    0x17B7_BE43,
    0x60B0_8ED5,
    0xD6D6_A3E8,
    0xA1D1_937E,
    0x38D8_C2C4,
    0x4FDF_F252,
    0xD1BB_67F1,
    0xA6BC_5767,
    0x3FB5_06DD,
    0x48B2_364B,
    0xD80D_2BDA,
    0xAF0A_1B4C,
    0x3603_4AF6,
    0x4104_7A60,
    0xDF60_EFC3,
    0xA867_DF55,
    0x316E_8EEF,
    0x4669_BE79,
    0xCB61_B38C,
    0xBC66_831A,
    0x256F_D2A0,
    0x5268_E236,
    0xCC0C_7795,
    0xBB0B_4703,
    0x2202_16B9,
    0x5505_262F,
    0xC5BA_3BBE,
    0xB2BD_0B28,
    0x2BB4_5A92,
    0x5CB3_6A04,
    0xC2D7_FFA7,
    0xB5D0_CF31,
    0x2CD9_9E8B,
    0x5BDE_AE1D,
    0x9B64_C2B0,
    0xEC63_F226,
    0x756A_A39C,
    0x026D_930A,
    0x9C09_06A9,
    0xEB0E_363F,
    0x7207_6785,
    0x0500_5713,
    0x95BF_4A82,
    0xE2B8_7A14,
    0x7BB1_2BAE,
    0x0CB6_1B38,
    0x92D2_8E9B,
    0xE5D5_BE0D,
    0x7CDC_EFB7,
    0x0BDB_DF21,
    0x86D3_D2D4,
    0xF1D4_E242,
    0x68DD_B3F8,
    0x1FDA_836E,
    0x81BE_16CD,
    0xF6B9_265B,
    0x6FB0_77E1,
    0x18B7_4777,
    0x8808_5AE6,
    0xFF0F_6A70,
    0x6606_3BCA,
    0x1101_0B5C,
    0x8F65_9EFF,
    0xF862_AE69,
    0x616B_FFD3,
    0x166C_CF45,
    0xA00A_E278,
    0xD70D_D2EE,
    0x4E04_8354,
    0x3903_B3C2,
    0xA767_2661,
    0xD060_16F7,
    0x4969_474D,
    0x3E6E_77DB,
    0xAED1_6A4A,
    0xD9D6_5ADC,
    0x40DF_0B66,
    0x37D8_3BF0,
    0xA9BC_AE53,
    0xDEBB_9EC5,
    0x47B2_CF7F,
    0x30B5_FFE9,
    0xBDBD_F21C,
    0xCABA_C28A,
    0x53B3_9330,
    0x24B4_A3A6,
    0xBAD0_3605,
    0xCDD7_0693,
    0x54DE_5729,
    0x23D9_67BF,
    0xB366_7A2E,
    0xC461_4AB8,
    0x5D68_1B02,
    0x2A6F_2B94,
    0xB40B_BE37,
    0xC30C_8EA1,
    0x5A05_DF1B,
    0x2D02_EF8D,
];

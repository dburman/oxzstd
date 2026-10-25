//! Decompression: frames (RFC 8878 §3.1), blocks (§3.1.1.2), literals (§3.1.1.3.1) and
//! sequences (§3.1.1.3.2), executed into one output buffer.

use crate::Error;
use crate::bits::BackwardBits;
use std::sync::OnceLock;

use crate::fse::{self, DecodeTable};
use crate::huffman;
use crate::inspect::SeqStats;
use crate::xxhash::Xxh64;

/// The Zstandard frame magic number.
pub(crate) const MAGIC: u32 = 0xFD2F_B528;
/// The largest block, decompressed.
pub(crate) const MAX_BLOCK: usize = 128 * 1024;
/// Spare bytes after the literals and after a block's output, so copies can move whole
/// 16-byte chunks without checking how much is left.
pub(crate) const SLACK: usize = 32;
/// The largest window the decoder accepts (log 31, as the format allows).
pub(crate) const MAX_WINDOW_LOG: u32 = 31;

/// Literal-length codes: baseline and extra bits.
pub(crate) const LL_CODES: [(u32, u8); 36] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 0),
    (12, 0),
    (13, 0),
    (14, 0),
    (15, 0),
    (16, 1),
    (18, 1),
    (20, 1),
    (22, 1),
    (24, 2),
    (28, 2),
    (32, 3),
    (40, 3),
    (48, 4),
    (64, 6),
    (128, 7),
    (256, 8),
    (512, 9),
    (1024, 10),
    (2048, 11),
    (4096, 12),
    (8192, 13),
    (16384, 14),
    (32768, 15),
    (65536, 16),
];

/// Match-length codes: baseline and extra bits.
pub(crate) const ML_CODES: [(u32, u8); 53] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 0),
    (12, 0),
    (13, 0),
    (14, 0),
    (15, 0),
    (16, 0),
    (17, 0),
    (18, 0),
    (19, 0),
    (20, 0),
    (21, 0),
    (22, 0),
    (23, 0),
    (24, 0),
    (25, 0),
    (26, 0),
    (27, 0),
    (28, 0),
    (29, 0),
    (30, 0),
    (31, 0),
    (32, 0),
    (33, 0),
    (34, 0),
    (35, 1),
    (37, 1),
    (39, 1),
    (41, 1),
    (43, 2),
    (47, 2),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 5),
    (131, 7),
    (259, 8),
    (515, 9),
    (1027, 10),
    (2051, 11),
    (4099, 12),
    (8195, 13),
    (16387, 14),
    (32771, 15),
    (65539, 16),
];

/// What a frame header says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameHeader {
    pub(crate) window_size: u64,
    pub(crate) content_size: Option<u64>,
    pub(crate) checksum: bool,
    pub(crate) dictionary_id: u32,
    pub(crate) header_len: usize,
}

pub(crate) fn le(data: &[u8]) -> u64 {
    data.iter()
        .rev()
        .fold(0u64, |acc, &b| (acc << 8) | u64::from(b))
}

/// Parses the frame header after the magic number.
pub(crate) fn frame_header(data: &[u8]) -> Result<FrameHeader, Error> {
    let descriptor = *data.first().ok_or(Error::Truncated)?;
    let fcs_flag = descriptor >> 6;
    let single_segment = descriptor & 0x20 != 0;
    if descriptor & 0x08 != 0 {
        return Err(Error::Corrupt("reserved frame header bit set"));
    }
    let checksum = descriptor & 0x04 != 0;
    let dict_len = [0usize, 1, 2, 4][(descriptor & 3) as usize];
    let fcs_len = match fcs_flag {
        0 => usize::from(single_segment),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let mut pos = 1;
    let mut window_size = 0u64;
    if !single_segment {
        let w = *data.get(pos).ok_or(Error::Truncated)?;
        pos += 1;
        let log = 10 + u32::from(w >> 3);
        if log > MAX_WINDOW_LOG {
            return Err(Error::Unsupported("window larger than 2 GiB"));
        }
        let base = 1u64 << log;
        window_size = base + (base / 8) * u64::from(w & 7);
    }
    let dict = data.get(pos..pos + dict_len).ok_or(Error::Truncated)?;
    let dictionary_id = le(dict) as u32;
    pos += dict_len;
    let content_size = if fcs_len == 0 {
        None
    } else {
        let raw = le(data.get(pos..pos + fcs_len).ok_or(Error::Truncated)?);
        pos += fcs_len;
        Some(if fcs_len == 2 { raw + 256 } else { raw })
    };
    if single_segment {
        window_size = content_size.unwrap_or(0);
    }
    Ok(FrameHeader {
        window_size,
        content_size,
        checksum,
        dictionary_id,
        header_len: pos,
    })
}

/// Entropy tables and repeat offsets carried from block to block within a frame, and the
/// buffers a [`Decompressor`] keeps from one frame to the next.
#[derive(Debug)]
pub(crate) struct State {
    huffman: Option<huffman::DecodeTable>,
    ll: Slot,
    of: Slot,
    ml: Slot,
    reps: [u32; 3],
    /// The current block's literals, then [`SLACK`] spare bytes.
    lits: Vec<u8>,
    stats: Option<SeqStats>,
}

/// A sequence table and whether this frame has set it yet (the table's memory outlives it).
#[derive(Debug, Default)]
struct Slot {
    table: SeqTable,
    valid: bool,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            huffman: None,
            ll: Slot::default(),
            of: Slot::default(),
            ml: Slot::default(),
            reps: [1, 4, 8],
            lits: Vec::new(),
            stats: None,
        }
    }

    /// Ready for a new frame: nothing carries over but memory.
    fn reset(&mut self) {
        self.huffman = None;
        self.ll.valid = false;
        self.of.valid = false;
        self.ml.valid = false;
        self.reps = [1, 4, 8];
        self.stats = None;
    }
}

/// Decompresses every frame in `src` (skippable frames are skipped).
///
/// # Errors
///
/// Any [`Error`] for malformed, truncated or unsupported input, or output over `limit`.
pub fn decompress_with_limit(src: &[u8], limit: usize) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    decompress_into(src, &mut out, limit)?;
    Ok(out)
}

/// Decompresses every frame in `src` onto the end of `out`, adding at most `limit` bytes.
/// Reusing `out` saves allocating (and faulting in) a new buffer each time.
///
/// # Errors
///
/// Any [`Error`] for malformed, truncated or unsupported input, or output over `limit`;
/// `out` is then as it was.
pub fn decompress_into(src: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), Error> {
    decompress_into_with(src, out, limit, &mut State::new())
}

/// Decompresses as much of `src` as is there: every whole block of the frames in it, stopping
/// without complaint where the input ends (in a frame's header, a block, or before the
/// checksum, which isn't checked then). For a stream that's still arriving, or one that was
/// flushed but not finished.
///
/// # Errors
///
/// Any [`Error`] but [`Error::Truncated`]: the data present is malformed.
pub fn decompress_partial(src: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    match decompress_onto(src, &mut out, usize::MAX, &mut State::new()) {
        Ok(()) | Err(Error::Truncated) => Ok(out),
        Err(e) => Err(e),
    }
}

/// [`decompress_into`] with the decoder's state (and its buffers) supplied.
pub(crate) fn decompress_into_with(
    src: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
    state: &mut State,
) -> Result<(), Error> {
    let start = out.len();
    let result = decompress_onto(src, out, start.saturating_add(limit), state);
    if result.is_err() {
        out.truncate(start);
    }
    result
}

/// [`decompress_into`] with `limit` as the most `out` may hold.
fn decompress_onto(
    src: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
    state: &mut State,
) -> Result<(), Error> {
    let mut pos = 0;
    if src.is_empty() {
        return Err(Error::Truncated);
    }
    while pos < src.len() {
        let magic = src
            .get(pos..pos + 4)
            .map(|m| u32::from_le_bytes([m[0], m[1], m[2], m[3]]))
            .ok_or(Error::Truncated)?;
        if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
            let size = src.get(pos + 4..pos + 8).ok_or(Error::Truncated)?;
            let size = u32::from_le_bytes([size[0], size[1], size[2], size[3]]) as usize;
            pos = pos
                .checked_add(8 + size)
                .filter(|&p| p <= src.len())
                .ok_or(Error::Truncated)?;
            continue;
        }
        if magic != MAGIC {
            return Err(Error::Corrupt("not a Zstandard frame"));
        }
        pos += 4;
        pos += decompress_frame(&src[pos..], out, limit, state)?;
    }
    Ok(())
}

/// Decompresses one frame (after its magic number) onto `out`; returns the bytes it took.
fn decompress_frame(
    src: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
    state: &mut State,
) -> Result<usize, Error> {
    decompress_frame_stats(src, out, limit, None, state)
}

pub(crate) fn decompress_frame_stats(
    src: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
    stats: Option<&mut SeqStats>,
    state: &mut State,
) -> Result<usize, Error> {
    let header = frame_header(src)?;
    if header.dictionary_id != 0 {
        return Err(Error::Unsupported("dictionaries"));
    }
    let frame_start = out.len();
    if let Some(size) = header.content_size {
        let size = usize::try_from(size).map_err(|_| Error::TooLarge)?;
        if out.len().saturating_add(size) > limit {
            return Err(Error::TooLarge);
        }
        // Reserve what's declared, but only as much as the frame could plausibly hold: a
        // tiny frame can claim any size.
        out.reserve(size.min((8 * MAX_BLOCK).max(src.len().saturating_mul(16))));
    }
    let block_max = usize::try_from(header.window_size)
        .unwrap_or(usize::MAX)
        .clamp(1, MAX_BLOCK);
    let mut pos = header.header_len;
    state.reset();
    if stats.is_some() {
        state.stats = Some(SeqStats::default());
    }
    loop {
        let h = src.get(pos..pos + 3).ok_or(Error::Truncated)?;
        let h = u32::from(h[0]) | (u32::from(h[1]) << 8) | (u32::from(h[2]) << 16);
        pos += 3;
        let last = h & 1 != 0;
        let kind = (h >> 1) & 3;
        let size = (h >> 3) as usize;
        match kind {
            0 => {
                if size > block_max {
                    return Err(Error::Corrupt("block larger than the maximum"));
                }
                let data = src.get(pos..pos + size).ok_or(Error::Truncated)?;
                check_limit(out, size, limit)?;
                out.extend_from_slice(data);
                pos += size;
            }
            1 => {
                if size > block_max {
                    return Err(Error::Corrupt("block larger than the maximum"));
                }
                let byte = *src.get(pos).ok_or(Error::Truncated)?;
                check_limit(out, size, limit)?;
                out.resize(out.len() + size, byte);
                pos += 1;
            }
            2 => {
                if size > block_max {
                    return Err(Error::Corrupt("block larger than the maximum"));
                }
                let data = src.get(pos..pos + size).ok_or(Error::Truncated)?;
                compressed_block(data, state, out, frame_start, block_max, limit)?;
                pos += size;
            }
            _ => return Err(Error::Corrupt("reserved block type")),
        }
        if last {
            break;
        }
    }
    let content = &out[frame_start..];
    if let Some(size) = header.content_size
        && content.len() as u64 != size
    {
        return Err(Error::Corrupt(
            "content size doesn't match the frame header",
        ));
    }
    if let (Some(dst), Some(st)) = (stats, state.stats) {
        *dst = st;
    }
    if header.checksum {
        let sum = src.get(pos..pos + 4).ok_or(Error::Truncated)?;
        let want = u32::from_le_bytes([sum[0], sum[1], sum[2], sum[3]]);
        let mut h = Xxh64::new(0);
        h.update(content);
        if h.digest() as u32 != want {
            return Err(Error::Checksum);
        }
        pos += 4;
    }
    Ok(pos)
}

fn check_limit(out: &[u8], add: usize, limit: usize) -> Result<(), Error> {
    if out.len().saturating_add(add) > limit {
        Err(Error::TooLarge)
    } else {
        Ok(())
    }
}

/// Reads the literals section into `state.lits`; returns how many literals there are and the
/// bytes the section took.
fn literals(data: &[u8], state: &mut State) -> Result<(usize, usize), Error> {
    let b0 = *data.first().ok_or(Error::Truncated)?;
    let kind = b0 & 3;
    let format = (b0 >> 2) & 3;
    match kind {
        0 | 1 => {
            let (size, hlen) = match format {
                0 | 2 => (usize::from(b0 >> 3), 1),
                1 => {
                    let b = data.get(..2).ok_or(Error::Truncated)?;
                    ((usize::from(b[0]) >> 4) | (usize::from(b[1]) << 4), 2)
                }
                _ => {
                    let b = data.get(..3).ok_or(Error::Truncated)?;
                    (
                        (usize::from(b[0]) >> 4)
                            | (usize::from(b[1]) << 4)
                            | (usize::from(b[2]) << 12),
                        3,
                    )
                }
            };
            if size > MAX_BLOCK {
                return Err(Error::Corrupt("too many literals"));
            }
            let lits = &mut state.lits;
            lits.clear();
            if kind == 0 {
                lits.extend_from_slice(data.get(hlen..hlen + size).ok_or(Error::Truncated)?);
                lits.resize(size + SLACK, 0);
                Ok((size, hlen + size))
            } else {
                let byte = *data.get(hlen).ok_or(Error::Truncated)?;
                lits.resize(size, byte);
                lits.resize(size + SLACK, 0);
                Ok((size, hlen + 1))
            }
        }
        _ => {
            let (hlen, bits) = match format {
                0 | 1 => (3, 10),
                2 => (4, 14),
                _ => (5, 18),
            };
            let four = format != 0;
            let h = le(data.get(..hlen).ok_or(Error::Truncated)?);
            let mask = (1u64 << bits) - 1;
            let regenerated = ((h >> 4) & mask) as usize;
            let compressed = ((h >> (4 + bits)) & mask) as usize;
            if regenerated > MAX_BLOCK {
                return Err(Error::Corrupt("too many literals"));
            }
            let body = data.get(hlen..hlen + compressed).ok_or(Error::Truncated)?;
            let streams = if kind == 2 {
                let (table, used) = huffman::read_tree(body)?;
                state.huffman = Some(table);
                &body[used..]
            } else {
                body
            };
            let table = state
                .huffman
                .as_ref()
                .ok_or(Error::Corrupt("treeless literals without a previous table"))?;
            table.decode(streams, regenerated, four, &mut state.lits)?;
            Ok((regenerated, hlen + compressed))
        }
    }
}

/// One cell of a sequence table: the FSE transition and what the symbol codes for, so the
/// decoder needs one lookup per field.
#[derive(Clone, Copy, Debug, Default)]
struct SeqCell {
    /// Where the next state starts, before its bits are added.
    next: u16,
    /// Bits to read for the next state.
    bits: u8,
    /// Extra bits of the value.
    extra: u8,
    /// The value with no extra bits.
    baseline: u32,
}

/// A literal-length, offset or match-length table, ready for decoding.
#[derive(Clone, Debug, Default)]
struct SeqTable {
    log: u32,
    cells: Vec<SeqCell>,
}

/// Which field of a sequence a table codes.
#[derive(Clone, Copy, Debug)]
enum Field {
    LitLen,
    Offset,
    MatchLen,
}

impl Field {
    /// The baseline and extra bits for a code, if it's one this field has.
    fn code(self, symbol: u8) -> Option<(u32, u8)> {
        match self {
            Field::LitLen => LL_CODES.get(usize::from(symbol)).copied(),
            Field::MatchLen => ML_CODES.get(usize::from(symbol)).copied(),
            Field::Offset => (symbol <= 31).then(|| (1 << symbol, symbol)),
        }
    }

    /// The largest symbol and accuracy log a description may have.
    fn limits(self) -> (usize, u32) {
        match self {
            Field::LitLen => (35, 9),
            Field::Offset => (31, 8),
            Field::MatchLen => (52, 9),
        }
    }

    /// The predefined table, built once.
    fn predefined(self) -> Result<&'static SeqTable, Error> {
        static TABLES: [OnceLock<Result<SeqTable, Error>>; 3] =
            [OnceLock::new(), OnceLock::new(), OnceLock::new()];
        let (i, counts, log): (usize, &[i16], u32) = match self {
            Field::LitLen => (0, &fse::LL_DEFAULT, 6),
            Field::Offset => (1, &fse::OF_DEFAULT, 5),
            Field::MatchLen => (2, &fse::ML_DEFAULT, 6),
        };
        TABLES[i]
            .get_or_init(|| SeqTable::new(&fse::build_decode_table(counts, log)?, self))
            .as_ref()
            .map_err(|e| *e)
    }
}

impl SeqTable {
    /// Rebuilds the table in place (reusing its cells) from normalized counts, as
    /// [`fse::build_decode_table`] spreads them, with each code's baseline and extra bits.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] if the counts don't fill the table exactly or name a code the field
    /// doesn't have.
    fn rebuild(&mut self, counts: &[i16], log: u32, field: Field) -> Result<(), Error> {
        // Sequence tables have at most 53 symbols and an accuracy log of at most 9.
        if counts.len() > 64 || log > 9 {
            return Err(Error::Corrupt("sequence table too large"));
        }
        let size = 1usize << log;
        let mut symbols = [0u8; 512];
        let mut next = [0u32; 64];
        let mut codes = [(0u32, 0u8); 64];
        let mut high = size;
        let mut total = 0usize;
        for (s, &c) in counts.iter().enumerate() {
            if c != 0 {
                codes[s] = field
                    .code(s as u8)
                    .ok_or(Error::Corrupt("sequence code out of range"))?;
            }
            if c == -1 {
                if high == 0 {
                    return Err(Error::Corrupt("FSE table overfull"));
                }
                high -= 1;
                symbols[high] = s as u8;
                next[s] = 1;
                total += 1;
            } else if c > 0 {
                next[s] = c as u32;
                total += c as usize;
            } else if c < -1 {
                return Err(Error::Corrupt("FSE count below -1"));
            }
        }
        if total != size {
            return Err(Error::Corrupt("FSE counts don't fill the table"));
        }
        let step = (size >> 1) + (size >> 3) + 3;
        let mask = size - 1;
        let mut position = 0usize;
        for (s, &c) in counts.iter().enumerate() {
            for _ in 0..c.max(0) {
                symbols[position] = s as u8;
                position = (position + step) & mask;
                while position >= high {
                    position = (position + step) & mask;
                }
            }
        }
        if position != 0 {
            return Err(Error::Corrupt("FSE spread did not end where it began"));
        }
        self.cells.clear();
        self.cells.extend(symbols[..size].iter().map(|&sym| {
            let s = usize::from(sym);
            let state = next[s];
            next[s] += 1;
            let bits = log - state.ilog2();
            let (baseline, extra) = codes[s];
            SeqCell {
                next: ((state << bits) - size as u32) as u16,
                bits: bits as u8,
                extra,
                baseline,
            }
        }));
        self.log = log;
        Ok(())
    }

    fn new(table: &DecodeTable, field: Field) -> Result<Self, Error> {
        let cells = table
            .cells
            .iter()
            .map(|c| {
                let (baseline, extra) = field
                    .code(c.symbol)
                    .ok_or(Error::Corrupt("sequence code out of range"))?;
                Ok(SeqCell {
                    next: c.base,
                    bits: c.bits,
                    extra,
                    baseline,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            log: table.log,
            cells,
        })
    }

    fn view(&self) -> TableView<'_> {
        TableView {
            log: self.log,
            cells: &self.cells,
        }
    }
}

/// A [`SeqTable`] borrowed for the decoding loop.
#[derive(Clone, Copy)]
struct TableView<'a> {
    log: u32,
    cells: &'a [SeqCell],
}

impl TableView<'_> {
    #[inline(always)]
    fn cell(&self, state: usize) -> SeqCell {
        self.cells[state]
    }

    /// Moves `state` on from `cell`, its cell, with the bits already in the window.
    #[inline(always)]
    fn update(cell: SeqCell, state: &mut usize, bits: &mut BackwardBits<'_>) {
        *state = usize::from(cell.next) + bits.read_fast(u32::from(cell.bits)) as usize;
    }
}

/// Picks or reads one sequence table per its mode; returns the bytes a description took.
fn table_for(mode: u8, data: &[u8], slot: &mut Slot, field: Field) -> Result<usize, Error> {
    let used = match mode {
        0 => {
            let predefined = field.predefined()?;
            slot.table.log = predefined.log;
            slot.table.cells.clone_from(&predefined.cells);
            0
        }
        1 => {
            let s = *data.first().ok_or(Error::Truncated)?;
            if usize::from(s) > field.limits().0 {
                return Err(Error::Corrupt("RLE symbol out of range"));
            }
            let (baseline, extra) = field
                .code(s)
                .ok_or(Error::Corrupt("sequence code out of range"))?;
            slot.table.log = 0;
            slot.table.cells.clear();
            slot.table.cells.push(SeqCell {
                next: 0,
                bits: 0,
                extra,
                baseline,
            });
            1
        }
        2 => {
            let (max_symbol, max_log) = field.limits();
            let (log, counts, used) = fse::read_counts(data, max_symbol, max_log)?;
            slot.table.rebuild(&counts, log, field)?;
            used
        }
        _ => {
            if !slot.valid {
                return Err(Error::Corrupt("repeated table without a previous one"));
            }
            0
        }
    };
    slot.valid = true;
    Ok(used)
}

/// The number of sequences and the bytes that field took (§3.1.1.3.2.1).
pub(crate) fn sequence_count(rest: &[u8]) -> Result<(usize, usize), Error> {
    let b0 = *rest.first().ok_or(Error::Truncated)?;
    Ok(if b0 < 128 {
        (usize::from(b0), 1)
    } else if b0 < 255 {
        let b1 = *rest.get(1).ok_or(Error::Truncated)?;
        (((usize::from(b0) - 128) << 8) + usize::from(b1), 2)
    } else {
        let b = rest.get(1..3).ok_or(Error::Truncated)?;
        (usize::from(b[0]) + (usize::from(b[1]) << 8) + 0x7F00, 3)
    })
}

fn compressed_block(
    data: &[u8],
    state: &mut State,
    out: &mut Vec<u8>,
    frame_start: usize,
    block_max: usize,
    limit: usize,
) -> Result<(), Error> {
    let (nlits, used) = literals(data, state)?;
    let rest = &data[used..];
    let (count, mut pos) = sequence_count(rest)?;
    if count == 0 {
        if pos != rest.len() {
            return Err(Error::Corrupt("data after an empty sequences section"));
        }
        check_limit(out, nlits, limit)?;
        check_block_size(nlits, block_max)?;
        out.extend_from_slice(&state.lits[..nlits]);
        return Ok(());
    }
    let modes = *rest.get(pos).ok_or(Error::Truncated)?;
    pos += 1;
    if modes & 3 != 0 {
        return Err(Error::Corrupt("reserved sequence mode bits set"));
    }
    pos += table_for(modes >> 6, &rest[pos..], &mut state.ll, Field::LitLen)?;
    pos += table_for((modes >> 4) & 3, &rest[pos..], &mut state.of, Field::Offset)?;
    pos += table_for(
        (modes >> 2) & 3,
        &rest[pos..],
        &mut state.ml,
        Field::MatchLen,
    )?;
    let stream = rest.get(pos..).ok_or(Error::Truncated)?;
    let block_start = out.len();
    // The block's whole output space, and slack for the chunked copies; cut back at the end.
    let room = block_max.min(limit.saturating_sub(block_start));
    let past = if room < block_max {
        Error::TooLarge
    } else {
        Error::Corrupt("block decompresses past the maximum")
    };
    out.resize(block_start + block_max + SLACK, 0);
    let seqs = Seqs {
        stream,
        count,
        nlits,
        frame_start,
        start: block_start,
        stop: block_start + room,
        past,
    };
    let result = if state.stats.is_some() {
        execute::<true>(&seqs, state, out)
    } else {
        execute::<false>(&seqs, state, out)
    };
    out.truncate(*result.as_ref().unwrap_or(&block_start));
    result.map(|_| ())
}

/// A block's sequences, and where their output goes.
struct Seqs<'a> {
    stream: &'a [u8],
    count: usize,
    nlits: usize,
    frame_start: usize,
    /// Where the block's output starts.
    start: usize,
    /// Where it has to end by.
    stop: usize,
    /// The error for output past `stop`.
    past: Error,
}

/// Decodes the sequences and executes them into `out`, writing nothing at or past `stop` but
/// the copies' slack. Returns where the output ends. `STATS` counts what the sequences use.
fn execute<const STATS: bool>(
    seqs: &Seqs<'_>,
    state: &mut State,
    out: &mut [u8],
) -> Result<usize, Error> {
    let State {
        ll,
        of,
        ml,
        reps,
        lits,
        stats,
        ..
    } = state;
    if !(ll.valid && of.valid && ml.valid) {
        return Err(Error::Corrupt("missing sequence tables"));
    }
    let (ll, of, ml) = (&ll.table, &of.table, &ml.table);
    let lits = lits.as_slice();
    // Views of the tables, so the loop keeps them in registers rather than rereading them
    // through `state`.
    let (ll, of, ml) = (ll.view(), of.view(), ml.view());
    let mut st = stats.unwrap_or_default();
    let mut bits = BackwardBits::new(seqs.stream)?;
    let mut ll_state = bits.read(ll.log) as usize;
    let mut of_state = bits.read(of.log) as usize;
    let mut ml_state = bits.read(ml.log) as usize;
    let mut lit_pos = 0usize;
    let mut op = seqs.start;
    for i in 0..seqs.count {
        let (llc, ofc, mlc) = (ll.cell(ll_state), of.cell(of_state), ml.cell(ml_state));
        // After a reload at least 57 bits are in the window. The extra bits of one sequence
        // can need 63 and the state updates 26, so reload between them when it's needed.
        // Reading past the start gives zeros, and the check at the end catches it.
        let extra = u32::from(ofc.extra) + u32::from(mlc.extra) + u32::from(llc.extra);
        bits.reload();
        let offset_value = u64::from(ofc.baseline) + bits.read_fast(u32::from(ofc.extra));
        if extra > 57 {
            bits.reload();
        }
        let match_len = mlc.baseline as usize + bits.read_fast(u32::from(mlc.extra)) as usize;
        let lit_len = llc.baseline as usize + bits.read_fast(u32::from(llc.extra)) as usize;
        if i + 1 < seqs.count {
            if extra > 57 - 26 {
                bits.reload();
            }
            TableView::update(llc, &mut ll_state, &mut bits);
            TableView::update(mlc, &mut ml_state, &mut bits);
            TableView::update(ofc, &mut of_state, &mut bits);
        }
        let offset = resolve_offset(offset_value, lit_len, reps)? as usize;
        if STATS {
            st.record(offset_value, lit_len, match_len);
        }
        if lit_len > seqs.nlits - lit_pos {
            return Err(Error::Corrupt("sequence uses more literals than there are"));
        }
        if lit_len + match_len > seqs.stop - op {
            return Err(seqs.past);
        }
        copy_literals(out, op, &lits[lit_pos..], lit_len);
        lit_pos += lit_len;
        op += lit_len;
        if offset > op - seqs.frame_start {
            return Err(Error::Corrupt("match offset before the start of the frame"));
        }
        copy_match(out, op, offset, match_len);
        op += match_len;
    }
    if !bits.finished() {
        return Err(Error::Corrupt("sequence stream has bits left over"));
    }
    if STATS {
        *stats = Some(st);
    }
    let rest = seqs.nlits - lit_pos;
    if rest > seqs.stop - op {
        return Err(seqs.past);
    }
    out[op..op + rest].copy_from_slice(&lits[lit_pos..seqs.nlits]);
    Ok(op + rest)
}

/// The actual offset for a decoded offset value, updating the repeat offsets (§3.1.2.5).
fn resolve_offset(offset_value: u64, lit_len: usize, reps: &mut [u32; 3]) -> Result<u32, Error> {
    Ok(if offset_value > 3 {
        let o = (offset_value - 3) as u32;
        reps[2] = reps[1];
        reps[1] = reps[0];
        reps[0] = o;
        o
    } else {
        // A repeat offset; with no literals before it, the codes shift by one.
        let index = offset_value as usize - usize::from(lit_len != 0);
        match index {
            0 => reps[0],
            1 => {
                reps.swap(0, 1);
                reps[0]
            }
            2 => {
                let o = reps[2];
                reps[2] = reps[1];
                reps[1] = reps[0];
                reps[0] = o;
                o
            }
            _ => {
                let o = reps[0].wrapping_sub(1);
                if o == 0 {
                    return Err(Error::Corrupt("repeat offset of zero"));
                }
                reps[2] = reps[1];
                reps[1] = reps[0];
                reps[0] = o;
                o
            }
        }
    })
}

fn check_block_size(size: usize, block_max: usize) -> Result<(), Error> {
    if size > block_max {
        Err(Error::Corrupt("block decompresses past the maximum"))
    } else {
        Ok(())
    }
}

/// Copies `len` literals from `lits` to `out` at `op`. Both have at least [`SLACK`] bytes
/// to spare, so a short run moves as one 16-byte chunk.
#[inline(always)]
fn copy_literals(out: &mut [u8], op: usize, lits: &[u8], len: usize) {
    out[op..op + 16].copy_from_slice(&lits[..16]);
    if len > 16 {
        if len <= LONG_COPY {
            let mut i = 16;
            while i < len {
                out[op + i..op + i + 16].copy_from_slice(&lits[i..i + 16]);
                i += 16;
            }
        } else {
            out[op + 16..op + len].copy_from_slice(&lits[16..len]);
        }
    }
}

/// Copies longer than this go to the library's copy; shorter ones move in 16-byte chunks.
const LONG_COPY: usize = 128;

/// Copies a match of `len` bytes from `offset` back to `op`, in 16-byte chunks that may run
/// up to 15 bytes past the end (into the slack, or over bytes still to be written).
#[inline(always)]
fn copy_match(out: &mut [u8], op: usize, offset: usize, len: usize) {
    let src = op - offset;
    let end = op + len;
    if offset >= 16 {
        // Each chunk reads only bytes already in place: they're at least 16 back.
        out.copy_within(src..src + 16, op);
        if len <= 16 {
            return;
        }
        if offset >= len && len > LONG_COPY {
            out.copy_within(src + 16..src + len, op + 16);
            return;
        }
        let mut d = op + 16;
        while d < end {
            out.copy_within(d - offset..d - offset + 16, d);
            d += 16;
        }
    } else {
        // A short offset repeats a pattern: lay out 16 bytes of it, then copy on from a
        // distance that's a whole number of periods and at least 16.
        let mut pattern = [0u8; 16];
        let mut j = 0;
        for b in &mut pattern {
            *b = out[src + j];
            j += 1;
            if j == offset {
                j = 0;
            }
        }
        out[op..op + 16].copy_from_slice(&pattern);
        let distance = 16usize.div_ceil(offset) * offset;
        let mut d = op + 16;
        while d < end {
            out.copy_within(d - distance..d - distance + 16, d);
            d += 16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_copy_like_one_byte_at_a_time() {
        // Lengths and offsets either side of the chunk size and of LONG_COPY.
        let history: Vec<u8> = (0..300u32).map(|i| (i * 89 + 7) as u8).collect();
        for offset in (1..=40).chain([127, 128, 129, 200, 300]) {
            for len in 3..=300 {
                let mut want = history.clone();
                for _ in 0..len {
                    want.push(want[want.len() - offset]);
                }
                let mut out = history.clone();
                out.resize(history.len() + len + SLACK, 0);
                copy_match(&mut out, history.len(), offset, len);
                assert_eq!(out[..want.len()], want[..], "offset {offset}, length {len}");
            }
        }
    }

    #[test]
    fn literals_copy_exactly_their_length() {
        let lits: Vec<u8> = (0..300u32)
            .map(|i| (i % 251) as u8)
            .chain([0; SLACK])
            .collect();
        for len in 0..=300 {
            let mut out = vec![0xEE; 3 + len + SLACK];
            copy_literals(&mut out, 3, &lits, len);
            assert_eq!(out[3..3 + len], lits[..len]);
            assert_eq!(out[..3], [0xEE; 3]);
        }
    }

    #[test]
    fn tables_rebuilt_in_place_match_the_two_step_build() {
        let fields = [
            (Field::LitLen, &fse::LL_DEFAULT[..], 6),
            (Field::Offset, &fse::OF_DEFAULT[..], 5),
            (Field::MatchLen, &fse::ML_DEFAULT[..], 6),
        ];
        // Reused for each, as a block after block would.
        let mut table = SeqTable {
            log: 0,
            cells: Vec::new(),
        };
        for (field, counts, log) in fields {
            let two_step =
                SeqTable::new(&fse::build_decode_table(counts, log).unwrap(), field).unwrap();
            table.rebuild(counts, log, field).unwrap();
            assert_eq!(table.log, two_step.log);
            assert_eq!(
                format!("{:?}", table.cells),
                format!("{:?}", two_step.cells)
            );
        }
        // A skewed distribution at the largest log, and one that doesn't fill the table.
        let mut skewed = vec![1i16; 36];
        skewed[0] = 512 - 35 - 4;
        skewed[35] = -1;
        skewed[34] = -1;
        skewed[33] = -1;
        skewed[32] = -1;
        skewed[1] = 5;
        let two_step =
            SeqTable::new(&fse::build_decode_table(&skewed, 9).unwrap(), Field::LitLen).unwrap();
        table.rebuild(&skewed, 9, Field::LitLen).unwrap();
        assert_eq!(
            format!("{:?}", table.cells),
            format!("{:?}", two_step.cells)
        );
        assert!(table.rebuild(&[1, 1, 1], 6, Field::LitLen).is_err());
    }

    #[test]
    fn frame_headers() {
        // Single segment, 1-byte content size 5, checksum.
        let h = frame_header(&[0x24, 5]).unwrap();
        assert_eq!(h.content_size, Some(5));
        assert_eq!(h.window_size, 5);
        assert!(h.checksum);
        assert_eq!(h.header_len, 2);
        // Window descriptor: exponent 0, mantissa 0 = 1 KiB; 2-byte size adds 256.
        let h = frame_header(&[0x40, 0x00, 0x10, 0x00]).unwrap();
        assert_eq!(h.window_size, 1024);
        assert_eq!(h.content_size, Some(16 + 256));
        assert!(frame_header(&[0x08]).is_err(), "reserved bit");
        assert!(frame_header(&[]).is_err());
    }
}

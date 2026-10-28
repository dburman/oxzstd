//! A compressed block's body: the literals section and the sequences section (RFC 8878
//! §3.1.1.3), with each table's mode chosen by estimated size.

use std::borrow::Cow;
use std::sync::OnceLock;

use super::seq::{Seq, SeqStore, ll_code_bits, ml_code_bits, of_code};
use crate::bitwriter::BitWriter;
use crate::fse;
use crate::fse_enc::{self, EncodeTable, Norm, TableView};
use crate::huf_enc::{self, Code};

/// A sequence table as the decoder will hold it after a block, for the repeat mode.
#[derive(Clone, Debug)]
pub(crate) enum Prior {
    Table { norm: Norm, log: u32 },
    Rle(u8),
}

/// What the decoder carries from block to block: tables for the repeat modes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Entropy {
    pub(crate) huffman: Option<Code>,
    pub(crate) ll: Option<Prior>,
    pub(crate) of: Option<Prior>,
    pub(crate) ml: Option<Prior>,
}

/// How far the literals have to shrink for Huffman to be worth it: zstd's rule of thumb.
fn min_gain(len: usize) -> usize {
    (len >> 6) + 2
}

/// The bytes of each end of the literals counted first when they look incompressible.
const SAMPLE: usize = 4096;

/// Whether the most frequent of `total` symbols, seen `largest` times, is too rare for
/// Huffman coding to gain anything worth having: the reference's rule.
fn flat(largest: u32, total: usize) -> bool {
    largest as usize <= (total >> 7) + 4
}

/// Writes the literals section for `lits` (between `seqs` sequences), possibly updating the
/// Huffman table in `entropy`.
pub(crate) fn literals(lits: &[u8], seqs: usize, entropy: &mut Entropy, out: &mut Vec<u8>) {
    if lits.len() > 1 && lits.iter().all(|&b| b == lits[0]) {
        rle_or_raw_header(1, lits.len(), out);
        out.push(lits[0]);
        return;
    }
    if lits.len() < 64 {
        raw(lits, out);
        return;
    }
    // Mostly literals: probably data that doesn't compress. Count a sample of each end first,
    // and if it's flat don't count the rest.
    if (seqs == 0 || lits.len() / seqs >= 20) && lits.len() >= 10 * SAMPLE {
        let mut sample = [0u32; 256];
        for &b in lits[..SAMPLE].iter().chain(&lits[lits.len() - SAMPLE..]) {
            sample[b as usize] += 1;
        }
        if flat(sample.iter().copied().max().unwrap_or(0), 2 * SAMPLE) {
            raw(lits, out);
            return;
        }
    }
    let mut counts = [0u32; 256];
    for &b in lits {
        counts[b as usize] += 1;
    }
    if flat(counts.iter().copied().max().unwrap_or(0), lits.len()) {
        raw(lits, out);
        return;
    }
    let Some(code) = huf_enc::build(&counts) else {
        raw(lits, out);
        return;
    };
    // A new tree, or the previous one if it covers these literals and costs less overall.
    let mut description = Vec::new();
    let fresh = code
        .write_description(&mut description)
        .map(|()| (code.cost(&counts) as usize).div_ceil(8) + description.len());
    let reuse = entropy
        .huffman
        .as_ref()
        .filter(|prior| prior.covers(&counts))
        .map(|prior| (prior.cost(&counts) as usize).div_ceil(8));
    let (treeless, estimate) = match (fresh, reuse) {
        (None, None) => {
            raw(lits, out);
            return;
        }
        (Some(f), Some(r)) => (r <= f, r.min(f)),
        (None, Some(r)) => (true, r),
        (Some(f), None) => (false, f),
    };
    // Not worth encoding if even the estimate doesn't gain enough (the jump table and header
    // only add to it).
    if estimate + 3 + min_gain(lits.len()) >= lits.len() {
        raw(lits, out);
        return;
    }
    let four = lits.len() >= 256;
    // Room for the streams as they're written (each in the worst case, 11 bits a literal).
    let mut body = Vec::with_capacity(description.len() + lits.len() * 11 / 8 + 64);
    let used = if treeless {
        entropy.huffman.clone()
    } else {
        body.extend_from_slice(&description);
        Some(code)
    };
    let Some(used) = used else {
        raw(lits, out);
        return;
    };
    let ok = if four {
        used.encode_four(lits, &mut body).is_some()
    } else {
        used.encode_stream(lits, &mut body);
        true
    };
    let header_len = if lits.len().max(body.len()) <= 1023 {
        3
    } else if lits.len().max(body.len()) <= 16383 {
        4
    } else {
        5
    };
    if !ok
        || body.len() + header_len + min_gain(lits.len()) >= lits.len()
        || body.len() > 262_143
        || (!four && header_len != 3)
    {
        raw(lits, out);
        return;
    }
    let kind = if treeless { 3u64 } else { 2 };
    let (format, bits) = match (four, header_len) {
        (false, _) => (0u64, 10),
        (true, 3) => (1, 10),
        (true, 4) => (2, 14),
        _ => (3, 18),
    };
    let h = kind | (format << 2) | ((lits.len() as u64) << 4) | ((body.len() as u64) << (4 + bits));
    out.extend_from_slice(&h.to_le_bytes()[..header_len]);
    out.extend_from_slice(&body);
    if !treeless {
        entropy.huffman = Some(used);
    }
}

fn raw(lits: &[u8], out: &mut Vec<u8>) {
    rle_or_raw_header(0, lits.len(), out);
    out.extend_from_slice(lits);
}

fn rle_or_raw_header(kind: u8, len: usize, out: &mut Vec<u8>) {
    if len <= 31 {
        out.push(kind | ((len as u8) << 3));
    } else if len <= 4095 {
        let v = u32::from(kind) | 0b0100 | ((len as u32) << 4);
        out.extend_from_slice(&v.to_le_bytes()[..2]);
    } else {
        let v = u32::from(kind) | 0b1100 | ((len as u32) << 4);
        out.extend_from_slice(&v.to_le_bytes()[..3]);
    }
}

/// One stream's choice: the mode bits, the table to encode with, and its description.
struct Choice {
    mode: u8,
    /// The predefined tables are shared, not copied.
    table: Cow<'static, EncodeTable>,
    description: Vec<u8>,
    prior: Prior,
}

/// A candidate mode, before its table is built.
enum Pick {
    Predefined,
    Repeat,
    Fresh { norm: Norm, log: u32 },
}

/// The predefined table for `default`, built once.
fn predefined(default: (&[i16], u32)) -> &'static EncodeTable {
    static TABLES: [OnceLock<EncodeTable>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    // The three predefined distributions differ in length.
    let i = match default.0.len() {
        36 => 0,
        29 => 1,
        _ => 2,
    };
    TABLES[i].get_or_init(|| EncodeTable::new(default.0, default.1))
}

/// Picks the cheapest mode for one stream's codes. Each candidate is priced first, and only
/// the winner's table is built.
fn choose(
    counts: &[u32],
    nb_seq: usize,
    default: (&[i16], u32),
    max_log: u32,
    prior: Option<&Prior>,
) -> Choice {
    let max = counts.iter().rposition(|&c| c > 0).unwrap_or(0);
    let used = counts.iter().filter(|&&c| c > 0).count();
    let rle = |mode| Choice {
        mode,
        table: Cow::Owned(EncodeTable::rle(max as u8)),
        description: if mode == 1 {
            vec![max as u8]
        } else {
            Vec::new()
        },
        prior: Prior::Rle(max as u8),
    };
    if used == 1 && nb_seq > 2 {
        return rle(1);
    }
    if let Some(Prior::Rle(s)) = prior
        && used == 1
        && max == *s as usize
    {
        return rle(3);
    }
    let mut best: Option<(u64, Pick)> = None;
    let mut consider = |bits: u64, pick: Pick| {
        if best.as_ref().is_none_or(|(b, _)| bits < *b) {
            best = Some((bits, pick));
        }
    };
    if let Some(bits) = fse_enc::cost(counts, default.0, default.1) {
        consider(bits, Pick::Predefined);
    }
    if let Some(Prior::Table { norm, log }) = prior
        && let Some(bits) = fse_enc::cost(counts, norm, *log)
    {
        consider(bits, Pick::Repeat);
    }
    let log = fse_enc::optimal_log(max_log, nb_seq, max);
    let mut description = Vec::new();
    if let Some(norm) = fse_enc::normalize(&counts[..=max], nb_seq, log, nb_seq >= 2048) {
        fse_enc::write_counts(&norm, log, &mut description);
        if let Some(bits) = fse_enc::cost(counts, &norm, log) {
            consider(
                bits + description.len() as u64 * 8 * 256,
                Pick::Fresh { norm, log },
            );
        }
    }
    match best.map(|(_, pick)| pick) {
        Some(Pick::Predefined) => Choice {
            mode: 0,
            table: Cow::Borrowed(predefined(default)),
            description: Vec::new(),
            prior: Prior::Table {
                norm: Norm::new(default.0),
                log: default.1,
            },
        },
        Some(Pick::Repeat) => {
            let Some(Prior::Table { norm, log }) = prior else {
                return rle(1);
            };
            Choice {
                mode: 3,
                table: Cow::Owned(EncodeTable::new(norm, *log)),
                description: Vec::new(),
                prior: Prior::Table {
                    norm: *norm,
                    log: *log,
                },
            }
        }
        Some(Pick::Fresh { norm, log }) => Choice {
            mode: 2,
            table: Cow::Owned(EncodeTable::new(&norm, log)),
            description,
            prior: Prior::Table { norm, log },
        },
        // Only reachable for an RLE-like stream of one or two sequences: code it as RLE.
        None => rle(1),
    }
}

/// Buffers the sequences section needs, kept from block to block.
#[derive(Debug, Default)]
pub(crate) struct Scratch {
    coded: Vec<Coded>,
}

/// Writes the sequences section for `seqs`, updating the tables in `entropy`.
pub(crate) fn sequences(
    seqs: &[Seq],
    entropy: &mut Entropy,
    scratch: &mut Scratch,
    out: &mut Vec<u8>,
) {
    let n = seqs.len();
    if n < 128 {
        out.push(n as u8);
    } else if n < 0x7F00 {
        out.extend_from_slice(&[((n >> 8) + 128) as u8, n as u8]);
    } else {
        let rest = (n - 0x7F00) as u16;
        out.push(255);
        out.extend_from_slice(&rest.to_le_bytes());
    }
    if n == 0 {
        return;
    }
    let coded = &mut scratch.coded;
    coded.clear();
    coded.extend(seqs.iter().map(Coded::new));
    let (ll_counts, of_counts, ml_counts) = count_codes(coded);
    let ll = choose(&ll_counts, n, (&fse::LL_DEFAULT, 6), 9, entropy.ll.as_ref());
    let of = choose(&of_counts, n, (&fse::OF_DEFAULT, 5), 8, entropy.of.as_ref());
    let ml = choose(&ml_counts, n, (&fse::ML_DEFAULT, 6), 9, entropy.ml.as_ref());
    out.push((ll.mode << 6) | (of.mode << 4) | (ml.mode << 2));
    out.extend_from_slice(&ll.description);
    out.extend_from_slice(&of.description);
    out.extend_from_slice(&ml.description);

    // A sequence takes at most 26 bits of states and 63 of extra bits: 12 bytes. The loop in
    // `encode_sequences` writes into exactly this, with 8 bytes to spare for its last flush.
    let mut w = BitWriter::appending(std::mem::take(out), n * 12 + 8);
    let last = coded[n - 1];
    let ml_state = ml.table.init(last.ml);
    let of_state = of.table.init(last.of);
    let ll_state = ll.table.init(last.ll);
    let lengths = u32::from(last.lengths);
    w.add(last.extra & ((1 << lengths) - 1), lengths);
    w.add(last.extra >> lengths, u32::from(last.of));
    w.flush();
    let views = [of.table.view(), ml.table.view(), ll.table.view()];
    let (mut w, [of_state, ml_state, ll_state]) =
        encode_sequences(w, &coded[..n - 1], views, [of_state, ml_state, ll_state]);
    ml.table.flush(&mut w, ml_state);
    of.table.flush(&mut w, of_state);
    ll.table.flush(&mut w, ll_state);
    *out = w.finish_backward();
    entropy.ll = Some(ll.prior);
    entropy.of = Some(of.prior);
    entropy.ml = Some(ml.prior);
}

/// A sequence as the encoding loop takes it: its three codes, and its extra bits already put
/// together, in the order they're written (literal length, match length, offset).
#[derive(Clone, Copy, Debug)]
struct Coded {
    extra: u64,
    /// How many extra bits there are, and how many of them are the lengths'.
    bits: u8,
    lengths: u8,
    ll: u8,
    of: u8,
    ml: u8,
}

impl Coded {
    #[inline(always)]
    fn new(s: &Seq) -> Self {
        let (ll, ll_bits) = ll_code_bits(s.lit_len);
        let (ml, ml_bits) = ml_code_bits(s.match_len);
        let of = of_code(s.off_value);
        let low = |value: u32, bits: u8| u64::from(value & ((1 << bits) - 1));
        // At most 16 + 16 + 31 bits.
        let lengths = ll_bits + ml_bits;
        let extra = low(s.lit_len, ll_bits)
            | low(s.match_len - 3, ml_bits) << ll_bits
            | low(s.off_value, of) << lengths;
        Self {
            extra,
            bits: lengths + of,
            lengths,
            ll,
            of,
            ml,
        }
    }
}

/// How often each literal-length, offset and match-length code occurs in `coded`. Counted
/// into four sets of tables in turn, so that a run of one code doesn't make each increment
/// wait on the one before it.
fn count_codes(coded: &[Coded]) -> ([u32; 36], [u32; 32], [u32; 53]) {
    // Each set: literal lengths at 0, offsets at 64, match lengths at 128. The masks only say
    // what's already so (the codes are in range), and spare the bounds checks.
    let mut sets = [[0u32; 192]; 4];
    let mut add = |set: usize, c: &Coded| {
        let t = &mut sets[set];
        t[c.ll as usize & 63] += 1;
        t[64 + (c.of as usize & 63)] += 1;
        t[128 + (c.ml as usize & 63)] += 1;
    };
    let (quads, rest) = coded.as_chunks::<4>();
    for q in quads {
        for (set, c) in q.iter().enumerate() {
            add(set, c);
        }
    }
    for c in rest {
        add(0, c);
    }
    let total = |at: usize, i: usize| sets.iter().map(|t| t[at + i]).sum::<u32>();
    (
        std::array::from_fn(|i| total(0, i)),
        std::array::from_fn(|i| total(64, i)),
        std::array::from_fn(|i| total(128, i)),
    )
}

/// Encodes `coded` last to first, from the states `[of, ml, ll]`; returns the writer and the
/// states after. A function of its own, taking the writer by value, so the writer, the
/// states and the tables all stay in registers.
#[inline(never)]
fn encode_sequences(
    w: BitWriter,
    coded: &[Coded],
    [of_view, ml_view, ll_view]: [TableView<'_>; 3],
    [mut of_state, mut ml_state, mut ll_state]: [u32; 3],
) -> (BitWriter, [u32; 3]) {
    let (mut out, mut w) = w.split();
    // Sized for the worst case (see `sequences`), so the loop never grows it.
    let buf = out.as_mut_slice();
    for c in coded.iter().rev() {
        // At most 7 bits are held after a flush; the states add up to 26 (8 + 9 + 9), the
        // extra bits up to 63, so flush between them as needed to stay within 64.
        of_view.encode(&mut w, &mut of_state, c.of);
        ml_view.encode(&mut w, &mut ml_state, c.ml);
        ll_view.encode(&mut w, &mut ll_state, c.ll);
        let bits = u32::from(c.bits);
        if bits >= 64 - 7 - 26 {
            w.flush_into(buf);
        }
        if bits > 56 {
            let lengths = u32::from(c.lengths);
            w.add_fast(c.extra & ((1 << lengths) - 1), lengths);
            w.flush_into(buf);
            w.add_fast(c.extra >> lengths, u32::from(c.of));
        } else {
            w.add_fast(c.extra, bits);
        }
        w.flush_into(buf);
    }
    (BitWriter::join(out, w), [of_state, ml_state, ll_state])
}

/// Writes the body of a compressed block for `store` onto `out`; returns the entropy state
/// after it. With `raw_literals` the literals are stored as they are.
pub(crate) fn body(
    store: &SeqStore,
    entropy: &Entropy,
    raw_literals: bool,
    scratch: &mut Scratch,
    out: &mut Vec<u8>,
) -> Entropy {
    let mut next = entropy.clone();
    if raw_literals {
        raw(store.lits(), out);
    } else {
        literals(store.lits(), store.seqs().len(), &mut next, out);
    }
    sequences(store.seqs(), &mut next, scratch, out);
    next
}

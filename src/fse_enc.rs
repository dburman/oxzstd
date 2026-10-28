//! FSE encoding: normalizing counts, writing table descriptions, and encoding symbols with
//! the same tables the decoder builds (RFC 8878 §4.1).

use std::sync::OnceLock;

use crate::bitwriter::{BitWriter, Bits};

/// The smallest and largest accuracy logs the encoder uses.
pub(crate) const MIN_LOG: u32 = 5;
pub(crate) const MAX_LOG: u32 = 12;

/// Picks an accuracy log for `total` symbols drawn from `0..=max_symbol`, at most `max_log`.
pub(crate) fn optimal_log(max_log: u32, total: usize, max_symbol: usize) -> u32 {
    let total = total.max(2) as u32;
    let max_bits_src = (total - 1).ilog2().saturating_sub(2);
    let min_bits = ((total).ilog2() + 1).min((max_symbol.max(1) as u32).ilog2() + 2);
    let mut log = max_log;
    if max_bits_src < log {
        log = max_bits_src;
    }
    if min_bits > log {
        log = min_bits;
    }
    log.clamp(MIN_LOG, MAX_LOG.min(max_log.max(MIN_LOG)))
}

/// Normalized counts of up to [`MAX_SYMBOLS`] symbols, held inline: tables are made for
/// every block, and most blocks are small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Norm {
    counts: [i16; MAX_SYMBOLS],
    len: usize,
}

impl Norm {
    /// `len` zeros.
    fn zeros(len: usize) -> Self {
        assert!(len <= MAX_SYMBOLS, "{len} symbols");
        Self {
            counts: [0; MAX_SYMBOLS],
            len,
        }
    }

    /// A copy of `counts` (at most [`MAX_SYMBOLS`] of them).
    pub(crate) fn new(counts: &[i16]) -> Self {
        let mut norm = Self::zeros(counts.len());
        norm.copy_from_slice(counts);
        norm
    }
}

impl std::ops::Deref for Norm {
    type Target = [i16];

    fn deref(&self) -> &[i16] {
        &self.counts[..self.len]
    }
}

impl std::ops::DerefMut for Norm {
    fn deref_mut(&mut self) -> &mut [i16] {
        &mut self.counts[..self.len]
    }
}

/// Scales `counts` (summing to `total`, at most [`MAX_SYMBOLS`] of them) to sum to
/// `1 << log`. Symbols too rare for one cell get −1 when `low_prob` is set (a cheaper single
/// cell), else 1. Returns `None` when one symbol has every count (RLE is the better choice
/// then).
pub(crate) fn normalize(counts: &[u32], total: usize, log: u32, low_prob: bool) -> Option<Norm> {
    const RTB: [u64; 8] = [
        0, 473_195, 504_333, 520_860, 550_000, 700_000, 750_000, 830_000,
    ];
    let total = total as u64;
    let scale = 62 - u64::from(log);
    let step = (1u64 << 62) / total;
    let v_step = 1u64 << (scale - 20);
    let mut to_distribute: i64 = 1 << log;
    let low_threshold = total >> log;
    let mut norm = Norm::zeros(counts.len());
    let mut largest = 0usize;
    let mut largest_p = 0i64;
    for (s, &c) in counts.iter().enumerate() {
        let c = u64::from(c);
        if c == total {
            return None;
        }
        if c == 0 {
            continue;
        }
        if c <= low_threshold {
            norm[s] = if low_prob { -1 } else { 1 };
            to_distribute -= 1;
            continue;
        }
        let mut proba = (c * step) >> scale;
        if proba < 8 {
            let rest_to_beat = v_step * RTB[proba as usize];
            if c * step - (proba << scale) > rest_to_beat {
                proba += 1;
            }
        }
        if proba as i64 > largest_p {
            largest_p = proba as i64;
            largest = s;
        }
        norm[s] = proba as i16;
        to_distribute -= proba as i64;
    }
    if -to_distribute >= i64::from(norm[largest]) >> 1 {
        // The largest symbol can't absorb the error: share it out instead.
        fix_by_steps(&mut norm, counts, 1 << log);
    } else {
        norm[largest] = (i64::from(norm[largest]) + to_distribute) as i16;
    }
    Some(norm)
}

/// Adjusts `norm` until its cells add up to `target`, one cell at a time on the symbols where
/// the change costs least.
fn fix_by_steps(norm: &mut [i16], counts: &[u32], target: i64) {
    let cells = |n: &[i16]| n.iter().map(|&v| i64::from(v.unsigned_abs())).sum::<i64>();
    let mut sum = cells(norm);
    while sum != target {
        // The symbol whose cell ratio (cells per count) is furthest from fair in the needed
        // direction, among those that can move.
        let mut best: Option<(usize, f64)> = None;
        for (s, &c) in counts.iter().enumerate() {
            let n = norm[s];
            if c == 0 || n <= 0 {
                continue;
            }
            if sum > target && n <= 1 {
                continue;
            }
            let ratio = f64::from(n) / f64::from(c);
            let better = match best {
                None => true,
                Some((_, r)) => {
                    if sum > target {
                        ratio > r
                    } else {
                        ratio < r
                    }
                }
            };
            if better {
                best = Some((s, ratio));
            }
        }
        let Some((s, _)) = best else {
            break;
        };
        if sum > target {
            norm[s] -= 1;
            sum -= 1;
        } else {
            norm[s] += 1;
            sum += 1;
        }
    }
}

/// Writes a table description (the inverse of `fse::read_counts`).
pub(crate) fn write_counts(norm: &[i16], log: u32, out: &mut Vec<u8>) {
    // At most 256 symbols of up to 16 bits, and the repeat flags.
    let mut w = BitWriter::appending(std::mem::take(out), 600);
    w.add(u64::from(log - 5), 4);
    let mut remaining: i32 = (1 << log) + 1;
    let mut threshold: i32 = 1 << log;
    let mut nbits = log + 1;
    let mut previous_zero = false;
    let mut s = 0usize;
    while s < norm.len() && remaining > 1 {
        if previous_zero {
            let mut start = s;
            while s < norm.len() && norm[s] == 0 {
                s += 1;
            }
            if s == norm.len() {
                break;
            }
            while s >= start + 24 {
                start += 24;
                w.add(0xFFFF, 16);
            }
            while s >= start + 3 {
                start += 3;
                w.add(3, 2);
            }
            w.add((s - start) as u64, 2);
        }
        let mut count = i32::from(norm[s]);
        s += 1;
        let max = (2 * threshold - 1) - remaining;
        remaining -= count.abs();
        count += 1;
        if count >= threshold {
            count += max;
        }
        let bits = if count < max { nbits - 1 } else { nbits };
        w.add(count as u64, bits);
        previous_zero = count == 1;
        while remaining < threshold && nbits > 1 {
            nbits -= 1;
            threshold >>= 1;
        }
    }
    *out = w.finish();
}

/// A symbol's encoding transform.
#[derive(Clone, Copy, Debug, Default)]
struct Transform {
    delta_bits: u32,
    delta_state: i32,
}

/// An [`EncodeTable`] borrowed for an encoding loop: one plain reference, so the loop keeps
/// it in a register.
#[derive(Clone, Copy)]
pub(crate) struct TableView<'a> {
    cells: &'a Cells,
}

impl TableView<'_> {
    /// Encodes `symbol` without flushing: at most the table's log in bits. An RLE table needs
    /// no special case: its one transform is all zeros, so it writes nothing and stays in
    /// state 0.
    #[inline(always)]
    pub(crate) fn encode(self, w: &mut Bits, state: &mut u32, symbol: u8) {
        // The masks only say what's already so (the tables are this big), and spare the
        // bounds checks.
        let t = self.cells.symbols[symbol as usize & (MAX_SYMBOLS - 1)];
        let bits_out = (*state + t.delta_bits) >> 16;
        w.add_fast(u64::from(*state & ((1 << bits_out) - 1)), bits_out);
        let next = ((*state >> bits_out) as i32 + t.delta_state) as usize;
        *state = u32::from(self.cells.states[next & (MAX_STATES - 1)]);
    }
}

const MAX_STATES: usize = 512;
const MAX_SYMBOLS: usize = 64;

/// An encoding table.
#[derive(Clone, Debug)]
pub(crate) struct EncodeTable {
    log: u32,
    cells: Box<Cells>,
}

/// A table's transforms and states, together: an encoding loop needs one pointer per table.
#[derive(Clone, Debug)]
struct Cells {
    /// Per symbol, for up to 64 (the most a sequence table has is 53).
    symbols: [Transform; MAX_SYMBOLS],
    /// The states, for a table of up to 512 (log 9).
    states: [u16; MAX_STATES],
}

impl EncodeTable {
    /// Builds the table for `norm` (the same spread as the decoder's).
    pub(crate) fn new(norm: &[i16], log: u32) -> Self {
        let size = 1usize << log;
        let mask = size - 1;
        let mut high = size - 1;
        // On the stack: tables are built for every block, and most blocks are small.
        let mut cumul = [0u32; MAX_SYMBOLS + 1];
        let mut spread = [0u8; MAX_STATES];
        for (s, &n) in norm.iter().enumerate() {
            if n == -1 {
                cumul[s + 1] = cumul[s] + 1;
                spread[high] = s as u8;
                high = high.wrapping_sub(1);
            } else {
                cumul[s + 1] = cumul[s] + n.max(0) as u32;
            }
        }
        let high_limit = high.wrapping_add(1);
        let step = (size >> 1) + (size >> 3) + 3;
        let mut position = 0usize;
        for (s, &n) in norm.iter().enumerate() {
            for _ in 0..n.max(0) {
                spread[position] = s as u8;
                position = (position + step) & mask;
                while position >= high_limit {
                    position = (position + step) & mask;
                }
            }
        }
        let mut cells = Box::new(Cells {
            symbols: [Transform::default(); MAX_SYMBOLS],
            states: [0; MAX_STATES],
        });
        let states = &mut cells.states;
        let mut next = cumul;
        for (u, &s) in spread[..size].iter().enumerate() {
            let s = s as usize;
            states[next[s] as usize] = (size + u) as u16;
            next[s] += 1;
        }
        let symbols = &mut cells.symbols;
        let mut total: i32 = 0;
        for (s, &n) in norm.iter().enumerate() {
            symbols[s] = match n {
                0 => Transform {
                    delta_bits: ((log + 1) << 16) - (1 << log),
                    delta_state: 0,
                },
                -1 | 1 => {
                    let t = Transform {
                        delta_bits: (log << 16) - (1 << log),
                        delta_state: total - 1,
                    };
                    total += 1;
                    t
                }
                _ => {
                    let n = n as u32;
                    let max_bits_out = log - (n - 1).ilog2();
                    let min_state_plus = n << max_bits_out;
                    let t = Transform {
                        delta_bits: (max_bits_out << 16).wrapping_sub(min_state_plus),
                        delta_state: total - n as i32,
                    };
                    total += n as i32;
                    t
                }
            };
        }
        Self { log, cells }
    }

    /// The table for one symbol that needs no bits (RLE mode).
    pub(crate) fn rle(symbol: u8) -> Self {
        let mut cells = Box::new(Cells {
            symbols: [Transform::default(); MAX_SYMBOLS],
            states: [0; MAX_STATES],
        });
        cells.symbols[symbol as usize] = Transform {
            delta_bits: 0,
            delta_state: 0,
        };
        Self { log: 0, cells }
    }

    /// The starting state for the last symbol encoded (the first one decoded).
    #[inline]
    pub(crate) fn init(&self, symbol: u8) -> u32 {
        if self.log == 0 {
            return 0;
        }
        let t = self.cells.symbols[symbol as usize];
        let bits_out = (t.delta_bits + (1 << 15)) >> 16;
        let value = (bits_out << 16).wrapping_sub(t.delta_bits);
        u32::from(self.cells.states[((value >> bits_out) as i32 + t.delta_state) as usize])
    }

    /// Encodes `symbol`, writing the bits that let the decoder step back to `state`.
    #[inline]
    pub(crate) fn encode(&self, w: &mut BitWriter, state: &mut u32, symbol: u8) {
        if self.log == 0 {
            return;
        }
        let t = self.cells.symbols[symbol as usize];
        let bits_out = (*state + t.delta_bits) >> 16;
        w.add(u64::from(*state), bits_out);
        let next = ((*state >> bits_out) as i32 + t.delta_state) as usize;
        *state = u32::from(self.cells.states[next]);
    }

    /// The table borrowed for an encoding loop.
    pub(crate) fn view(&self) -> TableView<'_> {
        TableView { cells: &self.cells }
    }

    /// Writes the final state.
    #[inline]
    pub(crate) fn flush(&self, w: &mut BitWriter, state: u32) {
        w.add(u64::from(state), self.log);
    }
}

/// The cost of `counts` coded with the distribution `norm` (accuracy `log`, at most 12), in
/// 1/256 bits; `None` if a counted symbol has no cell.
pub(crate) fn cost(counts: &[u32], norm: &[i16], log: u32) -> Option<u64> {
    // Fixed point: what a symbol of probability i / 4096 costs, in 1/256 bits, computed once.
    static COST: OnceLock<Vec<u32>> = OnceLock::new();
    let table = COST.get_or_init(|| {
        std::iter::once(0)
            .chain((1..=4096u32).map(|i| (-(f64::from(i) / 4096.0).log2() * 256.0) as u32))
            .collect()
    });
    let shift = 12 - log;
    let mut bits = 0u64;
    for (s, &c) in counts.iter().enumerate() {
        if c == 0 {
            continue;
        }
        let n = *norm.get(s)?;
        if n == 0 {
            return None;
        }
        bits += u64::from(c) * u64::from(table[usize::from(n.unsigned_abs()) << shift]);
    }
    Some(bits)
}

/// Encodes `symbols` with two interleaved states (how Huffman weight lists are coded).
/// Returns `None` for fewer than 3 symbols.
pub(crate) fn encode_interleaved(symbols: &[u8], table: &EncodeTable) -> Option<Vec<u8>> {
    if symbols.len() <= 2 {
        return None;
    }
    let mut w = BitWriter::with_capacity(symbols.len());
    let n = symbols.len();
    let (mut s1, mut s2) = (0u32, 0u32);
    for i in (0..n).rev() {
        let sym = symbols[i];
        // Even positions go through the first state, odd ones through the second; the last
        // two symbols seed the states.
        let state = if i % 2 == 0 { &mut s1 } else { &mut s2 };
        if i + 2 >= n {
            *state = table.init(sym);
        } else {
            table.encode(&mut w, state, sym);
        }
    }
    // The first state is read first, so it's flushed last.
    table.flush(&mut w, s2);
    table.flush(&mut w, s1);
    Some(w.finish_backward())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::BackwardBits;
    use crate::fse;

    fn counts_of(data: &[u8], max: usize) -> Vec<u32> {
        let mut c = vec![0u32; max + 1];
        for &b in data {
            c[b as usize] += 1;
        }
        c
    }

    #[test]
    fn normalized_counts_fill_the_table() {
        let data: Vec<u8> = (0..1000u32).map(|i| ((i * i) % 7) as u8).collect();
        let counts = counts_of(&data, 6);
        for log in 5..=9 {
            for low in [false, true] {
                let norm = normalize(&counts, data.len(), log, low).unwrap();
                let sum: i32 = norm.iter().map(|&n| i32::from(n.unsigned_abs())).sum();
                assert_eq!(sum, 1 << log, "log {log}");
            }
        }
        assert!(normalize(&[0, 5, 0], 5, 5, false).is_none(), "one symbol");
    }

    #[test]
    fn descriptions_read_back() {
        let data: Vec<u8> = (0..5000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 27) as u8 % 20)
            .collect();
        let mut counts = counts_of(&data, 30);
        counts[25] = 1; // a rare symbol, and zeros between
        let total: u32 = counts.iter().sum();
        for low in [false, true] {
            let norm = normalize(&counts, total as usize, 8, low).unwrap();
            let mut desc = Vec::new();
            write_counts(&norm, 8, &mut desc);
            let (log, read, used) = fse::read_counts(&desc, 30, 9).unwrap();
            assert_eq!(log, 8);
            assert_eq!(used, desc.len());
            assert_eq!(&read[..], &norm[..read.len()]);
            assert!(norm[read.len()..].iter().all(|&n| n == 0));
        }
    }

    #[test]
    fn encoded_symbols_decode() {
        let even: Vec<u8> = (0..3000u32).map(|i| ((i * 7919) % 13) as u8 % 9).collect();
        // Mostly 0 to 3, with a few rarer symbols: below one cell with `low_prob`.
        let skewed: Vec<u8> = (0..3000u32)
            .map(|i| {
                if i % 97 == 0 {
                    4 + (i % 5) as u8
                } else {
                    (i % 4) as u8
                }
            })
            .collect();
        for data in [&even, &skewed] {
            for low in [true, false] {
                let counts = counts_of(data, 8);
                let norm = normalize(&counts, data.len(), 7, low).unwrap();
                // Counts both with single cells at the top of the table and without.
                assert_eq!(norm.contains(&-1), low && data == &skewed);
                round_trip(data, &norm);
            }
        }
    }

    fn round_trip(data: &[u8], norm: &[i16]) {
        let enc = EncodeTable::new(norm, 7);
        // One state, symbols encoded last to first.
        let mut w = BitWriter::with_capacity(data.len());
        let mut state = enc.init(data[data.len() - 1]);
        for &s in data[..data.len() - 1].iter().rev() {
            enc.encode(&mut w, &mut state, s);
        }
        enc.flush(&mut w, state);
        let bytes = w.finish_backward();
        let dec = fse::build_decode_table(norm, 7).unwrap();
        let mut r = BackwardBits::new(&bytes).unwrap();
        let mut st = dec.init(&mut r);
        let mut out = Vec::new();
        for i in 0..data.len() {
            out.push(dec.symbol(st));
            if i + 1 < data.len() {
                dec.update(&mut st, &mut r);
            }
        }
        assert!(r.finished());
        assert_eq!(out, data);
    }

    #[test]
    fn small_logs_are_chosen_for_small_inputs() {
        assert_eq!(optimal_log(9, 10, 35), 5);
        assert_eq!(optimal_log(9, 100_000, 35), 9);
        assert!(optimal_log(6, 200, 12) <= 6);
    }
}

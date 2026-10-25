//! Sequences as the match finders produce them, their codes, and the repeat-offset history,
//! kept exactly as the decoder will.

use crate::decode::{LL_CODES, ML_CODES};

/// One sequence: literals, then a match `offset` bytes back of `match_len` bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seq {
    pub(crate) lit_len: u32,
    pub(crate) match_len: u32,
    /// The offset value as coded: 1-3 are repeat codes, above that the offset plus 3.
    pub(crate) off_value: u32,
}

/// The literals and sequences of one block, written by index into buffers sized for the
/// block up front: the finders' loops record a match with plain stores, never a call to grow
/// a buffer (which would make them keep their state in memory rather than registers).
#[derive(Debug, Default)]
pub(crate) struct SeqStore {
    /// `nlits` literals, then room.
    lits: Vec<u8>,
    nlits: usize,
    /// `nseqs` sequences, then room.
    seqs: Vec<Seq>,
    nseqs: usize,
}

impl SeqStore {
    /// Empties the store, with room for a block of `len` bytes: every byte as a literal (and
    /// 16 to spare, for the chunked copy), and a sequence for every 3 bytes (the shortest
    /// match).
    pub(crate) fn prepare(&mut self, len: usize) {
        self.nlits = 0;
        self.nseqs = 0;
        if self.lits.len() < len + 16 {
            self.lits.resize(len + 16, 0);
        }
        if self.seqs.len() < len / 3 + 1 {
            self.seqs.resize(len / 3 + 1, Seq::default());
        }
    }

    /// The literals so far.
    pub(crate) fn lits(&self) -> &[u8] {
        &self.lits[..self.nlits]
    }

    /// The sequences so far.
    pub(crate) fn seqs(&self) -> &[Seq] {
        &self.seqs[..self.nseqs]
    }

    #[inline(always)]
    pub(crate) fn push_seq(&mut self, seq: Seq) {
        self.seqs[self.nseqs] = seq;
        self.nseqs += 1;
    }

    /// Appends `src[from..to]` as literals. Most runs between matches are short or empty: a
    /// short one copies as one 16-byte chunk (the rest is overwritten by what comes next), so
    /// there's no call to the library copy.
    #[inline(always)]
    pub(crate) fn push_lits(&mut self, src: &[u8], from: usize, to: usize) {
        let len = to - from;
        if len == 0 {
            return;
        }
        let at = self.nlits;
        match src.get(from..from + 16) {
            Some(chunk) if len <= 16 => self.lits[at..at + 16].copy_from_slice(chunk),
            _ => self.lits[at..at + len].copy_from_slice(&src[from..to]),
        }
        self.nlits += len;
    }
}

/// The three repeat offsets, updated as the decoder updates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reps(pub(crate) [u32; 3]);

impl Default for Reps {
    fn default() -> Self {
        Self([1, 4, 8])
    }
}

impl Reps {
    /// The offset value for a match at `offset` after `lit_len` literals, using a repeat code
    /// when one fits, and the history after it.
    pub(crate) fn code(&mut self, offset: u32, lit_len: u32) -> u32 {
        let r = &mut self.0;
        if lit_len != 0 {
            if offset == r[0] {
                1
            } else if offset == r[1] {
                r.swap(0, 1);
                2
            } else if offset == r[2] {
                *r = [r[2], r[0], r[1]];
                3
            } else {
                *r = [offset, r[0], r[1]];
                offset + 3
            }
        } else if offset == r[1] {
            r.swap(0, 1);
            1
        } else if offset == r[2] {
            *r = [r[2], r[0], r[1]];
            2
        } else if r[0] > 1 && offset == r[0] - 1 {
            *r = [offset, r[0], r[1]];
            3
        } else {
            *r = [offset, r[0], r[1]];
            offset + 3
        }
    }
}

impl Reps {
    /// The offset an offset value means after `lit_len` literals, as the decoder reads it.
    pub(crate) fn resolve(&self, off_value: u32, lit_len: u32) -> u32 {
        let r = &self.0;
        if off_value > 3 {
            return off_value - 3;
        }
        match off_value - 1 + u32::from(lit_len == 0) {
            0 => r[0],
            1 => r[1],
            2 => r[2],
            _ => r[0].wrapping_sub(1),
        }
    }

    /// Updates the history for a match coded as `off_value`, as the decoder does.
    pub(crate) fn apply(&mut self, off_value: u32, lit_len: u32) {
        let offset = self.resolve(off_value, lit_len);
        let r = &mut self.0;
        if off_value > 3 {
            *r = [offset, r[0], r[1]];
            return;
        }
        match off_value - 1 + u32::from(lit_len == 0) {
            0 => {}
            1 => *r = [offset, r[0], r[2]],
            _ => *r = [offset, r[0], r[1]],
        }
    }
}

const fn build_ll() -> [u8; 64] {
    let mut t = [0u8; 64];
    let mut code = 0;
    while code < LL_CODES.len() {
        let (base, bits) = LL_CODES[code];
        let mut v = base;
        while v < base + (1 << bits) && v < 64 {
            t[v as usize] = code as u8;
            v += 1;
        }
        code += 1;
    }
    t
}

const fn build_ml() -> [u8; 128] {
    let mut t = [0u8; 128];
    let mut code = 0;
    while code < ML_CODES.len() {
        let (base, bits) = ML_CODES[code];
        let mut v = base - 3;
        while v < base - 3 + (1 << bits) && v < 128 {
            t[v as usize] = code as u8;
            v += 1;
        }
        code += 1;
    }
    t
}

const LL_TABLE: [u8; 64] = build_ll();
const ML_TABLE: [u8; 128] = build_ml();

/// The literal-length code for `lit_len`.
#[inline]
pub(crate) fn ll_code(lit_len: u32) -> u8 {
    if lit_len < 64 {
        LL_TABLE[lit_len as usize]
    } else {
        (lit_len.ilog2() + 19) as u8
    }
}

/// The match-length code for `match_len` (at least 3).
#[inline]
pub(crate) fn ml_code(match_len: u32) -> u8 {
    let base = match_len - 3;
    if base < 128 {
        ML_TABLE[base as usize]
    } else {
        (base.ilog2() + 36) as u8
    }
}

/// The offset code for an offset value.
#[inline]
pub(crate) fn of_code(off_value: u32) -> u8 {
    off_value.ilog2() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_agree_with_the_decoders_tables() {
        // Up to the largest lengths the format can code (a block is at most 128 KiB).
        for ll in 0..131_072u32 {
            let c = ll_code(ll) as usize;
            let (base, bits) = LL_CODES[c];
            assert!(ll >= base && ll < base + (1 << bits), "ll {ll} -> code {c}");
        }
        for ml in 3..131_075u32 {
            let c = ml_code(ml) as usize;
            let (base, bits) = ML_CODES[c];
            assert!(ml >= base && ml < base + (1 << bits), "ml {ml} -> code {c}");
        }
        assert_eq!(of_code(1), 0);
        assert_eq!(of_code(4), 2);
    }

    #[test]
    fn resolving_and_applying_agree_with_coding() {
        // Whatever `code` picks, `resolve` reads back the offset and `apply` the history.
        let mut rng = 12345u64;
        let mut coder = Reps::default();
        let mut reader = Reps::default();
        for _ in 0..10_000 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let offset = [1, 2, 3, 4, 8, 9, 100][(rng % 7) as usize] as u32;
            let lit_len = ((rng >> 8) % 3) as u32;
            let value = coder.code(offset, lit_len);
            assert_eq!(reader.resolve(value, lit_len), offset);
            reader.apply(value, lit_len);
            assert_eq!(reader, coder);
        }
    }

    #[test]
    fn repeat_codes_follow_the_decoder() {
        let mut r = Reps::default();
        assert_eq!(r.code(1, 5), 1, "rep 1 with literals");
        assert_eq!(r.code(8, 5), 3, "rep 3 with literals");
        assert_eq!(r.0, [8, 1, 4]);
        assert_eq!(r.code(1, 0), 1, "without literals, code 1 is rep 2");
        assert_eq!(r.0, [1, 8, 4]);
        // Without literals, code 3 means rep 1 minus one; rep 1 is 1 here, so it can't.
        assert_eq!(r.code(100, 0), 103);
        assert_eq!(r.0, [100, 1, 8]);
        assert_eq!(r.code(99, 0), 3, "rep 1 - 1");
        assert_eq!(r.0, [99, 100, 1]);
    }
}

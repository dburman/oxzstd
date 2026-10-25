//! What every match finder shares: reading the input, hashing, measuring matches, and
//! recording sequences.

use std::ops::Range;

use super::seq::{Reps, Seq, SeqStore};

/// A match finder: turns one block of `src` into sequences, using everything before the
/// block as history. `Send` and `Sync`, so the compressors that hold one are too.
pub(crate) trait MatchFinder: Send + Sync {
    /// Finds sequences for `src[block]`, appending them and their literals to `store` and
    /// updating `reps` the way the decoder will.
    fn find(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore);

    /// Forgets everything, as if new, keeping the tables' memory (for the next frame).
    fn reset(&mut self);
}

#[inline(always)]
pub(crate) fn read32(src: &[u8], pos: usize) -> u32 {
    // Copied whole, so it compiles to one load (built byte by byte it can become four).
    let mut a = [0u8; 4];
    a.copy_from_slice(&src[pos..pos + 4]);
    u32::from_le_bytes(a)
}

#[inline(always)]
pub(crate) fn read64(src: &[u8], pos: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&src[pos..pos + 8]);
    u64::from_le_bytes(a)
}

const PRIME3: u32 = 506_832_829;
const PRIME4: u32 = 2_654_435_761;
const PRIME5: u64 = 889_523_592_379;
const PRIME6: u64 = 227_718_039_650_203;
const PRIME7: u64 = 58_295_818_150_454_627;
const PRIME8: u64 = 0xCF1B_BCDC_B7A5_6463;

/// A hash of the `MLS` (3 to 8) bytes at `pos`, `bits` wide. Needs 8 readable bytes for
/// `MLS` over 4. The finders are generic over `MLS`, so this compiles to one case.
#[inline(always)]
pub(crate) fn hash<const MLS: u32>(src: &[u8], pos: usize, bits: u32) -> usize {
    hash_dyn(src, pos, MLS, bits)
}

/// [`hash`] with the length known only at run time.
#[inline(always)]
pub(crate) fn hash_dyn(src: &[u8], pos: usize, mls: u32, bits: u32) -> usize {
    match mls {
        3 => ((read32(src, pos) << 8).wrapping_mul(PRIME3) >> (32 - bits)) as usize,
        4 => (read32(src, pos).wrapping_mul(PRIME4) >> (32 - bits)) as usize,
        5 => ((read64(src, pos) << 24).wrapping_mul(PRIME5) >> (64 - bits)) as usize,
        6 => ((read64(src, pos) << 16).wrapping_mul(PRIME6) >> (64 - bits)) as usize,
        7 => ((read64(src, pos) << 8).wrapping_mul(PRIME7) >> (64 - bits)) as usize,
        _ => (read64(src, pos).wrapping_mul(PRIME8) >> (64 - bits)) as usize,
    }
}

/// How many bytes match at `a` and `b` (with `b` < `a`), not reading at or past `limit`.
#[inline(always)]
pub(crate) fn count(src: &[u8], a: usize, b: usize, limit: usize) -> usize {
    // Slice both once, so the loop runs without a bounds check per step.
    let n = limit.saturating_sub(a);
    let (x, y) = (&src[a..a + n], &src[b..b + n]);
    let (xs, _) = x.as_chunks::<8>();
    let (ys, _) = y.as_chunks::<8>();
    for (i, (p, q)) in xs.iter().zip(ys).enumerate() {
        let diff = u64::from_le_bytes(*p) ^ u64::from_le_bytes(*q);
        if diff != 0 {
            return i * 8 + (diff.trailing_zeros() / 8) as usize;
        }
    }
    let done = xs.len() * 8;
    done + x[done..]
        .iter()
        .zip(&y[done..])
        .take_while(|(p, q)| p == q)
        .count()
}

/// Records literals `src[anchor..start]` and a match of `len` bytes `offset` back.
#[inline(always)]
pub(crate) fn emit(
    src: &[u8],
    anchor: usize,
    start: usize,
    offset: usize,
    len: usize,
    reps: &mut Reps,
    store: &mut SeqStore,
) {
    debug_assert!(
        offset > 0
            && offset <= start
            && src[start..start + len] == src[start - offset..start - offset + len],
        "a bad match at {start}: offset {offset}, length {len}, anchor {anchor}"
    );
    let lit_len = (start - anchor) as u32;
    store.push_lits(src, anchor, start);
    let off_value = reps.code(offset as u32, lit_len);
    store.push_seq(Seq {
        lit_len,
        match_len: len as u32,
        off_value,
    });
}

/// Records literals `src[anchor..start]` and a match of `len` bytes coded as `off_value` (a
/// repeat code or an offset + 3), if the bytes really match there; returns whether they did
/// (a parser that tracks the repeat offsets itself can't make a wrong match this way).
pub(crate) fn emit_coded(
    src: &[u8],
    anchor: usize,
    start: usize,
    off_value: u32,
    len: usize,
    reps: &mut Reps,
    store: &mut SeqStore,
) -> bool {
    let lit_len = (start - anchor) as u32;
    let offset = reps.resolve(off_value, lit_len) as usize;
    let valid = offset > 0
        && offset <= start
        && start + len <= src.len()
        && src[start..start + len] == src[start - offset..start - offset + len];
    if valid {
        reps.apply(off_value, lit_len);
        store.push_lits(src, anchor, start);
        store.push_seq(Seq {
            lit_len,
            match_len: len as u32,
            off_value,
        });
    }
    valid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_stops_at_the_first_difference_or_the_limit() {
        let src = b"abcdefghijabcdefghXjabcdefghijabcdefghij";
        assert_eq!(count(src, 10, 0, src.len()), 8);
        assert_eq!(count(src, 30, 20, src.len()), 10);
        assert_eq!(count(src, 30, 20, 35), 5);
    }

    #[test]
    fn counting_agrees_with_one_byte_at_a_time() {
        let mut src: Vec<u8> = (0..200u32).map(|i| (i % 7) as u8).collect();
        src[150] = 99;
        for b in 0..20 {
            for a in b + 1..60 {
                for limit in [a, a + 1, a + 9, a + 30, 200] {
                    let limit = limit.min(200);
                    let slow = (a..limit).take_while(|&i| src[i] == src[i - a + b]).count();
                    assert_eq!(count(&src, a, b, limit), slow, "{a} {b} {limit}");
                }
            }
        }
    }

    #[test]
    fn literal_runs_append_exactly() {
        let src: Vec<u8> = (0..100u8).collect();
        for (from, to) in [(0, 0), (3, 4), (10, 26), (10, 27), (90, 100), (0, 100)] {
            let mut store = SeqStore::default();
            store.prepare(102);
            store.push_lits(b"xy", 0, 2);
            store.push_lits(&src, from, to);
            assert_eq!(store.lits()[..2], *b"xy");
            assert_eq!(store.lits()[2..], src[from..to]);
        }
    }

    #[test]
    fn hashes_fit_their_width() {
        let src: Vec<u8> = (0..64u8).collect();
        for mls in 3..=8 {
            for pos in 0..40 {
                assert!(hash_dyn(&src, pos, mls, 12) < 1 << 12);
            }
        }
    }
}

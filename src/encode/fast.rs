//! The `fast` strategy (levels 1 and below): one hash table, one candidate per position.
//!
//! The scan looks at two neighbouring positions per step (`ip0`, `ip1`), and checks the last
//! offset at `ip2`, a step ahead, before the hash candidate at `ip0`: a repeat match costs
//! almost nothing, so it's worth preferring. Over data that doesn't match the step grows by
//! one every [`STEP_INCREASE`] bytes.

use std::ops::Range;

use super::matcher::{MatchFinder, count, emit, hash_word, read32, read64};
use super::params::{Params, Strategy};
use super::seq::{Reps, SeqStore};

/// Bytes scanned without a match before the step grows by one.
const STEP_INCREASE: usize = 128;

/// The finder for matches of at least `MLS` bytes (4 to 7).
pub(crate) struct Fast<const MLS: u32> {
    /// The newest position for each hash. Empty entries say 0, which is harmless: a
    /// candidate is only taken when its bytes match.
    table: Vec<u32>,
    hash_log: u32,
    step: usize,
    window: usize,
}

/// The scan's state around a match.
struct Scan<'a> {
    /// The input up to the block's end.
    src: &'a [u8],
    /// The finder's table: a power of two long, so masking an index keeps it in range
    /// without a check.
    table: &'a mut [u32],
    hash_log: u32,
    anchor: usize,
    /// The last position the scan reads 8 bytes at: the block's end less 8.
    limit: usize,
    window: usize,
}

impl Scan<'_> {
    /// The table slot for the bytes `word` (read at some position).
    #[inline(always)]
    fn slot<const MLS: u32>(&self, word: u64) -> usize {
        hash_word(word, MLS, self.hash_log) & (self.table.len() - 1)
    }

    /// The table slot for the bytes at `pos`.
    #[inline(always)]
    fn slot_at<const MLS: u32>(&self, pos: usize) -> usize {
        self.slot::<MLS>(read64(self.src, pos))
    }
}

impl<const MLS: u32> Fast<MLS> {
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            table: vec![0; 1 << p.hash_log],
            hash_log: p.hash_log,
            // For the fast strategy the target length is the acceleration: 0 at levels 1
            // and 2 (a step of 2, so ip0 and ip1 cover every position), and more at the
            // negative levels. The other strategies' target lengths mean something else.
            step: if p.strategy == Strategy::Fast {
                (p.target_length as usize + 1).max(2)
            } else {
                2
            },
            window: 1 << p.window_log,
        }
    }
}

impl<const MLS: u32> MatchFinder for Fast<MLS> {
    fn reset(&mut self) {
        self.table.fill(0);
    }

    fn find(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore) {
        let end = block.end;
        // Hashing reads 8 bytes, and a match needs 4 more bytes after it to be worth it.
        let Some(limit) = end.checked_sub(8) else {
            store.push_lits(src, block.start, end);
            return;
        };
        // The lowest position a match may come from: one bound for the whole block, from its
        // end, as the reference does.
        let low = end.saturating_sub(self.window);
        let (step0, window) = (self.step, self.window);
        let mut s = Scan {
            src: &src[..end],
            table: &mut self.table,
            hash_log: self.hash_log,
            anchor: block.start,
            limit,
            window,
        };
        // Never so (the table has 1 << hash_log entries), but said, so the masked indexes
        // need no check.
        if s.table.is_empty() {
            store.push_lits(src, block.start, end);
            return;
        }
        let src = s.src;
        let mut ip0 = block.start.max(1);
        'scan: loop {
            // The last offset, checked once until the next match rather than at every
            // position: one in the block (or any offset found in it) stays usable.
            let rep = reps.0[0] as usize;
            let rep = if rep <= ip0 && rep <= window { rep } else { 0 };
            let mut step = step0;
            let mut next_step = ip0 + STEP_INCREASE;
            let mut ip1 = ip0 + 1;
            let mut ip2 = ip0 + step;
            let mut ip3 = ip2 + 1;
            if ip3 >= limit {
                break;
            }
            // The bytes at ip0 and ip1, read once: hashed, then compared with a candidate's.
            let mut word0 = read64(src, ip0);
            let mut word1 = read64(src, ip1);
            let mut hash0 = s.slot::<MLS>(word0);
            let mut hash1 = s.slot::<MLS>(word1);
            let mut entry = s.table[hash0];
            loop {
                // The last offset at ip2, then the candidate at ip0.
                s.table[hash0] = ip0 as u32;
                let word2 = read64(src, ip2);
                if rep > 0 && word2 as u32 == read32(src, ip2 - rep) {
                    let mut start = ip2;
                    let mut len = 4;
                    if start > s.anchor && src[start - 1] == src[start - 1 - rep] {
                        start -= 1;
                        len += 1;
                    }
                    let len = len + count(src, start + len, start + len - rep, end);
                    emit(src, s.anchor, start, rep, len, reps, store);
                    ip0 = after_match::<MLS>(&mut s, ip0, (ip1, hash1), start + len, reps, store);
                    continue 'scan;
                }
                let m = entry as usize;
                if m >= low && read32(src, m) == word0 as u32 {
                    ip0 = take::<MLS>(&mut s, ip0, m, (ip1, hash1), reps, store);
                    continue 'scan;
                }
                // Step to ip1.
                entry = s.table[hash1];
                hash0 = hash1;
                hash1 = s.slot::<MLS>(word2);
                word0 = word1;
                word1 = word2;
                ip0 = ip1;
                ip1 = ip2;
                ip2 = ip3;
                s.table[hash0] = ip0 as u32;
                let m = entry as usize;
                if m >= low && read32(src, m) == word0 as u32 {
                    ip0 = take::<MLS>(&mut s, ip0, m, (ip1, hash1), reps, store);
                    continue 'scan;
                }
                // And on by a whole step.
                let word2 = read64(src, ip2);
                entry = s.table[hash1];
                hash0 = hash1;
                hash1 = s.slot::<MLS>(word2);
                word0 = word1;
                word1 = word2;
                ip0 = ip1;
                ip1 = ip2;
                ip2 = ip0 + step;
                ip3 = ip2 + 1;
                if ip2 >= next_step {
                    step += 1;
                    next_step += STEP_INCREASE;
                }
                if ip3 >= limit {
                    break 'scan;
                }
            }
        }
        store.push_lits(src, s.anchor, end);
    }
}

/// A hash match at `ip` with `m`: extend it backwards, record it, and go on after it.
#[inline(always)]
fn take<const MLS: u32>(
    s: &mut Scan<'_>,
    ip: usize,
    m: usize,
    next: (usize, usize),
    reps: &mut Reps,
    store: &mut SeqStore,
) -> usize {
    let src = s.src;
    // Forwards past the 4 bytes known to match, then backwards: apart, so the count needn't
    // wait on the bytes before.
    let (mut a, mut c) = (ip, m);
    let mut len = 4 + count(src, ip + 4, m + 4, src.len());
    while a > s.anchor && c > 0 && src[a - 1] == src[c - 1] {
        a -= 1;
        c -= 1;
        len += 1;
    }
    emit(src, s.anchor, a, a - c, len, reps, store);
    after_match::<MLS>(s, ip, next, a + len, reps, store)
}

/// After a match found while scanning from `scan`, ending at `ip`: fill the table a little,
/// then take repeats of the second offset straight away (no literals between). `next` is the
/// position the scan had hashed next, and its slot. Returns where the scan resumes.
#[inline(always)]
fn after_match<const MLS: u32>(
    s: &mut Scan<'_>,
    scan: usize,
    next: (usize, usize),
    mut ip: usize,
    reps: &mut Reps,
    store: &mut SeqStore,
) -> usize {
    let src = s.src;
    s.anchor = ip;
    // Only a position the match covers: one past it is still to be scanned, and would find
    // itself in the table.
    if next.0 < ip {
        s.table[next.1] = next.0 as u32;
    }
    if ip > s.limit {
        return ip;
    }
    if scan + 2 < ip {
        let h = s.slot_at::<MLS>(scan + 2);
        s.table[h] = scan as u32 + 2;
    }
    let h = s.slot_at::<MLS>(ip - 2);
    s.table[h] = ip as u32 - 2;
    while ip <= s.limit {
        let rep2 = reps.0[1] as usize;
        if rep2 == 0 || rep2 > ip || rep2 > s.window {
            break;
        }
        if read32(src, ip) != read32(src, ip - rep2) {
            break;
        }
        let len = 4 + count(src, ip + 4, ip + 4 - rep2, src.len());
        let h = s.slot_at::<MLS>(ip);
        s.table[h] = ip as u32;
        emit(src, s.anchor, ip, rep2, len, reps, store);
        ip += len;
        s.anchor = ip;
    }
    ip
}

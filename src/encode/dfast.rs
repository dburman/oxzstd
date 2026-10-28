//! The `dfast` strategy (levels 3 and 4 for large inputs): two hash tables, one keyed on 8
//! bytes for long matches and one on the minimum match for short ones.
//!
//! At each position the scan checks the last offset one byte ahead, then the long table,
//! then the short table. A long match is taken at once; a short one only after looking for a
//! long match at the next position, which is usually the better choice when there is one.
//! Over data that doesn't match the step grows by one every [`STEP_INCREASE`] bytes.

use std::ops::Range;

use super::matcher::{MatchFinder, count, emit, hash_word, read32, read64};
use super::params::Params;
use super::seq::{Reps, SeqStore};

/// Bytes scanned without a match before the step grows by one.
const STEP_INCREASE: usize = 256;

/// The finder for short matches of at least `MLS` bytes (4 to 7).
pub(crate) struct DoubleFast<const MLS: u32> {
    long: Vec<u32>,
    long_log: u32,
    short: Vec<u32>,
    short_log: u32,
    window: usize,
}

/// What the scan found.
enum Found {
    /// The last offset again, at `at`.
    Repeat { at: usize, len: usize },
    /// A match at `at` with the earlier position `m`.
    Match { at: usize, m: usize, len: usize },
}

impl<const MLS: u32> DoubleFast<MLS> {
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            long: vec![0; 1 << p.hash_log],
            long_log: p.hash_log,
            short: vec![0; 1 << p.chain_log],
            short_log: p.chain_log,
            window: 1 << p.window_log,
        }
    }
}

/// The scan's state: the input up to the block's end, and the tables.
struct Scan<'a> {
    src: &'a [u8],
    /// Both tables are a power of two long, so masking an index keeps it in range without a
    /// check.
    long: &'a mut [u32],
    long_log: u32,
    short: &'a mut [u32],
    short_log: u32,
    /// The last position the scan reads 8 bytes at: the block's end less 8.
    limit: usize,
    /// The lowest position a match may come from: one bound for the whole block, from its
    /// end, as the reference does.
    low: usize,
    window: usize,
}

impl Scan<'_> {
    /// The long table's slot for the 8 bytes `word`.
    #[inline(always)]
    fn long_slot(&self, word: u64) -> usize {
        hash_word(word, 8, self.long_log) & (self.long.len() - 1)
    }

    /// The short table's slot for the bytes `word` (the first `MLS` count).
    #[inline(always)]
    fn short_slot<const MLS: u32>(&self, word: u64) -> usize {
        hash_word(word, MLS, self.short_log) & (self.short.len() - 1)
    }

    /// The position a table entry holds, if it's inside the window for the whole block.
    /// Empty entries say 0, which is harmless: a candidate is only taken when its bytes
    /// match.
    #[inline(always)]
    fn candidate(&self, entry: u32) -> Option<usize> {
        let m = entry as usize;
        (m >= self.low).then_some(m)
    }

    /// Whether `offset` can be used at `pos`.
    #[inline(always)]
    fn usable(&self, offset: usize, pos: usize) -> bool {
        offset > 0 && offset <= pos && offset <= self.window
    }

    /// Scans from `ip` for the next match. Returns it, and the position the scan's tables
    /// were last updated for; `None` when the block runs out first.
    #[inline(always)]
    fn scan<const MLS: u32>(&mut self, mut ip: usize, reps: &Reps) -> Option<(Found, usize)> {
        let src = self.src;
        let end = src.len();
        let mut step = 1;
        let mut next_step = ip + STEP_INCREASE;
        let mut ip1 = ip + step;
        if ip1 > self.limit {
            return None;
        }
        // The bytes at ip, read once: hashed for both tables, and compared with candidates.
        let mut word0 = read64(src, ip);
        let mut hl0 = self.long_slot(word0);
        let mut idxl0 = self.long[hl0];
        loop {
            if ip1 > self.limit {
                return None;
            }
            let hs0 = self.short_slot::<MLS>(word0);
            let idxs0 = self.short[hs0];
            self.long[hl0] = ip as u32;
            self.short[hs0] = ip as u32;
            let rep = reps.0[0] as usize;
            // The 4 bytes at ip + 1 are in the word read at ip.
            if self.usable(rep, ip + 1) && (word0 >> 8) as u32 == read32(src, ip + 1 - rep) {
                let len = 4 + count(src, ip + 5, ip + 5 - rep, end);
                return Some((Found::Repeat { at: ip + 1, len }, ip));
            }
            let word1 = read64(src, ip1);
            let hl1 = self.long_slot(word1);
            let found = if let Some(m) = self.candidate(idxl0)
                && read64(src, m) == word0
            {
                Some((ip, m, 8 + count(src, ip + 8, m + 8, end)))
            } else if let Some(m) = self.candidate(idxs0)
                && read32(src, m) == word0 as u32
            {
                // A short match: a long one at the next position beats it.
                match self.candidate(self.long[hl1]) {
                    Some(m1) if read64(src, m1) == word1 => {
                        Some((ip1, m1, 8 + count(src, ip1 + 8, m1 + 8, end)))
                    }
                    _ => Some((ip, m, 4 + count(src, ip + 4, m + 4, end))),
                }
            } else {
                None
            };
            if let Some((at, m, len)) = found {
                // The next position is covered by the match when the step is under 4 (the
                // shortest match), so it can go in the table without the scan meeting it.
                if step < 4 {
                    self.long[hl1] = ip1 as u32;
                }
                return Some((Found::Match { at, m, len }, ip));
            }
            if ip1 >= next_step {
                step += 1;
                next_step += STEP_INCREASE;
            }
            idxl0 = self.long[hl1];
            hl0 = hl1;
            word0 = word1;
            ip = ip1;
            ip1 += step;
        }
    }
}

impl<const MLS: u32> MatchFinder for DoubleFast<MLS> {
    fn reset(&mut self) {
        self.long.fill(0);
        self.short.fill(0);
    }

    fn find(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore) {
        let end = block.end;
        let mut anchor = block.start;
        // Hashing reads 8 bytes.
        let Some(limit) = end.checked_sub(8) else {
            store.push_lits(src, anchor, end);
            return;
        };
        let mut s = Scan {
            src: &src[..end],
            long: &mut self.long,
            long_log: self.long_log,
            short: &mut self.short,
            short_log: self.short_log,
            limit,
            low: end.saturating_sub(self.window),
            window: self.window,
        };
        // Never so (the tables have 1 << log entries), but said, so the masked indexes need
        // no check.
        if s.long.is_empty() || s.short.is_empty() {
            store.push_lits(src, anchor, end);
            return;
        }
        let src = s.src;
        let mut ip = block.start.max(1);
        while let Some((found, scanned)) = s.scan::<MLS>(ip, reps) {
            let match_end = match found {
                Found::Repeat { at, len } => {
                    emit(src, anchor, at, reps.0[0] as usize, len, reps, store);
                    at + len
                }
                Found::Match {
                    mut at,
                    mut m,
                    mut len,
                } => {
                    while at > anchor && m > 0 && src[at - 1] == src[m - 1] {
                        at -= 1;
                        m -= 1;
                        len += 1;
                    }
                    emit(src, anchor, at, at - m, len, reps, store);
                    at + len
                }
            };
            ip = match_end;
            anchor = ip;
            if ip > limit {
                break;
            }
            // Fill the tables a little inside the match: everything here is behind `ip`.
            let inside = scanned + 2;
            let word = read64(src, inside);
            let h = s.long_slot(word);
            s.long[h] = inside as u32;
            let h = s.short_slot::<MLS>(word);
            s.short[h] = inside as u32;
            let h = s.long_slot(read64(src, ip - 2));
            s.long[h] = ip as u32 - 2;
            let h = s.short_slot::<MLS>(read64(src, ip - 1));
            s.short[h] = ip as u32 - 1;
            // Repeats of the second offset straight away, with no literals between.
            while ip <= limit {
                let rep2 = reps.0[1] as usize;
                if !s.usable(rep2, ip) {
                    break;
                }
                let word = read64(src, ip);
                if word as u32 != read32(src, ip - rep2) {
                    break;
                }
                let len = 4 + count(src, ip + 4, ip + 4 - rep2, end);
                let h = s.short_slot::<MLS>(word);
                s.short[h] = ip as u32;
                let h = s.long_slot(word);
                s.long[h] = ip as u32;
                emit(src, anchor, ip, rep2, len, reps, store);
                ip += len;
                anchor = ip;
            }
        }
        store.push_lits(src, anchor, end);
    }
}

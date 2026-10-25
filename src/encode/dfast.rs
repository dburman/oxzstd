//! The `dfast` strategy (levels 3 and 4 for large inputs): two hash tables, one keyed on 8
//! bytes for long matches and one on the minimum match for short ones.
//!
//! At each position the scan checks the last offset one byte ahead, then the long table,
//! then the short table. A long match is taken at once; a short one only after looking for a
//! long match at the next position, which is usually the better choice when there is one.
//! Over data that doesn't match the step grows by one every [`STEP_INCREASE`] bytes.

use std::ops::Range;

use super::matcher::{MatchFinder, count, emit, hash, read32, read64};
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

    #[inline]
    fn hl(&self, src: &[u8], pos: usize) -> usize {
        hash::<8>(src, pos, self.long_log)
    }

    #[inline]
    fn hs(&self, src: &[u8], pos: usize) -> usize {
        hash::<MLS>(src, pos, self.short_log)
    }

    /// The position a table entry holds, if it's still inside the window from `pos`.
    #[inline]
    fn candidate(&self, entry: u32, pos: usize) -> Option<usize> {
        let m = (entry as usize).checked_sub(1)?;
        (pos - m <= self.window).then_some(m)
    }

    /// Whether `offset` can be used at `pos`.
    #[inline]
    fn usable(&self, offset: usize, pos: usize) -> bool {
        offset > 0 && offset <= pos && offset <= self.window
    }

    /// Scans from `ip` for the next match. Returns it, and the position the scan's tables
    /// were last updated for; `None` when the block runs out first.
    fn scan(
        &mut self,
        src: &[u8],
        mut ip: usize,
        limit: usize,
        end: usize,
        reps: &Reps,
    ) -> Option<(Found, usize)> {
        let mut step = 1;
        let mut next_step = ip + STEP_INCREASE;
        let mut ip1 = ip + step;
        if ip1 > limit {
            return None;
        }
        let mut hl0 = self.hl(src, ip);
        let mut idxl0 = self.long[hl0];
        loop {
            let hs0 = self.hs(src, ip);
            let idxs0 = self.short[hs0];
            self.long[hl0] = ip as u32 + 1;
            self.short[hs0] = ip as u32 + 1;
            let rep = reps.0[0] as usize;
            if self.usable(rep, ip + 1) && read32(src, ip + 1) == read32(src, ip + 1 - rep) {
                let len = 4 + count(src, ip + 5, ip + 5 - rep, end);
                return Some((Found::Repeat { at: ip + 1, len }, ip));
            }
            let hl1 = self.hl(src, ip1);
            let found = if let Some(m) = self.candidate(idxl0, ip)
                && read64(src, m) == read64(src, ip)
            {
                Some((ip, m, 8 + count(src, ip + 8, m + 8, end)))
            } else if let Some(m) = self.candidate(idxs0, ip)
                && read32(src, m) == read32(src, ip)
            {
                // A short match: a long one at the next position beats it.
                match self.candidate(self.long[hl1], ip1) {
                    Some(m1) if read64(src, m1) == read64(src, ip1) => {
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
                    self.long[hl1] = ip1 as u32 + 1;
                }
                return Some((Found::Match { at, m, len }, ip));
            }
            if ip1 >= next_step {
                step += 1;
                next_step += STEP_INCREASE;
            }
            idxl0 = self.long[hl1];
            hl0 = hl1;
            ip = ip1;
            ip1 += step;
            if ip1 > limit {
                return None;
            }
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
        // Hashing reads 8 bytes.
        let limit = end.saturating_sub(8);
        let mut anchor = block.start;
        let mut ip = block.start.max(1);
        while let Some((found, scanned)) = self.scan(src, ip, limit, end, reps) {
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
            let h = self.hl(src, inside);
            self.long[h] = inside as u32 + 1;
            let h = self.hl(src, ip - 2);
            self.long[h] = ip as u32 - 1;
            let h = self.hs(src, inside);
            self.short[h] = inside as u32 + 1;
            let h = self.hs(src, ip - 1);
            self.short[h] = ip as u32;
            // Repeats of the second offset straight away, with no literals between.
            while ip <= limit {
                let rep2 = reps.0[1] as usize;
                if !self.usable(rep2, ip) || read32(src, ip) != read32(src, ip - rep2) {
                    break;
                }
                let len = 4 + count(src, ip + 4, ip + 4 - rep2, end);
                let h = self.hs(src, ip);
                self.short[h] = ip as u32 + 1;
                let h = self.hl(src, ip);
                self.long[h] = ip as u32 + 1;
                emit(src, anchor, ip, rep2, len, reps, store);
                ip += len;
                anchor = ip;
            }
        }
        store.push_lits(src, anchor, end);
    }
}

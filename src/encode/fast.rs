//! The `fast` strategy (levels 1 and below): one hash table, one candidate per position.
//!
//! The scan looks at two neighbouring positions per step (`ip0`, `ip1`), and checks the last
//! offset at `ip2`, a step ahead, before the hash candidate at `ip0`: a repeat match costs
//! almost nothing, so it's worth preferring. Over data that doesn't match the step grows by
//! one every [`STEP_INCREASE`] bytes.

use std::ops::Range;

use super::matcher::{MatchFinder, count, emit, hash, read32};
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
    /// The lowest position a match may come from in the current block: one bound for the
    /// whole block, from its end, as the reference does.
    low: usize,
}

/// The scan's state around a match.
struct Scan<'a> {
    src: &'a [u8],
    anchor: usize,
    limit: usize,
    end: usize,
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
            low: 0,
        }
    }

    #[inline]
    fn h(&self, src: &[u8], pos: usize) -> usize {
        hash::<MLS>(src, pos, self.hash_log)
    }

    /// The position a table entry holds, if it's inside the window for the whole block.
    #[inline(always)]
    fn candidate(&self, entry: u32) -> Option<usize> {
        let m = entry as usize;
        (m >= self.low).then_some(m)
    }
}

impl<const MLS: u32> MatchFinder for Fast<MLS> {
    fn reset(&mut self) {
        self.table.fill(0);
        self.low = 0;
    }

    fn find(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore) {
        let end = block.end;
        // Hashing reads 8 bytes, and a match needs 4 more bytes after it to be worth it.
        let limit = end.saturating_sub(8);
        let mut s = Scan {
            src,
            anchor: block.start,
            limit,
            end,
        };
        let mut ip0 = block.start.max(1);
        self.low = end.saturating_sub(self.window);
        'scan: loop {
            // The last offset, checked once until the next match rather than at every
            // position: one in the block (or any offset found in it) stays usable.
            let rep = reps.0[0] as usize;
            let rep = if rep <= ip0 && rep <= self.window {
                rep
            } else {
                0
            };
            let mut step = self.step;
            let mut next_step = ip0 + STEP_INCREASE;
            let mut ip1 = ip0 + 1;
            let mut ip2 = ip0 + step;
            let mut ip3 = ip2 + 1;
            if ip3 >= limit {
                break;
            }
            let mut hash0 = self.h(src, ip0);
            let mut hash1 = self.h(src, ip1);
            let mut entry = self.table[hash0];
            loop {
                // The last offset at ip2, then the candidate at ip0.
                self.table[hash0] = ip0 as u32;
                if rep > 0 && read32(src, ip2) == read32(src, ip2 - rep) {
                    let mut start = ip2;
                    let mut len = 4;
                    if start > s.anchor && src[start - 1] == src[start - 1 - rep] {
                        start -= 1;
                        len += 1;
                    }
                    let len = len + count(src, start + len, start + len - rep, end);
                    emit(src, s.anchor, start, rep, len, reps, store);
                    ip0 = self.after_match(&mut s, ip0, (ip1, hash1), start + len, reps, store);
                    continue 'scan;
                }
                if let Some(m) = self.candidate(entry)
                    && read32(src, m) == read32(src, ip0)
                {
                    ip0 = self.take(&mut s, ip0, m, (ip1, hash1), reps, store);
                    continue 'scan;
                }
                // Step to ip1.
                entry = self.table[hash1];
                hash0 = hash1;
                hash1 = self.h(src, ip2);
                ip0 = ip1;
                ip1 = ip2;
                ip2 = ip3;
                self.table[hash0] = ip0 as u32;
                if let Some(m) = self.candidate(entry)
                    && read32(src, m) == read32(src, ip0)
                {
                    ip0 = self.take(&mut s, ip0, m, (ip1, hash1), reps, store);
                    continue 'scan;
                }
                // And on by a whole step.
                entry = self.table[hash1];
                hash0 = hash1;
                hash1 = self.h(src, ip2);
                ip0 = ip1;
                ip1 = ip2;
                ip2 = ip0 + step;
                ip3 = ip1 + step;
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

impl<const MLS: u32> Fast<MLS> {
    /// A hash match at `ip` with `m`: extend it backwards, record it, and go on after it.
    #[inline(always)]
    fn take(
        &mut self,
        s: &mut Scan<'_>,
        ip: usize,
        m: usize,
        next: (usize, usize),
        reps: &mut Reps,
        store: &mut SeqStore,
    ) -> usize {
        let src = s.src;
        let (mut a, mut c) = (ip, m);
        while a > s.anchor && c > 0 && src[a - 1] == src[c - 1] {
            a -= 1;
            c -= 1;
        }
        let len = count(src, a, c, s.end);
        emit(src, s.anchor, a, a - c, len, reps, store);
        self.after_match(s, ip, next, a + len, reps, store)
    }

    /// After a match found while scanning from `scan`, ending at `ip`: fill the table a
    /// little, then take repeats of the second offset straight away (no literals between).
    /// `next` is the position the scan had hashed next, and its hash. Returns where the scan
    /// resumes.
    #[inline(always)]
    fn after_match(
        &mut self,
        s: &mut Scan<'_>,
        scan: usize,
        next: (usize, usize),
        mut ip: usize,
        reps: &mut Reps,
        store: &mut SeqStore,
    ) -> usize {
        let src = s.src;
        s.anchor = ip;
        // Only a position the match covers: one past it is still to be scanned, and would
        // find itself in the table.
        if next.0 < ip {
            self.table[next.1] = next.0 as u32;
        }
        if ip > s.limit {
            return ip;
        }
        if scan + 2 < ip {
            let h = self.h(src, scan + 2);
            self.table[h] = scan as u32 + 2;
        }
        let h = self.h(src, ip - 2);
        self.table[h] = ip as u32 - 2;
        while ip <= s.limit {
            let rep2 = reps.0[1] as usize;
            if rep2 == 0 || rep2 > ip || rep2 > self.window {
                break;
            }
            if read32(src, ip) != read32(src, ip - rep2) {
                break;
            }
            let len = 4 + count(src, ip + 4, ip + 4 - rep2, s.end);
            let h = self.h(src, ip);
            self.table[h] = ip as u32;
            emit(src, s.anchor, ip, rep2, len, reps, store);
            ip += len;
            s.anchor = ip;
        }
        ip
    }
}

//! The `greedy`, `lazy` and `lazy2` strategies: the longest match at each position, and (for
//! lazy and lazy2) a look one or two positions on for a better one.
//!
//! Two searches find the matches: a hash chain ([`HashChain`]), and rows of recent positions
//! with a tag byte each ([`Row`]), which the reference uses once the window is over 16 KiB.
//! Positions are entered into the tables lazily, just before a search needs them; over long
//! stretches with no match only some are, as in the reference.

use std::ops::Range;

use super::matcher::{MatchFinder, count, emit, hash, read32};
use super::params::{Params, Strategy};
use super::seq::{Reps, SeqStore};

/// Bytes scanned without a match before the scan starts skipping (the step grows by one every
/// this many bytes).
const SEARCH_STRENGTH: u32 = 8;
/// A step above this stops entering every skipped position into the tables.
const SKIPPING_STEP: usize = 8;

/// How the parser finds the longest match at a position.
pub(crate) trait Search: Send + Sync {
    /// The longest match at `ip` of at least 4 bytes, not reading at or past `end`: its
    /// length and offset. Enters the positions before `ip` into the tables first, only some
    /// of them when `skipping`.
    fn longest(
        &mut self,
        src: &[u8],
        ip: usize,
        end: usize,
        skipping: bool,
    ) -> Option<(usize, usize)>;

    /// A new block starts at `start`.
    fn begin_block(&mut self, start: usize);

    /// Forgets everything, as if new, keeping the tables' memory.
    fn reset(&mut self);
}

/// The greedy, lazy and lazy2 parsers, over a [`Search`].
pub(crate) struct Lazy<S> {
    search: S,
    window: usize,
    /// 0 for greedy, 1 for lazy, 2 for lazy2.
    depth: u8,
    skipping: bool,
}

impl<S: Search> Lazy<S> {
    pub(crate) fn new(p: &Params, search: S) -> Self {
        Self {
            search,
            window: 1 << p.window_log,
            depth: match p.strategy {
                Strategy::Greedy => 0,
                Strategy::Lazy => 1,
                _ => 2,
            },
            skipping: false,
        }
    }

    /// Whether `offset` can be used at `pos`.
    #[inline]
    fn usable(&self, offset: usize, pos: usize) -> bool {
        offset > 0 && offset <= pos && offset <= self.window
    }

    /// The length of a match with the last offset at `pos`, if it's at least 4.
    #[inline]
    fn repeat_at(&self, src: &[u8], pos: usize, end: usize, rep: usize) -> Option<usize> {
        (self.usable(rep, pos) && read32(src, pos) == read32(src, pos - rep))
            .then(|| 4 + count(src, pos + 4, pos + 4 - rep, end))
    }
}

/// A candidate: where it starts, how long it is, and its offset (0 for the last offset).
#[derive(Clone, Copy)]
struct Candidate {
    start: usize,
    len: usize,
    offset: usize,
}

impl Candidate {
    /// The offset as the format codes it, for weighing what it costs: repeats are cheap.
    fn off_base(self) -> u32 {
        if self.offset == 0 {
            1
        } else {
            self.offset as u32 + 3
        }
    }
}

/// How the reference weighs a match: its length against its offset's cost in bits.
fn gain(len: usize, off_base: u32, weight: i64, bonus: i64) -> i64 {
    len as i64 * weight - i64::from(off_base.ilog2()) + bonus
}

/// A hash chain: the hash table holds the newest position for each hash and the chain table
/// links each position to the previous one with the same hash. A search follows the chain
/// for up to `1 << search_log` candidates, for matches of at least `MLS` bytes (4 to 6).
pub(crate) struct HashChain<const MLS: u32> {
    /// Newest position + 1 for each hash; 0 is empty.
    hash: Vec<u32>,
    hash_log: u32,
    /// Previous position + 1 with the same hash, indexed by position + 1 under the mask.
    chain: Vec<u32>,
    chain_mask: usize,
    attempts: u32,
    window: usize,
    /// The first position not yet entered into the tables.
    next_to_update: usize,
}

impl<const MLS: u32> HashChain<MLS> {
    pub(crate) fn new(p: &Params) -> Self {
        Self {
            hash: vec![0; 1 << p.hash_log],
            hash_log: p.hash_log,
            chain: vec![0; 1 << p.chain_log],
            chain_mask: (1 << p.chain_log) - 1,
            attempts: 1 << p.search_log,
            window: 1 << p.window_log,
            next_to_update: 0,
        }
    }

    /// Enters the positions up to `ip` into the tables; returns the newest earlier position
    /// with `ip`'s hash, + 1 (0 for none).
    #[inline]
    fn insert_and_find(&mut self, src: &[u8], ip: usize, skipping: bool) -> u32 {
        while self.next_to_update < ip {
            let pos = self.next_to_update;
            let h = hash::<MLS>(src, pos, self.hash_log);
            self.chain[(pos + 1) & self.chain_mask] = self.hash[h];
            self.hash[h] = pos as u32 + 1;
            self.next_to_update += 1;
            if skipping {
                break;
            }
        }
        self.next_to_update = self.next_to_update.max(ip);
        self.hash[hash::<MLS>(src, ip, self.hash_log)]
    }
}

impl<const MLS: u32> Search for HashChain<MLS> {
    fn reset(&mut self) {
        self.hash.fill(0);
        self.chain.fill(0);
        self.next_to_update = 0;
    }

    fn longest(
        &mut self,
        src: &[u8],
        ip: usize,
        end: usize,
        skipping: bool,
    ) -> Option<(usize, usize)> {
        let cur = ip + 1;
        let low = cur.saturating_sub(self.window).max(1);
        let min_chain = cur.saturating_sub(self.chain_mask + 1);
        let mut index = self.insert_and_find(src, ip, skipping) as usize;
        let mut best = Best::default();
        for _ in 0..self.attempts {
            if index < low {
                break;
            }
            if best.try_candidate(src, ip, index - 1, end) {
                break;
            }
            if index <= min_chain {
                break;
            }
            index = self.chain[index & self.chain_mask] as usize;
        }
        best.found
    }

    fn begin_block(&mut self, start: usize) {
        self.next_to_update = self.next_to_update.max(start.saturating_sub(self.window));
    }
}

/// The longest match found so far at a position.
struct Best {
    /// One less than the shortest match worth having next.
    ml: usize,
    found: Option<(usize, usize)>,
}

impl Default for Best {
    fn default() -> Self {
        Self { ml: 3, found: None }
    }
}

impl Best {
    /// Measures the candidate `m` for `ip`; returns whether the match reaches `end`, so no
    /// longer one is possible.
    #[inline(always)]
    fn try_candidate(&mut self, src: &[u8], ip: usize, m: usize, end: usize) -> bool {
        // Only a match longer than the best so far is worth counting: check the 4 bytes that
        // end where it would have to reach.
        let ml = self.ml;
        if read32(src, m + ml - 3) == read32(src, ip + ml - 3) {
            let len = count(src, ip, m, end);
            if len > ml {
                self.ml = len;
                self.found = Some((len, ip - m));
                return ip + len == end;
            }
        }
        false
    }
}

/// Rows of recent positions, as the reference's row-based finder keeps them. A hash picks a
/// row of 16, 32 or 64 entries and a tag byte; each entry holds a position and its tag, and
/// the row's first tag byte says where the newest entry is. A search compares the tag with
/// the whole row at once and looks only at the entries that match, newest first, for
/// matches of at least `MLS` bytes (4 to 6), with rows of `1 << ROW_LOG` entries (4 to 6).
pub(crate) struct Row<const MLS: u32, const ROW_LOG: u32> {
    /// Position + 1 per entry; 0 is empty.
    table: Vec<u32>,
    tags: Vec<u8>,
    /// Bits of the hash that pick the row; 8 more make the tag.
    hash_bits: u32,
    attempts: usize,
    window: usize,
    /// The first position not yet entered into the rows.
    next_to_update: usize,
}

/// After a match longer than this, only the positions at its start and end go into the rows.
const ROW_SKIP: usize = 384;
const ROW_SKIP_START: usize = 96;
const ROW_SKIP_END: usize = 32;

impl<const MLS: u32, const ROW_LOG: u32> Row<MLS, ROW_LOG> {
    const ENTRIES: usize = 1 << ROW_LOG;
    const MASK: usize = Self::ENTRIES - 1;

    /// The row size the reference uses for `p`, as a log.
    pub(crate) fn log_for(p: &Params) -> u32 {
        p.search_log.clamp(4, 6)
    }

    pub(crate) fn new(p: &Params) -> Self {
        Self {
            table: vec![0; 1 << p.hash_log],
            tags: vec![0; 1 << p.hash_log],
            hash_bits: p.hash_log - ROW_LOG,
            attempts: 1 << p.search_log.min(ROW_LOG),
            window: 1 << p.window_log,
            next_to_update: 0,
        }
    }

    /// The row (where its entries start) and tag for the position `pos`.
    #[inline]
    fn locate(&self, src: &[u8], pos: usize) -> (usize, u8) {
        let h = hash::<MLS>(src, pos, self.hash_bits + 8);
        ((h >> 8) << ROW_LOG, h as u8)
    }

    /// The entry the next position in `row` goes in: the one before the newest, going round
    /// and skipping the first (which holds the head).
    #[inline]
    fn next_slot(&mut self, row: usize) -> usize {
        let mut next = usize::from(self.tags[row]).wrapping_sub(1) & Self::MASK;
        if next == 0 {
            next = Self::MASK;
        }
        self.tags[row] = next as u8;
        next
    }

    #[inline]
    fn insert(&mut self, src: &[u8], pos: usize) {
        let (row, tag) = self.locate(src, pos);
        let slot = self.next_slot(row);
        self.tags[row + slot] = tag;
        self.table[row + slot] = pos as u32 + 1;
    }

    /// Enters the positions up to `target`; after a long match, only those at each end.
    fn update(&mut self, src: &[u8], target: usize) {
        let mut pos = self.next_to_update;
        if target - pos > ROW_SKIP {
            for p in pos..pos + ROW_SKIP_START {
                self.insert(src, p);
            }
            pos = target - ROW_SKIP_END;
        }
        for p in pos..target {
            self.insert(src, p);
        }
        self.next_to_update = target;
    }
}

/// Which of the `tags` equal `tag`, as a bit per tag (the low bit for the first).
#[inline]
fn match_mask(tags: &[u8], tag: u8) -> u64 {
    const LOW7: u64 = 0x7F7F_7F7F_7F7F_7F7F;
    let splat = u64::from(tag) * 0x0101_0101_0101_0101;
    let mut mask = 0;
    for (i, chunk) in tags.as_chunks::<8>().0.iter().enumerate() {
        let x = u64::from_le_bytes(*chunk) ^ splat;
        // The high bit of each byte that is zero, exactly (no borrows between bytes).
        let zero = !(((x & LOW7) + LOW7) | x | LOW7);
        // Gather those 8 bits into one byte.
        let bits = ((zero >> 7).wrapping_mul(0x0102_0408_1020_4080)) >> 56;
        mask |= bits << (8 * i);
    }
    mask
}

impl<const MLS: u32, const ROW_LOG: u32> Search for Row<MLS, ROW_LOG> {
    fn reset(&mut self) {
        self.table.fill(0);
        self.tags.fill(0);
        self.next_to_update = 0;
    }

    fn longest(
        &mut self,
        src: &[u8],
        ip: usize,
        end: usize,
        skipping: bool,
    ) -> Option<(usize, usize)> {
        if skipping {
            self.next_to_update = ip;
        } else {
            self.update(src, ip);
        }
        let low = (ip + 1).saturating_sub(self.window).max(1);
        let (row, tag) = self.locate(src, ip);
        let (entries, mask) = (Self::ENTRIES, Self::MASK);
        let head = usize::from(self.tags[row]) & mask;
        // The first tag byte is the head, not a tag.
        let matches = match_mask(&self.tags[row..row + entries], tag) & !1;
        // Newest first: the head's entry, then the ones written before it.
        let full = if entries == 64 {
            u64::MAX
        } else {
            (1 << entries) - 1
        };
        let mut matches = if head == 0 {
            matches
        } else {
            ((matches >> head) | (matches << (entries - head))) & full
        };
        let mut candidates = [0u32; 64];
        let mut found = 0;
        while matches != 0 && found < self.attempts {
            let slot = (head + matches.trailing_zeros() as usize) & mask;
            matches &= matches - 1;
            let index = self.table[row + slot];
            if (index as usize) < low {
                break;
            }
            candidates[found] = index;
            found += 1;
        }
        // The current position goes in now, saving the next update a step.
        let slot = self.next_slot(row);
        self.tags[row + slot] = tag;
        self.table[row + slot] = ip as u32 + 1;
        self.next_to_update = ip + 1;
        let mut best = Best::default();
        for &index in &candidates[..found] {
            if best.try_candidate(src, ip, index as usize - 1, end) {
                break;
            }
        }
        best.found
    }

    fn begin_block(&mut self, start: usize) {
        self.next_to_update = self.next_to_update.max(start.saturating_sub(self.window));
    }
}

impl<S: Search> MatchFinder for Lazy<S> {
    fn reset(&mut self) {
        self.search.reset();
        self.skipping = false;
    }

    fn find(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore) {
        let end = block.end;
        // Hashing reads 8 bytes.
        let limit = end.saturating_sub(8);
        let mut anchor = block.start;
        let mut ip = block.start.max(1);
        self.search.begin_block(block.start);
        while ip < limit {
            let rep = reps.0[0] as usize;
            let mut best = Candidate {
                start: ip + 1,
                len: 0,
                offset: 0,
            };
            if let Some(len) = self.repeat_at(src, ip + 1, end, rep) {
                best.len = len;
            }
            if self.depth > 0 || best.len == 0 {
                if let Some((len, offset)) = self.search.longest(src, ip, end, self.skipping)
                    && len > best.len
                {
                    best = Candidate {
                        start: ip,
                        len,
                        offset,
                    };
                }
                if best.len < 4 {
                    let step = ((ip - anchor) >> SEARCH_STRENGTH) + 1;
                    ip += step;
                    self.skipping = step > SKIPPING_STEP;
                    continue;
                }
                // A better match one or two positions on?
                let mut look = 0;
                while look < self.depth && ip < limit {
                    ip += 1;
                    look += 1;
                    let (rep_weight, rep_bonus, bonus) =
                        if look == 1 { (3, 1, 4) } else { (4, 1, 7) };
                    if let Some(len) = self.repeat_at(src, ip, end, rep)
                        && gain(len, 1, rep_weight, 0)
                            > gain(best.len, best.off_base(), rep_weight, rep_bonus)
                    {
                        best = Candidate {
                            start: ip,
                            len,
                            offset: 0,
                        };
                    }
                    if let Some((len, offset)) = self.search.longest(src, ip, end, self.skipping)
                        && gain(len, offset as u32 + 3, 4, 0)
                            > gain(best.len, best.off_base(), 4, bonus)
                    {
                        best = Candidate {
                            start: ip,
                            len,
                            offset,
                        };
                        // Found a better one: look on from here.
                        look = 0;
                    }
                }
            }
            let Candidate {
                mut start,
                mut len,
                offset,
            } = best;
            let offset = if offset == 0 {
                rep
            } else {
                // Extend backwards over bytes that match too.
                while start > anchor && start > offset && src[start - 1] == src[start - 1 - offset]
                {
                    start -= 1;
                    len += 1;
                }
                offset
            };
            emit(src, anchor, start, offset, len, reps, store);
            ip = start + len;
            anchor = ip;
            self.skipping = false;
            // Repeats of the second offset straight away, with no literals between.
            while ip <= limit {
                let rep2 = reps.0[1] as usize;
                let Some(len) = self.repeat_at(src, ip, end, rep2) else {
                    break;
                };
                emit(src, anchor, ip, rep2, len, reps, store);
                ip += len;
                anchor = ip;
            }
        }
        store.push_lits(src, anchor, end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_masks_mark_exactly_the_equal_bytes() {
        let tags: Vec<u8> = (0..64u32).map(|i| (i * 37 % 7) as u8).collect();
        for tag in 0..8u8 {
            let mask = match_mask(&tags, tag);
            for (i, &t) in tags.iter().enumerate() {
                assert_eq!(mask >> i & 1 == 1, t == tag, "tag {tag} at {i}");
            }
        }
        // Bytes next to a match, and 0x80 and 0xFF, aren't mistaken for one.
        let tricky = [0u8, 1, 0x80, 0xFF, 0, 0x7F, 0x81, 0];
        assert_eq!(match_mask(&tricky, 0), 0b1001_0001);
        assert_eq!(match_mask(&tricky, 0x80), 0b0000_0100);
    }
}

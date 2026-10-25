//! The deferred binary tree behind `btlazy2`, as the reference's `ZSTD_DUBT_*` keeps it.
//!
//! New positions go in cheaply, like a hash chain: each one points at the previous position
//! with its hash and is marked unsorted. A search walks back along the unsorted ones it meets
//! (up to its budget), sorts them into the tree, oldest first, and then searches the tree.
//! Positions no search reaches (most of those inside long matches) are never sorted at all,
//! which is where the time goes in a tree that sorts every position as it's entered.

use super::lazy::Search;
use super::matcher::{count, hash};
use super::params::Params;

/// The mark in a position's second slot until it has been sorted. Indices (position + 1) are
/// far below it.
const UNSORTED: u32 = u32::MAX;

/// The deferred tree, for matches of at least `MLS` bytes (4 to 6).
pub(crate) struct Dubt<const MLS: u32> {
    /// Newest position + 1 for each hash; 0 is empty.
    hash: Vec<u32>,
    hash_log: u32,
    /// Two slots per position (under the mask): sorted, its smaller and larger subtrees;
    /// unsorted, the previous position with its hash and [`UNSORTED`]. A last slot takes
    /// writes that go nowhere.
    tree: Vec<u32>,
    mask: usize,
    compares: usize,
    window: usize,
    /// The first position not yet entered.
    next_to_update: usize,
}

impl<const MLS: u32> Dubt<MLS> {
    pub(crate) fn new(p: &Params) -> Self {
        let bt_log = p.chain_log - 1;
        Self {
            hash: vec![0; 1 << p.hash_log],
            hash_log: p.hash_log,
            tree: vec![0; (2 << bt_log) + 1],
            mask: (1 << bt_log) - 1,
            compares: 1 << p.search_log,
            window: 1 << p.window_log,
            next_to_update: 0,
        }
    }

    fn dummy(&self) -> usize {
        self.tree.len() - 1
    }

    /// The lowest index a match for index `cur` may come from.
    fn low(&self, cur: usize) -> usize {
        cur.saturating_sub(self.window).max(1)
    }

    /// Enters the positions up to `ip`, unsorted.
    fn update(&mut self, src: &[u8], ip: usize) {
        for pos in self.next_to_update..ip {
            let h = hash::<MLS>(src, pos, self.hash_log);
            let index = pos + 1;
            let slot = 2 * (index & self.mask);
            self.tree[slot] = self.hash[h];
            self.tree[slot + 1] = UNSORTED;
            self.hash[h] = index as u32;
        }
        self.next_to_update = self.next_to_update.max(ip);
    }

    /// Sorts the entered position `cur` (an index) into the tree, its first slot pointing at
    /// the tree of older positions to sort it into.
    fn sort_one(&mut self, src: &[u8], cur: usize, end: usize, compares: usize, bt_low: usize) {
        let pos = cur - 1;
        let mut smaller = 2 * (cur & self.mask);
        let mut larger = smaller + 1;
        let mut index = self.tree[smaller] as usize;
        let low = self.low(cur);
        let (mut common_smaller, mut common_larger) = (0usize, 0usize);
        for _ in 0..compares {
            if index < low {
                break;
            }
            let next = 2 * (index & self.mask);
            let m = index - 1;
            let mut len = common_smaller.min(common_larger);
            len += count(src, pos + len, m + len, end);
            if pos + len == end {
                // No way to tell which side it goes: stop, to keep the tree consistent.
                break;
            }
            if src[m + len] < src[pos + len] {
                self.tree[smaller] = index as u32;
                common_smaller = len;
                if index <= bt_low {
                    smaller = self.dummy();
                    break;
                }
                smaller = next + 1;
                index = self.tree[next + 1] as usize;
            } else {
                self.tree[larger] = index as u32;
                common_larger = len;
                if index <= bt_low {
                    larger = self.dummy();
                    break;
                }
                larger = next;
                index = self.tree[next] as usize;
            }
        }
        self.tree[smaller] = 0;
        self.tree[larger] = 0;
    }

    /// Sorts the unsorted candidates for `ip`'s hash that the budget reaches, oldest first.
    fn sort_candidates(&mut self, src: &[u8], ip: usize, end: usize, unsort_limit: usize) {
        let h = hash::<MLS>(src, ip, self.hash_log);
        let mut index = self.hash[h] as usize;
        let mut budget = self.compares;
        let mut previous = 0usize;
        // Walk back along the unsorted ones, turning their marks into a chain forward.
        while index > unsort_limit
            && self.tree[2 * (index & self.mask) + 1] == UNSORTED
            && budget > 1
        {
            let slot = 2 * (index & self.mask);
            self.tree[slot + 1] = previous as u32;
            previous = index;
            index = self.tree[slot] as usize;
            budget -= 1;
        }
        // One still unsorted past the budget is dropped: a little compression for speed.
        if index > unsort_limit && self.tree[2 * (index & self.mask) + 1] == UNSORTED {
            let slot = 2 * (index & self.mask);
            self.tree[slot] = 0;
            self.tree[slot + 1] = 0;
        }
        // Sort them in, oldest first.
        let mut index = previous;
        while index != 0 {
            let next = self.tree[2 * (index & self.mask) + 1] as usize;
            self.sort_one(src, index, end, budget, unsort_limit);
            index = next;
            budget += 1;
        }
    }
}

impl<const MLS: u32> Search for Dubt<MLS> {
    fn longest(
        &mut self,
        src: &[u8],
        ip: usize,
        end: usize,
        _skipping: bool,
    ) -> Option<(usize, usize)> {
        if ip < self.next_to_update {
            // Passed over as part of a repetition.
            return None;
        }
        self.update(src, ip);
        let cur = ip + 1;
        let low = self.low(cur);
        let bt_low = cur.saturating_sub(self.mask);
        self.sort_candidates(src, ip, end, bt_low.max(low - 1));
        let h = hash::<MLS>(src, ip, self.hash_log);
        let mut index = self.hash[h] as usize;
        self.hash[h] = cur as u32;
        let mut smaller = 2 * (cur & self.mask);
        let mut larger = smaller + 1;
        let (mut common_smaller, mut common_larger) = (0usize, 0usize);
        let mut match_end = cur + 8 + 1;
        let mut best_len = 0usize;
        // As the reference starts it: so far from anything that any match is better.
        let mut best_off_base = 999_999_999u32;
        for _ in 0..self.compares {
            if index < low {
                break;
            }
            let next = 2 * (index & self.mask);
            let m = index - 1;
            let mut len = common_smaller.min(common_larger);
            len += count(src, ip + len, m + len, end);
            if len > best_len {
                if len > match_end - index {
                    match_end = index + len;
                }
                // Longer only counts if it's worth the bigger offset it may come with.
                let distance = (cur - index) as u32;
                if 4 * (len - best_len) as i64
                    > i64::from((distance + 1).ilog2()) - i64::from(best_off_base.ilog2())
                {
                    best_len = len;
                    best_off_base = distance + 3;
                }
                if ip + len == end {
                    break;
                }
            }
            if ip + len == end {
                break;
            }
            if src[m + len] < src[ip + len] {
                self.tree[smaller] = index as u32;
                common_smaller = len;
                if index <= bt_low {
                    smaller = self.dummy();
                    break;
                }
                smaller = next + 1;
                index = self.tree[next + 1] as usize;
            } else {
                self.tree[larger] = index as u32;
                common_larger = len;
                if index <= bt_low {
                    larger = self.dummy();
                    break;
                }
                larger = next;
                index = self.tree[next] as usize;
            }
        }
        self.tree[smaller] = 0;
        self.tree[larger] = 0;
        // Over a repetition, skip the positions it covers.
        self.next_to_update = match_end - 8 - 1;
        (best_len >= 4).then(|| (best_len, best_off_base as usize - 3))
    }

    fn begin_block(&mut self, start: usize) {
        self.next_to_update = self.next_to_update.max(start.saturating_sub(self.window));
    }

    fn reset(&mut self) {
        self.hash.fill(0);
        self.tree.fill(0);
        self.next_to_update = 0;
    }
}

//! The optimal parser (`btopt`, `btultra`, `btultra2`, levels 16 to 22 for large inputs),
//! modelled on the reference's `zstd_opt.c`, and the binary-tree match finder it uses.
//!
//! The finder keeps, for every position, a binary tree of the earlier positions with the
//! same hash, ordered by the bytes that follow; a search walks it once and comes back with
//! every match longer than the ones before it (the repeat offsets first). The parser prices
//! each literal, literal length, offset and match length in fractions of a bit, from
//! statistics that follow the data, and finds the cheapest way through up to 4096 positions
//! at a time: forward, recording the cheapest way to reach each position, then backward,
//! reading off the sequences. `btultra2` first parses the first block once only to collect
//! statistics.
//!
//! Every sequence is checked byte for byte before it's recorded, so a mistake in the tree
//! or the parse costs compression, never correctness.

use std::ops::Range;

use super::matcher::{MatchFinder, count, emit_coded, hash, read32};
use super::params::{Params, Strategy};
use super::seq::{Reps, SeqStore, ll_code, ml_code};
use crate::decode::{LL_CODES, ML_CODES};

/// Positions the parser looks ahead at most.
const OPT_NUM: usize = 1 << 12;
const MAX_PRICE: i32 = 1 << 30;
/// Prices are in 1/256 bits.
const BITCOST: u32 = 256;
/// Literal counts grow this much per literal, so they adapt faster.
const LITFREQ_ADD: u32 = 2;
/// Blocks this small are priced from fixed costs, not statistics.
const PREDEF_THRESHOLD: usize = 8;
/// The 3-byte hash table's size, as a log, at most.
const HASH3_LOG_MAX: u32 = 17;
/// The largest block, as a literal length.
const BLOCK_MAX: u32 = 1 << 17;

/// Where the reference starts the literal-length statistics.
const BASE_LL: [u32; 36] = [
    4, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1,
];
/// Where the reference starts the offset-code statistics.
const BASE_OF: [u32; 32] = [
    6, 2, 1, 1, 2, 3, 4, 4, 4, 3, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

/// The price model: how often each symbol has been seen, and what that makes it cost.
struct Stats {
    lit: [u32; 256],
    ll: [u32; 36],
    ml: [u32; 53],
    of: [u32; 32],
    lit_sum: u32,
    ll_sum: u32,
    ml_sum: u32,
    of_sum: u32,
    /// What the sums cost, the start of every price.
    lit_base: u32,
    ll_base: u32,
    ml_base: u32,
    of_base: u32,
    /// Fixed prices, for a tiny first block.
    predef: bool,
    /// Fractional weights (btultra and up) rather than whole bits.
    frac: bool,
    /// 0 for btopt, 2 for btultra and btultra2.
    level: u8,
}

/// The sum of `table` after dividing each count by `1 << shift`, keeping counts of 1 (or of
/// at least 1, with `keep_one`).
fn downscale(table: &mut [u32], shift: u32, keep_one: bool) -> u32 {
    let mut sum = 0;
    for t in table {
        let base = if keep_one { 1 } else { u32::from(*t > 0) };
        *t = base + (*t >> shift);
        sum += *t;
    }
    sum
}

/// Scales `table` down if its sum is over `1 << log_target`; returns the sum.
fn scale(table: &mut [u32], log_target: u32) -> u32 {
    let sum: u32 = table.iter().sum();
    let factor = sum >> log_target;
    if factor <= 1 {
        return sum;
    }
    downscale(table, factor.ilog2(), true)
}

impl Stats {
    fn new(level: u8) -> Self {
        Self {
            lit: [0; 256],
            ll: [0; 36],
            ml: [0; 53],
            of: [0; 32],
            lit_sum: 0,
            ll_sum: 0,
            ml_sum: 0,
            of_sum: 0,
            lit_base: 0,
            ll_base: 0,
            ml_base: 0,
            of_base: 0,
            predef: false,
            frac: level >= 1,
            level,
        }
    }

    /// What a count of `stat` is worth, in 1/256 bits: about its log.
    #[inline]
    fn weight(&self, stat: u32) -> u32 {
        let stat = stat + 1;
        let hb = stat.ilog2();
        if self.frac {
            hb * BITCOST + ((stat << 8) >> hb)
        } else {
            hb * BITCOST
        }
    }

    /// Starts a block: from the block's own literals and fixed starting points the first
    /// time, from the last block's statistics, scaled down, after.
    fn rescale(&mut self, block: &[u8]) {
        self.predef = false;
        if self.ll_sum == 0 {
            if block.len() <= PREDEF_THRESHOLD {
                self.predef = true;
            }
            self.lit = [0; 256];
            for &b in block {
                self.lit[b as usize] += 1;
            }
            self.lit_sum = downscale(&mut self.lit, 8, false);
            self.ll = BASE_LL;
            self.ll_sum = BASE_LL.iter().sum();
            self.ml = [1; 53];
            self.ml_sum = 53;
            self.of = BASE_OF;
            self.of_sum = BASE_OF.iter().sum();
        } else {
            self.lit_sum = scale(&mut self.lit, 12);
            self.ll_sum = scale(&mut self.ll, 11);
            self.ml_sum = scale(&mut self.ml, 11);
            self.of_sum = scale(&mut self.of, 11);
        }
        self.set_base_prices();
    }

    fn set_base_prices(&mut self) {
        self.lit_base = self.weight(self.lit_sum);
        self.ll_base = self.weight(self.ll_sum);
        self.ml_base = self.weight(self.ml_sum);
        self.of_base = self.weight(self.of_sum);
    }

    /// The price of one literal.
    #[inline]
    fn literal(&self, b: u8) -> i32 {
        if self.predef {
            return (6 * BITCOST) as i32;
        }
        let w = self
            .weight(self.lit[b as usize])
            .min(self.lit_base - BITCOST);
        (self.lit_base - w) as i32
    }

    /// The price of a literal length.
    fn lit_len(&self, len: u32) -> i32 {
        if self.predef {
            return self.weight(len) as i32;
        }
        // A whole block of literals can't be coded as a length; price it a bit over.
        if len == BLOCK_MAX {
            return BITCOST as i32 + self.lit_len(BLOCK_MAX - 1);
        }
        let c = ll_code(len) as usize;
        (u32::from(LL_CODES[c].1) * BITCOST + self.ll_base - self.weight(self.ll[c])) as i32
    }

    /// The price of a literal length one more than `len - 1`.
    fn lit_len_step(&self, len: u32) -> i32 {
        self.lit_len(len) - self.lit_len(len - 1)
    }

    /// The price of a match (its offset value and length), not counting its literals.
    #[inline]
    fn matched(&self, off_base: u32, len: u32) -> i32 {
        let off_code = off_base.ilog2();
        if self.predef {
            return (self.weight(len - 3) + (16 + off_code) * BITCOST) as i32;
        }
        let mut price = off_code * BITCOST + self.of_base - self.weight(self.of[off_code as usize]);
        if self.level < 2 && off_code >= 20 {
            // Far offsets are slow to decompress; btopt avoids them a little.
            price += (off_code - 19) * 2 * BITCOST;
        }
        let c = ml_code(len) as usize;
        price += u32::from(ML_CODES[c].1) * BITCOST + self.ml_base - self.weight(self.ml[c]);
        // A little more per match, for fewer sequences and faster decompression.
        (price + BITCOST / 5) as i32
    }

    /// Counts a sequence that was chosen.
    fn update(&mut self, lits: &[u8], off_base: u32, len: u32) {
        for &b in lits {
            self.lit[b as usize] += LITFREQ_ADD;
        }
        self.lit_sum += lits.len() as u32 * LITFREQ_ADD;
        self.ll[ll_code(lits.len() as u32) as usize] += 1;
        self.ll_sum += 1;
        self.of[off_base.ilog2() as usize] += 1;
        self.of_sum += 1;
        self.ml[ml_code(len) as usize] += 1;
        self.ml_sum += 1;
    }
}

/// A match found at a position: its offset value (a repeat code 1 to 3, or offset + 3) and
/// length.
#[derive(Clone, Copy, Debug)]
struct Match {
    off_base: u32,
    len: u32,
}

/// What a search is for: the repeat offsets at the position, whether no literals come
/// before it, and the length a match has to reach.
#[derive(Clone, Copy, Debug)]
struct Query {
    reps: [u32; 3],
    ll0: bool,
    beat: usize,
}

/// The binary-tree finder, for matches of at least `MLS` bytes (3 to 6).
pub(crate) struct Bt<const MLS: u32> {
    /// Newest position + 1 for each hash; 0 is empty.
    hash: Vec<u32>,
    hash_log: u32,
    /// Two entries per position (under the mask): the roots of its smaller and larger
    /// subtrees, as positions + 1. A last entry takes writes that go nowhere.
    tree: Vec<u32>,
    mask: usize,
    /// The 3-byte hash table (minimum match 3 only).
    hash3: Vec<u32>,
    hash3_log: u32,
    compares: u32,
    window: usize,
    /// A match longer than this is good enough to stop at.
    sufficient: usize,
    next_to_update: usize,
    next_to_update3: usize,
}

impl<const MLS: u32> Bt<MLS> {
    pub(crate) fn new(p: &Params) -> Self {
        let bt_log = p.chain_log - 1;
        let hash3_log = if MLS == 3 {
            HASH3_LOG_MAX.min(p.window_log)
        } else {
            0
        };
        Self {
            hash: vec![0; 1 << p.hash_log],
            hash_log: p.hash_log,
            tree: vec![0; (2 << bt_log) + 1],
            mask: (1 << bt_log) - 1,
            hash3: vec![0; if MLS == 3 { 1 << hash3_log } else { 0 }],
            hash3_log,
            compares: 1 << p.search_log,
            window: 1 << p.window_log,
            sufficient: (p.target_length as usize).min(OPT_NUM - 1),
            next_to_update: 0,
            next_to_update3: 0,
        }
    }

    /// Forgets everything, keeping the tables' memory.
    fn reset(&mut self) {
        self.hash.fill(0);
        self.tree.fill(0);
        self.hash3.fill(0);
        self.next_to_update = 0;
        self.next_to_update3 = 0;
    }

    fn dummy(&self) -> usize {
        self.tree.len() - 1
    }

    /// The lowest index (position + 1) a match for index `cur` may come from.
    fn low(&self, cur: usize) -> usize {
        cur.saturating_sub(self.window).max(1)
    }

    /// Enters `pos` into the tree, sorting the earlier positions with its hash around it.
    /// Returns how far to go before the next one: further over a long repetition, where
    /// every position would be much the same.
    fn insert_one(&mut self, src: &[u8], pos: usize, end: usize, target: usize) -> usize {
        let h = hash::<MLS>(src, pos, self.hash_log);
        let cur = pos + 1;
        let mut index = self.hash[h] as usize;
        let (mut common_smaller, mut common_larger) = (0usize, 0usize);
        let bt_low = cur.saturating_sub(self.mask);
        let mut smaller = 2 * (cur & self.mask);
        let mut larger = smaller + 1;
        let window_low = self.low(target + 1);
        let mut match_end = cur + 8 + 1;
        let mut best = 8;
        self.hash[h] = cur as u32;
        for _ in 0..self.compares {
            if index < window_low {
                break;
            }
            let next = 2 * (index & self.mask);
            let m = index - 1;
            let mut len = common_smaller.min(common_larger);
            len += count(src, pos + len, m + len, end);
            if len > best {
                best = len;
                if len > match_end - index {
                    match_end = index + len;
                }
            }
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
        let skip = if best > 384 { (best - 384).min(192) } else { 0 };
        skip.max(match_end - (cur + 8))
    }

    /// Enters the positions up to `ip`.
    fn update_tree(&mut self, src: &[u8], ip: usize, end: usize) {
        let mut pos = self.next_to_update;
        while pos < ip {
            pos += self.insert_one(src, pos, end, ip);
        }
        self.next_to_update = ip;
    }

    /// The newest earlier position (+ 1) with `ip`'s 3-byte hash, entering those before.
    fn hash3_find(&mut self, src: &[u8], ip: usize) -> usize {
        while self.next_to_update3 < ip {
            let p = self.next_to_update3;
            let h = hash::<3>(src, p, self.hash3_log);
            self.hash3[h] = p as u32 + 1;
            self.next_to_update3 += 1;
        }
        self.next_to_update3 = ip;
        self.hash3[hash::<3>(src, ip, self.hash3_log)] as usize
    }

    /// Whether `offset` can be used at `pos`.
    fn usable(&self, offset: usize, pos: usize) -> bool {
        offset > 0 && offset <= pos && offset <= self.window
    }

    /// Whether the minimum match at `a` and `b` is the same.
    #[inline]
    fn same(src: &[u8], a: usize, b: usize) -> bool {
        let diff = read32(src, a) ^ read32(src, b);
        if MLS == 3 {
            diff.trailing_zeros() >= 24
        } else {
            diff == 0
        }
    }

    /// The repeat offsets' matches at `ip`, longer than `*best`, into `out`; returns whether
    /// one is long enough to stop at.
    fn repeat_matches(
        &self,
        src: &[u8],
        ip: usize,
        end: usize,
        q: Query,
        best: &mut usize,
        out: &mut Vec<Match>,
    ) -> bool {
        let min_match = if MLS == 3 { 3 } else { 4 };
        // With no literals before, the codes shift by one, and code 3 is the first repeat
        // less one.
        let ll0 = usize::from(q.ll0);
        for code in ll0..3 + ll0 {
            let offset = if code == 3 {
                q.reps[0].wrapping_sub(1)
            } else {
                q.reps[code]
            } as usize;
            if !self.usable(offset, ip) || !Self::same(src, ip, ip - offset) {
                continue;
            }
            let len = min_match + count(src, ip + min_match, ip + min_match - offset, end);
            if len > *best {
                *best = len;
                out.push(Match {
                    off_base: (code - ll0 + 1) as u32,
                    len: len as u32,
                });
                if len > self.sufficient || ip + len == end {
                    return true;
                }
            }
        }
        false
    }

    /// A 3-byte match at `ip` from the small hash table, replacing what's in `out`; returns
    /// whether it's long enough to stop at.
    fn hash3_match(
        &mut self,
        src: &[u8],
        ip: usize,
        end: usize,
        best: &mut usize,
        out: &mut Vec<Match>,
    ) -> bool {
        let cur = ip + 1;
        let index3 = self.hash3_find(src, ip);
        if index3 < self.low(cur) || cur - index3 >= 1 << 18 {
            return false;
        }
        let len = count(src, ip, index3 - 1, end);
        if len < 3 {
            return false;
        }
        *best = len;
        out.clear();
        out.push(Match {
            off_base: (cur - index3 + 3) as u32,
            len: len as u32,
        });
        len > self.sufficient || ip + len == end
    }

    /// Every match at `ip` longer than the ones before it (and than `q.beat - 1`), shortest
    /// first, into `out`: the repeat offsets (as the decoder would read them after no
    /// literals, or some), then the tree. Enters `ip` into the tree on the way.
    fn all_matches(&mut self, src: &[u8], ip: usize, end: usize, q: Query, out: &mut Vec<Match>) {
        out.clear();
        if ip < self.next_to_update {
            // Passed over as part of a repetition.
            return;
        }
        self.update_tree(src, ip, end);
        let cur = ip + 1;
        let h = hash::<MLS>(src, ip, self.hash_log);
        let mut index = self.hash[h] as usize;
        let (mut common_smaller, mut common_larger) = (0usize, 0usize);
        let bt_low = cur.saturating_sub(self.mask);
        let low = self.low(cur);
        let mut smaller = 2 * (cur & self.mask);
        let mut larger = smaller + 1;
        let mut match_end = cur + 8 + 1;
        let mut best = q.beat - 1;
        if self.repeat_matches(src, ip, end, q, &mut best, out) {
            return;
        }
        if MLS == 3 && best < 3 && self.hash3_match(src, ip, end, &mut best, out) {
            self.next_to_update = ip + 1;
            return;
        }
        self.hash[h] = cur as u32;
        for _ in 0..self.compares {
            if index < low {
                break;
            }
            let next = 2 * (index & self.mask);
            let m = index - 1;
            let mut len = common_smaller.min(common_larger);
            len += count(src, ip + len, m + len, end);
            if len > best {
                if len > match_end - index {
                    match_end = index + len;
                }
                best = len;
                out.push(Match {
                    off_base: (cur - index + 3) as u32,
                    len: len as u32,
                });
                if len > OPT_NUM || ip + len == end {
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
    }
}

/// One position of the parse: the cheapest way found to reach it. During the forward pass a
/// "stretch": a match (`mlen` bytes, coded `off`) followed by `litlen` literals.
#[derive(Clone, Copy, Debug, Default)]
struct Opt {
    price: i32,
    off: u32,
    mlen: u32,
    litlen: u32,
    /// The repeat offsets after the match.
    rep: [u32; 3],
}

/// The repeat offsets after a match coded `off_base`, from `rep`, as the decoder updates
/// them (`ll0`: no literals before the match).
fn new_rep(rep: [u32; 3], off_base: u32, ll0: bool) -> [u32; 3] {
    if off_base > 3 {
        return [off_base - 3, rep[0], rep[1]];
    }
    let code = off_base - 1 + u32::from(ll0);
    match code {
        0 => rep,
        1 => [rep[1], rep[0], rep[2]],
        2 => [rep[2], rep[0], rep[1]],
        _ => [rep[0].wrapping_sub(1), rep[0], rep[1]],
    }
}

/// The optimal parser, over a binary tree of matches of at least `MLS` bytes.
pub(crate) struct Optimal<const MLS: u32> {
    bt: Bt<MLS>,
    stats: Stats,
    opt: Vec<Opt>,
    matches: Vec<Match>,
    level: u8,
    ultra2: bool,
    min_match: usize,
    sufficient: usize,
}

impl<const MLS: u32> Optimal<MLS> {
    pub(crate) fn new(p: &Params) -> Self {
        let level = if p.strategy == Strategy::BtOpt { 0 } else { 2 };
        Self {
            bt: Bt::new(p),
            stats: Stats::new(level),
            opt: vec![Opt::default(); OPT_NUM + 3],
            matches: Vec::with_capacity(64),
            level,
            ultra2: p.strategy == Strategy::BtUltra2,
            min_match: if p.min_match == 3 { 3 } else { 4 },
            sufficient: (p.target_length as usize).min(OPT_NUM - 1),
        }
    }

    /// Parses `block` into sequences.
    #[allow(
        clippy::too_many_lines,
        reason = "one pass, as the reference has it; split, it reads worse"
    )]
    fn parse(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore) {
        let end = block.end;
        let limit = end.saturating_sub(8);
        self.bt.next_to_update3 = self.bt.next_to_update;
        self.stats.rescale(&src[block.clone()]);
        let mut anchor = block.start;
        let mut ip = block.start + usize::from(block.start == 0);
        let mut rep = reps.0;
        let min_match = self.min_match;
        while ip < limit {
            let litlen = (ip - anchor) as u32;
            let ll0 = litlen == 0;
            let mut matches = std::mem::take(&mut self.matches);
            self.bt.all_matches(
                src,
                ip,
                end,
                Query {
                    reps: rep,
                    ll0,
                    beat: min_match,
                },
                &mut matches,
            );
            if matches.is_empty() {
                self.matches = matches;
                ip += 1;
                continue;
            }
            let opt = &mut self.opt;
            opt[0] = Opt {
                price: self.stats.lit_len(litlen),
                off: 0,
                mlen: 0,
                litlen,
                rep,
            };
            let longest = matches[matches.len() - 1];
            let last_stretch;
            let mut cur;
            let mut last_pos;
            if longest.len as usize > self.sufficient {
                // Long enough: take it without looking further.
                last_stretch = Opt {
                    mlen: longest.len,
                    off: longest.off_base,
                    litlen: 0,
                    ..Opt::default()
                };
                cur = 0;
                last_pos = longest.len as usize;
            } else {
                for (pos, o) in opt.iter_mut().enumerate().take(min_match).skip(1) {
                    *o = Opt {
                        price: MAX_PRICE,
                        mlen: 0,
                        litlen: litlen + pos as u32,
                        ..Opt::default()
                    };
                }
                let mut pos = min_match;
                let base = opt[0].price + self.stats.lit_len(0);
                for m in &matches {
                    while pos <= m.len as usize {
                        opt[pos] = Opt {
                            price: base + self.stats.matched(m.off_base, pos as u32),
                            off: m.off_base,
                            mlen: pos as u32,
                            litlen: 0,
                            rep: [0; 3],
                        };
                        pos += 1;
                    }
                }
                last_pos = pos - 1;
                opt[pos].price = MAX_PRICE;
                // Forward: the cheapest way to each position.
                cur = 1;
                let mut shortcut = None;
                while cur <= last_pos {
                    let inr = ip + cur;
                    let litlen = opt[cur - 1].litlen + 1;
                    let price = opt[cur - 1].price
                        + self.stats.literal(src[ip + cur - 1])
                        + self.stats.lit_len_step(litlen);
                    if price <= opt[cur].price {
                        let previous = opt[cur];
                        opt[cur] = opt[cur - 1];
                        opt[cur].litlen = litlen;
                        opt[cur].price = price;
                        // A match then one literal may still beat the run of literals.
                        if self.level >= 1
                            && previous.litlen == 0
                            && self.stats.lit_len_step(1) < 0
                            && ip + cur < end
                        {
                            let next_lit = self.stats.literal(src[ip + cur]);
                            let with_one = previous.price + next_lit + self.stats.lit_len_step(1);
                            let with_more = price + next_lit + self.stats.lit_len_step(litlen + 1);
                            if with_one < with_more && with_one < opt[cur + 1].price {
                                let prev = cur - previous.mlen as usize;
                                opt[cur + 1] = Opt {
                                    rep: new_rep(
                                        opt[prev].rep,
                                        previous.off,
                                        opt[prev].litlen == 0,
                                    ),
                                    litlen: 1,
                                    price: with_one,
                                    ..previous
                                };
                                last_pos = last_pos.max(cur + 1);
                            }
                        }
                    }
                    if opt[cur].litlen == 0 {
                        // Just after a match: its repeat offsets.
                        let prev = cur - opt[cur].mlen as usize;
                        opt[cur].rep = new_rep(opt[prev].rep, opt[cur].off, opt[prev].litlen == 0);
                    }
                    if inr > limit {
                        cur += 1;
                        continue;
                    }
                    if cur == last_pos {
                        break;
                    }
                    if self.level == 0
                        && opt[cur + 1].price <= opt[cur].price + (BITCOST / 2) as i32
                    {
                        // The next position is cheaper already: skip this one.
                        cur += 1;
                        continue;
                    }
                    let ll0 = opt[cur].litlen == 0;
                    let base = opt[cur].price + self.stats.lit_len(0);
                    self.bt.all_matches(
                        src,
                        inr,
                        end,
                        Query {
                            reps: opt[cur].rep,
                            ll0,
                            beat: min_match,
                        },
                        &mut matches,
                    );
                    if matches.is_empty() {
                        cur += 1;
                        continue;
                    }
                    let longest = matches[matches.len() - 1];
                    let longest_len = longest.len as usize;
                    if longest_len > self.sufficient
                        || cur + longest_len >= OPT_NUM
                        || ip + cur + longest_len >= end
                    {
                        shortcut = Some(Opt {
                            mlen: longest.len,
                            off: longest.off_base,
                            litlen: 0,
                            ..Opt::default()
                        });
                        last_pos = cur + longest_len;
                        break;
                    }
                    for (i, m) in matches.iter().enumerate() {
                        let start = if i > 0 {
                            matches[i - 1].len as usize + 1
                        } else {
                            min_match
                        };
                        for mlen in (start..=m.len as usize).rev() {
                            let pos = cur + mlen;
                            let price = base + self.stats.matched(m.off_base, mlen as u32);
                            if pos > last_pos || price < opt[pos].price {
                                while last_pos < pos {
                                    last_pos += 1;
                                    opt[last_pos].price = MAX_PRICE;
                                    opt[last_pos].litlen = 1;
                                }
                                opt[pos].mlen = mlen as u32;
                                opt[pos].off = m.off_base;
                                opt[pos].litlen = 0;
                                opt[pos].price = price;
                            } else if self.level == 0 {
                                break;
                            }
                        }
                    }
                    opt[last_pos + 1].price = MAX_PRICE;
                    cur += 1;
                }
                if let Some(s) = shortcut {
                    last_stretch = s;
                } else {
                    last_stretch = opt[last_pos];
                    cur = last_pos - last_stretch.mlen as usize;
                }
            }
            self.matches = matches;
            if last_stretch.mlen == 0 {
                // Nothing but literals.
                ip += last_pos;
                continue;
            }
            if last_stretch.litlen == 0 {
                rep = new_rep(
                    self.opt[cur].rep,
                    last_stretch.off,
                    self.opt[cur].litlen == 0,
                );
            } else {
                rep = last_stretch.rep;
                cur -= last_stretch.litlen as usize;
            }
            // Backward: lay the stretches out as sequences, last first.
            let opt = &mut self.opt;
            let store_end = cur + 2;
            let mut store_start;
            let mut stretch_pos = cur;
            if last_stretch.litlen > 0 {
                opt[store_end].litlen = last_stretch.litlen;
                opt[store_end].mlen = 0;
                store_start = store_end - 1;
                opt[store_start] = last_stretch;
            } else {
                opt[store_end] = last_stretch;
                store_start = store_end;
            }
            loop {
                let next = opt[stretch_pos];
                opt[store_start].litlen = next.litlen;
                if next.mlen == 0 {
                    break;
                }
                store_start -= 1;
                opt[store_start] = next;
                stretch_pos -= (next.litlen + next.mlen) as usize;
            }
            // Record them, checking each; one that doesn't hold (which shouldn't happen)
            // leaves its bytes, and the rest of the batch, as literals.
            let mut cursor = anchor;
            let mut sound = true;
            for p in store_start..=store_end {
                let Opt {
                    litlen, mlen, off, ..
                } = self.opt[p];
                if mlen == 0 {
                    ip = cursor + litlen as usize;
                    continue;
                }
                let start = cursor + litlen as usize;
                cursor = start + mlen as usize;
                ip = cursor;
                if sound && emit_coded(src, anchor, start, off, mlen as usize, reps, store) {
                    self.stats.update(&src[anchor..start], off, mlen);
                    anchor = cursor;
                } else {
                    sound = false;
                }
            }
            rep = if sound { rep } else { reps.0 };
            self.stats.set_base_prices();
        }
        store.push_lits(src, anchor, end);
    }
}

impl<const MLS: u32> MatchFinder for Optimal<MLS> {
    fn reset(&mut self) {
        self.bt.reset();
        self.stats = Stats::new(self.level);
    }

    fn find(&mut self, src: &[u8], block: Range<usize>, reps: &mut Reps, store: &mut SeqStore) {
        if self.ultra2
            && self.stats.ll_sum == 0
            && block.start == 0
            && block.len() > PREDEF_THRESHOLD
        {
            // A first pass over the first block, for its statistics only; then the tables
            // forget it.
            let mut scratch_reps = *reps;
            let mut scratch = SeqStore::default();
            scratch.prepare(block.len());
            self.parse(src, block.clone(), &mut scratch_reps, &mut scratch);
            self.bt.reset();
        }
        self.parse(src, block, reps, store);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_history_follows_the_decoder() {
        let r = [10, 20, 30];
        assert_eq!(new_rep(r, 50 + 3, false), [50, 10, 20]);
        assert_eq!(new_rep(r, 1, false), r, "repeat 1 with literals");
        assert_eq!(new_rep(r, 2, false), [20, 10, 30]);
        assert_eq!(new_rep(r, 3, false), [30, 10, 20]);
        assert_eq!(
            new_rep(r, 1, true),
            [20, 10, 30],
            "no literals: code 1 is repeat 2"
        );
        assert_eq!(
            new_rep(r, 3, true),
            [9, 10, 20],
            "no literals: code 3 is repeat 1 - 1"
        );
        // The same as the encoder's own history.
        for off_base in 1..=6 {
            for ll0 in [false, true] {
                let mut reps = Reps(r);
                reps.apply(off_base, u32::from(!ll0));
                assert_eq!(reps.0, new_rep(r, off_base, ll0), "{off_base} {ll0}");
            }
        }
    }

    #[test]
    fn prices_rise_with_rarity() {
        let mut s = Stats::new(2);
        // The first block's counts are divided by 256: enough of each to tell them apart.
        let mut block = vec![b'a'; 3000];
        block.extend([b'b'; 300]);
        s.rescale(&block);
        assert!(s.literal(b'a') < s.literal(b'b'));
        assert!(s.literal(b'b') < s.literal(b'z'));
        assert!(
            s.matched(1, 4) < s.matched(1000 + 3, 4),
            "repeats are cheaper"
        );
        assert!(s.lit_len(0) < s.lit_len(1000));
    }
}

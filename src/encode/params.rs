//! Compression parameters per level, as in the reference implementation's level table for
//! large inputs, adjusted down for small inputs.

/// How matches are searched for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strategy {
    /// One hash table, one candidate per position.
    Fast,
    /// Two hash tables (long and short matches).
    DoubleFast,
    /// Hash chains, first good match.
    Greedy,
    /// Hash chains, one step of lookahead.
    Lazy,
    /// Hash chains, two steps of lookahead.
    Lazy2,
    /// A binary tree of earlier positions, two steps of lookahead.
    BtLazy2,
    /// Optimal parsing over a binary tree.
    BtOpt,
    /// Optimal parsing with a finer price model.
    BtUltra,
    /// Optimal parsing, two passes.
    BtUltra2,
}

/// The parameters of one level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    /// Log2 of the window (the farthest a match may reach back).
    pub window_log: u32,
    /// Log2 of the chain table (hash chains or the binary tree).
    pub chain_log: u32,
    /// Log2 of the hash table.
    pub hash_log: u32,
    /// Log2 of the number of candidates searched.
    pub search_log: u32,
    /// The shortest match looked for.
    pub min_match: u32,
    /// A match this long ends the search (optimal parsers: the length worth taking as is).
    /// For the fast strategy, the step acceleration.
    pub target_length: u32,
    /// The search strategy.
    pub strategy: Strategy,
}

/// The highest level.
pub(crate) const MAX_LEVEL: i32 = 22;
/// The lowest (fastest) level.
pub(crate) const MIN_LEVEL: i32 = -131_072;
/// The level used for 0.
pub(crate) const DEFAULT_LEVEL: i32 = 3;

use Strategy::{BtLazy2, BtOpt, BtUltra, BtUltra2, DoubleFast, Fast, Greedy, Lazy, Lazy2};

const fn p(w: u32, c: u32, h: u32, s: u32, m: u32, t: u32, strategy: Strategy) -> Params {
    Params {
        window_log: w,
        chain_log: c,
        hash_log: h,
        search_log: s,
        min_match: m,
        target_length: t,
        strategy,
    }
}

/// The level tables, as the reference zstd 1.5.7 reports them: inputs over 256 KiB (or of
/// unknown size), up to 256 KiB, up to 128 KiB, and up to 16 KiB. Row 0 is the base for the
/// negative levels; rows 1 to 22 are the levels.
const TABLES: [[Params; 23]; 4] = [
    [
        p(19, 12, 13, 1, 6, 1, Fast),
        p(19, 13, 14, 1, 7, 0, Fast),
        p(20, 15, 16, 1, 6, 0, Fast),
        p(21, 16, 17, 1, 5, 0, DoubleFast),
        p(21, 18, 18, 1, 5, 0, DoubleFast),
        p(21, 18, 19, 3, 5, 2, Greedy),
        p(21, 18, 19, 3, 5, 4, Lazy),
        p(21, 19, 20, 4, 5, 8, Lazy),
        p(21, 19, 20, 4, 5, 16, Lazy2),
        p(22, 20, 21, 4, 5, 16, Lazy2),
        p(22, 21, 22, 5, 5, 16, Lazy2),
        p(22, 21, 22, 6, 5, 16, Lazy2),
        p(22, 22, 23, 6, 5, 32, Lazy2),
        p(22, 22, 22, 4, 5, 32, BtLazy2),
        p(22, 22, 23, 5, 5, 32, BtLazy2),
        p(22, 23, 23, 6, 5, 32, BtLazy2),
        p(22, 22, 22, 5, 5, 48, BtOpt),
        p(23, 23, 22, 5, 4, 64, BtOpt),
        p(23, 23, 22, 6, 3, 64, BtUltra),
        p(23, 24, 22, 7, 3, 256, BtUltra2),
        p(25, 25, 23, 7, 3, 256, BtUltra2),
        p(26, 26, 24, 7, 3, 512, BtUltra2),
        p(27, 27, 25, 9, 3, 999, BtUltra2),
    ],
    [
        p(18, 12, 13, 1, 5, 1, Fast),
        p(18, 13, 14, 1, 6, 0, Fast),
        p(18, 14, 14, 1, 5, 0, DoubleFast),
        p(18, 16, 16, 1, 4, 0, DoubleFast),
        p(18, 16, 17, 3, 5, 2, Greedy),
        p(18, 17, 18, 5, 5, 2, Greedy),
        p(18, 18, 19, 3, 5, 4, Lazy),
        p(18, 18, 19, 4, 4, 4, Lazy),
        p(18, 18, 19, 4, 4, 8, Lazy2),
        p(18, 18, 19, 5, 4, 8, Lazy2),
        p(18, 18, 19, 6, 4, 8, Lazy2),
        p(18, 18, 19, 5, 4, 12, BtLazy2),
        p(18, 19, 19, 7, 4, 12, BtLazy2),
        p(18, 18, 19, 4, 4, 16, BtOpt),
        p(18, 18, 19, 4, 3, 32, BtOpt),
        p(18, 18, 19, 6, 3, 128, BtOpt),
        p(18, 19, 19, 6, 3, 128, BtUltra),
        p(18, 19, 19, 8, 3, 256, BtUltra),
        p(18, 19, 19, 6, 3, 128, BtUltra2),
        p(18, 19, 19, 8, 3, 256, BtUltra2),
        p(18, 19, 19, 10, 3, 512, BtUltra2),
        p(18, 19, 19, 12, 3, 512, BtUltra2),
        p(18, 19, 19, 13, 3, 999, BtUltra2),
    ],
    [
        p(17, 12, 12, 1, 5, 1, Fast),
        p(17, 12, 13, 1, 6, 0, Fast),
        p(17, 13, 15, 1, 5, 0, Fast),
        p(17, 15, 16, 2, 5, 0, DoubleFast),
        p(17, 17, 17, 2, 4, 0, DoubleFast),
        p(17, 16, 17, 3, 4, 2, Greedy),
        p(17, 16, 17, 3, 4, 4, Lazy),
        p(17, 16, 17, 3, 4, 8, Lazy2),
        p(17, 16, 17, 4, 4, 8, Lazy2),
        p(17, 16, 17, 5, 4, 8, Lazy2),
        p(17, 16, 17, 6, 4, 8, Lazy2),
        p(17, 17, 17, 5, 4, 8, BtLazy2),
        p(17, 18, 17, 7, 4, 12, BtLazy2),
        p(17, 18, 17, 3, 4, 12, BtOpt),
        p(17, 18, 17, 4, 3, 32, BtOpt),
        p(17, 18, 17, 6, 3, 256, BtOpt),
        p(17, 18, 17, 6, 3, 128, BtUltra),
        p(17, 18, 17, 8, 3, 256, BtUltra),
        p(17, 18, 17, 10, 3, 512, BtUltra),
        p(17, 18, 17, 5, 3, 256, BtUltra2),
        p(17, 18, 17, 7, 3, 512, BtUltra2),
        p(17, 18, 17, 9, 3, 512, BtUltra2),
        p(17, 18, 17, 11, 3, 999, BtUltra2),
    ],
    [
        p(14, 12, 13, 1, 5, 1, Fast),
        p(14, 14, 15, 1, 5, 0, Fast),
        p(14, 14, 15, 1, 4, 0, Fast),
        p(14, 14, 15, 2, 4, 0, DoubleFast),
        p(14, 14, 14, 4, 4, 2, Greedy),
        p(14, 14, 14, 3, 4, 4, Lazy),
        p(14, 14, 14, 4, 4, 8, Lazy2),
        p(14, 14, 14, 6, 4, 8, Lazy2),
        p(14, 14, 14, 8, 4, 8, Lazy2),
        p(14, 15, 14, 5, 4, 8, BtLazy2),
        p(14, 15, 14, 9, 4, 8, BtLazy2),
        p(14, 15, 14, 3, 4, 12, BtOpt),
        p(14, 15, 14, 4, 3, 24, BtOpt),
        p(14, 15, 14, 5, 3, 32, BtUltra),
        p(14, 15, 15, 6, 3, 64, BtUltra),
        p(14, 15, 15, 7, 3, 256, BtUltra),
        p(14, 15, 15, 5, 3, 48, BtUltra2),
        p(14, 15, 15, 6, 3, 128, BtUltra2),
        p(14, 15, 15, 7, 3, 256, BtUltra2),
        p(14, 15, 15, 8, 3, 256, BtUltra2),
        p(14, 15, 15, 8, 3, 512, BtUltra2),
        p(14, 15, 15, 9, 3, 512, BtUltra2),
        p(14, 15, 15, 10, 3, 999, BtUltra2),
    ],
];

impl Params {
    /// The parameters for `level` (0 means the default; below 1 is the fast strategy with
    /// growing acceleration), fitted to `src_size` when it's known.
    #[must_use]
    pub fn for_level(level: i32, src_size: Option<u64>) -> Self {
        let level = if level == 0 { DEFAULT_LEVEL } else { level };
        let tier = src_size.map_or(0, |s| {
            usize::from(s <= 256 << 10) + usize::from(s <= 128 << 10) + usize::from(s <= 16 << 10)
        });
        let table = &TABLES[tier];
        let mut params = if level < 1 {
            Params {
                target_length: level.clamp(MIN_LEVEL, -1).unsigned_abs(),
                ..table[0]
            }
        } else {
            table[level.min(MAX_LEVEL) as usize]
        };
        if let Some(size) = src_size {
            params.fit(size);
        }
        params
    }

    /// Shrinks the tables for an input of `size` bytes, as the reference does: no window
    /// larger than the input, the hash table at most one bit over the window, and the chain
    /// table no longer than the window.
    fn fit(&mut self, size: u64) {
        if size < 1 << 30 {
            let src_log = if size <= 1 {
                1
            } else {
                64 - (size - 1).leading_zeros()
            };
            if self.window_log > src_log {
                self.window_log = src_log;
            }
        }
        self.hash_log = self.hash_log.min(self.window_log + 1);
        let tree = u32::from(self.strategy >= BtLazy2);
        if self.chain_log - tree > self.window_log {
            self.chain_log = self.window_log + tree;
        }
        self.window_log = self.window_log.max(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_map_to_their_strategies() {
        assert_eq!(Params::for_level(1, None).strategy, Fast);
        assert_eq!(Params::for_level(2, None).strategy, Fast);
        assert_eq!(Params::for_level(0, None), Params::for_level(3, None));
        assert_eq!(Params::for_level(22, None).strategy, BtUltra2);
        assert_eq!(Params::for_level(99, None), Params::for_level(22, None));
        let fast = Params::for_level(-5, None);
        assert_eq!((fast.strategy, fast.target_length), (Fast, 5));
    }

    #[test]
    fn small_inputs_get_small_tables() {
        let p = Params::for_level(19, Some(1000));
        assert_eq!(p.window_log, 10);
        assert!(p.hash_log <= 11 && p.chain_log <= 11);
        assert_eq!(Params::for_level(19, Some(5 << 20)).window_log, 23);
    }

    #[test]
    fn the_size_tiers_match_the_reference() {
        // What `zstd --show-default-cparams` reports for a 2,000,000-byte file.
        let p = Params::for_level(1, Some(2_000_000));
        assert_eq!(
            (p.window_log, p.chain_log, p.hash_log, p.min_match),
            (19, 13, 14, 7)
        );
        let p = Params::for_level(19, Some(2_000_000));
        assert_eq!(
            (p.window_log, p.chain_log, p.hash_log, p.search_log),
            (21, 22, 22, 7)
        );
        assert_eq!(Params::for_level(3, Some(100_000)).strategy, DoubleFast);
        assert_eq!(Params::for_level(4, Some(200_000)).strategy, Greedy);
        assert_eq!(Params::for_level(11, Some(10_000)).strategy, BtOpt);
    }
}

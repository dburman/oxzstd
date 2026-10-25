//! Compression: the frame (RFC 8878 §3.1.1), blocks of up to 128 KiB, and a match finder
//! per strategy.

mod block;
mod dfast;
mod dubt;
mod fast;
mod lazy;
mod matcher;
mod opt;
pub(crate) mod params;
mod seq;

use block::Entropy;
use dfast::DoubleFast;
use dubt::Dubt;
use fast::Fast;
use lazy::{HashChain, Lazy, Row};
use matcher::MatchFinder;
use opt::Optimal;
use params::{Params, Strategy};
use seq::{Reps, SeqStore};

use crate::decode::{MAGIC, MAX_BLOCK};
use crate::xxhash::Xxh64;

fn finder(p: &Params) -> Box<dyn MatchFinder> {
    /// One finder per minimum match (4 up to `max`), so each hashes with a constant length.
    macro_rules! by_length {
        ($max:literal, $make:expr) => {
            match p.min_match.clamp(4, $max) {
                4 => $make(4),
                5 => $make(5),
                6 => $make(6),
                _ => $make(7),
            }
        };
    }
    macro_rules! make {
        ($finder:ident) => {
            |mls| -> Box<dyn MatchFinder> {
                match mls {
                    4 => Box::new($finder::<4>::new(p)),
                    5 => Box::new($finder::<5>::new(p)),
                    6 => Box::new($finder::<6>::new(p)),
                    _ => Box::new($finder::<7>::new(p)),
                }
            }
        };
        (lazy $search:ident) => {
            |mls| -> Box<dyn MatchFinder> {
                match mls {
                    4 => Box::new(Lazy::new(p, $search::<4>::new(p))),
                    5 => Box::new(Lazy::new(p, $search::<5>::new(p))),
                    _ => Box::new(Lazy::new(p, $search::<6>::new(p))),
                }
            }
        };
    }
    /// Rows come in three sizes, each its own finder too.
    macro_rules! rows {
        ($mls:literal) => {
            match Row::<$mls, 4>::log_for(p) {
                4 => Box::new(Lazy::new(p, Row::<$mls, 4>::new(p))),
                5 => Box::new(Lazy::new(p, Row::<$mls, 5>::new(p))),
                _ => Box::new(Lazy::new(p, Row::<$mls, 6>::new(p))),
            }
        };
    }
    match p.strategy {
        Strategy::Fast => by_length!(7, make!(Fast)),
        Strategy::DoubleFast => by_length!(7, make!(DoubleFast)),
        // The reference searches rows once the window is over 16 KiB, chains below. Measured
        // here, chains are a little faster for 8 candidates (searchLog 3) and rows much faster
        // for more, so rows take over from 16.
        Strategy::Greedy | Strategy::Lazy | Strategy::Lazy2
            if p.window_log > 14 && p.search_log >= 4 =>
        {
            by_length!(6, |mls| -> Box<dyn MatchFinder> {
                match mls {
                    4 => rows!(4),
                    5 => rows!(5),
                    _ => rows!(6),
                }
            })
        }
        Strategy::Greedy | Strategy::Lazy | Strategy::Lazy2 => by_length!(6, make!(lazy HashChain)),
        // lazy2 over a deferred binary tree.
        Strategy::BtLazy2 => by_length!(6, make!(lazy Dubt)),
        // The optimal parser, which goes down to 3-byte matches.
        Strategy::BtOpt | Strategy::BtUltra | Strategy::BtUltra2 => match p.min_match.clamp(3, 6) {
            3 => Box::new(Optimal::<3>::new(p)),
            4 => Box::new(Optimal::<4>::new(p)),
            5 => Box::new(Optimal::<5>::new(p)),
            _ => Box::new(Optimal::<6>::new(p)),
        },
    }
}

/// Writes the frame header: content size always, a window descriptor unless the whole
/// content fits in the window (then the frame is a single segment).
fn frame_header(size: Option<u64>, window_log: u32, checksum: bool, out: &mut Vec<u8>) {
    out.extend_from_slice(&MAGIC.to_le_bytes());
    let Some(size) = size else {
        // Size unknown (a stream): no content size, a window descriptor.
        out.push(u8::from(checksum) << 2);
        out.push(((window_log - 10) << 3) as u8);
        return;
    };
    let single = size <= 1u64 << window_log;
    let (fcs_flag, fcs_len) = if single && size < 256 {
        (0u8, 1)
    } else if (256..65_536 + 256).contains(&size) {
        (1, 2)
    } else if u32::try_from(size).is_ok() {
        (2, 4)
    } else {
        (3, 8)
    };
    out.push((fcs_flag << 6) | (u8::from(single) << 5) | (u8::from(checksum) << 2));
    if !single {
        out.push(((window_log - 10) << 3) as u8);
    }
    let value = if fcs_len == 2 { size - 256 } else { size };
    out.extend_from_slice(&value.to_le_bytes()[..fcs_len]);
}

fn block_header(last: bool, kind: u32, size: usize, out: &mut Vec<u8>) {
    let h = u32::from(last) | (kind << 1) | ((size as u32) << 3);
    out.extend_from_slice(&h.to_le_bytes()[..3]);
}

/// What compressing one frame leaves behind for the next: the match finder (with its
/// tables) for the parameters it was made for, and the sequence buffers.
#[derive(Default)]
pub(crate) struct Context {
    finder: Option<(Params, Box<dyn MatchFinder>)>,
    store: SeqStore,
}

impl Context {
    /// A finder for `params`: the one from last time, cleared, if it was made for the same.
    fn finder(&mut self, params: &Params) -> &mut Box<dyn MatchFinder> {
        if self.finder.as_ref().is_some_and(|(p, _)| p != params) {
            self.finder = None;
        }
        let fresh = self.finder.is_none();
        let (_, f) = self.finder.get_or_insert_with(|| (*params, finder(params)));
        if !fresh {
            f.reset();
        }
        f
    }
}

/// What carries from block to block within a frame.
struct Frame {
    entropy: Entropy,
    reps: Reps,
    /// The negative levels store literals raw, as the reference does: bigger, but faster to
    /// write and much faster to read.
    raw_literals: bool,
}

impl Frame {
    fn new(params: &Params) -> Self {
        Self {
            entropy: Entropy::default(),
            reps: Reps::default(),
            raw_literals: params.strategy == Strategy::Fast && params.target_length > 0,
        }
    }

    /// Writes `src[range]` as one block (everything before it is history), as the cheapest of
    /// compressed, RLE and raw.
    fn block(
        &mut self,
        src: &[u8],
        range: std::ops::Range<usize>,
        last: bool,
        ctx: &mut Context,
        params: &Params,
        out: &mut Vec<u8>,
    ) {
        let data = &src[range.clone()];
        if data.is_empty() {
            block_header(last, 0, 0, out);
            return;
        }
        let before = self.reps;
        let mut store = std::mem::take(&mut ctx.store);
        store.prepare(data.len());
        let Some((_, finder)) = ctx.finder.as_mut().filter(|(p, _)| p == params) else {
            // The frame's finder is always set up before its blocks; raw is always right.
            block_header(last, 0, data.len(), out);
            out.extend_from_slice(data);
            ctx.store = store;
            return;
        };
        finder.find(src, range, &mut self.reps, &mut store);
        if data.len() > 1 && data.iter().all(|&b| b == data[0]) {
            block_header(last, 1, data.len(), out);
            out.push(data[0]);
            // The decoder doesn't see this block's sequences.
            self.reps = before;
        } else {
            let (body, next) = block::body(&store, &self.entropy, self.raw_literals);
            if body.len() < data.len() {
                block_header(last, 2, body.len(), out);
                out.extend_from_slice(&body);
                self.entropy = next;
            } else {
                block_header(last, 0, data.len(), out);
                out.extend_from_slice(data);
                self.reps = before;
            }
        }
        ctx.store = store;
    }
}

/// Compresses `src` as one frame onto the end of `out`, reusing what `ctx` holds.
pub(crate) fn compress_into(
    src: &[u8],
    options: crate::Options,
    ctx: &mut Context,
    out: &mut Vec<u8>,
) {
    let params = Params::for_level(options.level, Some(src.len() as u64));
    out.reserve(src.len() / 2 + 64);
    frame_header(
        Some(src.len() as u64),
        params.window_log,
        options.checksum,
        out,
    );
    ctx.finder(&params);
    let mut frame = Frame::new(&params);
    if src.is_empty() {
        block_header(true, 0, 0, out);
    }
    let mut start = 0;
    while start < src.len() {
        let end = (start + MAX_BLOCK).min(src.len());
        frame.block(src, start..end, end == src.len(), ctx, &params, out);
        start = end;
    }
    if options.checksum {
        let mut hash = Xxh64::new(0);
        hash.update(src);
        out.extend_from_slice(&(hash.digest() as u32).to_le_bytes());
    }
}

/// Compresses `src` as one frame.
pub(crate) fn compress(src: &[u8], options: crate::Options) -> Vec<u8> {
    let mut out = Vec::new();
    compress_into(src, options, &mut Context::default(), &mut out);
    out
}

/// By default a frame ends, and the next begins, after this much input: what a stream holds
/// on to for matching back. (The window is smaller, but a frame's history isn't slid along.)
pub(crate) const FRAME_INPUT: usize = 8 << 20;

/// A frame written as its input arrives: whole blocks as they fill, and whatever there is
/// when flushed. The frame header gives no content size, and a long stream becomes several
/// frames (each up to `frame_input`), which together decode to the whole input.
pub(crate) struct Stream {
    options: crate::Options,
    size_hint: Option<u64>,
    /// The most input a frame takes before the next begins.
    frame_input: usize,
    ctx: Context,
    /// This frame's input so far; the history its blocks match against.
    input: Vec<u8>,
    /// How much of `input` has been written as blocks.
    done: usize,
    /// The frame under way, if it has begun, with its parameters and checksum.
    frame: Option<(Params, Frame, Xxh64)>,
}

impl Stream {
    pub(crate) fn new(options: crate::Options, size_hint: Option<u64>) -> Self {
        Self {
            options,
            size_hint,
            frame_input: FRAME_INPUT,
            ctx: Context::default(),
            input: Vec::new(),
            done: 0,
            frame: None,
        }
    }

    /// The next stream's expected size (or `None`), for its parameters.
    pub(crate) fn set_size_hint(&mut self, size_hint: Option<u64>) {
        self.size_hint = size_hint;
    }

    /// The most input a frame takes (at least a block).
    pub(crate) fn set_frame_input(&mut self, bytes: usize) {
        self.frame_input = bytes.max(MAX_BLOCK);
    }

    /// Drops the stream under way, if any, writing nothing more: ready for a new one.
    pub(crate) fn abandon(&mut self) {
        self.frame = None;
        self.input.clear();
        self.done = 0;
    }

    /// Starts a frame if none is under way: its header, and a finder for its parameters.
    fn begin(&mut self, out: &mut Vec<u8>) {
        if self.frame.is_some() {
            return;
        }
        let params = Params::for_level(self.options.level, self.size_hint);
        frame_header(None, params.window_log, self.options.checksum, out);
        self.ctx.finder(&params);
        self.frame = Some((params, Frame::new(&params), Xxh64::new(0)));
    }

    /// Writes `input[done..end]` as blocks of at most [`MAX_BLOCK`], the last flagged `last`.
    fn blocks(&mut self, end: usize, last: bool, out: &mut Vec<u8>) {
        let Some((params, frame, hash)) = self.frame.as_mut() else {
            return;
        };
        if self.done == end && last {
            block_header(true, 0, 0, out);
        }
        while self.done < end {
            let stop = (self.done + MAX_BLOCK).min(end);
            hash.update(&self.input[self.done..stop]);
            frame.block(
                &self.input,
                self.done..stop,
                last && stop == end,
                &mut self.ctx,
                params,
                out,
            );
            self.done = stop;
        }
    }

    /// Takes `data`, writing to `out` whatever whole blocks it completes.
    pub(crate) fn write(&mut self, mut data: &[u8], out: &mut Vec<u8>) {
        while !data.is_empty() {
            // A full frame ends only once there's more to go in the next, so a stream that
            // ends on a frame's boundary doesn't finish with an empty one.
            if self.input.len() >= self.frame_input {
                self.end_frame(out);
            }
            self.begin(out);
            let room = self.frame_input - self.input.len();
            let (now, rest) = data.split_at(data.len().min(room));
            self.input.extend_from_slice(now);
            data = rest;
            // Whole blocks, keeping the last one back until there's more or a flush.
            let full = self.done + (self.input.len() - self.done) / MAX_BLOCK * MAX_BLOCK;
            if full > self.done && full < self.input.len() {
                self.blocks(full, false, out);
            }
        }
    }

    /// Writes everything taken so far as blocks, so a decoder can produce all of it.
    pub(crate) fn flush(&mut self, out: &mut Vec<u8>) {
        let end = self.input.len();
        self.blocks(end, false, out);
    }

    /// Ends the frame under way: the rest of its input, the last block and the checksum.
    fn end_frame(&mut self, out: &mut Vec<u8>) {
        let end = self.input.len();
        self.blocks(end, true, out);
        if let Some((_, _, hash)) = self.frame.take()
            && self.options.checksum
        {
            out.extend_from_slice(&(hash.digest() as u32).to_le_bytes());
        }
        self.input.clear();
        self.done = 0;
    }

    /// Ends the stream (an empty one is still a frame), ready for the next.
    pub(crate) fn finish(&mut self, out: &mut Vec<u8>) {
        self.begin(out);
        self.end_frame(out);
    }
}

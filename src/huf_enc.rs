//! Huffman encoding of literals: code lengths limited to 11 bits, codes laid out the way the
//! decoder's table reads them, tree descriptions and the 1- or 4-stream bit streams
//! (RFC 8878 §4.2).

use std::cmp::Reverse;

use crate::bitwriter::BitWriter;
use crate::fse_enc;

/// The longest code the encoder makes (zstd's default too).
pub(crate) const MAX_BITS: u32 = 11;

/// A Huffman code: per symbol its length (0 for unused) and its code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Code {
    pub(crate) max_bits: u32,
    pub(crate) lengths: [u8; 256],
    pub(crate) codes: [u16; 256],
    /// The highest symbol with a code: its weight is implied, not written.
    pub(crate) last: usize,
}

/// Builds a code for `counts`, or `None` if fewer than two symbols occur.
pub(crate) fn build(counts: &[u32; 256]) -> Option<Code> {
    let used: Vec<usize> = (0..256).filter(|&s| counts[s] > 0).collect();
    if used.len() < 2 {
        return None;
    }
    let mut lengths = [0u8; 256];
    tree_lengths(counts, &used, &mut lengths);
    limit_lengths(counts, &used, &mut lengths, MAX_BITS);
    let max_bits = used
        .iter()
        .map(|&s| u32::from(lengths[s]))
        .max()
        .unwrap_or(1);
    let mut codes = [0u16; 256];
    // Weights fill the decoder's table from the lowest weight up, symbols in order.
    let mut per_weight = [0u32; MAX_BITS as usize + 2];
    for &s in &used {
        per_weight[(max_bits + 1 - u32::from(lengths[s])) as usize] += 1;
    }
    let mut start = [0u32; MAX_BITS as usize + 2];
    let mut next = 0u32;
    for w in 1..=max_bits as usize {
        start[w] = next;
        next += per_weight[w] << (w - 1);
    }
    for &s in &used {
        let w = (max_bits + 1 - u32::from(lengths[s])) as usize;
        codes[s] = (start[w] >> (w - 1)) as u16;
        start[w] += 1 << (w - 1);
    }
    Some(Code {
        max_bits,
        lengths,
        codes,
        last: *used.last().unwrap_or(&0),
    })
}

/// The lighter of the next leaf and the next internal node, taking it from its queue.
fn lightest(
    leaves: &[(u64, usize)],
    weight: &[u64],
    next_leaf: &mut usize,
    next_inner: &mut usize,
    made: usize,
) -> (u64, usize) {
    if *next_leaf < leaves.len()
        && (*next_inner >= made || leaves[*next_leaf].0 <= weight[*next_inner])
    {
        *next_leaf += 1;
        leaves[*next_leaf - 1]
    } else {
        *next_inner += 1;
        (weight[*next_inner - 1], *next_inner - 1)
    }
}

/// Unlimited Huffman code lengths.
fn tree_lengths(counts: &[u32; 256], used: &[usize], lengths: &mut [u8; 256]) {
    // Nodes 0..n are leaves; internal nodes follow. Two queues instead of a heap: the leaves
    // sorted by weight, and the internal nodes, which come out in order of weight as they're
    // made. Ties go to the lower node (leaves before internal nodes), so the result is the
    // same as always taking the two lightest, lowest-numbered nodes.
    let n = used.len();
    // Sorted as one key each, the count above the index (under 256): the same order as the
    // pairs, and faster to sort.
    let mut keys = [0u64; 256];
    for (i, &s) in used.iter().enumerate() {
        keys[i] = u64::from(counts[s]) << 8 | i as u64;
    }
    keys[..n].sort_unstable();
    let mut leaves = [(0u64, 0usize); 256];
    for (leaf, &k) in leaves.iter_mut().zip(&keys[..n]) {
        *leaf = (k >> 8, (k & 0xFF) as usize);
    }
    let leaves = &leaves[..n];
    let mut parent = [0usize; 512];
    let mut weight = [0u64; 512];
    let (mut next_leaf, mut next_inner, mut next) = (0, n, n);
    for _ in 1..n {
        let (wa, a) = lightest(leaves, &weight, &mut next_leaf, &mut next_inner, next);
        let (wb, b) = lightest(leaves, &weight, &mut next_leaf, &mut next_inner, next);
        parent[a] = next;
        parent[b] = next;
        weight[next] = wa + wb;
        next += 1;
    }
    // Depth of each node from the root (the last one made).
    let mut depth = [0u32; 512];
    for node in (0..next.saturating_sub(1)).rev() {
        depth[node] = depth[parent[node]] + 1;
    }
    for (i, &s) in used.iter().enumerate() {
        lengths[s] = depth[i].min(255) as u8;
    }
}

/// Limits lengths to `max` while keeping the code complete (the Kraft sum exactly one), as
/// the decoder requires.
fn limit_lengths(counts: &[u32; 256], used: &[usize], lengths: &mut [u8; 256], max: u32) {
    // A tree within the limit is complete already: nothing to do (most blocks).
    if used.iter().all(|&s| u32::from(lengths[s]) <= max) {
        return;
    }
    let unit = |len: u8| 1u64 << (max - u32::from(len));
    let target = 1u64 << max;
    for &s in used {
        lengths[s] = lengths[s].min(max as u8);
    }
    // Rarest first, so lengthening costs the least.
    let mut by_count: Vec<usize> = used.to_vec();
    // The keys are all different (the symbol breaks ties), so an unstable sort gives the same
    // order, faster.
    by_count.sort_unstable_by_key(|&s| (counts[s], Reverse(s)));
    let mut kraft: u64 = used.iter().map(|&s| unit(lengths[s])).sum();
    while kraft > target {
        // Lengthen the rarest symbol that can still grow.
        let Some(&s) = by_count.iter().find(|&&s| u32::from(lengths[s]) < max) else {
            break;
        };
        kraft -= unit(lengths[s]) / 2;
        lengths[s] += 1;
    }
    // Now fill any room by shortening the most frequent symbols that fit.
    while kraft < target {
        let room = target - kraft;
        let Some(&s) = by_count
            .iter()
            .rev()
            .find(|&&s| lengths[s] > 1 && unit(lengths[s]) <= room)
        else {
            break;
        };
        kraft += unit(lengths[s]);
        lengths[s] -= 1;
    }
}

impl Code {
    /// Bits to code `counts` with this code.
    pub(crate) fn cost(&self, counts: &[u32; 256]) -> u64 {
        (0..256)
            .map(|s| u64::from(counts[s]) * u64::from(self.lengths[s]))
            .sum()
    }

    /// Whether every symbol in `counts` has a code.
    pub(crate) fn covers(&self, counts: &[u32; 256]) -> bool {
        (0..256).all(|s| counts[s] == 0 || self.lengths[s] > 0)
    }

    /// Writes the tree description (§4.2.1); `None` if it can't be written (more than 128
    /// weights that FSE can't compress).
    pub(crate) fn write_description(&self, out: &mut Vec<u8>) -> Option<()> {
        let weights: Vec<u8> = (0..self.last)
            .map(|s| {
                if self.lengths[s] == 0 {
                    0
                } else {
                    (self.max_bits + 1 - u32::from(self.lengths[s])) as u8
                }
            })
            .collect();
        let fse = fse_weights(&weights);
        let direct_len = 1 + weights.len().div_ceil(2);
        match fse {
            Some(f) if weights.len() > 128 || f.len() < direct_len => {
                out.push((f.len() - 1) as u8);
                out.extend_from_slice(&f[1..]);
                Some(())
            }
            _ if weights.len() <= 128 => {
                out.push(127 + weights.len() as u8);
                for pair in weights.chunks(2) {
                    let hi = pair[0] << 4;
                    let lo = pair.get(1).copied().unwrap_or(0);
                    out.push(hi | lo);
                }
                Some(())
            }
            _ => None,
        }
    }

    /// Encodes `lits` as one stream.
    pub(crate) fn encode_stream(&self, lits: &[u8], out: &mut Vec<u8>) {
        let mut w = BitWriter::appending(std::mem::take(out), (lits.len() * 11).div_ceil(8) + 1);
        // Codes are at most 11 bits: four fit after a flush.
        let tail = lits.len() % 4;
        for &b in lits[lits.len() - tail..].iter().rev() {
            w.add(
                u64::from(self.codes[b as usize]),
                u32::from(self.lengths[b as usize]),
            );
        }
        // The buffer is sized for every code at 11 bits, so the loop flushes into it without
        // ever growing it.
        let (mut buf, mut bits) = w.split();
        let slice = buf.as_mut_slice();
        for four in lits[..lits.len() - tail].rchunks_exact(4) {
            bits.flush_into(slice);
            for &b in four.iter().rev() {
                bits.add_fast(
                    u64::from(self.codes[b as usize]),
                    u32::from(self.lengths[b as usize]),
                );
            }
        }
        *out = BitWriter::join(buf, bits).finish_backward();
    }

    /// Encodes `lits` as four streams with their jump table; `None` if a stream is too large
    /// for the jump table.
    pub(crate) fn encode_four(&self, lits: &[u8], out: &mut Vec<u8>) -> Option<()> {
        let per = lits.len().div_ceil(4);
        let jump = out.len();
        out.extend_from_slice(&[0; 6]);
        let mut sizes = [0usize; 3];
        for (i, segment) in lits.chunks(per.max(1)).enumerate() {
            let before = out.len();
            self.encode_stream(segment, out);
            if i < 3 {
                sizes[i] = out.len() - before;
            }
        }
        for (i, &size) in sizes.iter().enumerate() {
            let size = u16::try_from(size).ok()?;
            out[jump + 2 * i..jump + 2 * i + 2].copy_from_slice(&size.to_le_bytes());
        }
        Some(())
    }
}

/// The weights coded with FSE: the header byte (the size) placeholder, description and
/// stream. `None` when FSE can't code them (too few, or one distinct weight).
fn fse_weights(weights: &[u8]) -> Option<Vec<u8>> {
    if weights.len() <= 2 {
        return None;
    }
    let mut counts = [0u32; 13];
    for &w in weights {
        counts[w as usize] += 1;
    }
    let max = (0..13).rev().find(|&w| counts[w] > 0).unwrap_or(0);
    let log = fse_enc::optimal_log(6, weights.len(), max);
    let norm = fse_enc::normalize(&counts[..=max], weights.len(), log, false)?;
    let table = fse_enc::EncodeTable::new(&norm, log);
    let mut out = vec![0u8];
    fse_enc::write_counts(&norm, log, &mut out);
    out.extend_from_slice(&fse_enc::encode_interleaved(weights, &table)?);
    (out.len() - 1 <= 127).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::huffman;

    fn counts(data: &[u8]) -> [u32; 256] {
        let mut c = [0u32; 256];
        for &b in data {
            c[b as usize] += 1;
        }
        c
    }

    fn round_trip(data: &[u8]) {
        let c = counts(data);
        let code = build(&c).unwrap();
        assert!(code.max_bits <= MAX_BITS);
        let mut desc = Vec::new();
        code.write_description(&mut desc).unwrap();
        let (table, used) = huffman::read_tree(&desc).unwrap();
        assert_eq!(used, desc.len());
        assert_eq!(table.max_bits, code.max_bits);
        let mut one = Vec::new();
        code.encode_stream(data, &mut one);
        let mut out = Vec::new();
        table.decode(&one, data.len(), false, &mut out).unwrap();
        assert_eq!(&out[..data.len()], data);
        let mut four = Vec::new();
        if code.encode_four(data, &mut four).is_some() && data.len() >= 4 {
            table.decode(&four, data.len(), true, &mut out).unwrap();
            assert_eq!(&out[..data.len()], data);
        }
    }

    #[test]
    fn codes_decode_with_the_decoders_table() {
        round_trip(b"abracadabra, abracadabra, the quick brown fox");
        let skewed: Vec<u8> = (0..20_000u32)
            .map(|i| {
                let r = i.wrapping_mul(2_654_435_761) >> 16;
                if r % 100 < 70 { b'e' } else { (r % 200) as u8 }
            })
            .collect();
        round_trip(&skewed);
        // Every byte value, unevenly: 255 weights, which only FSE can describe.
        let wide: Vec<u8> = (0..30_000u32)
            .map(|i| {
                (((i.wrapping_mul(2_654_435_761) >> 20) % 256) as u8 / 2 * 2) | u8::from(i % 3 == 0)
            })
            .collect();
        round_trip(&wide);
    }

    #[test]
    fn a_flat_code_over_every_byte_has_no_description() {
        // 256 symbols of one length: nothing to gain, and no way to describe 255 equal
        // weights (FSE can't code one value; the direct form holds 128). Raw literals then.
        let all: Vec<u8> = (0..=255u8).cycle().take(5120).collect();
        let code = build(&counts(&all)).unwrap();
        assert!(code.write_description(&mut Vec::new()).is_none());
    }

    #[test]
    fn very_skewed_counts_are_limited_to_11_bits() {
        // Fibonacci-like counts make an unlimited tree far deeper than 11.
        let mut c = [0u32; 256];
        let (mut a, mut b) = (1u32, 1u32);
        for slot in c.iter_mut().take(30) {
            *slot = a;
            (a, b) = (b, a.saturating_add(b));
        }
        let code = build(&c).unwrap();
        assert!(code.max_bits <= MAX_BITS);
        let kraft: u64 = (0..256)
            .filter(|&s| code.lengths[s] > 0)
            .map(|s| 1u64 << (MAX_BITS - u32::from(code.lengths[s])))
            .sum();
        assert_eq!(
            kraft,
            1 << MAX_BITS.min(code.max_bits) << (MAX_BITS - code.max_bits)
        );
    }

    #[test]
    fn one_symbol_has_no_code() {
        assert!(build(&counts(b"aaaa")).is_none());
    }
}

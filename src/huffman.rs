//! Huffman coding of literals, RFC 8878 §4.2: tree descriptions as weights, and the
//! decoding tables built from them.

use std::cell::OnceCell;

use crate::Error;
use crate::bits::BackwardBits;
use crate::fse;

/// The longest code a description may give (the decoder's limit; the encoder stays at 11).
pub(crate) const MAX_BITS: u32 = 12;

/// One decoding entry: the symbol and its code length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) symbol: u8,
    pub(crate) bits: u8,
}

/// A decoding table indexed by the next `max_bits` bits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DecodeTable {
    pub(crate) max_bits: u32,
    pub(crate) entries: Vec<Entry>,
    /// The same bits decoded two symbols at a time, built when first wanted.
    pairs: OnceCell<Vec<Pair>>,
}

/// An entry of the two-symbol table: the symbols the next `max_bits` bits start with (one,
/// or two when the second code fits in them too), and the bits they take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Pair {
    symbols: [u8; 2],
    len: u8,
    bits: u8,
}

/// Reads a tree description (§4.2.1). Returns the table and the bytes the description took.
///
/// # Errors
///
/// [`Error::Corrupt`] for a truncated or invalid description.
pub(crate) fn read_tree(data: &[u8]) -> Result<(DecodeTable, usize), Error> {
    let Some(&header) = data.first() else {
        return Err(Error::Corrupt("missing Huffman tree description"));
    };
    let (weights, used) = if header < 128 {
        let size = header as usize;
        let body = data
            .get(1..1 + size)
            .ok_or(Error::Corrupt("Huffman weights are truncated"))?;
        (fse_weights(body)?, 1 + size)
    } else {
        let n = header as usize - 127;
        let bytes = n.div_ceil(2);
        let body = data
            .get(1..1 + bytes)
            .ok_or(Error::Corrupt("Huffman weights are truncated"))?;
        let mut weights = Vec::with_capacity(n);
        for i in 0..n {
            let byte = body[i / 2];
            weights.push(if i % 2 == 0 { byte >> 4 } else { byte & 0x0F });
        }
        (weights, 1 + bytes)
    };
    Ok((build_table(&weights)?, used))
}

/// Weights coded with FSE: an accuracy log of at most 6 and two interleaved states.
fn fse_weights(body: &[u8]) -> Result<Vec<u8>, Error> {
    let (log, counts, header) = fse::read_counts(body, 255, 6)?;
    let table = fse::build_decode_table(&counts, log)?;
    let stream = body
        .get(header..)
        .filter(|s| !s.is_empty())
        .ok_or(Error::Corrupt("Huffman weights have no stream"))?;
    let mut bits = BackwardBits::new(stream)?;
    let mut s1 = table.init(&mut bits);
    let mut s2 = table.init(&mut bits);
    let mut weights = Vec::with_capacity(256);
    loop {
        weights.push(table.symbol(s1));
        table.update(&mut s1, &mut bits);
        if bits.overflowed() {
            weights.push(table.symbol(s2));
            break;
        }
        weights.push(table.symbol(s2));
        table.update(&mut s2, &mut bits);
        if bits.overflowed() {
            weights.push(table.symbol(s1));
            break;
        }
        if weights.len() > 255 {
            return Err(Error::Corrupt("too many Huffman weights"));
        }
    }
    if weights.len() > 255 {
        return Err(Error::Corrupt("too many Huffman weights"));
    }
    Ok(weights)
}

/// Builds the decoding table from the explicit weights; the last symbol's weight is implied
/// by the total having to be a power of two.
fn build_table(explicit: &[u8]) -> Result<DecodeTable, Error> {
    let mut weights = explicit.to_vec();
    let mut sum: u32 = 0;
    for &w in &weights {
        if u32::from(w) > MAX_BITS {
            return Err(Error::Corrupt("Huffman weight too large"));
        }
        if w > 0 {
            sum += 1 << (w - 1);
        }
    }
    if sum == 0 {
        return Err(Error::Corrupt("Huffman weights are all zero"));
    }
    let max_bits = 32 - sum.leading_zeros();
    if max_bits > MAX_BITS {
        return Err(Error::Corrupt("Huffman codes too long"));
    }
    let rest = (1u32 << max_bits) - sum;
    if !rest.is_power_of_two() {
        return Err(Error::Corrupt("Huffman weights don't complete the tree"));
    }
    weights.push((rest.trailing_zeros() + 1) as u8);

    // Codes of each weight start where the lower weights' end (RFC 8878 §4.2.1.3).
    let mut start = [0u32; MAX_BITS as usize + 2];
    let mut next = 0u32;
    let mut count = [0u32; MAX_BITS as usize + 2];
    for &w in &weights {
        count[w as usize] += 1;
    }
    for w in 1..=MAX_BITS as usize + 1 {
        start[w] = next;
        next += count[w] << (w - 1);
    }
    let size = 1usize << max_bits;
    let mut entries = vec![Entry::default(); size];
    for (symbol, &w) in weights.iter().enumerate() {
        if w == 0 {
            continue;
        }
        let w = w as usize;
        let len = 1usize << (w - 1);
        let from = start[w] as usize;
        let bits = (max_bits + 1 - w as u32) as u8;
        for e in &mut entries[from..from + len] {
            *e = Entry {
                symbol: symbol as u8,
                bits,
            };
        }
        start[w] += len as u32;
    }
    Ok(DecodeTable {
        max_bits,
        entries,
        pairs: OnceCell::new(),
    })
}

impl DecodeTable {
    /// The next symbol; the caller has made sure its bits are in the reader's window.
    #[inline(always)]
    fn next(&self, bits: &mut BackwardBits<'_>) -> u8 {
        let e = self.entries[bits.peek_fast(self.max_bits) as usize];
        bits.consume(u32::from(e.bits));
        e.symbol
    }

    /// Symbols that fit in one refill of the reader's window.
    fn per_refill(&self) -> usize {
        (57 / self.max_bits) as usize
    }

    /// Decodes all of `out` from `bits`, refilling between batches.
    fn run(&self, bits: &mut BackwardBits<'_>, out: &mut [u8]) {
        let mut chunks = out.chunks_exact_mut(self.per_refill());
        for chunk in &mut chunks {
            bits.reload();
            for o in chunk {
                *o = self.next(bits);
            }
        }
        bits.reload();
        for o in chunks.into_remainder() {
            *o = self.next(bits);
        }
    }

    /// The two-symbol table: for each `max_bits` bits, the first symbol, and the second when
    /// its whole code is in the bits left.
    fn pairs(&self) -> &[Pair] {
        self.pairs.get_or_init(|| {
            let mask = (1usize << self.max_bits) - 1;
            (0..=mask)
                .map(|i| {
                    let first = self.entries[i];
                    let second = self.entries[(i << first.bits) & mask];
                    if u32::from(first.bits) + u32::from(second.bits) <= self.max_bits {
                        Pair {
                            symbols: [first.symbol, second.symbol],
                            len: 2,
                            bits: first.bits + second.bits,
                        }
                    } else {
                        Pair {
                            symbols: [first.symbol, 0],
                            len: 1,
                            bits: first.bits,
                        }
                    }
                })
                .collect()
        })
    }

    /// One or two symbols into `out` at `*op`, which has room for two; the caller has made
    /// sure their bits are in the reader's window.
    #[inline(always)]
    fn next_pair(
        &self,
        pairs: &[Pair],
        bits: &mut BackwardBits<'_>,
        out: &mut [u8],
        op: &mut usize,
    ) {
        let p = pairs[bits.peek_fast(self.max_bits) as usize];
        out[*op..*op + 2].copy_from_slice(&p.symbols);
        *op += usize::from(p.len);
        bits.consume(u32::from(p.bits));
    }

    /// Decodes all of `out` from `bits` two symbols at a time where it can: in batches while
    /// a whole batch fits, then one lookup at a time while two symbols do, then the last
    /// symbol alone, so the stream ends exactly.
    fn run_pairs(
        &self,
        pairs: &[Pair],
        bits: &mut BackwardBits<'_>,
        out: &mut [u8],
        mut op: usize,
    ) {
        let steps = self.per_refill();
        while op + 2 * steps <= out.len() {
            bits.reload();
            for _ in 0..steps {
                self.next_pair(pairs, bits, out, &mut op);
            }
        }
        while op + 2 <= out.len() {
            bits.reload();
            self.next_pair(pairs, bits, out, &mut op);
        }
        self.run(bits, &mut out[op..]);
    }

    /// Decodes `out.len()` symbols from one stream, which must then be used up exactly.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] if the stream is malformed or doesn't end where the symbols do.
    pub(crate) fn decode_stream(&self, stream: &[u8], out: &mut [u8]) -> Result<(), Error> {
        let mut bits = BackwardBits::new(stream)?;
        self.run(&mut bits, out);
        finished(&bits)
    }

    /// Decodes four streams into four outputs, interleaved while the shortest (the last)
    /// lasts, so their table lookups overlap.
    fn decode_four(&self, streams: [&[u8]; 4], outs: [&mut [u8]; 4]) -> Result<(), Error> {
        let [o1, o2, o3, o4] = outs;
        let mut b1 = BackwardBits::new(streams[0])?;
        let mut b2 = BackwardBits::new(streams[1])?;
        let mut b3 = BackwardBits::new(streams[2])?;
        let mut b4 = BackwardBits::new(streams[3])?;
        let per = self.per_refill();
        let done = o4.len() / per * per;
        for at in (0..done).step_by(per) {
            b1.reload();
            b2.reload();
            b3.reload();
            b4.reload();
            for i in at..at + per {
                o1[i] = self.next(&mut b1);
                o2[i] = self.next(&mut b2);
                o3[i] = self.next(&mut b3);
                o4[i] = self.next(&mut b4);
            }
        }
        self.run(&mut b1, &mut o1[done..]);
        self.run(&mut b2, &mut o2[done..]);
        self.run(&mut b3, &mut o3[done..]);
        self.run(&mut b4, &mut o4[done..]);
        finished(&b1)?;
        finished(&b2)?;
        finished(&b3)?;
        finished(&b4)
    }

    /// [`decode_four`](Self::decode_four) two symbols at a time.
    fn decode_four_pairs(&self, streams: [&[u8]; 4], outs: [&mut [u8]; 4]) -> Result<(), Error> {
        let pairs = self.pairs();
        let [o1, o2, o3, o4] = outs;
        let mut b1 = BackwardBits::new(streams[0])?;
        let mut b2 = BackwardBits::new(streams[1])?;
        let mut b3 = BackwardBits::new(streams[2])?;
        let mut b4 = BackwardBits::new(streams[3])?;
        let steps = self.per_refill();
        let (mut p1, mut p2, mut p3, mut p4) = (0, 0, 0, 0);
        // The last stream is the shortest, but the others can fill faster.
        while p1 + 2 * steps <= o1.len()
            && p2 + 2 * steps <= o2.len()
            && p3 + 2 * steps <= o3.len()
            && p4 + 2 * steps <= o4.len()
        {
            b1.reload();
            b2.reload();
            b3.reload();
            b4.reload();
            for _ in 0..steps {
                self.next_pair(pairs, &mut b1, o1, &mut p1);
                self.next_pair(pairs, &mut b2, o2, &mut p2);
                self.next_pair(pairs, &mut b3, o3, &mut p3);
                self.next_pair(pairs, &mut b4, o4, &mut p4);
            }
        }
        self.run_pairs(pairs, &mut b1, o1, p1);
        self.run_pairs(pairs, &mut b2, o2, p2);
        self.run_pairs(pairs, &mut b3, o3, p3);
        self.run_pairs(pairs, &mut b4, o4, p4);
        finished(&b1)?;
        finished(&b2)?;
        finished(&b3)?;
        finished(&b4)
    }

    /// Whether to decode two symbols at a time: worth building the table for with at least
    /// 16 KiB of literals, and when codes average 6 bits or less, so pairs are common.
    /// Measured: 10% faster on literal-heavy text, the same elsewhere.
    fn prefers_pairs(compressed: usize, regenerated: usize) -> bool {
        regenerated >= 16 << 10 && compressed * 8 <= regenerated * 6
    }

    /// Decodes `regenerated` literals from one stream, or from four with a jump table, into
    /// `out`, which ends with [`crate::decode::SLACK`] spare bytes.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] for a bad jump table or stream.
    pub(crate) fn decode(
        &self,
        data: &[u8],
        regenerated: usize,
        four: bool,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        out.clear();
        out.resize(regenerated + crate::decode::SLACK, 0);
        let lits = &mut out[..regenerated];
        let pairs = Self::prefers_pairs(data.len(), regenerated);
        if !four {
            if pairs {
                let mut bits = BackwardBits::new(data)?;
                self.run_pairs(self.pairs(), &mut bits, lits, 0);
                return finished(&bits);
            }
            return self.decode_stream(data, lits);
        }
        if data.len() < 6 {
            return Err(Error::Corrupt("missing Huffman jump table"));
        }
        let size = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]) as usize;
        let (s1, s2, s3) = (size(0), size(2), size(4));
        let total = data.len() - 6;
        if s1 + s2 + s3 > total {
            return Err(Error::Corrupt("Huffman jump table points past the data"));
        }
        let streams = &data[6..];
        let per = regenerated.div_ceil(4);
        if per * 3 > regenerated {
            return Err(Error::Corrupt("too few literals for four streams"));
        }
        let (o1, rest) = lits.split_at_mut(per);
        let (o2, rest) = rest.split_at_mut(per);
        let (o3, o4) = rest.split_at_mut(per);
        let streams = [
            &streams[..s1],
            &streams[s1..s1 + s2],
            &streams[s1 + s2..s1 + s2 + s3],
            &streams[s1 + s2 + s3..],
        ];
        if pairs {
            self.decode_four_pairs(streams, [o1, o2, o3, o4])
        } else {
            self.decode_four(streams, [o1, o2, o3, o4])
        }
    }
}

fn finished(bits: &BackwardBits<'_>) -> Result<(), Error> {
    if bits.finished() {
        Ok(())
    } else {
        Err(Error::Corrupt("Huffman stream length mismatch"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_implied_weight_completes_the_tree() {
        // Weights 1, 1 sum to 2; the total must be 4, so the implied last weight is 2.
        let t = build_table(&[1, 1]).unwrap();
        assert_eq!(t.max_bits, 2);
        let symbols: Vec<u8> = t.entries.iter().map(|e| e.symbol).collect();
        assert_eq!(symbols, [0, 1, 2, 2]);
        let bits: Vec<u8> = t.entries.iter().map(|e| e.bits).collect();
        assert_eq!(bits, [2, 2, 1, 1]);
    }

    #[test]
    fn bad_weights_are_refused() {
        assert!(build_table(&[0, 0]).is_err(), "all zero");
        assert!(build_table(&[13]).is_err(), "too large");
        // 2 + 2 = 4... + 1 = 5 leaves 3 to 8, not a power of two.
        assert!(build_table(&[2, 2, 1]).is_err());
    }

    #[test]
    fn direct_weights_are_read_as_nibbles() {
        // Header 127 + 2 symbols, weights 1 and 1 in one byte.
        let (t, used) = read_tree(&[129, 0x11]).unwrap();
        assert_eq!(used, 2);
        assert_eq!(t.max_bits, 2);
        assert!(read_tree(&[129]).is_err(), "truncated");
        assert!(read_tree(&[]).is_err());
    }

    /// A table for `data`'s symbols, as the decoder reads it, and the encoder's code.
    fn table_for(data: &[u8]) -> (DecodeTable, crate::huf_enc::Code) {
        let mut counts = [0u32; 256];
        for &b in data {
            counts[b as usize] += 1;
        }
        let code = crate::huf_enc::build(&counts).unwrap();
        let mut desc = Vec::new();
        code.write_description(&mut desc).unwrap();
        (read_tree(&desc).unwrap().0, code)
    }

    #[test]
    fn pairs_decode_the_same_as_single_symbols() {
        // Skewed symbols, so many codes are short and pair up, at every length near the ends
        // of the batches, where the last symbols take the single-symbol path.
        let mut rng = 99u64;
        let text: Vec<u8> = (0..5000)
            .map(|_| {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                b"eeeeeeeettttaaoinshrdlu"[(rng % 23) as usize]
            })
            .collect();
        let (table, code) = table_for(&text);
        for len in (1..200).chain([1000, 4999, 5000]) {
            let data = &text[..len];
            let mut one = Vec::new();
            code.encode_stream(data, &mut one);
            let mut out = vec![0; len];
            let mut bits = BackwardBits::new(&one).unwrap();
            table.run_pairs(table.pairs(), &mut bits, &mut out, 0);
            assert!(bits.finished(), "length {len}");
            assert_eq!(out, data, "length {len}");
            // Four streams need something left for the last: 3 * ceil(len / 4) < len.
            if 3 * len.div_ceil(4) < len {
                let mut four = Vec::new();
                if code.encode_four(data, &mut four).is_some() {
                    let mut out = Vec::new();
                    table.decode(&four, len, true, &mut out).unwrap();
                    let (s1, s2, s3) = (
                        usize::from(u16::from_le_bytes([four[0], four[1]])),
                        usize::from(u16::from_le_bytes([four[2], four[3]])),
                        usize::from(u16::from_le_bytes([four[4], four[5]])),
                    );
                    let st = &four[6..];
                    let per = len.div_ceil(4);
                    let mut paired = vec![0; len];
                    let (o1, rest) = paired.split_at_mut(per);
                    let (o2, rest) = rest.split_at_mut(per);
                    let (o3, o4) = rest.split_at_mut(per);
                    table
                        .decode_four_pairs(
                            [
                                &st[..s1],
                                &st[s1..s1 + s2],
                                &st[s1 + s2..s1 + s2 + s3],
                                &st[s1 + s2 + s3..],
                            ],
                            [o1, o2, o3, o4],
                        )
                        .unwrap();
                    assert_eq!(paired, data, "four streams, length {len}");
                    assert_eq!(out[..len], *data);
                }
            }
        }
    }

    #[test]
    fn a_stream_must_end_exactly() {
        let t = build_table(&[1, 1]).unwrap();
        // Symbols 2 (code 1), 0 (code 00): bits "1 00" then the end marker above them.
        let stream = [0b0000_1100];
        let mut out = [0u8; 2];
        t.decode_stream(&stream, &mut out).unwrap();
        assert_eq!(out, [2, 0]);
        let mut one = [0u8; 1];
        assert!(
            t.decode_stream(&stream, &mut one).is_err(),
            "bits left over"
        );
    }
}

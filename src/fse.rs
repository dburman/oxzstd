//! Finite State Entropy (tANS), RFC 8878 §4.1: table descriptions, decoding tables, and the
//! predefined distributions for literal lengths, match lengths and offsets.

use crate::Error;
use crate::bits::{BackwardBits, ForwardBits};

/// One decoding cell: the symbol it stands for, the bits to read for the next state, and the
/// base those bits are added to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) symbol: u8,
    pub(crate) bits: u8,
    pub(crate) base: u16,
}

/// A decoding table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DecodeTable {
    pub(crate) log: u32,
    pub(crate) cells: Vec<Cell>,
}

/// Reads a table description (§4.1.1): the accuracy log and the normalized counts. Returns
/// them and the bytes the description took.
///
/// # Errors
///
/// [`Error::Corrupt`] for an accuracy log over `max_log`, too many symbols, counts that don't
/// add up, or a description longer than `data`.
pub(crate) fn read_counts(
    data: &[u8],
    max_symbol: usize,
    max_log: u32,
) -> Result<(u32, Vec<i16>, usize), Error> {
    let mut bits = ForwardBits::new(data);
    let log = bits.peek(4) + 5;
    bits.consume(4);
    if log > max_log {
        return Err(Error::Corrupt("FSE accuracy log too large"));
    }
    let mut counts = Vec::with_capacity(max_symbol + 1);
    let mut remaining: i32 = (1 << log) + 1;
    let mut threshold: i32 = 1 << log;
    let mut nbits = log + 1;
    let mut previous_zero = false;
    while remaining > 1 {
        if previous_zero {
            // Repeat flags: 2 bits each, 3 meaning "three more zeros and another flag".
            loop {
                let flag = bits.peek(2);
                bits.consume(2);
                counts.extend(std::iter::repeat_n(0, flag as usize));
                if flag != 3 {
                    break;
                }
                if counts.len() > max_symbol + 1 {
                    return Err(Error::Corrupt("FSE description has too many symbols"));
                }
            }
            if bits.bits_consumed() > bits.len_bits() {
                return Err(Error::Corrupt("FSE description is truncated"));
            }
        }
        if counts.len() > max_symbol {
            return Err(Error::Corrupt("FSE description has too many symbols"));
        }
        let max = (2 * threshold - 1) - remaining;
        let low = bits.peek(nbits - 1) as i32;
        let value = if (low & (threshold - 1)) < max {
            bits.consume(nbits - 1);
            low & (threshold - 1)
        } else {
            let mut v = bits.peek(nbits) as i32 & (2 * threshold - 1);
            if v >= threshold {
                v -= max;
            }
            bits.consume(nbits);
            v
        };
        let count = value - 1;
        remaining -= count.abs();
        counts.push(count as i16);
        previous_zero = count == 0;
        while remaining < threshold && nbits > 1 {
            nbits -= 1;
            threshold >>= 1;
        }
        if bits.bits_consumed() > bits.len_bits() {
            return Err(Error::Corrupt("FSE description is truncated"));
        }
    }
    if remaining != 1 || counts.len() > max_symbol + 1 {
        return Err(Error::Corrupt("FSE counts don't add up"));
    }
    Ok((log, counts, bits.bytes_consumed()))
}

/// Builds a decoding table from normalized counts (−1 is "less than one": a single cell at
/// the top of the table).
///
/// # Errors
///
/// [`Error::Corrupt`] if the counts don't fill the table exactly.
pub(crate) fn build_decode_table(counts: &[i16], log: u32) -> Result<DecodeTable, Error> {
    let size = 1usize << log;
    let mut cells = vec![Cell::default(); size];
    let mut next = vec![0u32; counts.len()];
    let mut high = size;
    let mut total = 0usize;
    for (s, &c) in counts.iter().enumerate() {
        if c == -1 {
            if high == 0 {
                return Err(Error::Corrupt("FSE table overfull"));
            }
            high -= 1;
            cells[high].symbol = s as u8;
            next[s] = 1;
            total += 1;
        } else if c > 0 {
            next[s] = c as u32;
            total += c as usize;
        } else if c < -1 {
            return Err(Error::Corrupt("FSE count below -1"));
        }
    }
    if total != size {
        return Err(Error::Corrupt("FSE counts don't fill the table"));
    }
    let step = (size >> 1) + (size >> 3) + 3;
    let mask = size - 1;
    let mut position = 0usize;
    for (s, &c) in counts.iter().enumerate() {
        for _ in 0..c.max(0) {
            cells[position].symbol = s as u8;
            position = (position + step) & mask;
            while position >= high {
                position = (position + step) & mask;
            }
        }
    }
    if position != 0 {
        return Err(Error::Corrupt("FSE spread did not end where it began"));
    }
    for cell in &mut cells {
        let s = cell.symbol as usize;
        let state = next[s];
        next[s] += 1;
        let bits = log - state.ilog2();
        cell.bits = bits as u8;
        cell.base = ((state << bits) - size as u32) as u16;
    }
    Ok(DecodeTable { log, cells })
}

impl DecodeTable {
    /// Reads the initial state.
    #[inline]
    pub(crate) fn init(&self, bits: &mut BackwardBits<'_>) -> usize {
        bits.read(self.log) as usize
    }

    /// The symbol for `state`.
    #[inline(always)]
    pub(crate) fn symbol(&self, state: usize) -> u8 {
        self.cells[state].symbol
    }

    /// Moves `state` to the next one, reading its bits.
    #[inline]
    pub(crate) fn update(&self, state: &mut usize, bits: &mut BackwardBits<'_>) {
        let c = self.cells[*state];
        *state = c.base as usize + bits.read(u32::from(c.bits)) as usize;
    }
}

/// The predefined literal-length distribution (accuracy log 6).
pub(crate) const LL_DEFAULT: [i16; 36] = [
    4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1,
    -1, -1, -1, -1,
];
/// The predefined match-length distribution (accuracy log 6).
pub(crate) const ML_DEFAULT: [i16; 53] = [
    1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
];
/// The predefined offset distribution (accuracy log 5).
pub(crate) const OF_DEFAULT: [i16; 29] = [
    1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_tables_build() {
        for (counts, log) in [
            (&LL_DEFAULT[..], 6),
            (&ML_DEFAULT[..], 6),
            (&OF_DEFAULT[..], 5),
        ] {
            let t = build_decode_table(counts, log).unwrap();
            assert_eq!(t.cells.len(), 1 << log);
            // Every symbol with a count appears exactly that many times.
            for (s, &c) in counts.iter().enumerate() {
                let n = t
                    .cells
                    .iter()
                    .filter(|cell| cell.symbol as usize == s)
                    .count();
                assert_eq!(n, c.unsigned_abs() as usize, "symbol {s}");
            }
        }
    }

    #[test]
    fn the_first_ll_cells_match_the_rfc() {
        // RFC 8878 Appendix A: Literals Length Code decoding table, first rows.
        let t = build_decode_table(&LL_DEFAULT, 6).unwrap();
        let rows: Vec<(u8, u8, u16)> = t.cells[..4]
            .iter()
            .map(|c| (c.symbol, c.bits, c.base))
            .collect();
        assert_eq!(rows, [(0, 4, 0), (0, 4, 16), (1, 5, 32), (3, 5, 0)]);
        assert_eq!(
            (t.cells[63].symbol, t.cells[63].bits, t.cells[63].base),
            (32, 6, 0)
        );
        assert_eq!(
            (t.cells[60].symbol, t.cells[60].bits, t.cells[60].base),
            (35, 6, 0)
        );
    }

    #[test]
    fn counts_that_dont_fill_the_table_are_refused() {
        assert!(build_decode_table(&[1, 1], 2).is_err());
        assert!(build_decode_table(&[3, 2], 2).is_err());
        assert!(build_decode_table(&[-2, 5], 2).is_err());
    }

    #[test]
    fn descriptions_must_add_up() {
        // Accuracy log 5 (nibble 0), then nothing: the counts can't reach the total.
        assert!(read_counts(&[0x00], 10, 9).is_err());
        assert!(
            read_counts(&[0x0F, 0xFF], 10, 9).is_err(),
            "log 20 is over the max"
        );
    }
}

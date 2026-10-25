//! Bit streams. Zstandard writes table headers as little-endian bit streams read forwards,
//! and entropy-coded data (Huffman literals, FSE sequences) as streams written forwards and
//! read backwards, from the last byte, whose highest set bit marks where the data ends.

use crate::Error;

/// Loads up to 8 bytes at `pos` as a little-endian u64, zero-padded past the end.
#[inline(always)]
fn load_le(data: &[u8], pos: usize) -> u64 {
    match data.get(pos..pos + 8) {
        Some(chunk) => {
            let mut a = [0u8; 8];
            a.copy_from_slice(chunk);
            u64::from_le_bytes(a)
        }
        None => load_partial(data, pos),
    }
}

/// [`load_le`] near the end, kept out of line so the hot loops stay small.
fn load_partial(data: &[u8], pos: usize) -> u64 {
    let mut a = [0u8; 8];
    if pos < data.len() {
        let rest = &data[pos..];
        a[..rest.len()].copy_from_slice(rest);
    }
    u64::from_le_bytes(a)
}

/// Reads a little-endian bit stream forwards (FSE table descriptions).
#[derive(Debug)]
pub(crate) struct ForwardBits<'a> {
    data: &'a [u8],
    /// Bits consumed from the start.
    pos: usize,
}

impl<'a> ForwardBits<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The next `n` (at most 32) bits, without consuming them. Past the end reads as zeros.
    pub(crate) fn peek(&self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        let word = load_le(self.data, self.pos / 8) >> (self.pos % 8);
        (word & ((1u64 << n) - 1)) as u32
    }

    pub(crate) fn consume(&mut self, n: u32) {
        self.pos += n as usize;
    }

    /// Bits consumed so far.
    pub(crate) fn bits_consumed(&self) -> usize {
        self.pos
    }

    /// Whole bytes the stream has used, rounding a partial byte up.
    pub(crate) fn bytes_consumed(&self) -> usize {
        self.pos.div_ceil(8)
    }

    pub(crate) fn len_bits(&self) -> usize {
        self.data.len() * 8
    }
}

/// Reads a backward bit stream: the first bits read are the highest of the last byte, below
/// its end marker.
///
/// The reader holds 8 bytes of the stream in `container` and counts the bits `consumed` from
/// its top. [`read`](Self::read) refills when it needs to; the hot loops call
/// [`reload`](Self::reload) once and then make up to 57 bits of
/// [`read_fast`](Self::read_fast) reads.
#[derive(Debug)]
pub(crate) struct BackwardBits<'a> {
    data: &'a [u8],
    /// Where the 8 bytes in `container` start.
    ptr: usize,
    container: u64,
    /// Bits read from the top of `container`. Over 64 once a read runs past the start of
    /// the stream, which is an overflow.
    consumed: u32,
}

impl<'a> BackwardBits<'a> {
    /// A reader for `data`.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] if `data` is empty or its last byte is zero (no end marker).
    pub(crate) fn new(data: &'a [u8]) -> Result<Self, Error> {
        let Some(&last) = data.last() else {
            return Err(Error::Corrupt("empty bit stream"));
        };
        if last == 0 {
            return Err(Error::Corrupt("bit stream without an end marker"));
        }
        let marker = last.leading_zeros() + 1;
        // A stream shorter than 8 bytes sits in the low bytes, under bytes of nothing.
        let (ptr, missing) = match data.len().checked_sub(8) {
            Some(ptr) => (ptr, 0),
            None => (0, (8 - data.len()) as u32 * 8),
        };
        Ok(Self {
            data,
            ptr,
            container: load_le(data, ptr),
            consumed: missing + marker,
        })
    }

    /// Moves the window down over the bytes already read, as far as the stream goes. After
    /// it at least 57 bits can be read, unless the stream is nearly done.
    #[inline(always)]
    pub(crate) fn reload(&mut self) {
        let step = ((self.consumed / 8) as usize).min(self.ptr);
        self.ptr -= step;
        self.consumed -= step as u32 * 8;
        self.container = load_le(self.data, self.ptr);
    }

    /// The next `n` (at most 57) bits, with no refill: the caller has made sure they're in
    /// the window. Bits just before the start of the stream read as zeros; once reads have
    /// gone a whole window past it they're meaningless, but still `n` bits wide, and
    /// [`overflowed`](Self::overflowed) says so.
    #[inline(always)]
    pub(crate) fn read_fast(&mut self, n: u32) -> u64 {
        let v = self.peek_fast(n);
        self.consumed += n;
        v
    }

    /// [`read_fast`](Self::read_fast) without consuming the bits.
    #[inline(always)]
    pub(crate) fn peek_fast(&self, n: u32) -> u64 {
        // A wrapping shift is one instruction; the shift only reaches 64 past the start.
        (self.container.wrapping_shl(self.consumed) >> 1) >> (63 - n)
    }

    /// The next `n` (at most 56) bits as a number whose highest bit was read first. Bits
    /// before the start read as zeros.
    #[inline]
    pub(crate) fn read(&mut self, n: u32) -> u64 {
        if self.consumed + n > 64 {
            self.reload();
        }
        let top = self.container.checked_shl(self.consumed).unwrap_or(0);
        self.consumed += n;
        (top >> 1) >> (63 - n)
    }

    /// Consumes `n` bits after a [`peek_fast`](Self::peek_fast).
    #[inline]
    pub(crate) fn consume(&mut self, n: u32) {
        self.consumed += n;
    }

    /// Bits not read yet; negative once a read has gone past the start.
    pub(crate) fn remaining(&self) -> isize {
        (self.ptr * 8) as isize + 64 - self.consumed as isize
    }

    /// Whether a read went past the start of the stream.
    #[inline]
    pub(crate) fn overflowed(&self) -> bool {
        self.remaining() < 0
    }

    /// Whether every bit has been read, exactly.
    pub(crate) fn finished(&self) -> bool {
        self.remaining() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_bits_are_little_endian() {
        let data = [0b1010_1100, 0b0000_0011];
        let mut r = ForwardBits::new(&data);
        assert_eq!(r.peek(2), 0b00);
        r.consume(2);
        assert_eq!(r.peek(4), 0b1011);
        r.consume(4);
        assert_eq!(r.peek(4), 0b1110);
        r.consume(4);
        assert_eq!(r.bytes_consumed(), 2);
        assert_eq!(r.peek(8), 0, "past the end reads zeros");
    }

    #[test]
    fn backward_bits_start_below_the_end_marker() {
        // Last byte 0b0001_0110: marker at bit 4, data bits 0110 read high first.
        let data = [0xAB, 0b0001_0110];
        let mut r = BackwardBits::new(&data).unwrap();
        assert_eq!(r.remaining(), 12);
        assert_eq!(r.read(2), 0b01);
        assert_eq!(r.read(2), 0b10);
        assert_eq!(r.read(8), 0xAB);
        assert!(r.finished());
        assert_eq!(r.read(3), 0, "zeros past the start");
        assert!(r.overflowed());
    }

    #[test]
    fn a_read_straddling_the_start_pads_with_zeros() {
        let data = [0b0000_0011, 0b0000_0001];
        let mut r = BackwardBits::new(&data).unwrap();
        assert_eq!(r.remaining(), 8);
        assert_eq!(r.read(6), 0);
        assert_eq!(r.read(4), 0b1100, "two real bits, two zero bits");
        assert!(r.overflowed());
    }

    #[test]
    fn long_streams_read_the_same_through_both_paths() {
        let data: Vec<u8> = (0..100u32)
            .map(|i| (i * 37 + 11) as u8)
            .chain([0x5A])
            .collect();
        let widths = [3u32, 11, 1, 0, 17, 25, 9, 5, 56, 2, 13];
        let mut slow = BackwardBits::new(&data).unwrap();
        let mut fast = BackwardBits::new(&data).unwrap();
        let mut total = 0;
        let mut i = 0;
        while total < 800 {
            let n = widths[i % widths.len()];
            i += 1;
            fast.reload();
            assert_eq!(slow.read(n), fast.read_fast(n), "{n} bits after {total}");
            total += n as isize;
            assert_eq!(slow.remaining(), fast.remaining());
        }
        assert!(slow.overflowed() && fast.overflowed());
    }

    #[test]
    fn streams_without_a_marker_are_refused() {
        assert!(BackwardBits::new(&[]).is_err());
        assert!(BackwardBits::new(&[1, 0]).is_err());
    }
}

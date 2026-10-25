//! Writes bit streams: little-endian, low bits first. [`BitWriter::finish_backward`] adds the
//! end marker a backward stream needs (the decoder starts from it and reads towards the
//! start).

/// Accumulates bits into bytes.
///
/// The buffer is sized up front and written by index, so the hot loops make no calls: give
/// [`with_capacity`](Self::with_capacity) a bound on the output and it never grows.
#[derive(Debug, Default)]
pub(crate) struct BitWriter {
    out: Vec<u8>,
    bits: Bits,
}

/// The bits a [`BitWriter`] holds and where it has got to in its buffer, apart from the
/// buffer: a loop that keeps this in a local, and passes the buffer only to
/// [`flush`](Self::flush), keeps it in registers.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Bits {
    /// Bytes written; the buffer holds 8 more for the next flush.
    pos: usize,
    acc: u64,
    count: u32,
}

impl Bits {
    /// Adds `n` bits holding `value`, which fits them, without flushing: the caller flushes
    /// before 64 bits are held.
    #[inline(always)]
    pub(crate) fn add_fast(&mut self, value: u64, n: u32) {
        debug_assert!(value >> n == 0 && self.count + n <= 64);
        self.acc |= value.wrapping_shl(self.count);
        self.count += n;
    }

    /// Writes out the whole bytes held into `out`, leaving at most 7 bits.
    #[inline(always)]
    pub(crate) fn flush(&mut self, out: &mut Vec<u8>) {
        let bytes = self.acc.to_le_bytes();
        match out.get_mut(self.pos..self.pos + 8) {
            Some(dst) => dst.copy_from_slice(&bytes),
            None => grow_and_write(out, self.pos, bytes),
        }
        self.advance();
    }

    /// [`flush`](Self::flush) into a buffer the caller has sized for everything it will
    /// write (8 bytes past the last whole byte): no growing, so no call in the loop.
    #[inline(always)]
    pub(crate) fn flush_into(&mut self, out: &mut [u8]) {
        out[self.pos..self.pos + 8].copy_from_slice(&self.acc.to_le_bytes());
        self.advance();
    }

    /// Moves past the whole bytes just written.
    #[inline(always)]
    fn advance(&mut self) {
        let whole = self.count / 8;
        self.pos += whole as usize;
        self.acc = self.acc.checked_shr(whole * 8).unwrap_or(0);
        self.count -= whole * 8;
    }
}

impl BitWriter {
    pub(crate) fn with_capacity(bytes: usize) -> Self {
        Self {
            out: vec![0; bytes + 8],
            bits: Bits::default(),
        }
    }

    /// The buffer and the bits apart, for a loop that writes many (see [`Bits`]).
    pub(crate) fn split(self) -> (Vec<u8>, Bits) {
        (self.out, self.bits)
    }

    /// Puts a writer back together after [`split`](Self::split).
    pub(crate) fn join(out: Vec<u8>, bits: Bits) -> Self {
        Self { out, bits }
    }

    /// Adds the low `n` (at most 56) bits of `value`.
    #[inline]
    pub(crate) fn add(&mut self, value: u64, n: u32) {
        debug_assert!(n <= 56);
        if self.bits.count + n > 63 {
            self.flush();
        }
        self.add_fast(value & ((1u64 << n) - 1), n);
    }

    /// Adds `n` bits holding `value`, which fits them, without flushing: the caller flushes
    /// before 64 bits are held.
    #[inline(always)]
    pub(crate) fn add_fast(&mut self, value: u64, n: u32) {
        self.bits.add_fast(value, n);
    }

    /// Writes out the whole bytes held, leaving at most 7 bits.
    #[inline(always)]
    pub(crate) fn flush(&mut self) {
        self.bits.flush(&mut self.out);
    }

    /// The bytes, the last one padded with zeros.
    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.flush();
        // The bits left are in the byte at `pos`, which the flush wrote.
        let end = self.bits.pos + usize::from(self.bits.count > 0);
        self.out.truncate(end);
        self.out
    }

    /// The bytes of a backward stream: a 1 bit marks the end, then zero padding.
    pub(crate) fn finish_backward(mut self) -> Vec<u8> {
        self.add(1, 1);
        self.finish()
    }
}

/// Only when the capacity given was too small. Takes just the buffer, so the writer's other
/// fields can stay in registers in the loops that flush.
#[cold]
#[inline(never)]
fn grow_and_write(out: &mut Vec<u8>, pos: usize, bytes: [u8; 8]) {
    out.resize((out.len() * 2).max(pos + 8), 0);
    out[pos..pos + 8].copy_from_slice(&bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::BackwardBits;

    #[test]
    fn a_backward_stream_reads_back_in_reverse() {
        let mut w = BitWriter::with_capacity(8);
        let values = [
            (5u64, 3u32),
            (0, 1),
            (0xABCD, 16),
            (1, 1),
            (0x1F_FFFF, 21),
            (3, 2),
        ];
        for &(v, n) in &values {
            w.add(v, n);
        }
        let bytes = w.finish_backward();
        let mut r = BackwardBits::new(&bytes).unwrap();
        for &(v, n) in values.iter().rev() {
            assert_eq!(r.read(n), v);
        }
        assert!(r.finished());
    }

    #[test]
    fn a_small_capacity_grows() {
        let mut w = BitWriter::with_capacity(0);
        for i in 0..1000u64 {
            w.add(i & 0x7FF, 11);
        }
        let bytes = w.finish();
        assert_eq!(bytes.len(), (1000 * 11usize).div_ceil(8));
        let mut r = crate::bits::ForwardBits::new(&bytes);
        for i in 0..1000u64 {
            assert_eq!(u64::from(r.peek(11)), i & 0x7FF);
            r.consume(11);
        }
    }

    #[test]
    fn forward_bytes_are_little_endian() {
        let mut w = BitWriter::with_capacity(2);
        w.add(0b101, 3);
        w.add(0b11, 2);
        assert_eq!(w.finish(), [0b0001_1101]);
    }
}

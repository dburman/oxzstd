//! A report on what a frame is made of, for comparing encoders: per-block literal and
//! sequence sizes, and what the sequences use.

use std::fmt::Write as _;

use crate::Error;
use crate::decode::{State, decompress_frame_stats, frame_header, le, sequence_count};

/// Counts of what the sequences in a frame use.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SeqStats {
    /// Sequences with a new offset, then with offset values 1, 2 and 3.
    by_value: [u64; 4],
    literals: u64,
    matched: u64,
}

impl SeqStats {
    pub(crate) fn record(&mut self, offset_value: u64, lit_len: usize, match_len: usize) {
        let i = if offset_value > 3 {
            0
        } else {
            offset_value as usize
        };
        self.by_value[i] += 1;
        self.literals += lit_len as u64;
        self.matched += match_len as u64;
    }
}

/// Per-block statistics of the frames in `src`, one line per block (for comparing encoders).
///
/// # Errors
///
/// An [`Error`] for malformed input.
#[doc(hidden)]
pub fn inspect(src: &[u8]) -> Result<String, Error> {
    let mut report = String::new();
    let mut pos = 4;
    let header = frame_header(src.get(pos..).ok_or(Error::Truncated)?)?;
    let _ = writeln!(
        report,
        "frame: window {} content {:?} checksum {}",
        header.window_size, header.content_size, header.checksum
    );
    pos += header.header_len;
    let mut totals = [0usize; 4];
    loop {
        let h = src.get(pos..pos + 3).ok_or(Error::Truncated)?;
        let h = u32::from(h[0]) | (u32::from(h[1]) << 8) | (u32::from(h[2]) << 16);
        pos += 3;
        let (last, kind, size) = (h & 1 != 0, (h >> 1) & 3, (h >> 3) as usize);
        if kind == 2 {
            let data = src.get(pos..pos + size).ok_or(Error::Truncated)?;
            let (lkind, regen, comp, hlen) = literals_header(data);
            let lits_bytes = hlen + comp;
            let rest = &data[lits_bytes..];
            let (count, cpos) = sequence_count(rest)?;
            let modes = if count > 0 {
                rest.get(cpos).copied().unwrap_or(0)
            } else {
                0
            };
            let seq_bytes = rest.len();
            totals[0] += regen;
            totals[1] += lits_bytes;
            totals[2] += count;
            totals[3] += seq_bytes;
            let _ = writeln!(
                report,
                "block {size}: lits {} {regen}->{lits_bytes}, seqs {count} ({:02b} {:02b} {:02b}) {seq_bytes} bytes",
                ["raw", "rle", "huf", "treeless"][lkind as usize],
                modes >> 6,
                (modes >> 4) & 3,
                (modes >> 2) & 3,
            );
        } else {
            let _ = writeln!(
                report,
                "block {}: {}",
                size,
                ["raw", "rle", "compressed", "reserved"][kind as usize]
            );
        }
        pos += if kind == 1 { 1 } else { size };
        if last {
            break;
        }
    }
    let _ = writeln!(
        report,
        "total: literals {} -> {} bytes, {} sequences in {} bytes",
        totals[0], totals[1], totals[2], totals[3]
    );
    let mut st = SeqStats::default();
    let mut out = Vec::new();
    decompress_frame_stats(
        &src[4..],
        &mut out,
        usize::MAX,
        Some(&mut st),
        &mut State::new(),
    )?;
    let n = st.by_value.iter().sum::<u64>().max(1);
    let _ = writeln!(
        report,
        "sequences: new offset {}, value 1 {}, value 2 {}, value 3 {}; average literals {:.2}, match {:.2}",
        st.by_value[0],
        st.by_value[1],
        st.by_value[2],
        st.by_value[3],
        st.literals as f64 / n as f64,
        st.matched as f64 / n as f64,
    );
    Ok(report)
}

/// A literals section's type, regenerated and stored sizes, and header length.
fn literals_header(data: &[u8]) -> (u8, usize, usize, usize) {
    let b0 = data[0];
    let lkind = b0 & 3;
    let format = (b0 >> 2) & 3;
    let (regen, comp, hlen) = if lkind < 2 {
        let (s, l) = match format {
            0 | 2 => (usize::from(b0 >> 3), 1),
            1 => ((usize::from(b0) >> 4) | (usize::from(data[1]) << 4), 2),
            _ => (
                (usize::from(b0) >> 4) | (usize::from(data[1]) << 4) | (usize::from(data[2]) << 12),
                3,
            ),
        };
        (s, if lkind == 0 { s } else { 1 }, l)
    } else {
        let (l, bits) = match format {
            0 | 1 => (3, 10),
            2 => (4, 14),
            _ => (5, 18),
        };
        let v = le(&data[..l]);
        let mask = (1u64 << bits) - 1;
        (
            ((v >> 4) & mask) as usize,
            ((v >> (4 + bits)) & mask) as usize,
            l,
        )
    };
    (lkind, regen, comp, hlen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_counts_what_the_frame_holds() {
        let data = b"abcdefgh-abcdefgh-abcdefgh-0123456789-abcdefgh-".repeat(50);
        let report = inspect(&crate::compress(&data, 1)).unwrap();
        assert!(report.starts_with("frame: window"), "{report}");
        assert!(report.contains("block"), "{report}");
        let seqs = report
            .lines()
            .find(|l| l.starts_with("sequences:"))
            .unwrap();
        assert!(
            !seqs.contains("average literals 0.00, match 0.00"),
            "{report}"
        );
        assert!(inspect(b"\x28\xb5\x2f\xfd").is_err());
    }
}

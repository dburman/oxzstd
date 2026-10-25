//! XXH64, the checksum in a Zstandard frame (RFC 8878 §3.1.1: the low 32 bits of XXH64 of
//! the decompressed content, seed 0).

const P1: u64 = 0x9E37_79B1_85EB_CA87;
const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const P3: u64 = 0x1656_67B1_9E37_79F9;
const P4: u64 = 0x85EB_CA77_C2B2_AE63;
const P5: u64 = 0x27D4_EB2F_1656_67C5;

fn round(acc: u64, input: u64) -> u64 {
    acc.wrapping_add(input.wrapping_mul(P2))
        .rotate_left(31)
        .wrapping_mul(P1)
}

fn merge(acc: u64, value: u64) -> u64 {
    (acc ^ round(0, value)).wrapping_mul(P1).wrapping_add(P4)
}

fn read64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}

fn read32(b: &[u8]) -> u32 {
    let mut a = [0u8; 4];
    a.copy_from_slice(&b[..4]);
    u32::from_le_bytes(a)
}

/// A streaming XXH64 hasher.
#[derive(Clone, Debug)]
pub struct Xxh64 {
    seed: u64,
    v: [u64; 4],
    buffer: [u8; 32],
    buffered: usize,
    total: u64,
}

impl Xxh64 {
    /// A hasher with `seed` (Zstandard uses 0).
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            v: [
                seed.wrapping_add(P1).wrapping_add(P2),
                seed.wrapping_add(P2),
                seed,
                seed.wrapping_sub(P1),
            ],
            buffer: [0; 32],
            buffered: 0,
            total: 0,
        }
    }

    fn stripe(v: &mut [u64; 4], s: &[u8]) {
        v[0] = round(v[0], read64(&s[0..]));
        v[1] = round(v[1], read64(&s[8..]));
        v[2] = round(v[2], read64(&s[16..]));
        v[3] = round(v[3], read64(&s[24..]));
    }

    /// Adds `data`.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.buffered > 0 {
            let take = (32 - self.buffered).min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered < 32 {
                return;
            }
            let buffer = self.buffer;
            Self::stripe(&mut self.v, &buffer);
            self.buffered = 0;
        }
        let (stripes, rest) = data.as_chunks::<32>();
        for s in stripes {
            Self::stripe(&mut self.v, s);
        }
        self.buffer[..rest.len()].copy_from_slice(rest);
        self.buffered = rest.len();
    }

    /// The hash of everything added so far.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut h = if self.total >= 32 {
            let [v1, v2, v3, v4] = self.v;
            let mut h = v1
                .rotate_left(1)
                .wrapping_add(v2.rotate_left(7))
                .wrapping_add(v3.rotate_left(12))
                .wrapping_add(v4.rotate_left(18));
            h = merge(h, v1);
            h = merge(h, v2);
            h = merge(h, v3);
            merge(h, v4)
        } else {
            self.seed.wrapping_add(P5)
        };
        h = h.wrapping_add(self.total);
        let mut tail = &self.buffer[..self.buffered];
        while tail.len() >= 8 {
            h ^= round(0, read64(tail));
            h = h.rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
            tail = &tail[8..];
        }
        if tail.len() >= 4 {
            h ^= u64::from(read32(tail)).wrapping_mul(P1);
            h = h.rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
            tail = &tail[4..];
        }
        for &b in tail {
            h ^= u64::from(b).wrapping_mul(P5);
            h = h.rotate_left(11).wrapping_mul(P1);
        }
        h ^= h >> 33;
        h = h.wrapping_mul(P2);
        h ^= h >> 29;
        h = h.wrapping_mul(P3);
        h ^ (h >> 32)
    }
}

/// XXH64 of `data` with `seed`.
#[must_use]
pub fn xxh64(data: &[u8], seed: u64) -> u64 {
    let mut h = Xxh64::new(seed);
    h.update(data);
    h.digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        // Published XXH64 test values.
        assert_eq!(xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
        assert_eq!(xxh64(b"a", 0), 0xD24E_C4F1_A98C_6E5B);
        assert_eq!(xxh64(b"abc", 0), 0x44BC_2CF5_AD77_0999);
        assert_eq!(
            xxh64(b"Nobody inspects the spammish repetition", 0),
            0xFBCE_A83C_8A37_8BF1
        );
    }

    #[test]
    fn streaming_matches_one_shot_at_every_split() {
        let data: Vec<u8> = (0..200u32).map(|i| (i * 7 + 3) as u8).collect();
        let whole = xxh64(&data, 5);
        for split in 0..data.len() {
            let mut h = Xxh64::new(5);
            h.update(&data[..split]);
            h.update(&data[split..]);
            assert_eq!(h.digest(), whole, "split at {split}");
        }
        let mut bytes = Xxh64::new(5);
        for b in &data {
            bytes.update(std::slice::from_ref(b));
        }
        assert_eq!(bytes.digest(), whole);
    }
}

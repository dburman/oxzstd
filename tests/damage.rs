//! Damaged input never makes the decoder panic, loop or write out of bounds: it returns an
//! error, or (rarely, for damage the format can't see) some output. Without the checksum
//! that's all that can be promised; with it, damage to the content is reported.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a panic is a test failure"
)]

mod common;

use common::{Rng, html, text};

/// Frames with every kind of block and table: small and large inputs, fast and slow levels,
/// short and long matches, Huffman and raw literals.
fn frames() -> Vec<Vec<u8>> {
    let mut sources = vec![text(3_000, 11), html(40_000, 12), text(300_000, 13)];
    let mut zeros_then_text = vec![0; 5_000];
    zeros_then_text.extend(text(5_000, 14));
    sources.push(zeros_then_text);
    // Random letters, skewed: nearly all literals, with short codes, so the decoder takes
    // its two-symbols-at-a-time path.
    let mut rng = Rng::new(16);
    sources.push(
        (0..150_000)
            .map(|_| b"eeeeeeettttaaoinshrdlu"[rng.below(22) as usize])
            .collect(),
    );
    let mut out = Vec::new();
    for data in &sources {
        for level in [-5, 1, 3, 19] {
            out.push(oxzstd::compress(data, level));
        }
    }
    out
}

/// Decompresses `frame`, with a limit so damage that claims a huge size stays cheap, fresh
/// and with the decompressor every damaged frame goes through: they have to agree, or the
/// reused one kept something from an earlier frame.
fn try_decompress(frame: &[u8]) {
    thread_local! {
        static REUSED: std::cell::RefCell<oxzstd::Decompressor> =
            std::cell::RefCell::new(oxzstd::Decompressor::new());
    }
    let fresh = oxzstd::decompress_with_limit(frame, 1 << 22);
    let mut out = Vec::new();
    let reused = REUSED.with(|d| d.borrow_mut().decompress_into(frame, &mut out, 1 << 22));
    match (fresh, reused) {
        (Ok(a), Ok(())) => assert!(a == out, "reused decompressor gave other output"),
        (Err(a), Err(b)) => assert_eq!(a, b),
        (a, b) => panic!("fresh {:?}, reused {:?}", a.map(|v| v.len()), b),
    }
}

#[test]
fn flipped_bytes_are_survived() {
    let mut rng = Rng::new(77);
    for frame in frames() {
        for _ in 0..400 {
            let mut bad = frame.clone();
            for _ in 0..=rng.below(3) {
                let at = rng.below(bad.len() as u64) as usize;
                bad[at] ^= 1 << rng.below(8);
            }
            try_decompress(&bad);
        }
    }
}

#[test]
fn random_bytes_are_survived() {
    let mut rng = Rng::new(78);
    for frame in frames() {
        for _ in 0..200 {
            let mut bad = frame.clone();
            let at = 4 + rng.below(bad.len() as u64 - 4) as usize;
            let len = (1 + rng.below(16) as usize).min(bad.len() - at);
            for b in &mut bad[at..at + len] {
                *b = rng.next() as u8;
            }
            try_decompress(&bad);
        }
    }
}

#[test]
fn every_truncation_is_an_error() {
    for frame in frames() {
        let step = (frame.len() / 300).max(1);
        for cut in (0..frame.len()).step_by(step) {
            assert!(
                oxzstd::decompress(&frame[..cut]).is_err(),
                "cut at {cut} of {}",
                frame.len()
            );
        }
    }
}

#[test]
fn damaged_content_fails_the_checksum() {
    let data = text(50_000, 15);
    let frame = oxzstd::compress(&data, 3);
    let mut rng = Rng::new(79);
    for _ in 0..300 {
        let mut bad = frame.clone();
        let at = 6 + rng.below(bad.len() as u64 - 6) as usize;
        bad[at] ^= 1 << rng.below(8);
        if let Ok(out) = oxzstd::decompress(&bad) {
            assert!(
                out == data,
                "damage at {at} gave wrong output without an error"
            );
        }
    }
}

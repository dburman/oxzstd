//! Everything oxzstd compresses, at every level, decompresses to the input with oxzstd.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a panic is a test failure"
)]

mod common;

use common::corpus;

#[test]
fn every_level_round_trips() {
    for (name, data) in corpus() {
        // The negative levels skip ahead further at each step: deep ones test that the scan
        // never meets a table entry for a position it hasn't passed.
        for level in (-30..=22).chain([-100, -1000, i32::MIN]) {
            let frame = oxzstd::compress(&data, level);
            let back = oxzstd::decompress(&frame)
                .unwrap_or_else(|e| panic!("{name} at level {level}: {e}"));
            assert!(back == data, "{name} at level {level}");
        }
    }
}

#[test]
fn decompressing_into_a_buffer_appends_and_respects_the_limit() {
    let data = corpus().remove(4).1;
    let frame = oxzstd::compress(&data, 3);
    let mut out = b"kept".to_vec();
    oxzstd::decompress_into(&frame, &mut out, usize::MAX).unwrap();
    assert!(out[..4] == *b"kept" && out[4..] == data);
    // The limit counts only what's added, and a failure leaves the buffer as it was.
    let mut out = b"kept".to_vec();
    assert_eq!(
        oxzstd::decompress_into(&frame, &mut out, data.len() - 1),
        Err(oxzstd::Error::TooLarge)
    );
    assert_eq!(out, b"kept");
    oxzstd::decompress_into(&frame, &mut out, data.len()).unwrap();
    assert_eq!(out.len(), 4 + data.len());
}

/// Random data with a short copy now and then: the scans' steps grow over the random
/// stretches, then meet a match barely longer than the minimum. A finder that records a
/// position it hasn't reached yet would find itself there and emit offset 0.
#[test]
fn sparse_short_matches_round_trip() {
    let mut rng = common::Rng::new(31);
    let mut data = common::random(4_000, 32);
    while data.len() < 400_000 {
        let gap = 500 + rng.below(3_000) as usize;
        data.extend(common::random(gap, rng.next()));
        let len = 4 + rng.below(5) as usize;
        let from = rng.below((data.len() - len) as u64) as usize;
        data.extend_from_within(from..from + len);
    }
    for level in (-7..=22).filter(|l| *l != 0) {
        let frame = oxzstd::compress(&data, level);
        assert!(
            oxzstd::decompress(&frame).ok().as_ref() == Some(&data),
            "level {level}"
        );
    }
}

#[test]
fn frames_without_a_checksum_round_trip() {
    for (name, data) in corpus() {
        for level in [-5, -1, 1, 3, 9, 19] {
            let with = oxzstd::compress(&data, level);
            let without = oxzstd::compress_with(&data, oxzstd::Options::new(level).checksum(false));
            assert_eq!(without.len() + 4, with.len(), "{name} at level {level}");
            assert!(
                oxzstd::decompress(&without).ok().as_ref() == Some(&data),
                "{name} at level {level}"
            );
        }
    }
}

#[test]
fn a_reused_compressor_makes_the_same_frames() {
    // Inputs of different sizes (so the tables are sometimes the same, sometimes not), each
    // twice, at every kind of level: a compressor that forgot anything would differ.
    let mut inputs: Vec<Vec<u8>> = corpus().into_iter().map(|(_, d)| d).collect();
    inputs.push(common::html(4_000, 41));
    inputs.push(common::html(4_000, 42));
    inputs.push(common::text(16_000, 43));
    for level in [-5, 1, 3, 5, 7, 13, 16, 19, 22] {
        let options = oxzstd::Options::new(level);
        let mut c = oxzstd::Compressor::new(options);
        for data in inputs.iter().chain(inputs.iter().rev()) {
            assert!(
                c.compress(data) == oxzstd::compress_with(data, options),
                "level {level}, {} bytes",
                data.len()
            );
        }
    }
}

#[test]
fn a_reused_decompressor_decodes_every_frame() {
    let mut d = oxzstd::Decompressor::new();
    let mut out = Vec::new();
    for (name, data) in corpus() {
        for level in [-5, 1, 3, 9, 19] {
            let frame = oxzstd::compress(&data, level);
            out.clear();
            d.decompress_into(&frame, &mut out, usize::MAX).unwrap();
            assert!(out == data, "{name} at level {level}");
        }
    }
}

#[test]
fn the_compressors_can_move_between_threads() {
    // A decompressor is Send but not Sync: it caches a table in a cell, which is faster than
    // a lock, and it's only ever used through `&mut` anyway.
    fn send<T: Send>() {}
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<oxzstd::Compressor>();
    send_and_sync::<oxzstd::StreamCompressor>();
    send_and_sync::<oxzstd::Options>();
    send::<oxzstd::Decompressor>();
}

#[test]
fn the_compressors_are_small_to_move() {
    // Their state lives behind a pointer, so they're cheap to put in an enum or a pool.
    assert!(size_of::<oxzstd::StreamCompressor>() <= 16);
    assert!(size_of::<oxzstd::Compressor>() <= 256);
}

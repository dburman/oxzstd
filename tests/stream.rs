//! The stream compressor: whatever the chunks and flushes, the output decodes to the input,
//! with oxzstd and the reference, and each flush makes everything so far decodable.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a panic is a test failure"
)]

mod common;

use common::{Rng, corpus, html, text, zstd};
use oxzstd::{Options, StreamCompressor};

/// Streams `data` through `c` in random chunks with random flushes; after each flush, checks
/// that what's out so far decodes to what's gone in.
fn stream(c: &mut StreamCompressor, data: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let len = (1 + rng.below(70_000) as usize).min(data.len() - at);
        c.write(&data[at..at + len], &mut out);
        at += len;
        if rng.below(3) == 0 {
            c.flush(&mut out);
            let so_far = oxzstd::decompress_partial(&out).unwrap();
            assert!(so_far == data[..at], "flushed after {at} bytes");
        }
    }
    c.finish(&mut out);
    out
}

#[test]
fn streams_decode_to_their_input() {
    let mut rng = Rng::new(51);
    for (name, data) in corpus() {
        for level in [-5, 1, 3, 9, 19] {
            let mut c = StreamCompressor::new(Options::new(level), None);
            let frame = stream(&mut c, &data, &mut rng);
            assert!(
                oxzstd::decompress(&frame).unwrap() == data,
                "{name} at {level}"
            );
            assert!(
                zstd(&["-d"], &frame) == data,
                "{name} at {level}: the reference"
            );
        }
    }
}

#[test]
fn an_empty_stream_is_a_frame() {
    let mut c = StreamCompressor::new(Options::new(3), None);
    let mut out = Vec::new();
    c.finish(&mut out);
    assert_eq!(oxzstd::decompress(&out).unwrap(), b"");
    assert_eq!(zstd(&["-d"], &out), b"");
}

#[test]
fn a_long_stream_becomes_several_frames() {
    let data = text(20 << 20, 52);
    let mut c = StreamCompressor::new(Options::new(1), None);
    let mut out = Vec::new();
    for chunk in data.chunks(1 << 20) {
        c.write(chunk, &mut out);
    }
    c.finish(&mut out);
    let magic = [0x28, 0xB5, 0x2F, 0xFD];
    let frames = out.windows(4).filter(|w| *w == magic).count();
    assert!(frames >= 3, "{frames} frames");
    assert!(oxzstd::decompress(&out).unwrap() == data, "oxzstd");
    assert!(zstd(&["-d"], &out) == data, "the reference");
}

#[test]
fn a_reused_stream_compressor_starts_afresh() {
    let inputs = [html(40_000, 53), text(300_000, 54), html(3_000, 55)];
    for level in [1, 3, 19] {
        let mut reused = StreamCompressor::new(Options::new(level), None);
        for data in &inputs {
            let mut a = Vec::new();
            reused.write(data, &mut a);
            reused.finish(&mut a);
            let mut fresh = StreamCompressor::new(Options::new(level), None);
            let mut b = Vec::new();
            fresh.write(data, &mut b);
            fresh.finish(&mut b);
            assert_eq!(a, b, "level {level}, {} bytes", data.len());
        }
    }
}

#[test]
fn without_flushes_a_stream_is_about_as_small_as_one_frame() {
    let data = text(1 << 20, 56);
    for level in [1, 3, 9] {
        let mut c = StreamCompressor::new(Options::new(level), Some(data.len() as u64));
        let mut out = Vec::new();
        for chunk in data.chunks(10_000) {
            c.write(chunk, &mut out);
        }
        c.finish(&mut out);
        let whole = oxzstd::compress(&data, level).len();
        assert!(
            out.len() * 100 <= whole * 101,
            "level {level}: {} against {whole}",
            out.len()
        );
    }
}

#[test]
fn smaller_frames_and_a_reset_mid_stream() {
    let data = text(3 << 20, 57);
    let mut c = StreamCompressor::new(Options::new(3), None);
    c.set_frame_size(1 << 20);
    // A stream given up on part way: nothing of it carries into the next.
    let mut abandoned = Vec::new();
    c.write(&data[..500_000], &mut abandoned);
    c.reset();
    let mut out = Vec::new();
    for chunk in data.chunks(100_000) {
        c.write(chunk, &mut out);
    }
    c.finish(&mut out);
    let magic = [0x28, 0xB5, 0x2F, 0xFD];
    assert_eq!(
        out.windows(4).filter(|w| *w == magic).count(),
        3,
        "one frame per MiB"
    );
    assert!(oxzstd::decompress(&out).unwrap() == data, "oxzstd");
    assert!(zstd(&["-d"], &out) == data, "the reference");
}

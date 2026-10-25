//! Interoperability with the reference `zstd` command-line tool (an outside program: nothing
//! from it is linked). Everything `zstd` writes must decompress here, and everything oxzstd
//! writes must decompress with `zstd -d`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a panic is a test failure"
)]

mod common;

use common::{corpus, zstd};

#[test]
fn every_level_of_the_reference_decodes() {
    for (name, data) in corpus() {
        for level in (-5..=22).filter(|l| *l != 0) {
            let frame = zstd(&level_args(level), &data);
            let back = oxzstd::decompress(&frame)
                .unwrap_or_else(|e| panic!("{name} at level {level}: {e}"));
            assert!(back == data, "{name} at level {level}: wrong output");
        }
    }
}

#[test]
fn reference_options_decode() {
    let (_, text) = corpus().into_iter().find(|(n, _)| *n == "text-2m").unwrap();
    for args in [
        &["-3", "--no-check"][..],
        &["-19", "--no-check"],
        &["-1", "--no-content-size"],
        &["-9", "--long=24"],
        &["--ultra", "-22", "--long=27"],
        &["-3", "--single-thread", "-B65536"],
    ] {
        let frame = zstd(args, &text);
        assert!(oxzstd::decompress(&frame).unwrap() == text, "{args:?}");
    }
}

#[test]
fn concatenated_and_skippable_frames_decode() {
    let a = b"first frame ".repeat(1000);
    let b = b"second frame ".repeat(1000);
    let mut frames = zstd(&["-5"], &a);
    // A skippable frame between them: magic 0x184D2A53, size 5, five bytes.
    frames.extend_from_slice(&0x184D_2A53_u32.to_le_bytes());
    frames.extend_from_slice(&5u32.to_le_bytes());
    frames.extend_from_slice(b"skip!");
    frames.extend_from_slice(&zstd(&["-5"], &b));
    let mut want = a.clone();
    want.extend_from_slice(&b);
    assert_eq!(oxzstd::decompress(&frames).unwrap(), want);
}

#[test]
fn the_reference_decodes_what_oxzstd_writes() {
    for (name, data) in corpus() {
        for level in [-7, -1, 1, 2, 3, 9, 19] {
            let frame = oxzstd::compress(&data, level);
            let bare = oxzstd::compress_with(&data, oxzstd::Options::new(level).checksum(false));
            assert!(
                zstd(&["-d"], &bare) == data,
                "{name} at level {level}, no checksum"
            );
            let back = zstd(&["-d"], &frame);
            assert!(back == data, "{name} at level {level}");
        }
    }
}

#[test]
fn damage_is_detected() {
    let (_, text) = corpus()
        .into_iter()
        .find(|(n, _)| *n == "text-200k")
        .unwrap();
    let frame = zstd(&["-3"], &text);
    // A flipped bit in the content is caught (by the decoder or the checksum); a cut is too.
    for at in [frame.len() / 3, frame.len() / 2, frame.len() - 6] {
        let mut bad = frame.clone();
        bad[at] ^= 0x10;
        assert!(oxzstd::decompress(&bad).is_err(), "flip at {at}");
    }
    for cut in [5, frame.len() / 2, frame.len() - 1] {
        assert!(oxzstd::decompress(&frame[..cut]).is_err(), "cut at {cut}");
    }
}

fn level_args(level: i32) -> Vec<String> {
    let mut args = Vec::new();
    if level > 19 {
        args.push("--ultra".to_owned());
    }
    if level < 0 {
        args.push(format!("--fast={}", -level));
    } else {
        args.push(format!("-{level}"));
    }
    args
}

#[test]
fn every_level_is_as_small_as_the_reference() {
    // The strategies are the reference's own, so the output should be the same size give or
    // take a little: at most 1% larger at any level. The negative levels store literals raw,
    // as the reference does, so they match it too.
    for (name, data) in corpus() {
        if !["text-200k", "html-500k", "skewed-300k"].contains(&name) {
            continue;
        }
        for level in (-5..=22).filter(|l| *l != 0) {
            let ours = oxzstd::compress(&data, level).len();
            // Told the size, as it would be reading a file, so it picks the same tables.
            let mut args = level_args(level);
            args.push(format!("--stream-size={}", data.len()));
            let theirs = zstd(&args, &data).len();
            // Up to level 10 much smaller would mean different choices too (say, literals
            // compressed where the reference stores them raw); above, smaller is just better.
            let (over, under) = if level > 10 { (101, 200) } else { (101, 101) };
            assert!(
                ours * 100 <= theirs * over && ours * under >= theirs * 100,
                "{name} at level {level}: {ours} bytes, the reference {theirs}"
            );
        }
    }
}

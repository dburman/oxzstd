//! Shared test helpers: a deterministic corpus and a runner for the reference `zstd` tool.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "shared test code: a panic is a test failure, and each file uses a subset"
)]

use std::io::Write;
use std::process::{Command, Stdio};

/// A small deterministic generator (xorshift), so the corpus is the same everywhere.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "the",
    "of",
    "and",
    "to",
    "in",
    "is",
    "that",
    "for",
    "it",
    "as",
    "was",
    "with",
    "be",
    "by",
    "on",
    "not",
    "he",
    "this",
    "are",
    "or",
    "his",
    "from",
    "at",
    "which",
    "but",
    "have",
    "an",
    "had",
    "they",
    "you",
    "were",
    "their",
    "one",
    "all",
    "we",
    "can",
    "her",
    "has",
    "there",
    "been",
    "if",
    "more",
    "when",
    "will",
    "would",
    "who",
    "so",
    "no",
    "garden",
    "weather",
    "proxy",
    "compression",
    "window",
    "sequence",
    "literal",
    "offset",
    "frame",
    "block",
];

/// English-like text of `len` bytes.
pub(crate) fn text(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        let w = WORDS[rng.below(WORDS.len() as u64) as usize];
        out.extend_from_slice(w.as_bytes());
        out.push(if rng.below(12) == 0 { b'\n' } else { b' ' });
    }
    out.truncate(len);
    out
}

/// HTML-like markup with repeated structure and varying content.
pub(crate) fn html(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = b"<!doctype html><html><head><title>test</title></head><body>\n".to_vec();
    while out.len() < len {
        let id = rng.below(100_000);
        let words = text(20 + rng.below(80) as usize, rng.next());
        out.extend_from_slice(
            format!("<div class=\"item\" id=\"i{id}\"><a href=\"/items/{id}\">").as_bytes(),
        );
        out.extend_from_slice(&words);
        out.extend_from_slice(b"</a></div>\n");
    }
    out.truncate(len);
    out
}

/// Random bytes (incompressible).
pub(crate) fn random(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    (0..len).map(|_| rng.next() as u8).collect()
}

/// The test corpus: names and contents, from empty to several blocks.
pub(crate) fn corpus() -> Vec<(&'static str, Vec<u8>)> {
    let mut skewed = Rng::new(9);
    vec![
        ("empty", Vec::new()),
        ("one-byte", b"x".to_vec()),
        ("short-text", text(100, 1)),
        ("zeros-300k", vec![0; 300_000]),
        ("text-200k", text(200_000, 2)),
        ("text-2m", text(2_000_000, 3)),
        ("html-500k", html(500_000, 4)),
        ("random-100k", random(100_000, 5)),
        (
            "skewed-300k",
            (0..300_000)
                .map(|_| {
                    let r = skewed.below(100);
                    if r < 60 {
                        b'a'
                    } else if r < 90 {
                        b'b'
                    } else {
                        (r as u8) | 0x40
                    }
                })
                .collect(),
        ),
    ]
}

/// Runs the reference `zstd` with `args`, feeding `input` on stdin; returns stdout.
pub(crate) fn zstd<S: AsRef<std::ffi::OsStr>>(args: &[S], input: &[u8]) -> Vec<u8> {
    let mut child = Command::new("zstd")
        .args(args)
        .args(["-q", "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the reference `zstd` tool must be installed (brew install zstd / apk add zstd)");
    let mut stdin = child.stdin.take().unwrap();
    let data = input.to_vec();
    let writer = std::thread::spawn(move || {
        drop(stdin.write_all(&data));
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        out.status.success(),
        "zstd {:?} failed: {}",
        args.iter()
            .map(|a| a.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

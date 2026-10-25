//! Decompresses a file: `cargo run --release --example decompress -- IN OUT [TIMES]`. With
//! TIMES it decompresses that many times first, with one reused decompressor (for profiling).

#![allow(clippy::unwrap_used, reason = "a command-line helper")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).unwrap();
    let times: usize = args.get(3).map_or(1, |t| t.parse().unwrap());
    let mut decompressor = oxzstd::Decompressor::new();
    let mut out = Vec::new();
    for _ in 0..times {
        out.clear();
        decompressor
            .decompress_into(&data, &mut out, usize::MAX)
            .unwrap();
    }
    std::fs::write(&args[2], out).unwrap();
}

//! Compresses a file: `cargo run --release --example compress -- LEVEL IN OUT [TIMES]`. With
//! TIMES it compresses that many times first, with one reused compressor (for profiling).

#![allow(clippy::unwrap_used, reason = "a command-line helper")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let level: i32 = args[1].parse().unwrap();
    let data = std::fs::read(&args[2]).unwrap();
    let times: usize = args.get(4).map_or(1, |t| t.parse().unwrap());
    let mut compressor = oxzstd::Compressor::new(oxzstd::Options::new(level));
    let mut out = Vec::new();
    for _ in 0..times {
        out = compressor.compress(&data);
    }
    std::fs::write(&args[3], out).unwrap();
}

//! Ratio and speed against the reference `zstd` tool, level by level.
//!
//! `cargo bench --bench versus_zstd -- [levels] [names]`: levels like `1,3,19` or `1-9`
//! (default 1-19), names filter the corpus. Files in `bench-data/` are added to the corpus.
//! The reference is timed with its own in-memory benchmark (`zstd -b`), oxzstd with the best
//! of several runs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "a benchmark: its output is its report, and a panic is a broken benchmark"
)]

#[path = "../tests/common/mod.rs"]
mod common;

use std::process::Command;
use std::time::{Duration, Instant};

fn levels(arg: Option<&str>) -> Vec<i32> {
    let Some(arg) = arg else {
        return (1..=19).collect();
    };
    let mut out = Vec::new();
    for part in arg.split(',') {
        if let Some((a, b)) = part.split_once('-').filter(|(a, _)| !a.is_empty()) {
            let (a, b): (i32, i32) = (a.parse().unwrap(), b.parse().unwrap());
            out.extend(a..=b);
        } else {
            out.push(part.parse().unwrap());
        }
    }
    out
}

/// Best time of `f` over runs filling about `budget`.
fn best<T>(budget: Duration, mut f: impl FnMut() -> T) -> (Duration, T) {
    let start = Instant::now();
    let mut best = Duration::MAX;
    let mut last;
    loop {
        let t = Instant::now();
        last = f();
        best = best.min(t.elapsed());
        if start.elapsed() >= budget {
            break;
        }
    }
    (best, last)
}

/// The reference's size and speeds (MB/s) from `zstd -b`.
fn reference(path: &std::path::Path, level: i32) -> Option<(usize, f64, f64)> {
    let mut cmd = Command::new("zstd");
    if level > 19 {
        cmd.arg("--ultra");
    }
    if level < 0 {
        cmd.arg("-b").arg(format!("--fast={}", -level));
    } else {
        cmd.arg(format!("-b{level}"));
    }
    let out = cmd.arg("-i1").arg(path).output().ok()?;
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    // The last report line with both speeds: " 3#name : 2000000 -> 600000 (x3.333),  400.1
    // MB/s, 1500.2 MB/s". At slow levels a line with only the first can come after it.
    let line = text
        .rsplit(['\r', '\n'])
        .find(|l| l.contains("->") && l.matches("MB/s").count() >= 2)?;
    let after = line.split("->").nth(1)?;
    let size: usize = after.split_whitespace().next()?.parse().ok()?;
    let speeds: Vec<f64> = line
        .split("MB/s")
        .filter_map(|s| s.rsplit([',', ' ']).find(|t| !t.is_empty())?.parse().ok())
        .collect();
    Some((size, *speeds.first()?, *speeds.get(1)?))
}

fn main() {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let levels = levels(args.first().map(String::as_str));
    let filters: Vec<&String> = args.iter().skip(1).collect();
    let dir = std::env::temp_dir().join(format!("oxzstd-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut corpus: Vec<(String, Vec<u8>)> = common::corpus()
        .into_iter()
        .filter(|(_, d)| d.len() >= 100_000)
        .map(|(n, d)| (n.to_owned(), d))
        .collect();
    // Small inputs, the size of most web responses: here the cost of setting up a frame
    // (allocating and clearing tables) counts as much as compressing.
    for (name, len) in [
        ("html-4k", 4_000),
        ("html-16k", 16_000),
        ("html-64k", 64_000),
    ] {
        corpus.push((name.to_owned(), common::html(len, 21)));
    }
    if let Ok(entries) = std::fs::read_dir("bench-data") {
        for e in entries.flatten() {
            if let Ok(data) = std::fs::read(e.path()) {
                corpus.push((e.file_name().to_string_lossy().into_owned(), data));
            }
        }
    }
    println!(
        "{:<14} {:>5} {:>10} {:>10} {:>7} {:>9} {:>9} {:>9} {:>9}",
        "input", "level", "oxzstd", "zstd", "size", "ox comp", "zs comp", "ox dec", "zs dec"
    );
    for (name, data) in &corpus {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        let path = dir.join(name);
        std::fs::write(&path, data).unwrap();
        let mb = data.len() as f64 / 1e6;
        for &level in &levels {
            let budget = Duration::from_millis(if level >= 16 { 300 } else { 600 });
            // No checksum, like the reference's benchmark frames, and contexts and buffers
            // reused from run to run, as its benchmark does.
            let options = oxzstd::Options::new(level).checksum(false);
            let mut compressor = oxzstd::Compressor::new(options);
            let mut frame = Vec::new();
            let (ct, ()) = best(budget, || {
                frame.clear();
                compressor.compress_into(data, &mut frame);
            });
            let mut decompressor = oxzstd::Decompressor::new();
            let mut buf = Vec::new();
            let (dt, ()) = best(budget, || {
                buf.clear();
                decompressor
                    .decompress_into(&frame, &mut buf, usize::MAX)
                    .unwrap();
            });
            let back = buf;
            assert!(back == *data, "{name} at level {level} doesn't round-trip");
            let Some((zsize, zc, zd)) = reference(&path, level) else {
                println!("{name:<14} {level:>5}: the reference benchmark failed");
                continue;
            };
            println!(
                "{:<14} {:>5} {:>10} {:>10} {:>+6.2}% {:>7.0}MB {:>7.0}MB {:>7.0}MB {:>7.0}MB",
                name,
                level,
                frame.len(),
                zsize,
                (frame.len() as f64 / zsize as f64 - 1.0) * 100.0,
                mb / ct.as_secs_f64(),
                zc,
                mb / dt.as_secs_f64(),
                zd,
            );
        }
    }
    drop(std::fs::remove_dir_all(&dir));
}

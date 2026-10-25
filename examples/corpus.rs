//! Writes the test corpus to a directory: `cargo run --example corpus -- DIR`.

#![allow(clippy::unwrap_used, reason = "a command-line helper")]

#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    for (name, data) in common::corpus() {
        std::fs::write(dir.join(name), data).unwrap();
    }
}

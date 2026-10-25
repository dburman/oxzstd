//! Prints per-block statistics of a Zstandard frame: `cargo run --example inspect -- FILE`.

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "a command-line helper"
)]

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let data = std::fs::read(path).unwrap();
    print!("{}", oxzstd::inspect(&data).unwrap());
}

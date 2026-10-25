# oxzstd implementation plan

oxzstd is a Zstandard (RFC 8878) compressor and decompressor in safe Rust: `unsafe` is
forbidden, there are no dependencies, and no C is built or linked. The C `zstd` command-line
tool is used only from outside, as a reference: tests check that it decodes everything
oxzstd writes (and that oxzstd decodes everything it writes), and benchmarks compare ratio
and speed level by level.

Goal: every level from 1 to 22 (and the negative "fast" levels) with compression ratios
within a few tenths of a percent of C zstd's at the same level, and speed as close as safe
Rust allows. Beating C zstd is the stretch goal; where it isn't reached, the benchmarks say by
how much.

## Rules for every work order

1. **No `unsafe`, no dependencies, no C.** `Cargo.toml` forbids `unsafe_code`; `deny.toml`
   allows no crates; `scripts/check-policy.sh` checks both.
2. **Tests in the same change,** for the success path and every failure path. Malformed input
   must give an error, never a panic: `tests/damage.rs` enforces it.
3. **Interoperability:** anything oxzstd compresses must decompress with `zstd -d`, and
   anything `zstd` compresses (every level, with and without checksums, single and multiple
   frames, skippable frames) must decompress with oxzstd. `tests/interop.rs` runs both ways.
4. **Mutation check** on new logic: break it on purpose and see a test fail.
5. **Gate:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test`, `scripts/check-policy.sh`, `cargo deny check`.

## Milestones

| Milestone | Contents | Exit | Status |
|---|---|---|---|
| Z0 Skeleton | Crate, lints, CI, plan, policy | Gate passes on an empty library | Done |
| Z1 Decoder | XXH64; frame and block headers; raw, RLE and compressed blocks; Huffman and FSE decoding; sequence execution; checksums; skippable frames; streaming `Read` adapter | Decodes everything `zstd` 1.5.7 writes at every level, byte for byte; fuzzed | Done, except the streaming `Read` adapter (`decompress_partial` decodes a flushed prefix). Fuzzed in-process (`tests/damage.rs`); a `cargo fuzz` target needs a dependency approved |
| Z2 Entropy encoder | Bit writer; Huffman table building and weight compression; FSE normalization and encoding; literals and sequences sections; block format choice (raw/RLE/compressed); frame writer with checksum | Round-trips through `zstd -d`, with a placeholder match finder | Done |
| Z3 Fast levels | `fast` and `dfast` match finders, repeat offsets, levels -5..4 | Ratio within 0.5% of C zstd at levels 1-4 on the corpus | Done: within 0.5% at levels 1-4; the negative levels store literals raw, as the reference does |
| Z4 Lazy levels | Hash chains: `greedy`, `lazy`, `lazy2`; binary tree: `btlazy2`; levels 5-15 | Ratio within 0.5% at levels 5-15 | Done. Levels 5-12 within 0.5% (hash chains and the reference's rows). `btlazy2` is lazy2 over the Z5 binary tree, not the reference's deferred tree: levels 13-15 range from 2.4% smaller to 1.7% larger |
| Z5 Optimal levels | Price model, `btopt`, `btultra`, `btultra2`; levels 16-22 | Ratio within 0.5% at levels 16-22 | Done: within 0.1% at levels 16-22 |
| Z6 Speed | Profiling-driven work on the hot loops, without `unsafe`; streaming `Write` encoder with flush | Speed reported against C zstd per level; regressions gated | In progress: see `docs/benchmarks.md`. `StreamCompressor` streams with flushes (not as a `Write`); `Compressor` and `Decompressor` reuse their contexts |

## The corpus and the comparison

`benches/versus_zstd.rs` compresses a fixed corpus (generated text and HTML with a fixed
seed, plus any files in `bench-data/`, which is not committed) at every level with oxzstd and
with the `zstd` CLI, and prints ratio, compression speed and decompression speed for both.
The numbers go in `docs/benchmarks.md` with the machine and versions.

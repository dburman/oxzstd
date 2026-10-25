<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/logo-dark.png">
    <img src="docs/assets/logo-light.png" alt="oxzstd" width="220">
  </picture>
</h1>

**oxzstd is a Rust library that compresses and decompresses data in the Zstandard format**,
written entirely in safe Rust, with no dependencies and no C code.

[Zstandard](https://facebook.github.io/zstd/) (zstd, RFC 8878) is a widely used compression
format: it compresses about as well as gzip or better while running several times faster,
and it's used for `.zst` files, web traffic (`Content-Encoding: zstd`), package archives,
file systems, databases and backups. The usual way to use it from Rust is to bind the reference
C library. oxzstd is a complete implementation of its own instead:

- **Same format, both ways:** anything oxzstd compresses decompresses with the reference
  `zstd` tool and library, and anything they compress, at any level, decompresses with
  oxzstd. The tests check both directions against the real `zstd`.
- **Same compression:** it has every one of the reference's compression strategies, from the
  fastest to the optimal parser, and its output at each level is within half a percent of
  the reference's size (levels 13 to 15: within 2.5%, smaller or larger).
- **Close in speed:** compression runs at 70 to 100% of the reference's speed at most levels,
  decompression at 60 to 100%; see [`docs/benchmarks.md`](docs/benchmarks.md).
- **No `unsafe`:** the crate forbids it, and CI checks that nothing re-allows it. Malformed
  input gives an error, never a panic.
- **No dependencies and no C:** no C compiler or `cmake` in the build, nothing to
  cross-compile but Rust, and nothing else to audit.

**Status:** in development. The format support and compression are complete (except
dictionaries), but the API may still change before a first release.

## Use

oxzstd isn't on crates.io yet. Add it from Git:

```toml
[dependencies]
oxzstd = { git = "https://github.com/dburman/oxzstd" }
```

Compress and decompress in one call:

```rust
let data = b"hello hello hello hello";
let frame = oxzstd::compress(data, 3);
assert_eq!(oxzstd::decompress(&frame).unwrap(), data);
```

**Levels** run from 1 (fastest) to 22 (smallest), as in the reference; 3 is the default
(`0` also means 3), and negative levels trade more size for more speed.

**Untrusted input:** `decompress` has no limit on the output size, so a small malicious
frame could expand to gigabytes. Use `decompress_with_limit` or `decompress_into` with a
limit when the input comes from outside:

```rust
let frame = oxzstd::compress(b"hello", 3);
let data = oxzstd::decompress_with_limit(&frame, 16 << 20).unwrap(); // at most 16 MiB
assert_eq!(data, b"hello");
```

**Many small inputs** (web responses, messages): `Compressor` and `Decompressor` keep their
tables and buffers from one input to the next, as the reference's contexts do, which is much
faster than setting up each time.

```rust
let mut compressor = oxzstd::Compressor::new(oxzstd::Options::new(3).checksum(false));
let mut decompressor = oxzstd::Decompressor::new();
let mut out = Vec::new();
for page in [&b"<p>one</p>"[..], b"<p>two</p>"] {
    let frame = compressor.compress(page);
    out.clear();
    decompressor.decompress_into(&frame, &mut out, 1 << 20).unwrap();
    assert_eq!(out, page);
}
```

**Streams:** `StreamCompressor` takes input as it arrives and can flush at any point, so the
other end can decompress everything sent so far (`decompress_partial` decodes such a prefix).

```rust
let mut stream = oxzstd::StreamCompressor::new(oxzstd::Options::new(3), None);
let mut out = Vec::new();
stream.write(b"hello ", &mut out);
stream.flush(&mut out); // `out` now decompresses to "hello "
stream.write(b"world", &mut out);
stream.finish(&mut out);
assert_eq!(oxzstd::decompress(&out).unwrap(), b"hello world");
```

Frames carry a checksum of their content by default, as the reference's do;
`Options::checksum(false)` leaves it out for transports that check the data already.

**Not supported yet:** dictionaries (frames that need one are refused with
`Error::Unsupported`), `std::io::Read` and `Write` adapters, and multithreaded compression.
The [plan](docs/plan/README.md) has the details.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release          # needs the reference `zstd` tool installed
scripts/check-policy.sh
cargo deny check
```

## License

MIT. See [LICENSE](LICENSE).

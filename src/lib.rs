//! Zstandard (RFC 8878) compression and decompression in safe Rust.
//!
//! No `unsafe`, no dependencies and no C. Frames written by this crate decompress with the
//! reference `zstd` tool, and frames written by it decompress here.
//!
//! ```
//! let data = b"hello hello hello hello";
//! let frame = oxzstd::compress(data, 3);
//! assert_eq!(oxzstd::decompress(&frame).unwrap(), data);
//! ```

#![forbid(unsafe_code)]

mod bits;
mod bitwriter;
mod decode;
mod encode;
mod fse;
mod fse_enc;
mod huf_enc;
mod huffman;
mod inspect;
mod xxhash;

pub use decode::{decompress_into, decompress_partial, decompress_with_limit};
pub use inspect::inspect;
pub use xxhash::{Xxh64, xxh64};

/// Why data couldn't be decompressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The input ended in the middle of a frame.
    Truncated,
    /// The input isn't valid Zstandard; the text says where it went wrong.
    Corrupt(&'static str),
    /// The frame's checksum doesn't match its content.
    Checksum,
    /// A valid frame that uses something this crate doesn't support.
    Unsupported(&'static str),
    /// The output would exceed the limit given.
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated Zstandard data"),
            Self::Corrupt(what) => write!(f, "corrupt Zstandard data: {what}"),
            Self::Checksum => f.write_str("Zstandard checksum mismatch"),
            Self::Unsupported(what) => write!(f, "unsupported Zstandard feature: {what}"),
            Self::TooLarge => f.write_str("decompressed data exceeds the limit"),
        }
    }
}

impl std::error::Error for Error {}

/// Decompresses every frame in `src`, with no limit on the output size.
///
/// # Errors
///
/// An [`Error`] for malformed, truncated or unsupported input.
pub fn decompress(src: &[u8]) -> Result<Vec<u8>, Error> {
    decompress_with_limit(src, usize::MAX)
}

/// Compresses `src` at `level` as one frame with a checksum. Levels run from 1 (fastest) to
/// 22 (smallest); 0 means the default (3), and negative levels are faster still.
#[must_use]
pub fn compress(src: &[u8], level: i32) -> Vec<u8> {
    encode::compress(src, Options::new(level))
}

/// Compresses `src` as one frame, as `options` say.
///
/// ```
/// let data = b"hello hello hello hello";
/// // No checksum: a little smaller and faster, for transports that check the data already.
/// let frame = oxzstd::compress_with(data, oxzstd::Options::new(3).checksum(false));
/// assert_eq!(oxzstd::decompress(&frame).unwrap(), data);
/// ```
#[must_use]
pub fn compress_with(src: &[u8], options: Options) -> Vec<u8> {
    encode::compress(src, options)
}

/// Compresses one input after another, keeping its tables and buffers from one to the next.
/// For many small inputs (web responses, say) that saves allocating and setting up each time;
/// the frames are the same as [`compress_with`] makes.
///
/// ```
/// let mut c = oxzstd::Compressor::new(oxzstd::Options::new(3));
/// for page in [&b"<p>one</p>"[..], b"<p>two</p>"] {
///     let frame = c.compress(page);
///     assert_eq!(oxzstd::decompress(&frame).unwrap(), page);
/// }
/// ```
pub struct Compressor {
    options: Options,
    context: encode::Context,
}

impl Compressor {
    /// A compressor that compresses as `options` say.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            options,
            context: encode::Context::default(),
        }
    }

    /// Compresses `src` as one frame.
    #[must_use]
    pub fn compress(&mut self, src: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        self.compress_into(src, &mut out);
        out
    }

    /// Compresses `src` as one frame onto the end of `out`.
    pub fn compress_into(&mut self, src: &[u8], out: &mut Vec<u8>) {
        encode::compress_into(src, self.options, &mut self.context, out);
    }

    /// The options it compresses with.
    #[must_use]
    pub fn options(&self) -> Options {
        self.options
    }
}

impl std::fmt::Debug for Compressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compressor")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

/// Compresses a stream as it arrives, for when the input isn't all there at once (a response
/// being proxied, say). [`write`](Self::write) takes input and writes out whole blocks as they
/// fill; [`flush`](Self::flush) writes out everything so far, so the other end can decompress
/// all of it; [`finish`](Self::finish) ends the stream, and the compressor is ready for the
/// next one, with its tables.
///
/// The frame header gives no content size, and a stream of more than 8 MiB (or what
/// [`set_frame_size`](Self::set_frame_size) says) becomes several frames, which decode to the
/// whole of it.
///
/// ```
/// let mut c = oxzstd::StreamCompressor::new(oxzstd::Options::new(3), None);
/// let mut out = Vec::new();
/// c.write(b"hello ", &mut out);
/// c.flush(&mut out);
/// assert_eq!(oxzstd::decompress_partial(&out).unwrap(), b"hello ");
/// c.write(b"world", &mut out);
/// c.finish(&mut out);
/// assert_eq!(oxzstd::decompress(&out).unwrap(), b"hello world");
/// ```
pub struct StreamCompressor {
    /// Boxed: the stream's state (its entropy tables among it) is over a kilobyte, and a
    /// compressor gets moved around (into a pool, an enum of encoders).
    stream: Box<encode::Stream>,
}

impl StreamCompressor {
    /// A stream compressor; `size_hint` is the stream's expected size, if known, to fit the
    /// tables (and the window the decoder needs) to it.
    #[must_use]
    pub fn new(options: Options, size_hint: Option<u64>) -> Self {
        Self {
            stream: Box::new(encode::Stream::new(options, size_hint)),
        }
    }

    /// The next stream's expected size, if known (after [`finish`](Self::finish)).
    pub fn set_size_hint(&mut self, size_hint: Option<u64>) {
        self.stream.set_size_hint(size_hint);
    }

    /// The most input one frame takes before the next begins: what the compressor holds on
    /// to for matching back (8 MiB by default; at least 128 KiB). Smaller saves memory per
    /// stream, at a little compression at each frame's start.
    pub fn set_frame_size(&mut self, bytes: usize) {
        self.stream.set_frame_input(bytes);
    }

    /// Drops the stream under way, if any, without ending it: ready for a new one. For a
    /// stream given up on (its reader went away) before the compressor is used again.
    pub fn reset(&mut self) {
        self.stream.abandon();
    }

    /// Takes `data`, appending to `out` whatever it completes.
    pub fn write(&mut self, data: &[u8], out: &mut Vec<u8>) {
        self.stream.write(data, out);
    }

    /// Appends to `out` everything taken so far, so a decoder can produce all of it.
    pub fn flush(&mut self, out: &mut Vec<u8>) {
        self.stream.flush(out);
    }

    /// Ends the stream, appending the rest to `out`. The compressor is then ready for a new
    /// stream.
    pub fn finish(&mut self, out: &mut Vec<u8>) {
        self.stream.finish(out);
    }
}

impl std::fmt::Debug for StreamCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamCompressor").finish_non_exhaustive()
    }
}

/// Decompresses one input after another, keeping its buffers and tables from one to the
/// next. The output is the same as [`decompress_into`]'s.
///
/// ```
/// let frame = oxzstd::compress(b"hello hello hello", 3);
/// let mut d = oxzstd::Decompressor::new();
/// let mut out = Vec::new();
/// for _ in 0..3 {
///     out.clear();
///     d.decompress_into(&frame, &mut out, 1 << 20).unwrap();
///     assert_eq!(out, b"hello hello hello");
/// }
/// ```
#[derive(Debug)]
pub struct Decompressor {
    state: decode::State,
}

impl Decompressor {
    /// A decompressor with nothing allocated yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: decode::State::new(),
        }
    }

    /// Decompresses every frame in `src` onto the end of `out`, adding at most `limit` bytes.
    ///
    /// # Errors
    ///
    /// Any [`Error`] for malformed, truncated or unsupported input, or output over `limit`;
    /// `out` is then as it was.
    pub fn decompress_into(
        &mut self,
        src: &[u8],
        out: &mut Vec<u8>,
        limit: usize,
    ) -> Result<(), Error> {
        decode::decompress_into_with(src, out, limit, &mut self.state)
    }
}

impl Default for Decompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// How to compress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    /// The level, as for [`compress`].
    pub level: i32,
    /// Whether the frame ends with a checksum of the content (on by default). Decompression
    /// verifies it, which costs about 5% of the decoding time.
    pub checksum: bool,
}

impl Options {
    /// Level `level`, with a checksum.
    #[must_use]
    pub const fn new(level: i32) -> Self {
        Self {
            level,
            checksum: true,
        }
    }

    /// With or without a checksum.
    #[must_use]
    pub const fn checksum(self, checksum: bool) -> Self {
        Self { checksum, ..self }
    }
}

impl Default for Options {
    /// The default level (3), with a checksum.
    fn default() -> Self {
        Self::new(0)
    }
}

pub use encode::params::{Params, Strategy};

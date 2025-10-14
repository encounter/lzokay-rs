#![cfg_attr(not(feature = "std"), no_std)]
//! # LZ👌-rs
//!
//! Pure-Rust port of [LZ👌](https://github.com/jackoalan/lzokay), a minimal, MIT-licensed
//! implementation of the [LZO compression format](http://www.oberhumer.com/opensource/lzo/).
//!
//! See the original [README](https://github.com/jackoalan/lzokay/blob/master/README.md) for more information.
//!
//! ### Features
//!
//! - MIT-licensed
//! - Simple compression and decompression routines
//! - `#![no_std]` compatible
//!
//! ### Usage
//!
//! See the [`compress`] or [`decompress`] documentation for reference.
//!
//! In `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! lzokay = "2.0.0"
//! ```
//!
//! Or, to only enable certain features:
//!
//! ```toml
//! [dependencies.lzokay]
//! version = "2.0.0"
//! default-features = false
//! features = ["decompress", "compress"]
//! ```
//!
//! - `decompress`: Enables decompression functions.
//! - `compress`: Enables compression functions.
//! - `alloc`: Enables optional compression functions that perform heap allocation.
//!            Without `std`, this uses `extern crate alloc`.
//! - `std`: Enables use of `std`. Implies `alloc`.
//!
//! All features are enabled by default.
//!
//! ### License
//!
//! LZ👌 and LZ👌-rs are available under the MIT License and have no external dependencies.

#[cfg(feature = "compress")]
pub mod compress;
#[cfg(feature = "decompress")]
pub mod decompress;

/// Error result codes
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Likely indicates bad compressed LZO input.
    LookbehindOverrun,
    /// Output buffer was not large enough to store the compression/decompression result.
    OutputOverrun,
    /// Compressed input buffer is invalid or truncated.
    InputOverrun,
    /// Unknown error.
    Error,
    /// Decompression succeeded, but input buffer has remaining data.
    InputNotConsumed,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Error::LookbehindOverrun => write!(f, "lookbehind overrun"),
            Error::OutputOverrun => write!(f, "output overrun"),
            Error::InputOverrun => write!(f, "input overrun"),
            Error::Error => write!(f, "unknown error"),
            Error::InputNotConsumed => write!(f, "input not consumed"),
        }
    }
}

impl core::error::Error for Error {}

#[cfg(test)]
#[cfg(all(feature = "compress", feature = "decompress", feature = "alloc"))]
mod tests {
    #[cfg(not(feature = "std"))]
    extern crate alloc;

    #[cfg(not(feature = "std"))]
    use alloc::vec;

    use super::{compress::compress, decompress::decompress};

    const INPUT1: &[u8] = include_bytes!("test1.txt");
    const INPUT2: &[u8] = include_bytes!("test2.txt");

    #[test]
    fn test_round_trip1() {
        let compressed = compress(INPUT1).expect("Failed to compress");
        let mut dst = vec![0u8; INPUT1.len()];
        decompress(&compressed, &mut dst).expect("Failed to decompress");
        assert_eq!(INPUT1, dst.as_slice());
    }

    #[test]
    fn test_round_trip2() {
        let compressed = compress(INPUT2).expect("Failed to compress");
        let mut dst = vec![0u8; INPUT2.len()];
        decompress(&compressed, &mut dst).expect("Failed to decompress");
        assert_eq!(INPUT2, dst.as_slice());
    }
}

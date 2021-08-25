#![cfg_attr(not(feature = "std"), no_std)]
//! # LZ👌-rs
//!
//! Rust wrapper for [LZ👌](https://github.com/jackoalan/lzokay), a minimal, MIT-licensed
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
//! lzokay = "1.0.0"
//! ```
//!
//! Or, to only enable certain features:
//!
//! ```toml
//! [dependencies.lzokay]
//! version = "1.0.0"
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

mod bindings {
    #![allow(unknown_lints)]
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(deref_nullptr)]
    #![allow(dead_code)]
    #[cfg(not(feature = "std"))]
    mod types {
        pub type c_uchar = u8;
        pub type c_ushort = u16;
        pub type c_uint = u32;
        pub type c_int = i32;
    }
    #[cfg(feature = "std")]
    mod types {
        pub type c_uchar = ::std::os::raw::c_uchar;
        pub type c_ushort = ::std::os::raw::c_ushort;
        pub type c_uint = ::std::os::raw::c_uint;
        pub type c_int = ::std::os::raw::c_int;
    }
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// Error result codes
#[derive(Debug, Eq, PartialEq)]
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

fn lzokay_result<T>(result: T, error: bindings::lzokay_EResult) -> Result<T, Error> {
    if error == bindings::lzokay_EResult_Success {
        Result::Ok(result)
    } else {
        Result::Err(match error {
            bindings::lzokay_EResult_LookbehindOverrun => Error::LookbehindOverrun,
            bindings::lzokay_EResult_OutputOverrun => Error::OutputOverrun,
            bindings::lzokay_EResult_InputOverrun => Error::InputOverrun,
            bindings::lzokay_EResult_InputNotConsumed => Error::InputNotConsumed,
            _ => Error::Error,
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(not(feature = "std"), feature = "alloc"))]
    extern crate alloc;

    #[cfg(all(not(feature = "std"), feature = "alloc"))]
    use alloc::vec::Vec;

    #[cfg(all(feature = "compress", feature = "alloc"))]
    use super::compress::compress;
    #[cfg(feature = "decompress")]
    use super::decompress::decompress;

    #[test]
    #[cfg(all(feature = "compress", feature = "decompress", feature = "alloc"))]
    fn test_round_trip() {
        let src = include_bytes!("test1.txt");
        let compressed = compress(src).expect("Failed to compress");
        let mut dst = vec![0u8; src.len()];
        decompress(&compressed, &mut dst).expect("Failed to decompress");
        assert_eq!(&src[..], dst.as_slice());
    }
}

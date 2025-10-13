//! # Compression routines
//!
//! Available with feature `compress`.
//!
//! [`compress`] and [`compress_with_dict`] available with features `std` and/or `alloc`.
//!
//! # Examples
//!
//! Compressing a buffer into a heap-allocated vector:
//! ```
//! use lzokay::compress::*;
//! # #[allow(non_upper_case_globals)] const input: [u8; 512] = [0u8; 512];
//!
//! # #[cfg(feature = "alloc")] {
//! let dst: Vec<u8> = compress(&input)?;
//! # assert_eq!(dst.len(), 10);
//! # }
//! # Ok::<(), lzokay::Error>(())
//! ```
//!
//! Several compression calls with shared dictionary, avoiding needless work:
//! ```
//! use lzokay::compress::*;
//! # #[allow(non_upper_case_globals)] const input1: [u8; 512] = [0u8; 512];
//! # #[allow(non_upper_case_globals)] const input2: [u8; 512] = [0u8; 512];
//!
//! # #[cfg(feature = "alloc")] {
//! let mut dict = new_dict();
//! let dst1 = compress_with_dict(&input1, &mut dict)?;
//! let dst2 = compress_with_dict(&input2, &mut dict)?;
//! # assert_eq!(dst1.len(), 10);
//! # assert_eq!(dst2.len(), 10);
//! # }
//! # Ok::<(), lzokay::Error>(())
//! ```
//!
//! `#![no_std]` compatible compression:
//! ```
//! use lzokay::compress::*;
//! # #[allow(non_upper_case_globals)] const input: [u8; 512] = [0u8; 512];
//!
//! // Allocate dst on stack, with worst-case compression size
//! let mut dst = [0u8; compress_worst_size(input.len())];
//! // Allocate dictionary storage on stack
//! let mut storage = [0u8; dict_storage_size()];
//! // Create dictionary from storage
//! let mut dict = dict_from_storage(&mut storage);
//! let size = compress_no_alloc(&input, &mut dst, &mut dict)?;
//! # assert_eq!(size, 10);
//! # Ok::<(), lzokay::Error>(())
//! ```

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::{boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::ptr::null_mut;
use core::{marker::PhantomData, mem::size_of};

use crate::{bindings, lzokay_result, Error};

type DictStorage = bindings::lzokay_DictBase_storage_type;

/// Dictionary type
pub struct Dict<'a> {
    base: bindings::lzokay_DictBase,
    #[cfg(feature = "alloc")]
    storage: Option<Box<[u8; dict_storage_size()]>>,
    phantom: PhantomData<&'a DictStorage>,
}

/// Creates a new heap-allocated dictionary.
#[cfg(feature = "alloc")]
pub fn new_dict() -> Dict<'static> {
    let mut dict = Dict {
        base: bindings::lzokay_DictBase { _storage: null_mut() },
        storage: Option::Some(Box::new([0u8; dict_storage_size()])),
        phantom: PhantomData,
    };
    dict.base._storage = dict.storage.as_mut().unwrap().as_mut_ptr() as *mut DictStorage;
    dict
}

/// Dictionary storage size, for manual or stack allocation.
pub const fn dict_storage_size() -> usize {
    size_of::<DictStorage>()
}

/// Creates a dictionary from the supplied storage.
///
/// Storage **must** be at least [`dict_storage_size()`] bytes,
/// otherwise this function will panic.
pub fn dict_from_storage(storage: &mut [u8]) -> Dict<'_> {
    if storage.len() < dict_storage_size() {
        panic!(
            "Dictionary storage is not large enough: {}, expected {}",
            storage.len(),
            dict_storage_size()
        );
    }
    Dict {
        base: bindings::lzokay_DictBase { _storage: storage.as_mut_ptr() as *mut DictStorage },
        #[cfg(feature = "alloc")]
        storage: Option::None,
        phantom: PhantomData,
    }
}

/// Worst-case compression size.
pub const fn compress_worst_size(s: usize) -> usize {
    s + s / 16 + 64 + 3
}

/// Compress the supplied buffer into a heap-allocated vector.
///
/// Creates a new dictionary for each invocation.
#[cfg(feature = "alloc")]
pub fn compress(src: &[u8]) -> Result<Vec<u8>, Error> {
    compress_with_dict(src, &mut new_dict())
}

/// Compress the supplied buffer into a heap-allocated vector,
/// with the supplied pre-allocated dictionary.
#[cfg(feature = "alloc")]
pub fn compress_with_dict(src: &[u8], dict: &mut Dict) -> Result<Vec<u8>, Error> {
    let mut out_size = 0usize;
    let capacity = compress_worst_size(src.len());
    let mut dst = Vec::with_capacity(capacity);
    let result = unsafe {
        let result = bindings::lzokay_compress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            capacity,
            &mut out_size,
            &mut dict.base,
        );
        if result == bindings::lzokay_EResult_Success {
            dst.set_len(out_size as usize);
        }
        result
    };
    lzokay_result(dst, result)
}

/// Compress the supplied buffer.
///
/// For sizing `dst`, use [`compress_worst_size`].
pub fn compress_no_alloc(src: &[u8], dst: &mut [u8], dict: &mut Dict) -> Result<usize, Error> {
    let mut out_size = 0usize;
    let result = unsafe {
        bindings::lzokay_compress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            dst.len(),
            &mut out_size,
            &mut dict.base,
        )
    };
    lzokay_result(out_size as usize, result)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use crate::compress::{compress, compress_with_dict, new_dict};
    use crate::compress::{
        compress_no_alloc, compress_worst_size, dict_from_storage, dict_storage_size,
    };

    const INPUT_1: &[u8] = include_bytes!("test1.txt");
    const EXPECTED_1: &[u8] = include_bytes!("test1.bin");
    const INPUT_2: &[u8] = include_bytes!("test2.txt");
    const EXPECTED_2: &[u8] = include_bytes!("test2.bin");

    #[test]
    #[cfg(feature = "alloc")]
    fn test_compress() {
        let dst = compress(INPUT_1).expect("Failed to compress");
        assert_eq!(dst, EXPECTED_1);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn test_compress_with_dict() {
        let mut dict = new_dict();
        let dst = compress_with_dict(INPUT_1, &mut dict).expect("Failed to compress (1)");
        assert_eq!(dst, EXPECTED_1);
        // Compress a second time to test dictionary reuse
        let dst = compress_with_dict(INPUT_2, &mut dict).expect("Failed to compress (2)");
        assert_eq!(dst, EXPECTED_2);
    }

    #[test]
    fn test_compress_no_alloc() {
        let mut dst = [0u8; compress_worst_size(INPUT_1.len())];
        let mut storage = [0u8; dict_storage_size()];
        let mut dict = dict_from_storage(&mut storage);
        let out_size =
            compress_no_alloc(INPUT_1, &mut dst, &mut dict).expect("Failed to compress (1)");
        assert_eq!(&dst[0..out_size], EXPECTED_1);
        // Compress a second time to test dictionary reuse
        let out_size =
            compress_no_alloc(INPUT_2, &mut dst, &mut dict).expect("Failed to compress (2)");
        assert_eq!(&dst[0..out_size], EXPECTED_2);
    }
}

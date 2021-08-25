//! # Decompression routines
//!
//! Available with feature `decompress`.
//!
//! # Examples
//!
//! Decompressing a buffer with known output size:
//! ```
//! use lzokay::decompress::decompress;
//! # #[allow(non_upper_case_globals)] const input: [u8; 10] = [0x12, 0, 0x20, 0, 0xdf, 0, 0, 0x11, 0, 0];
//! # #[allow(non_upper_case_globals)] const decompressed_size: usize = 512;
//!
//! let mut dst = vec![0u8; decompressed_size];
//! let size = decompress(&input, &mut dst)?;
//! # assert_eq!(size, decompressed_size);
//! # Ok::<(), lzokay::Error>(())
//! ```

use crate::{bindings, lzokay_result, Error};

/// Decompress `src` into `dst`.
///
/// `dst` must be large enough to hold the entire decompressed output.
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    let mut out_size = 0usize;
    let result = unsafe {
        bindings::lzokay_decompress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            dst.len(),
            &mut out_size,
        )
    };
    lzokay_result(out_size as usize, result)
}

#[cfg(test)]
mod tests {
    use crate::decompress::decompress;

    const INPUT_1: &[u8] = include_bytes!("test1.bin");
    const EXPECTED_1: &[u8] = include_bytes!("test1.txt");
    const INPUT_2: &[u8] = include_bytes!("test2.bin");
    const EXPECTED_2: &[u8] = include_bytes!("test2.txt");

    const fn max(a: usize, b: usize) -> usize {
        if a > b {
            a
        } else {
            b
        }
    }

    #[test]
    fn test_decompress() {
        let mut dst = [0u8; max(EXPECTED_1.len(), EXPECTED_2.len())];
        let size = decompress(INPUT_1, &mut dst).expect("Failed to decompress (1)");
        assert_eq!(&dst[0..size], EXPECTED_1);
        let size = decompress(INPUT_2, &mut dst).expect("Failed to decompress (2)");
        assert_eq!(&dst[0..size], EXPECTED_2);
    }
}

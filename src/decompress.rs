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

use crate::Error;

/// Maximum repeat count representable via zero marker bytes when extending
/// literal or match lengths.
const MAX255_COUNT: usize = usize::MAX / 255 - 2;
/// Opcode marker for mid-range matches (labelled "M3" in the LZO reference).
const M3_MARKER: u8 = 0x20;
/// Opcode marker for far matches ("M4") and the terminator instruction.
const M4_MARKER: u8 = 0x10;

/// Decompress `src` into `dst`.
///
/// `dst` must be large enough to hold the entire decompressed output. The
/// function follows the documented LZO opcode semantics and state transitions.
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    if src.len() < 3 {
        return Err(Error::InputOverrun);
    }

    let mut inp = 0usize;
    let mut outp = 0usize;
    let mut state = 0usize;
    let mut nstate: usize;
    let mut lblen: usize;
    let mut lbcur: usize;

    let mut inst = input_byte(src, &mut inp)?;
    // The LZO bitstream reserves the first byte for literal priming. Codes >= 22
    // copy a literal block immediately; 18..21 seed the literal countdown (`state`).
    if inst >= 22 {
        let len = (inst as usize) - 17;
        copy_slice(src, &mut inp, dst, &mut outp, len)?;
        state = 4;
    } else if inst >= 18 {
        nstate = (inst as usize) - 17;
        state = nstate;
        copy_slice(src, &mut inp, dst, &mut outp, nstate)?;
    }

    loop {
        if inp > 1 || state > 0 {
            inst = input_byte(src, &mut inp)?;
        }
        if inst & 0xC0 != 0 {
            // [M2]
            // 1 L L D D D S S  (128..255)
            //   Copy 5-8 bytes from block within 2kB distance
            //   state = S
            //   length = 5 + L
            // 0 1 L D D D S S  (64..127)
            //   Copy 3-4 bytes from block within 2kB distance
            //   length = 3 + L
            // Always followed by one byte: distance = (next << 3) + D + 1
            let next = input_byte(src, &mut inp)?;
            let distance = ((next as usize) << 3) + (((inst as usize) >> 2) & 0x7) + 1;
            lbcur = outp.checked_sub(distance).ok_or(Error::LookbehindOverrun)?;
            lblen = ((inst as usize) >> 5) + 1;
            nstate = (inst as usize) & 0x3;
        } else if inst & M3_MARKER != 0 {
            // [M3]
            // 0 0 1 L L L L L  (32..63)
            //   Copy from <= 16kB distance
            //   length = 2 + (L ?: 31 + zero-runs + tail)
            // Followed by LE16: distance = (value >> 2) + 1, state = value & 3
            lblen = ((inst as usize) & 0x1F) + 2;
            if lblen == 2 {
                let offset = consume_zero_byte_length(src, &mut inp)?;
                let tail = input_byte(src, &mut inp)?;
                lblen += offset * 255 + 31 + tail as usize;
            }
            let raw = read_le16(src, &mut inp)?;
            let distance = ((raw as usize) >> 2) + 1;
            lbcur = outp.checked_sub(distance).ok_or(Error::LookbehindOverrun)?;
            nstate = (raw as usize) & 0x3;
        } else if inst & M4_MARKER != 0 {
            // [M4]
            // 0 0 0 1 H L L L  (16..31)
            //   Copy from 16..48kB distance
            //   length = 2 + (L ?: 7 + zero-runs + tail)
            // Followed by LE16: distance = 16384 + (H << 14) + value, state = value & 3
            //   Terminating opcode when distance == 16384.
            lblen = ((inst as usize) & 0x7) + 2;
            if lblen == 2 {
                let offset = consume_zero_byte_length(src, &mut inp)?;
                let tail = input_byte(src, &mut inp)?;
                lblen += offset * 255 + 7 + tail as usize;
            }
            let raw = read_le16(src, &mut inp)?;
            let base_dist = ((inst as usize & 0x8) << 11) + ((raw as usize) >> 2);
            if base_dist == 0 {
                // Stream finished
                break;
            }
            let distance = base_dist + 16384;
            lbcur = outp.checked_sub(distance).ok_or(Error::LookbehindOverrun)?;
            nstate = (raw as usize) & 0x3;
        } else {
            if state == 0 {
                // [Literal]
                // 0 0 0 0 L L L L  (0..15)
                //   Copy long literal string: length = 3 + extended length bytes.
                let mut len = inst as usize + 3;
                if len == 3 {
                    let offset = consume_zero_byte_length(src, &mut inp)?;
                    let tail = input_byte(src, &mut inp)?;
                    len += offset * 255 + 15 + tail as usize;
                }
                copy_slice(src, &mut inp, dst, &mut outp, len)?;
                state = 4;
                continue;
            } else if state != 4 {
                // [M1, short]
                // state = 1..3
                // 0 0 0 0 D D S S  (0..15)
                //   Copy 2 bytes within 1kB distance, state = S afterwards.
                let tail = input_byte(src, &mut inp)?;
                let distance = ((inst as usize) >> 2) + ((tail as usize) << 2) + 1;
                lbcur = outp.checked_sub(distance).ok_or(Error::LookbehindOverrun)?;
                lblen = 2;
                nstate = (inst as usize) & 0x3;
            } else {
                // [M1, long]
                // state == 4
                // 0 0 0 0 D D S S  (0..15)
                //   Copy 3 bytes within 2..3kB distance, state = S afterwards.
                let tail = input_byte(src, &mut inp)?;
                let distance = ((inst as usize) >> 2) + ((tail as usize) << 2) + 2049;
                lbcur = outp.checked_sub(distance).ok_or(Error::LookbehindOverrun)?;
                lblen = 3;
                nstate = (inst as usize) & 0x3;
            }
        }

        // Copy the lookback run (source and destination may overlap).
        if lblen > 0 {
            let out_end = outp.checked_add(lblen).ok_or(Error::OutputOverrun)?;
            let lb_end = lbcur.checked_add(lblen).ok_or(Error::OutputOverrun)?;
            if out_end > dst.len() || lb_end > dst.len() {
                return Err(Error::OutputOverrun);
            }
            for i in 0..lblen {
                dst[outp + i] = dst[lbcur + i];
            }
            outp = out_end;
        }

        // Copy the following literal run dictated by `nstate`.
        copy_slice(src, &mut inp, dst, &mut outp, nstate)?;

        state = nstate;
    }

    // The stream must end with the terminating M4 instruction (length == 3).
    if lblen != 3 {
        return Err(Error::Error);
    }

    if inp == src.len() {
        Ok(outp)
    } else if inp < src.len() {
        Err(Error::InputNotConsumed)
    } else {
        Err(Error::InputOverrun)
    }
}

/// Read a single byte from `src`.
#[inline(always)]
fn input_byte(src: &[u8], idx: &mut usize) -> Result<u8, Error> {
    let n = src.get(*idx).copied().ok_or(Error::InputOverrun)?;
    *idx += 1;
    Ok(n)
}

/// Read a slice of length `len` starting at `start` from `src`.
#[inline(always)]
fn input_slice<'a>(src: &'a [u8], start: &mut usize, len: usize) -> Result<&'a [u8], Error> {
    let end = start.checked_add(len).ok_or(Error::InputOverrun)?;
    let slice = src.get(*start..end).ok_or(Error::InputOverrun)?;
    *start = end;
    Ok(slice)
}

/// Read a little-endian `u16` starting at `pos`.
#[inline(always)]
fn read_le16(bytes: &[u8], pos: &mut usize) -> Result<u16, Error> {
    let slice = input_slice(bytes, pos, 2)?;
    Ok(u16::from_le_bytes(slice.try_into().unwrap()))
}

/// Get a mutable slice of length `len` starting at `start` from `dst`.
#[inline(always)]
fn output_slice<'a>(
    dst: &'a mut [u8],
    start: &mut usize,
    len: usize,
) -> Result<&'a mut [u8], Error> {
    let end = start.checked_add(len).ok_or(Error::OutputOverrun)?;
    let slice = dst.get_mut(*start..end).ok_or(Error::OutputOverrun)?;
    *start = end;
    Ok(slice)
}

/// Copy a slice from `src` to `dst`.
#[inline(always)]
fn copy_slice(
    src: &[u8],
    src_start: &mut usize,
    dst: &mut [u8],
    dst_start: &mut usize,
    len: usize,
) -> Result<(), Error> {
    if len == 0 {
        return Ok(());
    }
    let src_slice = input_slice(src, src_start, len)?;
    let dst_slice = output_slice(dst, dst_start, len)?;
    dst_slice.copy_from_slice(src_slice);
    Ok(())
}

/// Consume a run of zero marker bytes used for long length encodings.
#[inline(always)]
fn consume_zero_byte_length(src: &[u8], inp: &mut usize) -> Result<usize, Error> {
    let start = *inp;
    while src.get(*inp).copied() == Some(0) {
        *inp += 1;
    }
    let offset = *inp - start;
    if offset > MAX255_COUNT {
        Err(Error::Error)
    } else {
        Ok(offset)
    }
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

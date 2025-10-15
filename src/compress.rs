//! # Compression routines
//!
//! Available with feature `compress`.
//!
//! [`compress`] and [`compress_with_dict`] are available when the `alloc` feature is enabled.
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
//! let mut dict = Dict::new();
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
//! // Allocate dictionary storage (real applications should use thread_local or Mutex)
//! static mut DICT: Dict = Dict::new_const();
//! # #[allow(static_mut_refs)]
//! let size = compress_no_alloc(&input, &mut dst, unsafe { &mut DICT })?;
//! # assert_eq!(size, 10);
//! # Ok::<(), lzokay::Error>(())
//! ```

#[cfg(all(feature = "alloc", not(feature = "std")))]
extern crate alloc;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{boxed::Box, vec, vec::Vec};
use core::cmp;
#[cfg(all(feature = "alloc", feature = "std"))]
use std::{boxed::Box, vec, vec::Vec};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::Error;

#[cfg(feature = "alloc")]
/// Compress `src` into a freshly allocated `Vec<u8>` using a temporary dictionary.
pub fn compress(src: &[u8]) -> Result<Vec<u8>, Error> {
    let mut dict = Dict::new();
    compress_with_dict(src, &mut dict)
}

#[cfg(feature = "alloc")]
/// Compress `src`, reusing the provided dictionary across calls.
pub fn compress_with_dict(src: &[u8], dict: &mut Dict) -> Result<Vec<u8>, Error> {
    let capacity = compress_worst_size(src.len());
    let mut buf = vec![0u8; capacity];
    let size = compress_no_alloc(src, &mut buf, dict)?;
    buf.truncate(size);
    Ok(buf)
}

/// Worst-case compressed size according to the LZO format guarantees.
pub const fn compress_worst_size(s: usize) -> usize { s + s / 16 + 64 + 3 }

/// Compress without heap allocations, writing the output into `dst`.
pub fn compress_no_alloc(src: &[u8], dst: &mut [u8], dict: &mut Dict) -> Result<usize, Error> {
    compress_impl(src, dst, dict)
}

const HASH_SIZE: usize = 0x4000;
const MAX_DIST: usize = 0xBFFF;
const MAX_MATCH_LEN: usize = 0x800;
const BUF_SIZE: usize = MAX_DIST + MAX_MATCH_LEN;
const MAX_MATCH_TABLE: usize = 34;
const BUF_GUARD: usize = BUF_SIZE + MAX_MATCH_LEN + 1 /* alignment */;

const M1_MAX_OFFSET: u32 = 0x0400;
const M2_MAX_OFFSET: u32 = 0x0800;
const M3_MAX_OFFSET: u32 = 0x4000;
const M4_BASE_OFFSET: u32 = 0x4000;

const M2_MIN_LEN: u32 = 3;
const M2_MAX_LEN: u32 = 8;
const M3_MAX_LEN: u32 = 33;
const M4_MAX_LEN: u32 = 9;

const M3_MARKER: u8 = 0x20;
const M4_MARKER: u8 = 0x10;

/// Hash chains tracking recent 3-byte sequences, keeping per-key chains and
/// remembering the best match length at each node.
#[derive(Clone, FromBytes, IntoBytes, KnownLayout, Immutable)]
struct Match3 {
    head: [u16; HASH_SIZE],
    chain_sz: [u16; HASH_SIZE],
    chain: [u16; BUF_SIZE],
    best_len: [u16; BUF_SIZE],
}

impl Match3 {
    #[inline]
    fn make_key(bytes: &[u8]) -> usize {
        let a = bytes[0] as u32;
        let b = bytes[1] as u32;
        let c = bytes[2] as u32;
        let mix = (((a << 5) ^ b).wrapping_shl(5)) ^ c;
        let prod = 0x9f5f_u32.wrapping_mul(mix);
        ((prod >> 5) & 0x3fff) as usize
    }

    #[inline]
    fn get_head(&self, key: usize) -> u16 {
        if self.chain_sz[key] == 0 {
            u16::MAX
        } else {
            self.head[key]
        }
    }

    fn init(&mut self) { self.chain_sz.fill(0); }

    fn remove(&mut self, pos: usize, buffer: &[u8; BUF_GUARD]) {
        let key = Self::make_key(&buffer[pos..]);
        self.chain_sz[key] = self.chain_sz[key].saturating_sub(1);
    }

    /// Insert the current position into the hash chains and return the head
    /// position alongside the bounded chain length to inspect.
    fn advance(&mut self, state: &State, buffer: &[u8; BUF_GUARD]) -> (u16, u32) {
        let key = Self::make_key(&buffer[state.wind_b as usize..]);
        let head = self.get_head(key);
        self.chain[state.wind_b as usize] = head;
        let mut count = self.chain_sz[key] as u32;
        self.chain_sz[key] = self.chain_sz[key].wrapping_add(1);
        if count > MAX_MATCH_LEN as u32 {
            count = MAX_MATCH_LEN as u32;
        }
        self.head[key] = state.wind_b as u16;
        (head, count)
    }

    /// Fast path for known matches: advance the hash chains without searching.
    fn skip_advance(&mut self, state: &State, buffer: &[u8; BUF_GUARD]) {
        let key = Self::make_key(&buffer[state.wind_b as usize..]);
        self.chain[state.wind_b as usize] = self.get_head(key);
        self.head[key] = state.wind_b as u16;
        self.best_len[state.wind_b as usize] = (MAX_MATCH_LEN + 1) as u16;
        self.chain_sz[key] = self.chain_sz[key].wrapping_add(1);
    }
}

/// Direct lookup table for 2-byte prefixes used to seed matches quickly.
#[derive(Clone, FromBytes, IntoBytes, KnownLayout, Immutable)]
struct Match2 {
    head: [u16; 1 << 16],
}

impl Match2 {
    #[inline]
    fn make_key(bytes: &[u8]) -> usize { (bytes[0] as usize) ^ ((bytes[1] as usize) << 8) }

    fn init(&mut self) { self.head.fill(u16::MAX); }

    fn add(&mut self, pos: u16, buffer: &[u8; BUF_GUARD]) {
        let key = Self::make_key(&buffer[pos as usize..]);
        self.head[key] = pos;
    }

    fn remove(&mut self, pos: usize, buffer: &[u8; BUF_GUARD]) {
        let key = Self::make_key(&buffer[pos..]);
        if self.head[key] as usize == pos {
            self.head[key] = u16::MAX;
        }
    }

    /// Try to find a 2-byte prefix match at the current window position.
    fn search(
        &self,
        state: &State,
        lb_pos: &mut u32,
        lb_len: &mut u32,
        best_pos: &mut [u32; MAX_MATCH_TABLE],
        buffer: &[u8; BUF_GUARD],
    ) -> bool {
        let key = Self::make_key(&buffer[state.wind_b as usize..]);
        let pos = self.head[key];
        if pos == u16::MAX {
            return false;
        }
        if best_pos[2] == 0 {
            best_pos[2] = pos as u32 + 1;
        }
        if *lb_len < 2 {
            *lb_len = 2;
            *lb_pos = pos as u32;
        }
        true
    }
}

/// Concrete storage backing a dictionary instance. Buffers and match tables are
/// stored side by side so the encoder can share logic across heap and stack
/// configurations.
#[derive(Clone, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct Dict {
    match3: Match3,
    match2: Match2,
    buffer: [u8; BUF_GUARD],
}

impl Dict {
    #[cfg(feature = "alloc")]
    pub fn new() -> Box<Self> {
        use zerocopy::FromZeros;
        Self::new_box_zeroed().unwrap()
    }

    pub const fn new_const() -> Self {
        Self {
            match3: Match3 {
                head: [0; HASH_SIZE],
                chain_sz: [0; HASH_SIZE],
                chain: [0; BUF_SIZE],
                best_len: [0; BUF_SIZE],
            },
            match2: Match2 { head: [0; 1 << 16] },
            buffer: [0; BUF_GUARD],
        }
    }

    /// Initialize dictionary tables and preload the first window from `state.src`.
    fn init(&mut self, state: &mut State<'_>) {
        self.match3.init();
        self.match2.init();

        state.cycle1_countdown = MAX_DIST as u32;
        state.inp = 0;
        state.wind_sz = cmp::min(state.src.len(), MAX_MATCH_LEN) as u32;
        state.wind_b = 0;
        state.wind_e = state.wind_sz;
        if state.wind_sz > 0 {
            let len = state.wind_sz as usize;
            self.buffer[..len].copy_from_slice(&state.src[..len]);
        }
        state.inp += state.wind_sz as usize;

        if state.wind_sz < 3 {
            let start = state.wind_b as usize + state.wind_sz as usize;
            let end = start + (3 - state.wind_sz as usize);
            self.buffer[start..end].fill(0);
        }
    }

    /// Remove stale entries before the sliding window overwrites them.
    fn reset_next_input_entry(&mut self, state: &mut State) {
        if state.cycle1_countdown == 0 {
            let pos = state.wind_e as usize;
            self.match3.remove(pos, &self.buffer);
            self.match2.remove(pos, &self.buffer);
        } else {
            state.cycle1_countdown -= 1;
        }
    }

    /// Advance the dictionary by one position, returning the best match offset
    /// and length. When `skip` is true the already-emitted match bytes are
    /// fast-forwarded first so the dictionary stays aligned with the encoded
    /// output.
    fn advance(
        &mut self,
        state: &mut State,
        prev_len: u32,
        best_off: &mut [u32; MAX_MATCH_TABLE],
        skip: bool,
    ) -> (u32, u32) {
        if skip {
            // Skip phase: advance through already-encoded match bytes while
            // keeping the dictionary in sync with the emitted output.
            for _ in 0..prev_len.saturating_sub(1) {
                self.reset_next_input_entry(state);
                self.match3.skip_advance(state, &self.buffer);
                self.match2.add(state.wind_b as u16, &self.buffer);
                state.get_byte(&mut self.buffer);
            }
        }

        let mut lb_len = 1u32;
        let mut lb_off = 0u32;
        let mut lb_pos = 0u32;
        let mut best_pos = [0u32; MAX_MATCH_TABLE];

        let (match_head, mut match_count) = self.match3.advance(state, &self.buffer);
        if match_head == u16::MAX {
            match_count = 0;
        }

        let mut should_terminate = false;
        let best_len = lb_len;

        if lb_len >= state.wind_sz {
            // Window exhausted: no further matches possible once we reach EOF.
            if state.wind_sz == 0 {
                should_terminate = true;
            }
            self.match3.best_len[state.wind_b as usize] = (MAX_MATCH_LEN + 1) as u16;
        } else {
            if self.match2.search(state, &mut lb_pos, &mut lb_len, &mut best_pos, &self.buffer)
                && state.wind_sz >= 3
            {
                let mut match_pos = match_head as usize;
                for _ in 0..match_count {
                    if match_pos >= BUF_SIZE {
                        break;
                    }
                    let ref_pos = state.wind_b as usize;
                    let window = state.wind_sz as usize;
                    let mut matched = 0usize;
                    while matched < window
                        && self.buffer[ref_pos + matched] == self.buffer[match_pos + matched]
                    {
                        matched += 1;
                    }
                    if matched >= 2 {
                        if matched < MAX_MATCH_TABLE && best_pos[matched] == 0 {
                            // Remember first occurrence for potential match length tweaks.
                            best_pos[matched] = match_pos as u32 + 1;
                        }
                        let matched_u32 = matched as u32;
                        if matched_u32 > lb_len {
                            lb_len = matched_u32;
                            lb_pos = match_pos as u32;
                            if lb_len == state.wind_sz
                                || lb_len > self.match3.best_len[match_pos] as u32
                            {
                                break;
                            }
                        }
                    }
                    match_pos = self.match3.chain[match_pos] as usize;
                }
            }
            if lb_len > best_len {
                lb_off = state.pos2off(lb_pos);
            }
            self.match3.best_len[state.wind_b as usize] = lb_len as u16;
            for i in 2..MAX_MATCH_TABLE {
                best_off[i] = if best_pos[i] != 0 { state.pos2off(best_pos[i] - 1) } else { 0 };
            }
        }

        self.reset_next_input_entry(state);
        self.match2.add(state.wind_b as u16, &self.buffer);
        state.get_byte(&mut self.buffer);

        if should_terminate {
            state.buf_sz = 0;
            lb_len = 0;
        } else {
            // Buffer size counts the current byte plus the lookahead window.
            state.buf_sz = state.wind_sz + 1;
        }
        state.bufp = state.inp - state.buf_sz as usize;

        (lb_off, lb_len)
    }
}

/// Sliding window state tracked while searching for matches.
struct State<'a> {
    src: &'a [u8],
    inp: usize,
    wind_sz: u32,
    wind_b: u32,
    wind_e: u32,
    cycle1_countdown: u32,
    bufp: usize,
    buf_sz: u32,
}

impl<'a> State<'a> {
    /// Create a new window over `src`.
    fn new(src: &'a [u8]) -> Self {
        Self {
            src,
            inp: 0,
            wind_sz: 0,
            wind_b: 0,
            wind_e: 0,
            cycle1_countdown: 0,
            bufp: 0,
            buf_sz: 0,
        }
    }

    /// Advance the window by one byte, copying from `src` and maintaining the
    /// duplicated tail used for wrap-around reads.
    fn get_byte(&mut self, buffer: &mut [u8; BUF_GUARD]) {
        if self.inp >= self.src.len() {
            if self.wind_sz > 0 {
                self.wind_sz -= 1;
            }
            let idx = self.wind_e as usize;
            buffer[idx] = 0;
            if idx < MAX_MATCH_LEN {
                buffer[BUF_SIZE + idx] = 0;
            }
        } else {
            let value = self.src[self.inp];
            let idx = self.wind_e as usize;
            buffer[idx] = value;
            if idx < MAX_MATCH_LEN {
                buffer[BUF_SIZE + idx] = value;
            }
            self.inp += 1;
        }
        self.wind_e = (self.wind_e + 1) % BUF_SIZE as u32;
        self.wind_b = (self.wind_b + 1) % BUF_SIZE as u32;
    }

    /// Convert a buffer index into a backwards distance within the window.
    #[inline]
    fn pos2off(&self, pos: u32) -> u32 {
        if self.wind_b > pos {
            self.wind_b - pos
        } else {
            BUF_SIZE as u32 - (pos - self.wind_b)
        }
    }
}

/// Emit the repeated zero-byte encoding used for long literal/match lengths.
fn write_zero_byte_length(
    dst: &mut [u8],
    out_pos: &mut usize,
    mut len: usize,
) -> Result<(), Error> {
    while len > 255 {
        write_dst(dst, out_pos, &[0])?;
        len -= 255;
    }
    write_dst(dst, out_pos, &[len as u8])?;
    Ok(())
}

/// Emit a literal run following the LZO opcode rules.
fn encode_literal_run(
    dst: &mut [u8],
    out_pos: &mut usize,
    src: &[u8],
    lit_ptr: usize,
    lit_len: usize,
) -> Result<(), Error> {
    if *out_pos == 0 && lit_len <= 238 {
        write_dst(dst, out_pos, &[17 + lit_len as u8])?;
    } else if lit_len <= 3 {
        let idx = out_pos.checked_sub(2).ok_or(Error::OutputOverrun)?;
        *dst_byte_mut(dst, idx)? |= lit_len as u8;
    } else if lit_len <= 18 {
        write_dst(dst, out_pos, &[(lit_len - 3) as u8])?;
    } else {
        write_dst(dst, out_pos, &[0])?;
        write_zero_byte_length(dst, out_pos, lit_len - 18)?;
    }
    let src_chunk = src.get(lit_ptr..lit_ptr + lit_len).ok_or(Error::InputOverrun)?;
    write_dst(dst, out_pos, src_chunk)?;
    Ok(())
}

/// Emit a back-reference according to the LZOKAY/LZO opcode encoding.
fn encode_lookback_match(
    dst: &mut [u8],
    out_pos: &mut usize,
    lb_len: u32,
    mut lb_off: u32,
    last_lit_len: u32,
) -> Result<(), Error> {
    if lb_len == 2 {
        lb_off -= 1;
        write_dst(dst, out_pos, &[((lb_off & 0x3) << 2) as u8, (lb_off >> 2) as u8])?;
    } else if lb_len <= M2_MAX_LEN && lb_off <= M2_MAX_OFFSET {
        lb_off -= 1;
        write_dst(dst, out_pos, &[
            (((lb_len - 1) << 5) | ((lb_off & 0x7) << 2)) as u8,
            (lb_off >> 3) as u8,
        ])?;
    } else if lb_len == M2_MIN_LEN && lb_off <= M1_MAX_OFFSET + M2_MAX_OFFSET && last_lit_len >= 4 {
        lb_off -= 1 + M2_MAX_OFFSET;
        write_dst(dst, out_pos, &[((lb_off & 0x3) << 2) as u8, (lb_off >> 2) as u8])?;
    } else if lb_off <= M3_MAX_OFFSET {
        lb_off -= 1;
        if lb_len <= M3_MAX_LEN {
            write_dst(dst, out_pos, &[M3_MARKER | (lb_len as u8 - 2)])?;
        } else {
            let extra = (lb_len - M3_MAX_LEN) as usize;
            write_dst(dst, out_pos, &[M3_MARKER])?;
            write_zero_byte_length(dst, out_pos, extra)?;
        }
        write_dst(dst, out_pos, &[(lb_off << 2) as u8, (lb_off >> 6) as u8])?;
    } else {
        lb_off -= M4_BASE_OFFSET;
        if lb_len <= M4_MAX_LEN {
            write_dst(dst, out_pos, &[M4_MARKER
                | (((lb_off & 0x4000) >> 11) as u8)
                | (lb_len as u8 - 2)])?;
        } else {
            let extra = (lb_len - M4_MAX_LEN) as usize;
            write_dst(dst, out_pos, &[M4_MARKER | (((lb_off & 0x4000) >> 11) as u8)])?;
            write_zero_byte_length(dst, out_pos, extra)?;
        }
        write_dst(dst, out_pos, &[(lb_off << 2) as u8, (lb_off >> 6) as u8])?;
    }
    Ok(())
}

/// Apply the heuristics that prefer cheaper opcodes when a shorter match can be
/// emitted at a closer distance.
fn find_better_match(best_off: &[u32; MAX_MATCH_TABLE], lb_len: &mut u32, lb_off: &mut u32) {
    let len = *lb_len;
    let off = *lb_off;
    if len <= M2_MIN_LEN || off <= M2_MAX_OFFSET {
        return;
    }
    // Prefer re-encoding long matches as cheaper opcodes whenever the distance
    // permits switching to a shorter back-reference class.
    if off > M2_MAX_OFFSET
        && len >= M2_MIN_LEN + 1
        && len <= M2_MAX_LEN + 1
        && best_off[len as usize - 1] != 0
        && best_off[len as usize - 1] <= M2_MAX_OFFSET
    {
        *lb_len = len - 1;
        *lb_off = best_off[len as usize - 1];
    } else if off > M3_MAX_OFFSET
        && len >= M4_MAX_LEN + 1
        && len <= M2_MAX_LEN + 2
        && best_off[len as usize - 2] != 0
        && best_off[len as usize] <= M2_MAX_OFFSET
    {
        *lb_len = len - 2;
        *lb_off = best_off[len as usize - 2];
    } else if off > M3_MAX_OFFSET
        && len >= M4_MAX_LEN + 1
        && len <= M3_MAX_LEN + 1
        && best_off[len as usize - 1] != 0
        && best_off[len as usize - 2] <= M3_MAX_OFFSET
    {
        *lb_len = len - 1;
        *lb_off = best_off[len as usize - 1];
    }
}

/// Core compression routine shared by the heap-allocating and stack variants.
/// Maintains the window management and opcode selection heuristics required by
/// the LZO format while using safe Rust semantics.
fn compress_impl(src: &[u8], dst: &mut [u8], storage: &mut Dict) -> Result<usize, Error> {
    let mut state = State::new(src);
    storage.init(&mut state);

    let mut out_pos = 0usize;
    let mut lit_len = 0u32;
    let mut best_off = [0u32; MAX_MATCH_TABLE];

    let mut lit_ptr = state.inp;
    let (mut lb_off, mut lb_len) = storage.advance(&mut state, 0, &mut best_off, false);

    while state.buf_sz > 0 {
        if lit_len == 0 {
            // Capture the starting point for the next literal run.
            lit_ptr = state.bufp;
        }

        if lb_len < 2
            || (lb_len == 2 && (lb_off > M1_MAX_OFFSET || lit_len == 0 || lit_len >= 4))
            || (lb_len == 2 && out_pos == 0)
            || (out_pos == 0 && lit_len == 0)
        {
            lb_len = 0;
        } else if lb_len == M2_MIN_LEN && lb_off > M1_MAX_OFFSET + M2_MAX_OFFSET && lit_len >= 4 {
            lb_len = 0;
        }

        if lb_len == 0 {
            lit_len += 1;
            // No match chosen: step forward by one literal byte.
            let (next_off, next_len) = storage.advance(&mut state, 0, &mut best_off, false);
            lb_off = next_off;
            lb_len = next_len;
            continue;
        }

        find_better_match(&best_off, &mut lb_len, &mut lb_off);
        encode_literal_run(dst, &mut out_pos, src, lit_ptr, lit_len as usize)?;
        encode_lookback_match(dst, &mut out_pos, lb_len, lb_off, lit_len)?;
        let prev_len = lb_len;
        lit_len = 0;
        // Advance over the matched bytes, updating the search structures.
        let (next_off, next_len) = storage.advance(&mut state, prev_len, &mut best_off, true);
        lb_off = next_off;
        lb_len = next_len;
    }

    // Flush any trailing literal bytes.
    encode_literal_run(dst, &mut out_pos, src, lit_ptr, lit_len as usize)?;

    // Emit terminating M4 instruction (distance 0x4000, length 3).
    write_dst(dst, &mut out_pos, &[M4_MARKER | 1, 0, 0])?;

    Ok(out_pos)
}

#[inline(always)]
fn dst_byte_mut<'a>(dst: &'a mut [u8], idx: usize) -> Result<&'a mut u8, Error> {
    dst.get_mut(idx).ok_or(Error::OutputOverrun)
}

#[inline(always)]
fn write_dst(dst: &mut [u8], out_pos: &mut usize, slice: &[u8]) -> Result<(), Error> {
    let pos = *out_pos;
    let end = pos.checked_add(slice.len()).ok_or(Error::OutputOverrun)?;
    let dst_chunk = dst.get_mut(pos..end).ok_or(Error::OutputOverrun)?;
    dst_chunk.copy_from_slice(slice);
    *out_pos = end;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use super::{compress, compress_with_dict};
    use super::{compress_no_alloc, compress_worst_size, Dict};

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
        let mut dict = Dict::new();
        let dst = compress_with_dict(INPUT_1, &mut dict).expect("Failed to compress (1)");
        assert_eq!(dst, EXPECTED_1);
        let dst = compress_with_dict(INPUT_2, &mut dict).expect("Failed to compress (2)");
        assert_eq!(dst, EXPECTED_2);
    }

    static mut DICT: Dict = Dict::new_const();

    #[test]
    fn test_compress_no_alloc() {
        let mut dst = [0u8; compress_worst_size(INPUT_1.len())];
        #[allow(static_mut_refs)]
        let dict = unsafe { &mut DICT };
        let out_size = compress_no_alloc(INPUT_1, &mut dst, dict).expect("Failed to compress (1)");
        assert_eq!(&dst[0..out_size], EXPECTED_1);
        let out_size = compress_no_alloc(INPUT_2, &mut dst, dict).expect("Failed to compress (2)");
        assert_eq!(&dst[0..out_size], EXPECTED_2);
    }
}

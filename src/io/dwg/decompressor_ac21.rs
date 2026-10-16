//! LZ77 decompressor for DWG AC1021 (R2007) format
//!
//! AC1021 uses a different LZ77 variant than AC1018. The opcode encoding
//! (instruction format) and copy semantics differ from the AC18 compressor.
//!
//! Based on the reference `DwgLZ77AC21Decompressor`.
//!
//! ## Opcode format
//!
//! The upper nibble of the opcode determines the instruction type:
//! - `0x0_`: Long match (length + 0x13, medium offset)
//! - `0x1_`: Short match (length + 3, medium offset)
//! - `0x2_`: Extended match (16-bit offset, variable length)
//! - `0x3_`–`0xF_`: Compact match (length from nibble, short offset)

use crate::error::DxfError;

fn corrupt(what: &str) -> DxfError {
    DxfError::Compression(format!("AC21 LZ77: {what}"))
}

/// Decompress AC1021 LZ77 compressed data.
///
/// Every read from `source` and every write to `buffer` is bounds-checked
/// (this fork, patch H1): malformed input returns
/// [`DxfError::Compression`] instead of panicking.
///
/// # Arguments
/// * `source` - Compressed input data
/// * `initial_offset` - Starting offset within source
/// * `length` - Number of compressed bytes to process
/// * `buffer` - Output buffer (must be pre-allocated to decompressed size)
pub fn decompress_ac21(
    source: &[u8],
    initial_offset: u32,
    length: u32,
    buffer: &mut [u8],
) -> Result<(), DxfError> {
    // AC1021 literal copies have a minimum length of 8 and process in chunks of
    // up to 32 bytes.  When the decompressed output is smaller than 32 bytes the
    // internal copy routines (copy_n_reordered / copy_literal) may write past the
    // end of the caller's buffer.  Work around this by using a padded internal
    // buffer when the output is small, then copying the result back.
    const MIN_SAFE: usize = 32;
    if buffer.len() < MIN_SAFE {
        let mut padded = [0u8; MIN_SAFE];
        decompress_ac21_inner(source, initial_offset, length, &mut padded)?;
        let len = buffer.len();
        buffer.copy_from_slice(&padded[..len]);
        Ok(())
    } else {
        decompress_ac21_inner(source, initial_offset, length, buffer)
    }
}

/// Bounds-checked cursor over the compressed bytes.
struct Input<'a> {
    data: &'a [u8],
    pos: usize,
    /// Reads at or beyond `limit` fail.
    limit: usize,
}

impl<'a> Input<'a> {
    fn byte(&mut self) -> Result<u32, DxfError> {
        if self.pos >= self.limit {
            return Err(corrupt("read past the end of the compressed data"));
        }
        let value = *self
            .data
            .get(self.pos)
            .ok_or_else(|| corrupt("read past the end of the source"))?;
        self.pos += 1;
        Ok(u32::from(value))
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], DxfError> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.limit)
            .ok_or_else(|| corrupt("literal runs past the end of the compressed data"))?;
        let chunk = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| corrupt("literal runs past the end of the source"))?;
        self.pos = end;
        Ok(chunk)
    }
}

/// Bounds-checked cursor over the output buffer.
struct Output<'a> {
    data: &'a mut [u8],
    pos: usize,
}

impl Output<'_> {
    fn take(&mut self, len: usize) -> Result<&mut [u8], DxfError> {
        let start = self.pos;
        let end = start
            .checked_add(len)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| corrupt("literal overflows the output buffer"))?;
        self.pos = end;
        Ok(&mut self.data[start..end])
    }
}

/// Inner decompression loop — caller guarantees `buffer.len() >= 32`.
fn decompress_ac21_inner(
    source: &[u8],
    initial_offset: u32,
    length: u32,
    buffer: &mut [u8],
) -> Result<(), DxfError> {
    let start = initial_offset as usize;
    if start >= source.len() {
        return Err(corrupt("initial offset is outside the source"));
    }
    let end_index = start
        .checked_add(length as usize)
        .ok_or_else(|| corrupt("compressed length overflows"))?;
    let mut input = Input {
        data: source,
        pos: start,
        limit: source.len(),
    };
    let mut output = Output {
        data: buffer,
        pos: 0,
    };
    let mut source_offset: u32 = 0;
    let mut match_length: u32 = 0;
    let mut op_code = input.byte()?;

    if input.pos >= end_index {
        return Ok(());
    }

    // Handle initial opcode with upper nibble == 0x20
    if (op_code & 0xF0) == 0x20 {
        input.take(2)?;
        match_length = input.byte()? & 0x07;
    }

    while input.pos < end_index {
        // Copy literal bytes
        if match_length == 0 {
            match_length = read_literal_length(&mut input, op_code)?;
        }

        copy_literal(&mut input, &mut output, match_length)?;

        if input.pos >= end_index {
            break;
        }

        // Process compressed chunks
        copy_decompressed_chunks(
            &mut input,
            end_index,
            &mut output,
            &mut match_length,
            &mut op_code,
            &mut source_offset,
        )?;
    }
    Ok(())
}

/// Copy literal bytes from source to destination using AC1021 byte reordering.
///
/// AC1021 format uses specific byte reordering during literal copies.
/// This matches the C# `copy` method and `m_copyMethods` delegate table exactly.
///
/// For 32-byte chunks: groups of 8 bytes are reversed in order.
/// For sub-32 chunks: each length (1–31) has a specific reordering pattern.
fn copy_literal(input: &mut Input<'_>, output: &mut Output<'_>, length: u32) -> Result<(), DxfError> {
    let mut remaining = length as usize;

    // Copy in chunks of 32 (with byte group reordering)
    while remaining >= 32 {
        let src = input.take(32)?;
        let dst = output.take(32)?;

        // Reverse the 4 groups of 8 bytes within the 32-byte block
        copy_4b(src, 24, dst, 0);
        copy_4b(src, 28, dst, 4);
        copy_4b(src, 16, dst, 8);
        copy_4b(src, 20, dst, 12);
        copy_4b(src, 8, dst, 16);
        copy_4b(src, 12, dst, 20);
        copy_4b(src, 0, dst, 24);
        copy_4b(src, 4, dst, 28);

        remaining -= 32;
    }

    if remaining > 0 {
        let src = input.take(remaining)?;
        let dst = output.take(remaining)?;
        copy_n_reordered(src, 0, dst, 0, remaining);
    }
    Ok(())
}

/// Process compressed back-reference chunks.
fn copy_decompressed_chunks(
    input: &mut Input<'_>,
    end_index: usize,
    output: &mut Output<'_>,
    match_length: &mut u32,
    op_code: &mut u32,
    source_offset: &mut u32,
) -> Result<(), DxfError> {
    *match_length = 0;
    *op_code = input.byte()?;

    read_instructions(input, op_code, match_length, source_offset)?;

    loop {
        // Copy from already-decompressed data (back-reference)
        copy_back_reference(output, *match_length, *source_offset)?;

        *match_length = *op_code & 0x07;

        if *match_length != 0 || input.pos >= end_index {
            break;
        }

        *op_code = input.byte()?;

        if (*op_code >> 4) == 0 {
            break;
        }

        if (*op_code >> 4) == 15 {
            *op_code &= 15;
        }

        read_instructions(input, op_code, match_length, source_offset)?;
    }

    Ok(())
}

/// Decode instruction opcodes to determine match length and back-reference offset.
fn read_instructions(
    input: &mut Input<'_>,
    op_code: &mut u32,
    length: &mut u32,
    source_offset: &mut u32,
) -> Result<(), DxfError> {
    match *op_code >> 4 {
        0 => {
            *length = (*op_code & 0x0F) + 0x13;
            *source_offset = input.byte()?;
            *op_code = input.byte()?;
            *length = (*op_code >> 3 & 0x10) + *length;
            *source_offset = ((*op_code & 0x78) << 5) + 1 + *source_offset;
        }
        1 => {
            *length = (*op_code & 0x0F) + 3;
            *source_offset = input.byte()?;
            *op_code = input.byte()?;
            *source_offset = ((*op_code & 0xF8) << 5) + 1 + *source_offset;
        }
        2 => {
            *source_offset = input.byte()?;
            *source_offset = (input.byte()? << 8 & 0xFF00) | *source_offset;
            *length = *op_code & 7;

            if (*op_code & 8) == 0 {
                *op_code = input.byte()?;
                *length = (*op_code & 0xF8) + *length;
            } else {
                *source_offset += 1;
                *length = (input.byte()? << 3) + *length;
                *op_code = input.byte()?;
                *length = ((*op_code & 0xF8) << 8) + *length + 0x100;
            }
        }
        _ => {
            // Default: compact match (upper nibble 3–15)
            *length = *op_code >> 4;
            *source_offset = *op_code & 0x0F;
            *op_code = input.byte()?;
            *source_offset = ((*op_code & 0xF8) << 1) + *source_offset + 1;
        }
    }
    Ok(())
}

/// Read the literal run length from the opcode stream.
fn read_literal_length(input: &mut Input<'_>, op_code: u32) -> Result<u32, DxfError> {
    let overflow = || corrupt("literal length overflows");
    let mut length = op_code.checked_add(8).ok_or_else(overflow)?;
    if length == 0x17 {
        let mut n = input.byte()?;
        length = length.checked_add(n).ok_or_else(overflow)?;

        if n == 0xFF {
            loop {
                n = input.byte()?;
                n |= input.byte()? << 8;
                length = length.checked_add(n).ok_or_else(overflow)?;
                if n != 0xFFFF {
                    break;
                }
            }
        }
    }
    Ok(length)
}

/// Copy bytes from a back-reference in the output buffer.
///
/// A reference before the start of the output or past its end is corrupt
/// data (upstream skipped or truncated such copies silently).
fn copy_back_reference(output: &mut Output<'_>, length: u32, src_offset: u32) -> Result<(), DxfError> {
    if length == 0 {
        return Ok(());
    }
    let length = length as usize;
    let src_offset = src_offset as usize;
    let dst_index = output.pos;
    if src_offset > dst_index {
        return Err(corrupt("back-reference points before the start of the output"));
    }
    let end = dst_index
        .checked_add(length)
        .filter(|&end| end <= output.data.len())
        .ok_or_else(|| corrupt("back-reference overflows the output buffer"))?;
    let initial_index = dst_index - src_offset;
    // Byte by byte: overlapping references repeat earlier output.
    for i in 0..length {
        output.data[dst_index + i] = output.data[initial_index + i];
    }
    output.pos = end;
    Ok(())
}

/// Copy N bytes using the reference byte-reordering copy methods.
///
/// Each length (1–31) has a specific reordering pattern that matches
/// the `m_copyMethods` delegate table in `DwgLZ77AC21Decompressor.cs`.
/// This reordering is essential for correct AC1021 decompression.
///
/// Made public for cross-validation by the compressor's inverse permutation tests.
#[doc(hidden)]
pub fn decompress_copy_n_reordered(
    src: &[u8],
    si: usize,
    dst: &mut [u8],
    di: usize,
    length: usize,
) {
    copy_n_reordered(src, si, dst, di, length);
}

fn copy_n_reordered(src: &[u8], si: usize, dst: &mut [u8], di: usize, length: usize) {
    match length {
        0 => {}
        1 => copy_1b(src, si, dst, di),
        2 => copy_2b(src, si, dst, di),
        3 => copy_3b(src, si, dst, di),
        4 => copy_4b(src, si, dst, di),
        5 => {
            copy_1b(src, si + 4, dst, di);
            copy_4b(src, si, dst, di + 1);
        }
        6 => {
            copy_1b(src, si + 5, dst, di);
            copy_4b(src, si + 1, dst, di + 1);
            copy_1b(src, si, dst, di + 5);
        }
        7 => {
            copy_2b(src, si + 5, dst, di);
            copy_4b(src, si + 1, dst, di + 2);
            copy_1b(src, si, dst, di + 6);
        }
        8 => copy_8b(src, si, dst, di),
        9 => {
            copy_1b(src, si + 8, dst, di);
            copy_8b(src, si, dst, di + 1);
        }
        10 => {
            copy_1b(src, si + 9, dst, di);
            copy_8b(src, si + 1, dst, di + 1);
            copy_1b(src, si, dst, di + 9);
        }
        11 => {
            copy_2b(src, si + 9, dst, di);
            copy_8b(src, si + 1, dst, di + 2);
            copy_1b(src, si, dst, di + 10);
        }
        12 => {
            copy_4b(src, si + 8, dst, di);
            copy_8b(src, si, dst, di + 4);
        }
        13 => {
            copy_1b(src, si + 12, dst, di);
            copy_4b(src, si + 8, dst, di + 1);
            copy_8b(src, si, dst, di + 5);
        }
        14 => {
            copy_1b(src, si + 13, dst, di);
            copy_4b(src, si + 9, dst, di + 1);
            copy_8b(src, si + 1, dst, di + 5);
            copy_1b(src, si, dst, di + 13);
        }
        15 => {
            copy_2b(src, si + 13, dst, di);
            copy_4b(src, si + 9, dst, di + 2);
            copy_8b(src, si + 1, dst, di + 6);
            copy_1b(src, si, dst, di + 14);
        }
        16 => copy_16b(src, si, dst, di),
        17 => {
            copy_8b(src, si + 9, dst, di);
            copy_1b(src, si + 8, dst, di + 8);
            copy_8b(src, si, dst, di + 9);
        }
        18 => {
            copy_1b(src, si + 17, dst, di);
            copy_16b(src, si + 1, dst, di + 1);
            copy_1b(src, si, dst, di + 17);
        }
        19 => {
            copy_3b(src, si + 16, dst, di);
            copy_16b(src, si, dst, di + 3);
        }
        20 => {
            copy_4b(src, si + 16, dst, di);
            copy_8b(src, si + 8, dst, di + 4);
            copy_8b(src, si, dst, di + 12);
        }
        21 => {
            copy_1b(src, si + 20, dst, di);
            copy_4b(src, si + 16, dst, di + 1);
            copy_8b(src, si + 8, dst, di + 5);
            copy_8b(src, si, dst, di + 13);
        }
        22 => {
            copy_2b(src, si + 20, dst, di);
            copy_4b(src, si + 16, dst, di + 2);
            copy_8b(src, si + 8, dst, di + 6);
            copy_8b(src, si, dst, di + 14);
        }
        23 => {
            copy_3b(src, si + 20, dst, di);
            copy_4b(src, si + 16, dst, di + 3);
            copy_8b(src, si + 8, dst, di + 7);
            copy_8b(src, si, dst, di + 15);
        }
        24 => {
            copy_8b(src, si + 16, dst, di);
            copy_16b(src, si, dst, di + 8);
        }
        25 => {
            copy_8b(src, si + 17, dst, di);
            copy_1b(src, si + 16, dst, di + 8);
            copy_16b(src, si, dst, di + 9);
        }
        26 => {
            copy_1b(src, si + 25, dst, di);
            copy_8b(src, si + 17, dst, di + 1);
            copy_1b(src, si + 16, dst, di + 9);
            copy_16b(src, si, dst, di + 10);
        }
        27 => {
            copy_2b(src, si + 25, dst, di);
            copy_8b(src, si + 17, dst, di + 2);
            copy_1b(src, si + 16, dst, di + 10);
            copy_16b(src, si, dst, di + 11);
        }
        28 => {
            copy_4b(src, si + 24, dst, di);
            copy_8b(src, si + 16, dst, di + 4);
            copy_8b(src, si + 8, dst, di + 12);
            copy_8b(src, si, dst, di + 20);
        }
        29 => {
            copy_1b(src, si + 28, dst, di);
            copy_4b(src, si + 24, dst, di + 1);
            copy_8b(src, si + 16, dst, di + 5);
            copy_8b(src, si + 8, dst, di + 13);
            copy_8b(src, si, dst, di + 21);
        }
        30 => {
            copy_2b(src, si + 28, dst, di);
            copy_4b(src, si + 24, dst, di + 2);
            copy_8b(src, si + 16, dst, di + 6);
            copy_8b(src, si + 8, dst, di + 14);
            copy_8b(src, si, dst, di + 22);
        }
        31 => {
            copy_1b(src, si + 30, dst, di);
            copy_4b(src, si + 26, dst, di + 1);
            copy_8b(src, si + 18, dst, di + 5);
            copy_8b(src, si + 10, dst, di + 13);
            copy_8b(src, si + 2, dst, di + 21);
            copy_2b(src, si, dst, di + 29);
        }
        _ => unreachable!("copy_n_reordered called with length >= 32"),
    }
}

/// Copy 1 byte (straight).
#[inline(always)]
fn copy_1b(src: &[u8], si: usize, dst: &mut [u8], di: usize) {
    dst[di] = src[si];
}

/// Copy 2 bytes (byte-swapped — this is critical for AC1021!).
#[inline(always)]
fn copy_2b(src: &[u8], si: usize, dst: &mut [u8], di: usize) {
    dst[di] = src[si + 1];
    dst[di + 1] = src[si];
}

/// Copy 3 bytes (reversed — this is critical for AC1021!).
#[inline(always)]
fn copy_3b(src: &[u8], si: usize, dst: &mut [u8], di: usize) {
    dst[di] = src[si + 2];
    dst[di + 1] = src[si + 1];
    dst[di + 2] = src[si];
}

/// Copy 4 bytes (straight).
#[inline(always)]
fn copy_4b(src: &[u8], si: usize, dst: &mut [u8], di: usize) {
    dst[di] = src[si];
    dst[di + 1] = src[si + 1];
    dst[di + 2] = src[si + 2];
    dst[di + 3] = src[si + 3];
}

/// Copy 8 bytes (two groups of 4, straight order).
#[inline(always)]
fn copy_8b(src: &[u8], si: usize, dst: &mut [u8], di: usize) {
    copy_4b(src, si, dst, di);
    copy_4b(src, si + 4, dst, di + 4);
}

/// Copy 16 bytes (swap two 8-byte halves).
#[inline(always)]
fn copy_16b(src: &[u8], si: usize, dst: &mut [u8], di: usize) {
    copy_8b(src, si + 8, dst, di);
    copy_8b(src, si, dst, di + 8);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_empty() {
        let source = [0u8; 1];
        let mut buffer = [0u8; 16];
        // Single opcode byte, length = 0 → no output
        decompress_ac21(&source, 0, 0, &mut buffer).unwrap();
        assert!(buffer.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_copy_back_reference() {
        let mut buf = [0u8; 16];
        buf[0] = 0xAA;
        buf[1] = 0xBB;
        buf[2] = 0xCC;
        // Copy 3 bytes from offset 3 (i.e., from position 0)
        let mut output = Output { data: &mut buf, pos: 3 };
        copy_back_reference(&mut output, 3, 3).unwrap();
        assert_eq!(output.pos, 6);
        assert_eq!(buf[3], 0xAA);
        assert_eq!(buf[4], 0xBB);
        assert_eq!(buf[5], 0xCC);
    }

    #[test]
    fn test_copy_back_reference_overlapping() {
        let mut buf = [0u8; 16];
        buf[0] = 0x42;
        // Copy with offset=1 repeats the single byte
        let mut output = Output { data: &mut buf, pos: 1 };
        copy_back_reference(&mut output, 4, 1).unwrap();
        assert_eq!(&buf[1..5], &[0x42, 0x42, 0x42, 0x42]);
    }
}

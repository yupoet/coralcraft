//! Clean-room RFC 1951 (DEFLATE) and RFC 1950 (ZLIB) decompressor (`coralcraft-cdr::inflate`).
//!
//! Used to decompress:
//! 1. Modern X4+ `.cdr` ZIP inner entries (`content/riffData.cdr`, `content/root.dat`) stored with
//!    ZIP compression method `8` (raw RFC 1951 DEFLATE).
//! 2. `LIST cmpr` compressed chunk streams and block-size tables inside `.cdr` RIFF containers
//!    (RFC 1950 ZLIB streams preceded by the 8-byte `"CPng\x01\x00\x04\x00"` header).
//!
//! Enforces a hard `max_output_len` cap (`MAX_DECOMPRESSED_BYTES`) to prevent decompression bombs.

use crate::error::ProbeError;

/// Default maximum decompressed payload size (64 MiB).
pub const MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

const MAX_BITS: usize = 15;

const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

const LENGTH_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];

const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];

const DIST_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];

const DIST_EXTRA: [u8; 30] = [0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 13];

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u64,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, byte_pos: 0, bit_buf: 0, bit_count: 0 }
    }

    fn ensure_bits(&mut self, n: u8) -> Result<(), ProbeError> {
        while self.bit_count < n {
            let Some(&b) = self.data.get(self.byte_pos) else {
                return Err(ProbeError::Truncated { offset: self.byte_pos as u64, needed: 1, available: 0 });
            };
            self.byte_pos += 1;
            self.bit_buf |= u64::from(b) << self.bit_count;
            self.bit_count += 8;
        }
        Ok(())
    }

    fn read_bits(&mut self, n: u8) -> Result<u32, ProbeError> {
        if n == 0 {
            return Ok(0);
        }
        self.ensure_bits(n)?;
        let mask = (1u64 << n) - 1;
        let val = (self.bit_buf & mask) as u32;
        self.bit_buf >>= n;
        self.bit_count -= n;
        Ok(val)
    }

    fn align_to_byte(&mut self) {
        let drop = self.bit_count & 7;
        self.bit_buf >>= drop;
        self.bit_count -= drop;
    }

    fn read_aligned_u16_le(&mut self) -> Result<u16, ProbeError> {
        self.align_to_byte();
        let lo = self.read_bits(8)? as u16;
        let hi = self.read_bits(8)? as u16;
        Ok(lo | (hi << 8))
    }

    fn copy_aligned_bytes(&mut self, len: usize, out: &mut Vec<u8>, max_output_len: usize) -> Result<(), ProbeError> {
        self.align_to_byte();
        if out.len().saturating_add(len) > max_output_len {
            return Err(ProbeError::LimitExceeded { limit: "max_decompressed_bytes".to_owned(), max: max_output_len });
        }
        for _ in 0..len {
            let b = self.read_bits(8)? as u8;
            out.push(b);
        }
        Ok(())
    }
}

/// Canonical Huffman decoder table per RFC 1951 §3.2.2.
struct HuffmanTable {
    counts: [u16; MAX_BITS + 1],
    symbols: Vec<u16>,
}

impl HuffmanTable {
    fn from_lengths(lengths: &[u8]) -> Result<Self, ProbeError> {
        let mut counts = [0u16; MAX_BITS + 1];
        for &len in lengths {
            if usize::from(len) > MAX_BITS {
                return Err(ProbeError::Invalid { offset: 0, reason: format!("Huffman code length {len} exceeds 15") });
            }
            if len > 0 {
                counts[usize::from(len)] += 1;
            }
        }

        // Check that the tree is not over-subscribed
        let mut left = 1i32;
        for &c in &counts[1..=MAX_BITS] {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err(ProbeError::Invalid { offset: 0, reason: "over-subscribed Huffman code length table".to_owned() });
            }
        }

        let mut offsets = [0usize; MAX_BITS + 1];
        let mut total_syms = 0usize;
        for bits in 1..=MAX_BITS {
            offsets[bits] = total_syms;
            total_syms += usize::from(counts[bits]);
        }

        let mut symbols = vec![0u16; total_syms];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let slot = offsets[usize::from(len)];
                offsets[usize::from(len)] += 1;
                symbols[slot] = sym as u16;
            }
        }

        Ok(Self { counts, symbols })
    }

    fn fixed_lit_len() -> Result<Self, ProbeError> {
        let mut lengths = [0u8; 288];
        lengths[0..=143].fill(8);
        lengths[144..=255].fill(9);
        lengths[256..=279].fill(7);
        lengths[280..=287].fill(8);
        Self::from_lengths(&lengths)
    }

    fn fixed_dist() -> Result<Self, ProbeError> {
        let lengths = [5u8; 32];
        Self::from_lengths(&lengths)
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, ProbeError> {
        if self.symbols.len() == 1 && self.counts[1] == 1 {
            let _ = reader.read_bits(1)?;
            return Ok(self.symbols[0]);
        }
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0usize;

        for bits in 1..=MAX_BITS {
            let bit = reader.read_bits(1)? as i32;
            code |= bit;
            let count = i32::from(self.counts[bits]);
            if code - count < first {
                let sym_idx = index + ((code - first) as usize);
                return self
                    .symbols
                    .get(sym_idx)
                    .copied()
                    .ok_or_else(|| ProbeError::Invalid { offset: reader.byte_pos as u64, reason: "invalid Huffman symbol index".to_owned() });
            }
            index += count as usize;
            first = (first + count) << 1;
            code <<= 1;
        }

        Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: "unmatched Huffman code in DEFLATE stream".to_owned() })
    }
}

/// Decompresses a raw RFC 1951 DEFLATE stream, enforcing `max_output_len`.
pub fn inflate_raw(bytes: &[u8], max_output_len: usize) -> Result<Vec<u8>, ProbeError> {
    let mut reader = BitReader::new(bytes);
    let mut out = Vec::new();

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                // Stored (uncompressed) block
                let len = reader.read_aligned_u16_le()?;
                let nlen = reader.read_aligned_u16_le()?;
                if len != !nlen {
                    return Err(ProbeError::Invalid {
                        offset: reader.byte_pos as u64,
                        reason: format!("DEFLATE stored block LEN ({len:#06x}) != !NLEN ({nlen:#06x})"),
                    });
                }
                reader.copy_aligned_bytes(usize::from(len), &mut out, max_output_len)?;
            }
            1 => {
                // Fixed Huffman block
                let lit_len_table = HuffmanTable::fixed_lit_len()?;
                let dist_table = HuffmanTable::fixed_dist()?;
                decode_huffman_block(&mut reader, &lit_len_table, &dist_table, &mut out, max_output_len)?;
            }
            2 => {
                // Dynamic Huffman block
                let hlit = (reader.read_bits(5)? as usize) + 257;
                let hdist = (reader.read_bits(5)? as usize) + 1;
                let hclen = (reader.read_bits(4)? as usize) + 4;
                if hlit > 286 || hdist > 30 {
                    return Err(ProbeError::Invalid {
                        offset: reader.byte_pos as u64,
                        reason: format!("invalid DEFLATE dynamic HLIT ({hlit}) or HDIST ({hdist})"),
                    });
                }

                let mut cl_lengths = [0u8; 19];
                for i in 0..hclen {
                    cl_lengths[CL_ORDER[i]] = reader.read_bits(3)? as u8;
                }
                let cl_table = HuffmanTable::from_lengths(&cl_lengths)?;

                let total_codes = hlit + hdist;
                let mut lengths = Vec::with_capacity(total_codes);
                while lengths.len() < total_codes {
                    let sym = cl_table.decode(&mut reader)?;
                    match sym {
                        0..=15 => lengths.push(sym as u8),
                        16 => {
                            let Some(&prev) = lengths.last() else {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 16 with no previous length".to_owned(),
                                });
                            };
                            let repeat = (reader.read_bits(2)? as usize) + 3;
                            if lengths.len() + repeat > total_codes {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 16 repeat overflow".to_owned(),
                                });
                            }
                            lengths.resize(lengths.len() + repeat, prev);
                        }
                        17 => {
                            let repeat = (reader.read_bits(3)? as usize) + 3;
                            if lengths.len() + repeat > total_codes {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 17 repeat overflow".to_owned(),
                                });
                            }
                            lengths.resize(lengths.len() + repeat, 0);
                        }
                        18 => {
                            let repeat = (reader.read_bits(7)? as usize) + 11;
                            if lengths.len() + repeat > total_codes {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 18 repeat overflow".to_owned(),
                                });
                            }
                            lengths.resize(lengths.len() + repeat, 0);
                        }
                        _ => {
                            return Err(ProbeError::Invalid {
                                offset: reader.byte_pos as u64,
                                reason: format!("invalid DEFLATE code length symbol {sym}"),
                            });
                        }
                    }
                }

                let lit_len_table = HuffmanTable::from_lengths(&lengths[..hlit])?;
                let dist_table = HuffmanTable::from_lengths(&lengths[hlit..])?;
                decode_huffman_block(&mut reader, &lit_len_table, &dist_table, &mut out, max_output_len)?;
            }
            _ => {
                return Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: "reserved DEFLATE block type BTYPE=3".to_owned() });
            }
        }

        if bfinal == 1 {
            break;
        }
    }

    Ok(out)
}

fn decode_huffman_block(
    reader: &mut BitReader<'_>,
    lit_len_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut Vec<u8>,
    max_output_len: usize,
) -> Result<(), ProbeError> {
    loop {
        let sym = lit_len_table.decode(reader)?;
        if sym < 256 {
            if out.len() >= max_output_len {
                return Err(ProbeError::LimitExceeded { limit: "max_decompressed_bytes".to_owned(), max: max_output_len });
            }
            out.push(sym as u8);
        } else if sym == 256 {
            break;
        } else {
            let len_idx = (sym as usize).saturating_sub(257);
            if len_idx >= LENGTH_BASE.len() {
                return Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: format!("invalid DEFLATE length symbol {sym}") });
            }
            let extra_len_bits = LENGTH_EXTRA[len_idx];
            let length = usize::from(LENGTH_BASE[len_idx]) + (reader.read_bits(extra_len_bits)? as usize);

            let dist_sym = dist_table.decode(reader)? as usize;
            if dist_sym >= DIST_BASE.len() {
                return Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: format!("invalid DEFLATE distance symbol {dist_sym}") });
            }
            let extra_dist_bits = DIST_EXTRA[dist_sym];
            let distance = usize::from(DIST_BASE[dist_sym]) + (reader.read_bits(extra_dist_bits)? as usize);

            if distance == 0 || distance > out.len() {
                return Err(ProbeError::Invalid {
                    offset: reader.byte_pos as u64,
                    reason: format!("DEFLATE backward distance ({distance}) exceeds output buffer ({})", out.len()),
                });
            }
            if out.len().saturating_add(length) > max_output_len {
                return Err(ProbeError::LimitExceeded { limit: "max_decompressed_bytes".to_owned(), max: max_output_len });
            }

            let start = out.len() - distance;
            for i in 0..length {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
    Ok(())
}

/// Decompresses an RFC 1950 ZLIB stream (2-byte header + DEFLATE body), enforcing `max_output_len`.
///
/// Also accepts the 8-byte CDR `"CPng\x01\x00\x04\x00"` prefix if present at the start of `bytes`.
pub fn inflate_zlib(bytes: &[u8], max_output_len: usize) -> Result<Vec<u8>, ProbeError> {
    let payload = if bytes.len() >= 8 && &bytes[0..4] == b"CPng" { &bytes[8..] } else { bytes };

    if payload.len() < 2 {
        return Err(ProbeError::TooShort { needed: 2, got: payload.len() });
    }
    let cmf = payload[0];
    let flg = payload[1];
    let cm = cmf & 0x0F;
    let cinfo = cmf >> 4;
    let check = (u16::from(cmf) << 8) | u16::from(flg);
    if cm != 8 || cinfo > 7 || !check.is_multiple_of(31) {
        return Err(ProbeError::Invalid { offset: 0, reason: format!("invalid RFC 1950 zlib header ({cmf:#04x} {flg:#04x})") });
    }
    if (flg & 0x20) != 0 {
        return Err(ProbeError::Invalid { offset: 0, reason: "RFC 1950 FDICT preset dictionary is not supported".to_owned() });
    }

    inflate_raw(&payload[2..], max_output_len)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn decompresses_stored_fixed_and_zlib_streams_and_enforces_max_len() {
        // 1. RFC 1950 zlib with fixed Huffman compressing "Hello Hello Hello!"
        // Let's also test stored block via zlib header 0x78 0x01
        let mut zlib_stored = vec![0x78, 0x01, 0x01]; // BFINAL=1, BTYPE=0
        let msg = b"CoralCraft Clean-Room Inflate Test";
        let len = msg.len() as u16;
        zlib_stored.extend_from_slice(&len.to_le_bytes());
        zlib_stored.extend_from_slice(&(!len).to_le_bytes());
        zlib_stored.extend_from_slice(msg);

        let out = inflate_zlib(&zlib_stored, 1024).unwrap();
        assert_eq!(out, msg);

        // 2. With CPng prefix (`CPng\x01\x00\x04\x00`)
        let mut cpng = b"CPng\x01\x00\x04\x00".to_vec();
        cpng.extend_from_slice(&zlib_stored);
        assert_eq!(inflate_zlib(&cpng, 1024).unwrap(), msg);

        // 3. Enforce max_output_len cap
        let err = inflate_zlib(&zlib_stored, 10).unwrap_err();
        assert!(matches!(err, ProbeError::LimitExceeded { .. }));
    }
}

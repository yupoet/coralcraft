//! Clean-room RFC 1951 (DEFLATE) and RFC 1950 (ZLIB) decompressor (`coralcraft-cdr::inflate`).
//!
//! Used to decompress:
//! 1. Modern X4+ `.cdr` ZIP inner entries (`content/riffData.cdr`, `content/root.dat`) stored with
//!    ZIP compression method `8` (raw RFC 1951 DEFLATE).
//! 2. `LIST cmpr` compressed chunk streams and block-size tables inside `.cdr` RIFF containers
//!    (RFC 1950 ZLIB streams preceded by the 8-byte `"CPng\x01\x00\x04\x00"` header).
//!
//! Enforces a hard `max_output_len` cap (`MAX_DECOMPRESSED_BYTES`) to prevent decompression bombs.

use std::sync::OnceLock;

use crate::error::ProbeError;

/// Default maximum decompressed payload size (64 MiB).
pub const MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

/// Computes the RFC 1950 §8.2 Adler-32 checksum of `bytes`.
pub fn adler32(bytes: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for chunk in bytes.chunks(5_552) {
        for &b in chunk {
            s1 = s1.wrapping_add(u32::from(b));
            s2 = s2.wrapping_add(s1);
        }
        s1 %= MOD_ADLER;
        s2 %= MOD_ADLER;
    }
    (s2 << 16) | s1
}

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
            self.byte_pos = self.byte_pos.saturating_add(1);
            self.bit_buf |= u64::from(b).wrapping_shl(u32::from(self.bit_count));
            self.bit_count = self.bit_count.saturating_add(8);
        }
        Ok(())
    }

    fn read_bits(&mut self, n: u8) -> Result<u32, ProbeError> {
        if n == 0 {
            return Ok(0);
        }
        self.ensure_bits(n)?;
        let mask = 1u64.wrapping_shl(u32::from(n)).wrapping_sub(1);
        let val = (self.bit_buf & mask) as u32;
        self.bit_buf = self.bit_buf.wrapping_shr(u32::from(n));
        self.bit_count = self.bit_count.saturating_sub(n);
        Ok(val)
    }

    fn align_to_byte(&mut self) {
        let drop = self.bit_count & 7;
        self.bit_buf = self.bit_buf.wrapping_shr(u32::from(drop));
        self.bit_count = self.bit_count.saturating_sub(drop);
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
            if len > 0
                && let Some(slot) = counts.get_mut(usize::from(len))
            {
                *slot = slot.saturating_add(1);
            }
        }

        // Check that the tree is not over-subscribed
        let mut left = 1i32;
        for &c in counts.get(1..=MAX_BITS).unwrap_or(&[]) {
            left = left.wrapping_shl(1).saturating_sub(i32::from(c));
            if left < 0 {
                return Err(ProbeError::Invalid { offset: 0, reason: "over-subscribed Huffman code length table".to_owned() });
            }
        }

        let mut offsets = [0usize; MAX_BITS + 1];
        let mut total_syms = 0usize;
        for bits in 1..=MAX_BITS {
            if let Some(off) = offsets.get_mut(bits) {
                *off = total_syms;
            }
            total_syms = total_syms.saturating_add(usize::from(counts.get(bits).copied().unwrap_or(0)));
        }

        let mut symbols = vec![0u16; total_syms];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0
                && let Some(off) = offsets.get_mut(usize::from(len))
            {
                let slot = *off;
                *off = off.saturating_add(1);
                if let Some(sym_slot) = symbols.get_mut(slot) {
                    *sym_slot = sym as u16;
                }
            }
        }

        Ok(Self { counts, symbols })
    }

    fn fixed_lit_len() -> &'static Self {
        static TABLE: OnceLock<HuffmanTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            let mut lengths = [0u8; 288];
            for (idx, slot) in lengths.iter_mut().enumerate() {
                *slot = match idx {
                    0..=143 => 8,
                    144..=255 => 9,
                    256..=279 => 7,
                    _ => 8,
                };
            }
            Self::from_lengths(&lengths).unwrap_or(Self { counts: [0; MAX_BITS + 1], symbols: Vec::new() })
        })
    }

    fn fixed_dist() -> &'static Self {
        static TABLE: OnceLock<HuffmanTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            let lengths = [5u8; 32];
            Self::from_lengths(&lengths).unwrap_or(Self { counts: [0; MAX_BITS + 1], symbols: Vec::new() })
        })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, ProbeError> {
        if self.symbols.len() == 1 && self.counts.get(1).copied() == Some(1) {
            let _ = reader.read_bits(1)?;
            return Ok(self.symbols.first().copied().unwrap_or(0));
        }
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0usize;

        for bits in 1..=MAX_BITS {
            let bit = reader.read_bits(1)? as i32;
            code |= bit;
            let count = i32::from(self.counts.get(bits).copied().unwrap_or(0));
            if code.saturating_sub(count) < first {
                let sym_idx = index.saturating_add(code.saturating_sub(first) as usize);
                return self
                    .symbols
                    .get(sym_idx)
                    .copied()
                    .ok_or_else(|| ProbeError::Invalid { offset: reader.byte_pos as u64, reason: "invalid Huffman symbol index".to_owned() });
            }
            index = index.saturating_add(count as usize);
            first = first.saturating_add(count).wrapping_shl(1);
            code = code.wrapping_shl(1);
        }

        Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: "unmatched Huffman code in DEFLATE stream".to_owned() })
    }
}

/// Decompresses a raw RFC 1951 DEFLATE stream, enforcing `max_output_len`.
pub fn inflate_raw(bytes: &[u8], max_output_len: usize) -> Result<Vec<u8>, ProbeError> {
    inflate_raw_with_consumed(bytes, max_output_len).map(|(out, _)| out)
}

fn inflate_raw_with_consumed(bytes: &[u8], max_output_len: usize) -> Result<(Vec<u8>, usize), ProbeError> {
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
                let lit_len_table = HuffmanTable::fixed_lit_len();
                let dist_table = HuffmanTable::fixed_dist();
                decode_huffman_block(&mut reader, lit_len_table, dist_table, &mut out, max_output_len)?;
            }
            2 => {
                // Dynamic Huffman block
                let hlit = (reader.read_bits(5)? as usize).saturating_add(257);
                let hdist = (reader.read_bits(5)? as usize).saturating_add(1);
                let hclen = (reader.read_bits(4)? as usize).saturating_add(4);
                if hlit > 286 || hdist > 30 {
                    return Err(ProbeError::Invalid {
                        offset: reader.byte_pos as u64,
                        reason: format!("invalid DEFLATE dynamic HLIT ({hlit}) or HDIST ({hdist})"),
                    });
                }

                let mut cl_lengths = [0u8; 19];
                for &cl_idx in CL_ORDER.iter().take(hclen) {
                    if let Some(slot) = cl_lengths.get_mut(cl_idx) {
                        *slot = reader.read_bits(3)? as u8;
                    }
                }
                let cl_table = HuffmanTable::from_lengths(&cl_lengths)?;

                let total_codes = hlit.saturating_add(hdist);
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
                            let repeat = (reader.read_bits(2)? as usize).saturating_add(3);
                            if lengths.len().saturating_add(repeat) > total_codes {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 16 repeat overflow".to_owned(),
                                });
                            }
                            lengths.resize(lengths.len().saturating_add(repeat), prev);
                        }
                        17 => {
                            let repeat = (reader.read_bits(3)? as usize).saturating_add(3);
                            if lengths.len().saturating_add(repeat) > total_codes {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 17 repeat overflow".to_owned(),
                                });
                            }
                            lengths.resize(lengths.len().saturating_add(repeat), 0);
                        }
                        18 => {
                            let repeat = (reader.read_bits(7)? as usize).saturating_add(11);
                            if lengths.len().saturating_add(repeat) > total_codes {
                                return Err(ProbeError::Invalid {
                                    offset: reader.byte_pos as u64,
                                    reason: "DEFLATE code length 18 repeat overflow".to_owned(),
                                });
                            }
                            lengths.resize(lengths.len().saturating_add(repeat), 0);
                        }
                        _ => {
                            return Err(ProbeError::Invalid {
                                offset: reader.byte_pos as u64,
                                reason: format!("invalid DEFLATE code length symbol {sym}"),
                            });
                        }
                    }
                }

                let lit_len_slice = lengths.get(..hlit).unwrap_or(&[]);
                let dist_slice = lengths.get(hlit..).unwrap_or(&[]);
                let lit_len_table = HuffmanTable::from_lengths(lit_len_slice)?;
                let dist_table = HuffmanTable::from_lengths(dist_slice)?;
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

    reader.align_to_byte();
    let consumed = reader.byte_pos.saturating_sub(usize::from(reader.bit_count / 8));
    Ok((out, consumed))
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
            let Some(&base_len) = LENGTH_BASE.get(len_idx) else {
                return Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: format!("invalid DEFLATE length symbol {sym}") });
            };
            let extra_len_bits = LENGTH_EXTRA.get(len_idx).copied().unwrap_or(0);
            let length = usize::from(base_len).saturating_add(reader.read_bits(extra_len_bits)? as usize);

            let dist_sym = dist_table.decode(reader)? as usize;
            let Some(&base_dist) = DIST_BASE.get(dist_sym) else {
                return Err(ProbeError::Invalid { offset: reader.byte_pos as u64, reason: format!("invalid DEFLATE distance symbol {dist_sym}") });
            };
            let extra_dist_bits = DIST_EXTRA.get(dist_sym).copied().unwrap_or(0);
            let distance = usize::from(base_dist).saturating_add(reader.read_bits(extra_dist_bits)? as usize);

            if distance == 0 || distance > out.len() {
                return Err(ProbeError::Invalid {
                    offset: reader.byte_pos as u64,
                    reason: format!("DEFLATE backward distance ({distance}) exceeds output buffer ({})", out.len()),
                });
            }
            if out.len().saturating_add(length) > max_output_len {
                return Err(ProbeError::LimitExceeded { limit: "max_decompressed_bytes".to_owned(), max: max_output_len });
            }

            let start = out.len().saturating_sub(distance);
            for i in 0..length {
                let b = out.get(start.saturating_add(i)).copied().unwrap_or(0);
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
    let payload = if bytes.get(0..4) == Some(b"CPng") { bytes.get(8..).unwrap_or(&[]) } else { bytes };

    if payload.len() < 2 {
        return Err(ProbeError::TooShort { needed: 2, got: payload.len() });
    }
    let cmf = payload.first().copied().unwrap_or(0);
    let flg = payload.get(1).copied().unwrap_or(0);
    let cm = cmf & 0x0F;
    let cinfo = cmf >> 4;
    let check = (u16::from(cmf) << 8) | u16::from(flg);
    if cm != 8 || cinfo > 7 || !check.is_multiple_of(31) {
        return Err(ProbeError::Invalid { offset: 0, reason: format!("invalid RFC 1950 zlib header ({cmf:#04x} {flg:#04x})") });
    }
    if (flg & 0x20) != 0 {
        return Err(ProbeError::Invalid { offset: 0, reason: "RFC 1950 FDICT preset dictionary is not supported".to_owned() });
    }

    let (out, consumed) = inflate_raw_with_consumed(payload.get(2..).unwrap_or(&[]), max_output_len)?;
    let trailer_start = 2usize.saturating_add(consumed);
    let Some(trailer_slice) = payload.get(trailer_start..trailer_start.saturating_add(4)) else {
        return Err(ProbeError::Truncated { offset: trailer_start as u64, needed: 4, available: payload.len().saturating_sub(trailer_start) as u64 });
    };
    let expected_adler = u32::from_be_bytes([
        trailer_slice.first().copied().unwrap_or(0),
        trailer_slice.get(1).copied().unwrap_or(0),
        trailer_slice.get(2).copied().unwrap_or(0),
        trailer_slice.get(3).copied().unwrap_or(0),
    ]);
    let actual_adler = adler32(&out);
    if actual_adler != expected_adler {
        return Err(ProbeError::Invalid {
            offset: trailer_start as u64,
            reason: format!("RFC 1950 Adler-32 checksum mismatch (expected {expected_adler:#010x}, got {actual_adler:#010x})"),
        });
    }
    Ok(out)
}

struct BitWriter {
    buf: Vec<u8>,
    bit_buf: u64,
    bit_count: u8,
}

impl BitWriter {
    fn with_capacity(cap: usize) -> Self {
        Self { buf: Vec::with_capacity(cap), bit_buf: 0, bit_count: 0 }
    }

    fn write_bits(&mut self, val: u32, n: u8) {
        if n == 0 {
            return;
        }
        let mask = 1u64.wrapping_shl(u32::from(n)).wrapping_sub(1);
        self.bit_buf |= (u64::from(val) & mask).wrapping_shl(u32::from(self.bit_count));
        self.bit_count = self.bit_count.saturating_add(n);
        while self.bit_count >= 8 {
            self.buf.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf = self.bit_buf.wrapping_shr(8);
            self.bit_count = self.bit_count.saturating_sub(8);
        }
    }

    fn write_huffman_msb(&mut self, canonical_code: u16, len: u8) {
        let mut rev = 0u32;
        let mut c = u32::from(canonical_code);
        for _ in 0..len {
            rev = (rev << 1) | (c & 1);
            c >>= 1;
        }
        self.write_bits(rev, len);
    }

    fn flush_zero_pad(&mut self) {
        if self.bit_count > 0 {
            self.buf.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf = 0;
            self.bit_count = 0;
        }
    }
}

fn emit_fixed_lit_len(writer: &mut BitWriter, sym: u16) {
    match sym {
        0..=143 => writer.write_huffman_msb(0x30u16.saturating_add(sym), 8),
        144..=255 => writer.write_huffman_msb(0x190u16.saturating_add(sym.saturating_sub(144)), 9),
        256..=279 => writer.write_huffman_msb(sym.saturating_sub(256), 7),
        _ => writer.write_huffman_msb(0xC0u16.saturating_add(sym.saturating_sub(280)), 8),
    }
}

fn emit_fixed_length(writer: &mut BitWriter, length: usize) {
    if length >= 258 {
        emit_fixed_lit_len(writer, 285);
        return;
    }
    let mut idx = 0usize;
    for (i, &base) in LENGTH_BASE.iter().take(28).enumerate() {
        if usize::from(base) <= length {
            idx = i;
        } else {
            break;
        }
    }
    let sym = (257usize.saturating_add(idx)) as u16;
    emit_fixed_lit_len(writer, sym);
    let extra_bits = LENGTH_EXTRA.get(idx).copied().unwrap_or(0);
    let base_len = usize::from(LENGTH_BASE.get(idx).copied().unwrap_or(3));
    writer.write_bits(length.saturating_sub(base_len) as u32, extra_bits);
}

fn emit_fixed_distance(writer: &mut BitWriter, distance: usize) {
    let mut idx = 0usize;
    for (i, &base) in DIST_BASE.iter().enumerate() {
        if usize::from(base) <= distance {
            idx = i;
        } else {
            break;
        }
    }
    writer.write_huffman_msb(idx as u16, 5);
    let extra_bits = DIST_EXTRA.get(idx).copied().unwrap_or(0);
    let base_dist = usize::from(DIST_BASE.get(idx).copied().unwrap_or(1));
    writer.write_bits(distance.saturating_sub(base_dist) as u32, extra_bits);
}

fn hash3(bytes: &[u8], pos: usize) -> usize {
    let b0 = u32::from(bytes.get(pos).copied().unwrap_or(0));
    let b1 = u32::from(bytes.get(pos.saturating_add(1)).copied().unwrap_or(0));
    let b2 = u32::from(bytes.get(pos.saturating_add(2)).copied().unwrap_or(0));
    (b0.wrapping_mul(2_654_435_761) ^ b1.wrapping_mul(40_503) ^ b2) as usize & 4095
}

/// Compresses `bytes` into a raw RFC 1951 DEFLATE stream using fixed Huffman codes (`BTYPE = 01`)
/// and LZ77 sliding-window back-references (`length 3..=258`, `distance 1..=32768`).
pub fn deflate_raw(bytes: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::with_capacity(bytes.len().saturating_div(2).saturating_add(8));
    // Single final fixed-Huffman block: BFINAL = 1 (1 bit), BTYPE = 01 (2 bits LSB-first)
    writer.write_bits(1, 1);
    writer.write_bits(1, 2);

    if bytes.is_empty() {
        emit_fixed_lit_len(&mut writer, 256);
        writer.flush_zero_pad();
        return writer.buf;
    }

    let mut head = vec![usize::MAX; 4096];
    let mut prev = vec![usize::MAX; bytes.len()];
    let mut pos = 0usize;

    while pos < bytes.len() {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        if pos.saturating_add(3) <= bytes.len() {
            let h = hash3(bytes, pos);
            let mut cand = head.get(h).copied().unwrap_or(usize::MAX);
            if let Some(p_slot) = prev.get_mut(pos) {
                *p_slot = cand;
            }
            if let Some(h_slot) = head.get_mut(h) {
                *h_slot = pos;
            }

            let max_match = bytes.len().saturating_sub(pos).min(258);
            let mut steps = 0usize;
            while cand != usize::MAX && steps < 32 {
                let dist = pos.saturating_sub(cand);
                if dist == 0 || dist > 32_768 {
                    break;
                }
                let mut mlen = 0usize;
                while mlen < max_match && bytes.get(cand.saturating_add(mlen)) == bytes.get(pos.saturating_add(mlen)) {
                    mlen = mlen.saturating_add(1);
                }
                if mlen >= 3 && mlen > best_len {
                    best_len = mlen;
                    best_dist = dist;
                    if best_len == 258 {
                        break;
                    }
                }
                cand = prev.get(cand).copied().unwrap_or(usize::MAX);
                steps = steps.saturating_add(1);
            }
        }

        if best_len >= 3 {
            emit_fixed_length(&mut writer, best_len);
            emit_fixed_distance(&mut writer, best_dist);
            let end_pos = pos.saturating_add(best_len);
            let mut adv = pos.saturating_add(1);
            while adv < end_pos {
                if adv.saturating_add(3) <= bytes.len() {
                    let h = hash3(bytes, adv);
                    if let Some(p_slot) = prev.get_mut(adv) {
                        *p_slot = head.get(h).copied().unwrap_or(usize::MAX);
                    }
                    if let Some(h_slot) = head.get_mut(h) {
                        *h_slot = adv;
                    }
                }
                adv = adv.saturating_add(1);
            }
            pos = end_pos;
        } else {
            let lit = u16::from(bytes.get(pos).copied().unwrap_or(0));
            emit_fixed_lit_len(&mut writer, lit);
            pos = pos.saturating_add(1);
        }
    }

    emit_fixed_lit_len(&mut writer, 256);
    writer.flush_zero_pad();
    writer.buf
}

/// Compresses `bytes` into an RFC 1950 ZLIB stream (`0x78 0x9C` header + `deflate_raw` + big-endian `Adler-32`).
pub fn deflate_zlib(bytes: &[u8]) -> Vec<u8> {
    let raw = deflate_raw(bytes);
    let mut out = Vec::with_capacity(raw.len().saturating_add(6));
    out.extend_from_slice(&[0x78, 0x9C]);
    out.extend_from_slice(&raw);
    out.extend_from_slice(&adler32(bytes).to_be_bytes());
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    #[test]
    fn decompresses_stored_fixed_dynamic_and_zlib_streams_with_adler32_r7_3() {
        // 1. RFC 1950 zlib with stored block + valid big-endian Adler-32 trailer
        let mut zlib_stored = vec![0x78, 0x01, 0x01]; // BFINAL=1, BTYPE=0
        let msg = b"CoralCraft Clean-Room Inflate Test";
        let len = msg.len() as u16;
        zlib_stored.extend_from_slice(&len.to_le_bytes());
        zlib_stored.extend_from_slice(&(!len).to_le_bytes());
        zlib_stored.extend_from_slice(msg);
        zlib_stored.extend_from_slice(&adler32(msg).to_be_bytes());

        let out = inflate_zlib(&zlib_stored, 1024).unwrap();
        assert_eq!(out, msg);

        // 2. Corrupted Adler-32 trailer must be rejected
        let mut bad_adler = zlib_stored.clone();
        let last = bad_adler.len() - 1;
        bad_adler[last] ^= 0xFF;
        let err = inflate_zlib(&bad_adler, 1024).unwrap_err();
        assert!(matches!(err, ProbeError::Invalid { .. }));

        // 3. With CPng prefix (`CPng\x01\x00\x04\x00`)
        let mut cpng = b"CPng\x01\x00\x04\x00".to_vec();
        cpng.extend_from_slice(&zlib_stored);
        assert_eq!(inflate_zlib(&cpng, 1024).unwrap(), msg);

        // 4. Enforce max_output_len cap
        let err = inflate_zlib(&zlib_stored, 10).unwrap_err();
        assert!(matches!(err, ProbeError::LimitExceeded { .. }));

        // 5. Real RFC 1951 fixed Huffman (BTYPE=1) + LZ77 back-reference zlib vector
        // Compresses "Hello Hello Hello!" (18 bytes) with fixed Huffman codes + Adler-32 0x3c4c063e
        let zlib_fixed: &[u8] = &[120, 218, 243, 72, 205, 201, 201, 87, 240, 64, 144, 138, 0, 60, 76, 6, 62];
        assert_eq!((zlib_fixed[2] >> 1) & 0x03, 1);
        let fixed_out = inflate_zlib(zlib_fixed, 1024).unwrap();
        assert_eq!(fixed_out, b"Hello Hello Hello!");

        // 6. Real RFC 1951 dynamic Huffman (BTYPE=2) zlib vector
        let zlib_dynamic: &[u8] = &[
            120, 1, 5, 193, 1, 13, 0, 0, 0, 130, 176, 108, 108, 66, 255, 72, 254, 56, 41, 156, 20, 78, 10, 39, 133, 147, 194, 73, 225, 164, 112, 82,
            56, 41, 156, 20, 78, 10, 39, 133, 147, 194, 73, 225, 164, 112, 82, 56, 41, 14, 136, 96, 31, 222,
        ];
        assert_eq!((zlib_dynamic[2] >> 1) & 0x03, 2);
        let dyn_out = inflate_zlib(zlib_dynamic, 1024).unwrap();
        let expected_dyn: Vec<u8> = (0..120).map(|i| 65 + (((i * i + i * 3) % 7) as u8)).collect();
        assert_eq!(dyn_out, expected_dyn);
    }
}

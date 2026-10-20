//! Clean-room ISO/IEC 15948 (PNG) + RFC 1950 (zlib stored-deflate) encoder and round-trip verifier.

use coralcraft_doc::crc32;

use crate::surface::{RenderError, Surface};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Encodes a [`Surface`] into a deterministic 8-bit truecolor-with-alpha (color type 6) PNG byte stream.
pub fn encode_png(surface: &Surface) -> Result<Vec<u8>, RenderError> {
    let w = surface.width;
    let h = surface.height;
    let row_len = (w as usize).checked_mul(4).and_then(|n| n.checked_add(1)).ok_or(RenderError::PngTooLarge)?;
    let raw_len = row_len.checked_mul(h as usize).ok_or(RenderError::PngTooLarge)?;
    if raw_len > (u32::MAX as usize) / 2 {
        return Err(RenderError::PngTooLarge);
    }

    // Build unfiltered RGBA8 scanlines (filter byte 0 per row)
    let mut raw_scanlines = Vec::with_capacity(raw_len);
    for y in 0..h {
        raw_scanlines.push(0_u8); // Filter type 0 (None)
        for x in 0..w {
            let rgba = surface.get_straight_rgba(x, y);
            raw_scanlines.extend_from_slice(&rgba);
        }
    }

    // Wrap raw scanlines in RFC 1950 zlib stream using Stored (BTYPE=00) deflate blocks (max 65,535 bytes each)
    let zlib_data = wrap_zlib_stored(&raw_scanlines)?;

    let mut out = Vec::with_capacity(64usize.saturating_add(zlib_data.len()));
    out.extend_from_slice(&PNG_SIGNATURE);

    // IHDR chunk (13 bytes)
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_png_chunk(&mut out, *b"IHDR", &ihdr)?;

    // IDAT chunk
    write_png_chunk(&mut out, *b"IDAT", &zlib_data)?;

    // IEND chunk
    write_png_chunk(&mut out, *b"IEND", &[])?;

    Ok(out)
}

/// Encodes raw RGBA8 pixels into a deterministic 8-bit truecolor-with-alpha PNG byte stream.
pub fn encode_raw_png_rgba(w: u32, h: u32, rgba_pixels: &[u8]) -> Result<Vec<u8>, RenderError> {
    let row_bytes_no_filter = (w as usize).checked_mul(4).ok_or(RenderError::PngTooLarge)?;
    let row_len = row_bytes_no_filter.checked_add(1).ok_or(RenderError::PngTooLarge)?;
    let raw_len = row_len.checked_mul(h as usize).ok_or(RenderError::PngTooLarge)?;
    let expected_rgba = row_bytes_no_filter.checked_mul(h as usize).ok_or(RenderError::PngTooLarge)?;
    if raw_len > (u32::MAX as usize) / 2 || rgba_pixels.len() < expected_rgba {
        return Err(RenderError::PngTooLarge);
    }

    let mut raw_scanlines = Vec::with_capacity(raw_len);
    for y in 0..h {
        raw_scanlines.push(0_u8); // Filter type 0 (None)
        let row_start = (y as usize).saturating_mul(row_bytes_no_filter);
        let row_end = row_start.saturating_add(row_bytes_no_filter);
        raw_scanlines.extend_from_slice(rgba_pixels.get(row_start..row_end).ok_or(RenderError::PngTooLarge)?);
    }

    let zlib_data = wrap_zlib_stored(&raw_scanlines)?;

    let mut out = Vec::with_capacity(64usize.saturating_add(zlib_data.len()));
    out.extend_from_slice(&PNG_SIGNATURE);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_png_chunk(&mut out, *b"IHDR", &ihdr)?;
    write_png_chunk(&mut out, *b"IDAT", &zlib_data)?;
    write_png_chunk(&mut out, *b"IEND", &[])?;

    Ok(out)
}

const fn b64_char(idx: u32) -> char {
    const B64_CHARS: [u8; 64] = *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    match idx & 0x3F {
        i @ 0..=25 => (b'A'.saturating_add(i as u8)) as char,
        i @ 26..=51 => (b'a'.saturating_add((i as u8).saturating_sub(26))) as char,
        i @ 52..=61 => (b'0'.saturating_add((i as u8).saturating_sub(52))) as char,
        62 => B64_CHARS[62] as char,
        _ => B64_CHARS[63] as char,
    }
}

/// Standard RFC 4648 Base64 encoder for embedding binary assets (e.g. data URIs in SVG).
#[must_use]
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3).saturating_mul(4));
    let (chunks, rem) = data.as_chunks::<3>();
    for &[c0, c1, c2] in chunks {
        let b = (u32::from(c0) << 16) | (u32::from(c1) << 8) | u32::from(c2);
        out.push(b64_char(b >> 18));
        out.push(b64_char(b >> 12));
        out.push(b64_char(b >> 6));
        out.push(b64_char(b));
    }
    if let &[r0] = rem {
        let b = u32::from(r0) << 16;
        out.push(b64_char(b >> 18));
        out.push(b64_char(b >> 12));
        out.push('=');
        out.push('=');
    } else if let &[r0, r1] = rem {
        let b = (u32::from(r0) << 16) | (u32::from(r1) << 8);
        out.push(b64_char(b >> 18));
        out.push(b64_char(b >> 12));
        out.push(b64_char(b >> 6));
        out.push('=');
    }
    out
}

/// Standard RFC 4648 Base64 decoder (ignores ASCII whitespace).
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    if !clean.len().is_multiple_of(4) {
        return Err("invalid base64 length (must be a multiple of 4)".to_owned());
    }
    let decode_val = |b: u8| -> Result<u8, String> {
        match b {
            b'A'..=b'Z' => Ok(b.saturating_sub(b'A')),
            b'a'..=b'z' => Ok(b.saturating_sub(b'a').saturating_add(26)),
            b'0'..=b'9' => Ok(b.saturating_sub(b'0').saturating_add(52)),
            b'+' | b'-' => Ok(62),
            b'/' | b'_' => Ok(63),
            _ => Err(format!("invalid base64 character: 0x{b:02X}")),
        }
    };
    let mut out = Vec::with_capacity((clean.len() / 4).saturating_mul(3));
    for &[c0, c1, c2, c3] in clean.as_chunks::<4>().0 {
        let v0 = decode_val(c0)?;
        let v1 = decode_val(c1)?;
        let pad2 = c2 == b'=';
        let pad3 = c3 == b'=';
        let v2 = if pad2 { 0 } else { decode_val(c2)? };
        let v3 = if pad3 { 0 } else { decode_val(c3)? };
        let triple = (u32::from(v0) << 18) | (u32::from(v1) << 12) | (u32::from(v2) << 6) | u32::from(v3);
        out.push(((triple >> 16) & 0xFF) as u8);
        if !pad2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if !pad2 && !pad3 {
            out.push((triple & 0xFF) as u8);
        }
    }
    Ok(out)
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: [u8; 4], data: &[u8]) -> Result<(), RenderError> {
    let Ok(len_u32) = u32::try_from(data.len()) else {
        return Err(RenderError::PngTooLarge);
    };
    out.extend_from_slice(&len_u32.to_be_bytes());
    let crc_start = out.len();
    out.extend_from_slice(&chunk_type);
    out.extend_from_slice(data);
    let crc = crc32(out.get(crc_start..).unwrap_or_default());
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn wrap_zlib_stored(raw: &[u8]) -> Result<Vec<u8>, RenderError> {
    let num_blocks = raw.len().div_ceil(65_535).max(1);
    let total_len = 2usize.saturating_add(raw.len()).saturating_add(num_blocks.saturating_mul(5)).saturating_add(4);
    if total_len > (u32::MAX as usize) {
        return Err(RenderError::PngTooLarge);
    }

    let mut z = Vec::with_capacity(total_len);
    // RFC 1950 header: CMF = 0x78 (Deflate, 32K window), FLG = 0x01 ((0x78 * 256 + 0x01) % 31 == 0)
    z.push(0x78);
    z.push(0x01);

    if raw.is_empty() {
        z.push(0x01); // BFINAL = 1, BTYPE = 00
        z.extend_from_slice(&0_u16.to_le_bytes());
        z.extend_from_slice(&(!0_u16).to_le_bytes());
    } else {
        let chunks: Vec<&[u8]> = raw.chunks(65_535).collect();
        for (i, block) in chunks.iter().enumerate() {
            let is_final = u8::from(i.saturating_add(1) == chunks.len());
            z.push(is_final); // BFINAL in bit 0, BTYPE = 00 in bits 1..2
            let len = block.len() as u16;
            let nlen = !len;
            z.extend_from_slice(&len.to_le_bytes());
            z.extend_from_slice(&nlen.to_le_bytes());
            z.extend_from_slice(block);
        }
    }

    let adler = adler32(raw);
    z.extend_from_slice(&adler.to_be_bytes());
    Ok(z)
}

/// RFC 1950 Adler-32 checksum.
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for chunk in data.chunks(5_552) {
        for &b in chunk {
            s1 = s1.wrapping_add(u32::from(b));
            s2 = s2.wrapping_add(s1);
        }
        s1 %= MOD_ADLER;
        s2 %= MOD_ADLER;
    }
    (s2 << 16) | s1
}

fn png_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    let s = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes(<[u8; 4]>::try_from(s).ok()?))
}

fn png_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let s = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(s).ok()?))
}

/// Verifies and decodes a stored-deflate RGBA8 PNG produced by [`encode_png`],
/// checking chunk CRCs, zlib header, `LEN`/`NLEN` complements, and Adler-32 checksum.
pub fn decode_stored_png_rgba(png: &[u8]) -> Result<(u32, u32, Vec<[u8; 4]>), String> {
    if png.get(0..8) != Some(&PNG_SIGNATURE) {
        return Err("invalid PNG signature".to_owned());
    }
    let mut pos = 8usize;
    let mut width = 0_u32;
    let mut height = 0_u32;
    let mut idat = Vec::new();
    let mut saw_iend = false;

    while pos < png.len() {
        if pos.saturating_add(12) > png.len() {
            return Err("truncated PNG chunk header".to_owned());
        }
        let len = png_u32_be(png, pos).unwrap_or(0) as usize;
        let ctype = png.get(pos.saturating_add(4)..pos.saturating_add(8)).and_then(|s| <[u8; 4]>::try_from(s).ok()).unwrap_or_default();
        let data_start = pos.saturating_add(8);
        let Some(data_end) = data_start.checked_add(len) else {
            return Err("PNG chunk length overflow".to_owned());
        };
        if data_end.saturating_add(4) > png.len() {
            return Err("PNG chunk exceeds EOF".to_owned());
        }
        let expected_crc = png_u32_be(png, data_end).unwrap_or(0);
        let actual_crc = crc32(png.get(pos.saturating_add(4)..data_end).unwrap_or_default());
        if actual_crc != expected_crc {
            return Err(format!("PNG chunk CRC mismatch on {:?}", String::from_utf8_lossy(&ctype)));
        }

        let data = png.get(data_start..data_end).unwrap_or_default();
        match &ctype {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err("invalid IHDR length".to_owned());
                }
                width = png_u32_be(data, 0).unwrap_or(0);
                height = png_u32_be(data, 4).unwrap_or(0);
                if data.get(8) != Some(&8) || data.get(9) != Some(&6) {
                    return Err("unsupported PNG bit depth or color type".to_owned());
                }
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => {
                saw_iend = true;
                break;
            }
            _ => {}
        }
        pos = data_end.saturating_add(4);
    }

    if !saw_iend || width == 0 || height == 0 {
        return Err("missing IHDR or IEND".to_owned());
    }
    if idat.len() < 6 || idat.first() != Some(&0x78) || idat.get(1) != Some(&0x01) {
        return Err("invalid zlib header in IDAT".to_owned());
    }

    let mut raw = Vec::new();
    let mut zpos = 2usize;
    let zend = idat.len().saturating_sub(4);
    loop {
        if zpos.saturating_add(5) > zend {
            return Err("truncated stored deflate block header".to_owned());
        }
        let hdr = *idat.get(zpos).unwrap_or(&0);
        let bfinal = (hdr & 1) != 0;
        let btype = (hdr >> 1) & 0x03;
        if btype != 0 {
            return Err("expected stored (BTYPE=00) deflate block".to_owned());
        }
        let len = png_u16_le(&idat, zpos.saturating_add(1)).unwrap_or(0);
        let nlen = png_u16_le(&idat, zpos.saturating_add(3)).unwrap_or(0);
        if len != !nlen {
            return Err("stored block LEN/NLEN mismatch".to_owned());
        }
        let bstart = zpos.saturating_add(5);
        let bend = bstart.saturating_add(len as usize);
        if bend > zend {
            return Err("stored block exceeds zlib payload".to_owned());
        }
        raw.extend_from_slice(idat.get(bstart..bend).unwrap_or_default());
        zpos = bend;
        if bfinal {
            break;
        }
    }

    let expected_adler = png_u32_be(&idat, zend).unwrap_or(0);
    if adler32(&raw) != expected_adler {
        return Err("zlib Adler-32 checksum mismatch".to_owned());
    }

    let row_bytes = (width as usize).saturating_mul(4).saturating_add(1);
    if raw.len() != row_bytes.saturating_mul(height as usize) {
        return Err("decoded scanline byte count mismatch".to_owned());
    }

    let mut pixels = Vec::with_capacity((width as usize).saturating_mul(height as usize));
    for row in raw.chunks_exact(row_bytes) {
        if row.first() != Some(&0) {
            return Err("unexpected scanline filter byte".to_owned());
        }
        for px in row.get(1..).unwrap_or_default().as_chunks::<4>().0 {
            pixels.push(*px);
        }
    }

    Ok((width, height, pixels))
}

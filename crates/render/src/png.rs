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

    let mut out = Vec::with_capacity(64 + zlib_data.len());
    out.extend_from_slice(&PNG_SIGNATURE);

    // IHDR chunk (13 bytes)
    let mut ihdr = [0_u8; 13];
    ihdr[0..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8; // bit depth 8
    ihdr[9] = 6; // color type 6 = RGBA
    ihdr[10] = 0; // compression method 0
    ihdr[11] = 0; // filter method 0
    ihdr[12] = 0; // interlace method 0
    write_png_chunk(&mut out, *b"IHDR", &ihdr)?;

    // IDAT chunk
    write_png_chunk(&mut out, *b"IDAT", &zlib_data)?;

    // IEND chunk
    write_png_chunk(&mut out, *b"IEND", &[])?;

    Ok(out)
}

/// Encodes raw RGBA8 pixels into a deterministic 8-bit truecolor-with-alpha PNG byte stream.
pub fn encode_raw_png_rgba(w: u32, h: u32, rgba_pixels: &[u8]) -> Result<Vec<u8>, RenderError> {
    let row_len = (w as usize).checked_mul(4).and_then(|n| n.checked_add(1)).ok_or(RenderError::PngTooLarge)?;
    let raw_len = row_len.checked_mul(h as usize).ok_or(RenderError::PngTooLarge)?;
    if raw_len > (u32::MAX as usize) / 2 || rgba_pixels.len() < (w as usize) * (h as usize) * 4 {
        return Err(RenderError::PngTooLarge);
    }

    let mut raw_scanlines = Vec::with_capacity(raw_len);
    for y in 0..h {
        raw_scanlines.push(0_u8); // Filter type 0 (None)
        let row_start = (y as usize) * (w as usize) * 4;
        let row_end = row_start + (w as usize) * 4;
        raw_scanlines.extend_from_slice(&rgba_pixels[row_start..row_end]);
    }

    let zlib_data = wrap_zlib_stored(&raw_scanlines)?;

    let mut out = Vec::with_capacity(64 + zlib_data.len());
    out.extend_from_slice(&PNG_SIGNATURE);

    let mut ihdr = [0_u8; 13];
    ihdr[0..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    ihdr[10] = 0;
    ihdr[11] = 0;
    ihdr[12] = 0;
    write_png_chunk(&mut out, *b"IHDR", &ihdr)?;
    write_png_chunk(&mut out, *b"IDAT", &zlib_data)?;
    write_png_chunk(&mut out, *b"IEND", &[])?;

    Ok(out)
}

/// Standard RFC 4648 Base64 encoder for embedding binary assets (e.g. data URIs in SVG).
#[must_use]
pub fn base64_encode(data: &[u8]) -> String {
    const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut chunks = data.chunks_exact(3);
    for chunk in &mut chunks {
        let b = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(B64_CHARS[((b >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((b >> 12) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((b >> 6) & 0x3F) as usize] as char);
        out.push(B64_CHARS[(b & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    if rem.len() == 1 {
        let b = u32::from(rem[0]) << 16;
        out.push(B64_CHARS[((b >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((b >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem.len() == 2 {
        let b = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
        out.push(B64_CHARS[((b >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((b >> 12) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((b >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: [u8; 4], data: &[u8]) -> Result<(), RenderError> {
    let Ok(len_u32) = u32::try_from(data.len()) else {
        return Err(RenderError::PngTooLarge);
    };
    out.extend_from_slice(&len_u32.to_be_bytes());
    let crc_start = out.len();
    out.extend_from_slice(&chunk_type);
    out.extend_from_slice(data);
    let crc = crc32(&out[crc_start..]);
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn wrap_zlib_stored(raw: &[u8]) -> Result<Vec<u8>, RenderError> {
    let num_blocks = raw.len().div_ceil(65_535).max(1);
    let total_len = 2 + raw.len() + num_blocks * 5 + 4;
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
            let is_final = u8::from(i + 1 == chunks.len());
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
            s1 += u32::from(b);
            s2 += s1;
        }
        s1 %= MOD_ADLER;
        s2 %= MOD_ADLER;
    }
    (s2 << 16) | s1
}

/// Verifies and decodes a stored-deflate RGBA8 PNG produced by [`encode_png`],
/// checking chunk CRCs, zlib header, `LEN`/`NLEN` complements, and Adler-32 checksum.
pub fn decode_stored_png_rgba(png: &[u8]) -> Result<(u32, u32, Vec<[u8; 4]>), String> {
    if png.len() < 8 || png[0..8] != PNG_SIGNATURE {
        return Err("invalid PNG signature".to_owned());
    }
    let mut pos = 8;
    let mut width = 0_u32;
    let mut height = 0_u32;
    let mut idat = Vec::new();
    let mut saw_iend = false;

    while pos < png.len() {
        if pos + 12 > png.len() {
            return Err("truncated PNG chunk header".to_owned());
        }
        let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
        let ctype = [png[pos + 4], png[pos + 5], png[pos + 6], png[pos + 7]];
        let data_start = pos + 8;
        let Some(data_end) = data_start.checked_add(len) else {
            return Err("PNG chunk length overflow".to_owned());
        };
        if data_end + 4 > png.len() {
            return Err("PNG chunk exceeds EOF".to_owned());
        }
        let expected_crc = u32::from_be_bytes([png[data_end], png[data_end + 1], png[data_end + 2], png[data_end + 3]]);
        let actual_crc = crc32(&png[pos + 4..data_end]);
        if actual_crc != expected_crc {
            return Err(format!("PNG chunk CRC mismatch on {:?}", String::from_utf8_lossy(&ctype)));
        }

        let data = &png[data_start..data_end];
        match &ctype {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err("invalid IHDR length".to_owned());
                }
                width = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                height = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                if data[8] != 8 || data[9] != 6 {
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
        pos = data_end + 4;
    }

    if !saw_iend || width == 0 || height == 0 {
        return Err("missing IHDR or IEND".to_owned());
    }
    if idat.len() < 6 || idat[0] != 0x78 || idat[1] != 0x01 {
        return Err("invalid zlib header in IDAT".to_owned());
    }

    let mut raw = Vec::new();
    let mut zpos = 2;
    let zend = idat.len() - 4;
    loop {
        if zpos + 5 > zend {
            return Err("truncated stored deflate block header".to_owned());
        }
        let hdr = idat[zpos];
        let bfinal = (hdr & 1) != 0;
        let btype = (hdr >> 1) & 0x03;
        if btype != 0 {
            return Err("expected stored (BTYPE=00) deflate block".to_owned());
        }
        let len = u16::from_le_bytes([idat[zpos + 1], idat[zpos + 2]]);
        let nlen = u16::from_le_bytes([idat[zpos + 3], idat[zpos + 4]]);
        if len != !nlen {
            return Err("stored block LEN/NLEN mismatch".to_owned());
        }
        let bstart = zpos + 5;
        let bend = bstart + (len as usize);
        if bend > zend {
            return Err("stored block exceeds zlib payload".to_owned());
        }
        raw.extend_from_slice(&idat[bstart..bend]);
        zpos = bend;
        if bfinal {
            break;
        }
    }

    let expected_adler = u32::from_be_bytes([idat[zend], idat[zend + 1], idat[zend + 2], idat[zend + 3]]);
    if adler32(&raw) != expected_adler {
        return Err("zlib Adler-32 checksum mismatch".to_owned());
    }

    let row_bytes = (width as usize) * 4 + 1;
    if raw.len() != row_bytes * (height as usize) {
        return Err("decoded scanline byte count mismatch".to_owned());
    }

    let mut pixels = Vec::with_capacity((width as usize) * (height as usize));
    for row in raw.chunks_exact(row_bytes) {
        if row[0] != 0 {
            return Err("unexpected scanline filter byte".to_owned());
        }
        for px in row[1..].chunks_exact(4) {
            pixels.push([px[0], px[1], px[2], px[3]]);
        }
    }

    Ok((width, height, pixels))
}

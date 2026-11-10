//! Bounded, explicit CDR text decoding. Fixtures use public character-map definitions.

/// Largest byte string accepted by any of these decoders.
pub const MAX_TEXT_BYTES: usize = 1 << 20;

/// Malformed or over-budget text, reported instead of silently replacing its contents.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextDecodeError {
    #[error("text is {len} bytes, over the {limit}-byte limit")]
    TooLong { len: usize, limit: usize },
    #[error("text is not valid {encoding}")]
    Invalid { encoding: &'static str },
    #[error("UTF-16LE text has an odd byte count")]
    OddUtf16,
}

fn bounded(bytes: &[u8]) -> Result<&[u8], TextDecodeError> {
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(TextDecodeError::TooLong { len: bytes.len(), limit: MAX_TEXT_BYTES });
    }
    Ok(bytes.split(|byte| *byte == 0).next().unwrap_or_default())
}

/// Decode explicitly identified legacy text, stopping at its first NUL.
pub fn decode_legacy(bytes: &[u8], encoding: &'static encoding_rs::Encoding) -> Result<String, TextDecodeError> {
    let (text, _, errors) = encoding.decode(bounded(bytes)?);
    if errors {
        return Err(TextDecodeError::Invalid { encoding: encoding.name() });
    }
    Ok(text.into_owned())
}

/// Decode strict ASCII, stopping at its first NUL.
pub fn decode_ascii(bytes: &[u8]) -> Result<String, TextDecodeError> {
    let bytes = bounded(bytes)?;
    if bytes.iter().any(|byte| !byte.is_ascii()) {
        return Err(TextDecodeError::Invalid { encoding: "ASCII" });
    }
    Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
}

/// Decode strict UTF-8, stopping at its first NUL.
pub fn decode_utf8(bytes: &[u8]) -> Result<String, TextDecodeError> {
    std::str::from_utf8(bounded(bytes)?).map(str::to_owned).map_err(|_| TextDecodeError::Invalid { encoding: "UTF-8" })
}

/// Decode strict UTF-16LE, stopping at its first NUL code unit.
pub fn decode_utf16le(bytes: &[u8]) -> Result<String, TextDecodeError> {
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(TextDecodeError::TooLong { len: bytes.len(), limit: MAX_TEXT_BYTES });
    }
    if !bytes.len().is_multiple_of(2) {
        return Err(TextDecodeError::OddUtf16);
    }
    let units = bytes.as_chunks::<2>().0.iter().map(|bytes| u16::from_le_bytes(*bytes)).take_while(|unit| *unit != 0).collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| TextDecodeError::Invalid { encoding: "UTF-16LE" })
}

#[cfg(test)]
mod tests {
    use super::{TextDecodeError, decode_ascii, decode_legacy, decode_utf8, decode_utf16le};

    #[test]
    fn explicit_legacy_code_pages_decode_exact_unicode_and_stop_at_nul() {
        assert_eq!(decode_legacy(b"A\x80\xe9\0B", encoding_rs::WINDOWS_1252).unwrap(), "A€é");
        assert_eq!(decode_legacy(b"\xa3\xf3\x64\x9f\0ignored", encoding_rs::WINDOWS_1250).unwrap(), "Łódź");
    }

    #[test]
    fn strict_builtin_decoders_reject_malformed_input() {
        assert!(decode_ascii(b"caf\xe9").is_err());
        assert!(decode_utf8(b"caf\xff").is_err());
        assert!(decode_utf16le(b"A").is_err());
        assert_eq!(decode_utf16le(b"A\0B\0\0\0").unwrap(), "AB");
    }

    #[test]
    fn oversized_input_is_a_bounded_error() {
        let bytes = vec![b'A'; super::MAX_TEXT_BYTES + 1];
        let error = decode_legacy(&bytes, encoding_rs::WINDOWS_1252).unwrap_err();
        assert!(matches!(error, TextDecodeError::TooLong { .. }));
    }
}

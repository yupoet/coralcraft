//! Error and warning types for `coralcraft-cdr`.
//!
//! All inputs are treated as untrusted (hostile) byte streams: parse failures always
//! return a structured [`ProbeError`], while non-fatal specification deviations
//! (such as trailing bytes, missing EOF pad byte, or lowercase FourCC form tags)
//! are recorded as [`ProbeWarning`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type returned by `probe` and low-level container inspectors.
/// Recoverable and serializable for CLI/UI display; library code never panics.
#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProbeError {
    /// Input is shorter than the minimum magic or container header length.
    #[error("input too short (expected at least {needed} bytes, got {got} bytes)")]
    TooShort { needed: usize, got: usize },

    /// Input is not a CDR/CMX file (magic mismatch or ZIP archive lacks CDR entries).
    #[error("not a CDR/CMX file: {reason}")]
    NotCdr { reason: String },

    /// Container or chunk declared size exceeds available bytes (truncated input).
    #[error("file truncated at offset {offset} (needed {needed} bytes, available {available} bytes)")]
    Truncated { offset: u64, needed: u64, available: u64 },

    /// Malformed container structure (e.g., LIST payload < 4 bytes, corrupted ZIP directory, encrypted entry).
    #[error("invalid structure at offset {offset}: {reason}")]
    Invalid { offset: u64, reason: String },

    /// Hostile-input safety bound exceeded (e.g., excessive LIST nesting depth or total chunk count).
    #[error("safety limit exceeded ({limit} limit is {max})")]
    LimitExceeded { limit: String, max: usize },
}

/// Non-fatal specification deviation or fallback notice recorded during probing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProbeWarning {
    /// Actual file length exceeds the RIFF header's `8 + declared_size` (trailing bytes present).
    TrailingBytes { declared_end: u64, actual_len: u64 },

    /// Final odd-length chunk at EOF omitted its 1-byte alignment pad (common in real-world RIFF files).
    MissingTrailingPad { offset: u64 },

    /// Alignment pad byte following an odd-length chunk is non-zero.
    NonZeroPadByte { offset: u64, byte: u8 },

    /// Form-type uses a non-canonical lowercase prefix (e.g., `cdr6` instead of `CDR6`).
    NonCanonicalForm { form: String },

    /// Fourth byte of the form-type is not in the known ASCII version mapping table.
    UnknownVersionChar { byte: u8 },

    /// Top-level `vrsn` chunk version differs from the version implied by the form-type character.
    VersionMismatch { form_version: u16, vrsn_chunk: u16 },

    /// Encountered a special `LIST` (`cmpr` or `stlt`) with non-standard sub-chunk layout; skipped as opaque payload.
    OpaqueListSkipped { list_type: String, offset: u64, size: u32 },

    /// Primary entry inside a ZIP container is compressed (e.g., Deflate); inner RIFF header was not decompressed in M1 probe.
    ZipInnerCompressed { entry: String, compression_method: u16 },
}

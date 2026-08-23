const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];
const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[derive(Debug, PartialEq)]
pub enum ParseError {
    UnexpectedEof,
    InvalidMagic([u8; 4]),
    InvalidVersion([u8; 4]),
    Leb128Overflow,
    UnknownValType(u8),
    InvalidFuncType(u8),
    UnknownExportKind(u8),
    UnknownImportKind(u8),
    UnknownRefType(u8),
    UnknownLimitsFlag(u8),
    InvalidMutability(u8),
    InvalidUtf8,
    SizeMismatch,
    UnsupportedInitExpr(u8),
    UnsupportedDataFlag(u8),
    UnsupportedElementFlag(u8),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEof => write!(f, "unexpected end of input"),
            ParseError::InvalidMagic(got) => write!(f, "invalid magic: {:02X?}", got),
            ParseError::InvalidVersion(got) => write!(f, "unsupported version: {:02X?}", got),
            ParseError::Leb128Overflow => write!(f, "LEB128 value overflows u32"),
            ParseError::UnknownValType(b) => write!(f, "unknown value type: {:#04X}", b),
            ParseError::InvalidFuncType(b) => {
                write!(f, "expected 0x60 functype marker, got {:#04X}", b)
            }
            ParseError::UnknownExportKind(b) => write!(f, "unknown export kind: {:#04X}", b),
            ParseError::UnknownImportKind(b) => write!(f, "unknown import kind: {:#04X}", b),
            ParseError::UnknownRefType(b) => write!(f, "unknown reftype: {:#04X}", b),
            ParseError::UnknownLimitsFlag(b) => write!(f, "unknown limits flag: {:#04X}", b),
            ParseError::InvalidMutability(b) => write!(f, "invalid mutability byte: {:#04X}", b),
            ParseError::InvalidUtf8 => write!(f, "name is not valid UTF-8"),
            ParseError::SizeMismatch => {
                write!(f, "declared function body size does not match payload")
            }
            ParseError::UnsupportedInitExpr(b) => {
                write!(f, "unsupported init_expr opcode: {:#04X}", b)
            }
            ParseError::UnsupportedDataFlag(b) => {
                write!(f, "unsupported data segment flag: {:#04X}", b)
            }
            ParseError::UnsupportedElementFlag(b) => {
                write!(f, "unsupported element segment flag: {:#04X}", b)
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// [`ParseError`] enriched with the byte offset and section id where the
/// error occurred. Returned by [`crate::module::parse_module_with_context`].
#[derive(Debug)]
pub struct ParseErrorContext {
    /// The underlying parse error.
    pub error: ParseError,
    /// Byte offset of the section payload that triggered the error, if known.
    pub offset: Option<usize>,
    /// Section id that was being decoded when the error occurred, if known.
    pub section_id: Option<u8>,
}

impl ParseErrorContext {
    pub fn new(error: ParseError) -> Self {
        Self {
            error,
            offset: None,
            section_id: None,
        }
    }

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn with_section(mut self, id: u8) -> Self {
        self.section_id = Some(id);
        self
    }
}

impl std::fmt::Display for ParseErrorContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.section_id, self.offset) {
            (Some(id), Some(off)) => {
                write!(f, "section {} at offset {}: {}", id, off, self.error)
            }
            (Some(id), None) => write!(f, "section {}: {}", id, self.error),
            (None, Some(off)) => write!(f, "at offset {}: {}", off, self.error),
            (None, None) => write!(f, "{}", self.error),
        }
    }
}

impl std::error::Error for ParseErrorContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Decodes an unsigned LEB128 u32 from `bytes[offset..]`.
/// Returns `(value, bytes_consumed)`.
pub fn decode_leb128_u32(bytes: &[u8], offset: usize) -> Result<(u32, usize), ParseError> {
    let mut result: u32 = 0;
    let mut shift = 0u32;
    let mut pos = offset;

    loop {
        if pos >= bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let byte = bytes[pos];
        pos += 1;

        if shift == 28 && (byte & 0xF0) != 0 {
            // bits 28-31: only low 4 bits may be set for a valid u32
            return Err(ParseError::Leb128Overflow);
        }
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, pos - offset));
        }
        shift += 7;
        if shift > 28 {
            return Err(ParseError::Leb128Overflow);
        }
    }
}

/// Decodes a signed LEB128 i32 from `bytes[offset..]`.
/// Returns `(value, bytes_consumed)`.
pub fn decode_leb128_i32(bytes: &[u8], offset: usize) -> Result<(i32, usize), ParseError> {
    let (value, consumed) = decode_leb128_signed(bytes, offset, 32)?;
    Ok((value as i32, consumed))
}

/// Decodes a signed LEB128 i64 from `bytes[offset..]`.
/// Returns `(value, bytes_consumed)`.
pub fn decode_leb128_i64(bytes: &[u8], offset: usize) -> Result<(i64, usize), ParseError> {
    decode_leb128_signed(bytes, offset, 64)
}

/// Shared signed LEB128 core. `bits` is the target width (32 or 64) and bounds
/// the byte count so an over-long encoding is rejected as overflow.
fn decode_leb128_signed(
    bytes: &[u8],
    offset: usize,
    bits: u32,
) -> Result<(i64, usize), ParseError> {
    let mut result: i64 = 0;
    let mut shift: u32 = 0;
    let mut pos = offset;

    loop {
        if pos >= bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let byte = bytes[pos];
        pos += 1;

        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;

        if byte & 0x80 == 0 {
            // Sign-extend when the sign bit (0x40) of the final byte is set.
            if shift < 64 && (byte & 0x40) != 0 {
                result |= -1i64 << shift;
            }
            return Ok((result, pos - offset));
        }
        if shift >= bits + 7 {
            return Err(ParseError::Leb128Overflow);
        }
    }
}

/// A parsed section header from a wasm binary.
#[derive(Debug, PartialEq)]
pub struct SectionHeader {
    pub id: u8,
    pub size: u32,
    /// Byte offset where the section payload starts.
    pub offset: usize,
}

/// Returns an iterator over section headers in a wasm binary.
/// `bytes` must include the 8-byte module header.
pub fn section_iter(bytes: &[u8]) -> SectionIter<'_> {
    SectionIter { bytes, pos: 8 }
}

pub struct SectionIter<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for SectionIter<'a> {
    type Item = Result<SectionHeader, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.bytes.len() {
            return None;
        }

        let id = self.bytes[self.pos];
        self.pos += 1;

        let (size, consumed) = match decode_leb128_u32(self.bytes, self.pos) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        self.pos += consumed;

        let offset = self.pos;
        self.pos += size as usize;

        Some(Ok(SectionHeader { id, size, offset }))
    }
}

/// Validates the 8-byte wasm header and returns the version number.
pub fn parse_header(bytes: &[u8]) -> Result<u32, ParseError> {
    if bytes.len() < 8 {
        return Err(ParseError::UnexpectedEof);
    }

    let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if magic != WASM_MAGIC {
        return Err(ParseError::InvalidMagic(magic));
    }

    let version = [bytes[4], bytes[5], bytes[6], bytes[7]];
    if version != WASM_VERSION {
        return Err(ParseError::InvalidVersion(version));
    }

    Ok(u32::from_le_bytes(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_HEADER: &[u8] = &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    #[test]
    fn valid_header_returns_version_1() {
        assert_eq!(parse_header(VALID_HEADER), Ok(1));
    }

    #[test]
    fn valid_header_with_trailing_bytes() {
        let mut bytes = VALID_HEADER.to_vec();
        bytes.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(parse_header(&bytes), Ok(1));
    }

    #[test]
    fn too_short_returns_eof() {
        assert_eq!(
            parse_header(&[0x00, 0x61, 0x73]),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn empty_returns_eof() {
        assert_eq!(parse_header(&[]), Err(ParseError::UnexpectedEof));
    }

    #[test]
    fn invalid_magic_returns_error() {
        let bytes = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x00, 0x00, 0x00];
        assert_eq!(
            parse_header(&bytes),
            Err(ParseError::InvalidMagic([0xDE, 0xAD, 0xBE, 0xEF]))
        );
    }

    #[test]
    fn invalid_version_returns_error() {
        let bytes = [0x00, 0x61, 0x73, 0x6D, 0x02, 0x00, 0x00, 0x00];
        assert_eq!(
            parse_header(&bytes),
            Err(ParseError::InvalidVersion([0x02, 0x00, 0x00, 0x00]))
        );
    }

    // ── LEB128 ──────────────────────────────────────────────────────────────

    #[test]
    fn leb128_single_byte() {
        assert_eq!(decode_leb128_u32(&[0x01], 0), Ok((1, 1)));
    }

    #[test]
    fn leb128_two_bytes() {
        // 0x80 0x01 = 128
        assert_eq!(decode_leb128_u32(&[0x80, 0x01], 0), Ok((128, 2)));
    }

    #[test]
    fn leb128_max_u32() {
        // 0xFFFFFFFF encoded in LEB128 = [FF FF FF FF 0F]
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        assert_eq!(decode_leb128_u32(&bytes, 0), Ok((0xFFFF_FFFF, 5)));
    }

    #[test]
    fn leb128_overflow_returns_error() {
        // bit 32 set → overflow
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0x1F];
        assert_eq!(
            decode_leb128_u32(&bytes, 0),
            Err(ParseError::Leb128Overflow)
        );
    }

    #[test]
    fn leb128_eof_returns_error() {
        assert_eq!(
            decode_leb128_u32(&[0x80], 0),
            Err(ParseError::UnexpectedEof)
        );
    }

    // ── signed LEB128 ─────────────────────────────────────────────────────────

    #[test]
    fn leb128_i32_positive_single_byte() {
        // 0x2A = 42 (sign bit 0x40 clear → positive)
        assert_eq!(decode_leb128_i32(&[0x2A], 0), Ok((42, 1)));
    }

    #[test]
    fn leb128_i32_negative_single_byte() {
        // 0x7F = -1 (sign bit set, sign-extended)
        assert_eq!(decode_leb128_i32(&[0x7F], 0), Ok((-1, 1)));
        // 0x40 = -64
        assert_eq!(decode_leb128_i32(&[0x40], 0), Ok((-64, 1)));
    }

    #[test]
    fn leb128_i32_positive_two_bytes() {
        // 64 requires 0xC0 0x00 (single byte 0x40 would be -64)
        assert_eq!(decode_leb128_i32(&[0xC0, 0x00], 0), Ok((64, 2)));
    }

    #[test]
    fn leb128_i32_negative_multibyte() {
        // -128 = 0x80 0x7F
        assert_eq!(decode_leb128_i32(&[0x80, 0x7F], 0), Ok((-128, 2)));
    }

    #[test]
    fn leb128_i64_large_negative() {
        // -12345 in signed LEB128 = 0xC7 0x9F 0x7F
        assert_eq!(decode_leb128_i64(&[0xC7, 0x9F, 0x7F], 0), Ok((-12345, 3)));
    }

    #[test]
    fn leb128_i32_overflow_returns_error() {
        // 6 continuation bytes exceed i32 width
        let bytes = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80];
        assert_eq!(
            decode_leb128_i32(&bytes, 0),
            Err(ParseError::Leb128Overflow)
        );
    }

    #[test]
    fn leb128_i32_eof_returns_error() {
        assert_eq!(
            decode_leb128_i32(&[0x80], 0),
            Err(ParseError::UnexpectedEof)
        );
    }

    // ── section_iter ─────────────────────────────────────────────────────────

    fn make_wasm(sections: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        for &(id, payload) in sections {
            out.push(id);
            // size as single-byte LEB128 (payload < 128 bytes in tests)
            out.push(payload.len() as u8);
            out.extend_from_slice(payload);
        }
        out
    }

    #[test]
    fn section_iter_empty_module() {
        let wasm = make_wasm(&[]);
        let sections: Vec<_> = section_iter(&wasm).collect();
        assert!(sections.is_empty());
    }

    #[test]
    fn section_iter_single_section() {
        let payload = [0x01, 0x02, 0x03];
        let wasm = make_wasm(&[(1, &payload)]);
        let sections: Vec<_> = section_iter(&wasm).collect();
        assert_eq!(sections.len(), 1);
        let hdr = sections[0].as_ref().unwrap();
        assert_eq!(hdr.id, 1);
        assert_eq!(hdr.size, 3);
        assert_eq!(&wasm[hdr.offset..hdr.offset + hdr.size as usize], &payload);
    }

    #[test]
    fn section_iter_multiple_sections() {
        let wasm = make_wasm(&[(1, &[0xAA]), (3, &[0xBB, 0xCC]), (7, &[0xDD])]);
        let ids: Vec<u8> = section_iter(&wasm).map(|r| r.unwrap().id).collect();
        assert_eq!(ids, vec![1, 3, 7]);
    }
}

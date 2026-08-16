const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];
const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[derive(Debug, PartialEq)]
pub enum ParseError {
    UnexpectedEof,
    InvalidMagic([u8; 4]),
    InvalidVersion([u8; 4]),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEof => write!(f, "unexpected end of input"),
            ParseError::InvalidMagic(got) => {
                write!(f, "invalid magic: {:02X?}", got)
            }
            ParseError::InvalidVersion(got) => {
                write!(f, "unsupported version: {:02X?}", got)
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Validates the 8-byte wasm header and returns the version number.
pub fn parse_header(bytes: &[u8]) -> Result<u32, ParseError> {
    if bytes.len() < 8 {
        return Err(ParseError::UnexpectedEof);
    }

    let magic: [u8; 4] = bytes[0..4].try_into().unwrap();
    if magic != WASM_MAGIC {
        return Err(ParseError::InvalidMagic(magic));
    }

    let version: [u8; 4] = bytes[4..8].try_into().unwrap();
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
}

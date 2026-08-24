use crate::parser::{decode_leb128_u32, ParseError};

/// Decodes the start section: a single function index naming the module's entry
/// point. The payload must contain exactly that index and no trailing bytes.
pub fn decode_start_section(payload: &[u8]) -> Result<u32, ParseError> {
    let (func_index, n) = decode_leb128_u32(payload, 0)?;
    if n != payload.len() {
        return Err(ParseError::SizeMismatch);
    }
    Ok(func_index)
}

/// Decodes the data count section: a single u32 declaring the number of data
/// segments ahead of the data section (used by bulk-memory `data.drop` /
/// `memory.init`). The payload must contain exactly that count and no trailing
/// bytes.
pub fn decode_datacount_section(payload: &[u8]) -> Result<u32, ParseError> {
    let (count, n) = decode_leb128_u32(payload, 0)?;
    if n != payload.len() {
        return Err(ParseError::SizeMismatch);
    }
    Ok(count)
}

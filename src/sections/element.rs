use crate::parser::{decode_leb128_u32, ParseError};
use std::fmt;

use super::{
    global::{parse_const_expr, ConstExpr},
    RefType,
};

/// How an element segment is placed. `Active` segments copy their elements into
/// `table[table_index]` at `offset` during instantiation; `Passive` and
/// `Declarative` are referenced later (`table.init`) or used only for validation.
#[derive(Debug, PartialEq, Clone)]
pub enum ElementMode {
    Active { table_index: u32, offset: ConstExpr },
    Passive,
    Declarative,
}

/// The element list of a segment: either a vector of function indices (flags
/// 0..=3) or a vector of raw const-expr byte streams (flags 4..=7, each including
/// its trailing `0x0B`).
#[derive(Debug, PartialEq, Clone)]
pub enum ElementInit {
    FuncIndices(Vec<u32>),
    Exprs(Vec<Vec<u8>>),
}

/// An element segment (spec §5.5.12). Covers all eight binary encodings (flag
/// 0..=7); flags ≥ 8 are rejected with [`ParseError::UnsupportedElementFlag`]
/// (or [`ParseError::UnsupportedElementFlagWide`] above `0xFF`).
#[derive(Debug, PartialEq, Clone)]
pub struct ElementSegment {
    pub mode: ElementMode,
    pub reftype: RefType,
    pub init: ElementInit,
}

impl fmt::Display for ElementSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.mode {
            ElementMode::Active {
                table_index,
                offset,
            } => write!(f, "active table[{}] offset={} ", table_index, offset)?,
            ElementMode::Passive => write!(f, "passive ")?,
            ElementMode::Declarative => write!(f, "declarative ")?,
        }
        write!(f, "{} ", self.reftype)?;
        match &self.init {
            ElementInit::FuncIndices(indices) => {
                let funcs = indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "funcs=[{}]", funcs)
            }
            ElementInit::Exprs(exprs) => write!(f, "exprs={}", exprs.len()),
        }
    }
}

/// Reads a `count`-prefixed vector of LEB128 u32 values, advancing `*pos`.
fn read_u32_vec(payload: &[u8], pos: &mut usize) -> Result<Vec<u32>, ParseError> {
    let (count, n) = decode_leb128_u32(payload, *pos)?;
    *pos += n;
    let mut out = super::capped_vec(count, payload.len());
    for _ in 0..count {
        let (v, n) = decode_leb128_u32(payload, *pos)?;
        *pos += n;
        out.push(v);
    }
    Ok(out)
}

/// Reads a `count`-prefixed vector of const-expr byte streams (element exprs).
fn read_expr_vec(payload: &[u8], pos: &mut usize) -> Result<Vec<Vec<u8>>, ParseError> {
    let (count, n) = decode_leb128_u32(payload, *pos)?;
    *pos += n;
    let mut out = super::capped_vec(count, payload.len());
    for _ in 0..count {
        let expr = super::read_init_expr(payload, pos)?;
        validate_elem_expr(&expr)?;
        out.push(expr);
    }
    Ok(out)
}

/// Checks that an element expr is exactly one const instruction plus `end`
/// (`read_init_expr` alone accepts any run of const instructions).
fn validate_elem_expr(bytes: &[u8]) -> Result<(), ParseError> {
    // index of the expected `end` byte
    let end_at = match bytes.first() {
        Some(0xD0) => 2,
        Some(0xD2) => 1 + decode_leb128_u32(bytes, 1)?.1,
        _ => return parse_const_expr(bytes).map(|_| ()),
    };
    match bytes.get(end_at) {
        Some(0x0B) if bytes.len() == end_at + 1 => Ok(()),
        Some(&other) => Err(ParseError::UnsupportedInitExpr(other)),
        None => Err(ParseError::UnexpectedEof),
    }
}

/// Reads an `elemkind` byte, which must be `0x00` (funcref); anything else is
/// rejected as an unknown reference type.
fn read_elemkind(payload: &[u8], pos: &mut usize) -> Result<RefType, ParseError> {
    if *pos >= payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let byte = payload[*pos];
    *pos += 1;
    match byte {
        0x00 => Ok(RefType::FuncRef),
        _ => Err(ParseError::UnknownRefType(byte)),
    }
}

/// Reads a single `reftype` byte (`0x70` funcref / `0x6F` externref).
fn read_reftype(payload: &[u8], pos: &mut usize) -> Result<RefType, ParseError> {
    if *pos >= payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let reftype = RefType::try_from(payload[*pos])?;
    *pos += 1;
    Ok(reftype)
}

pub fn decode_element_section(payload: &[u8]) -> Result<Vec<ElementSegment>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut segments = super::capped_vec(count, payload.len());
    for _ in 0..count {
        let (flag, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        // Flags >= 8 are undefined; reject rather than silently misread.
        if flag >= 8 {
            return Err(match u8::try_from(flag) {
                Ok(b) => ParseError::UnsupportedElementFlag(b),
                Err(_) => ParseError::UnsupportedElementFlagWide(flag),
            });
        }

        // The three low bits select the encoding (spec §5.5.12):
        //   bits 0..=1: 0 = active, table 0     1 = passive
        //               2 = active, explicit table   3 = declarative
        //   bit 2     : element list is an expr vector (else a funcidx vector)
        // Fields are read in binary order: table, offset, reftype/elemkind, list.
        let kind = flag & 0b11;
        let table_index = if kind == 2 {
            let (t, n) = decode_leb128_u32(payload, pos)?;
            pos += n;
            t
        } else {
            0
        };
        let offset_bytes = if kind & 1 == 0 {
            Some(super::read_init_expr(payload, &mut pos)?)
        } else {
            None
        };
        let is_exprs = flag & 0b100 != 0;
        // Kinds 0 implies funcref; the others carry an explicit byte.
        let reftype = match (kind, is_exprs) {
            (0, _) => RefType::FuncRef,
            (_, false) => read_elemkind(payload, &mut pos)?,
            (_, true) => read_reftype(payload, &mut pos)?,
        };
        let init = if is_exprs {
            ElementInit::Exprs(read_expr_vec(payload, &mut pos)?)
        } else {
            ElementInit::FuncIndices(read_u32_vec(payload, &mut pos)?)
        };
        // The offset is validated only after the list is read (error order).
        let mode = match offset_bytes {
            Some(ob) => ElementMode::Active {
                table_index,
                offset: parse_const_expr(&ob)?,
            },
            None if kind == 1 => ElementMode::Passive,
            None => ElementMode::Declarative,
        };
        let seg = ElementSegment {
            mode,
            reftype,
            init,
        };
        segments.push(seg);
    }
    super::ensure_consumed(payload, pos)?;
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One well-formed single-segment payload per flag 0..=7, followed by
    /// malformed variants. The error (or Ok) result for every truncation point
    /// is pinned so the flag-decoding order cannot drift.
    fn cases() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("f0", vec![1, 0, 0x41, 0, 0x0B, 1, 7]),
            ("f1", vec![1, 1, 0, 1, 7]),
            ("f2", vec![1, 2, 3, 0x41, 0, 0x0B, 0, 1, 7]),
            ("f3", vec![1, 3, 0, 1, 7]),
            ("f4", vec![1, 4, 0x41, 0, 0x0B, 1, 0xD2, 7, 0x0B]),
            ("f5", vec![1, 5, 0x6F, 1, 0xD0, 0x6F, 0x0B]),
            ("f6", vec![1, 6, 3, 0x41, 0, 0x0B, 0x70, 1, 0xD2, 7, 0x0B]),
            ("f7", vec![1, 7, 0x70, 1, 0xD2, 7, 0x0B]),
            // `ref.func 0 end` passes read_init_expr but parse_const_expr rejects
            // it as an offset; that error must come after the element list is
            // read, so every truncation inside the list is UnexpectedEof.
            ("f0_badoff", vec![1, 0, 0xD2, 0, 0x0B, 1, 7]),
            ("f2_badoff", vec![1, 2, 3, 0xD2, 0, 0x0B, 0, 1, 7]),
            ("f4_badoff", vec![1, 4, 0xD2, 0, 0x0B, 1, 0xD2, 7, 0x0B]),
            (
                "f6_badoff",
                vec![1, 6, 3, 0xD2, 0, 0x0B, 0x70, 1, 0xD2, 7, 0x0B],
            ),
            ("f0_badoff_badvec", vec![1, 0, 0xD2, 0, 0x0B, 2, 7]),
            ("f1_badkind", vec![1, 1, 9, 1, 7]),
            ("f2_badkind", vec![1, 2, 3, 0x41, 0, 0x0B, 9, 1, 7]),
            ("f3_badkind", vec![1, 3, 9, 1, 7]),
            ("f5_badtype", vec![1, 5, 0x71, 1, 0xD0, 0x6F, 0x0B]),
            (
                "f6_badtype",
                vec![1, 6, 3, 0x41, 0, 0x0B, 0x71, 1, 0xD2, 7, 0x0B],
            ),
            ("f7_badtype", vec![1, 7, 0x71, 1, 0xD2, 7, 0x0B]),
            ("f8", vec![1, 8]),
            ("trailing", vec![1, 3, 0, 1, 7, 0xAA]),
        ]
    }

    #[test]
    fn element_flag_decoding_is_pinned() {
        let mut out = String::new();
        for (name, payload) in cases() {
            for len in 0..=payload.len() {
                out.push_str(&format!(
                    "{name}[{len}]: {:?}\n",
                    decode_element_section(&payload[..len])
                ));
            }
        }
        let expected = include_str!("element_flags.expected");
        assert_eq!(out, expected);
    }
}

use crate::parser::{decode_leb128_u32, ParseError};
use std::fmt;

use super::RefType;

/// How an element segment is placed. `Active` segments copy their elements into
/// `table[table_index]` at `offset_expr` during instantiation; `Passive` and
/// `Declarative` are referenced later (`table.init`) or used only for validation.
/// `offset_expr` holds the raw const-expr bytes (including the trailing `0x0B`).
#[derive(Debug, PartialEq, Clone)]
pub enum ElementMode {
    Active {
        table_index: u32,
        offset_expr: Vec<u8>,
    },
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
/// 0..=7); flags ≥ 8 are rejected with [`ParseError::UnsupportedElementFlag`].
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
                offset_expr,
            } => write!(
                f,
                "active table[{}] offset={} ",
                table_index,
                super::fmt_init_expr(offset_expr)
            )?,
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
    let mut out = Vec::with_capacity(count as usize);
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
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        out.push(super::read_init_expr(payload, pos)?);
    }
    Ok(out)
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

    let mut segments = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (flag, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        // The three low bits select the encoding: bit0 = passive/declarative,
        // bit1 = explicit-table (active) or declarative selector, bit2 = the
        // element list is a vector of exprs instead of funcidxs.
        let seg = match flag {
            // active, table 0, funcidx vector
            0 => {
                let offset_expr = super::read_init_expr(payload, &mut pos)?;
                let func_indices = read_u32_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index: 0,
                        offset_expr,
                    },
                    reftype: RefType::FuncRef,
                    init: ElementInit::FuncIndices(func_indices),
                }
            }
            // passive, elemkind + funcidx vector
            1 => {
                let reftype = read_elemkind(payload, &mut pos)?;
                let func_indices = read_u32_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Passive,
                    reftype,
                    init: ElementInit::FuncIndices(func_indices),
                }
            }
            // active, explicit table, elemkind + funcidx vector
            2 => {
                let (table_index, n) = decode_leb128_u32(payload, pos)?;
                pos += n;
                let offset_expr = super::read_init_expr(payload, &mut pos)?;
                let reftype = read_elemkind(payload, &mut pos)?;
                let func_indices = read_u32_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index,
                        offset_expr,
                    },
                    reftype,
                    init: ElementInit::FuncIndices(func_indices),
                }
            }
            // declarative, elemkind + funcidx vector
            3 => {
                let reftype = read_elemkind(payload, &mut pos)?;
                let func_indices = read_u32_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Declarative,
                    reftype,
                    init: ElementInit::FuncIndices(func_indices),
                }
            }
            // active, table 0, expr vector (funcref implied)
            4 => {
                let offset_expr = super::read_init_expr(payload, &mut pos)?;
                let exprs = read_expr_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index: 0,
                        offset_expr,
                    },
                    reftype: RefType::FuncRef,
                    init: ElementInit::Exprs(exprs),
                }
            }
            // passive, reftype + expr vector
            5 => {
                let reftype = read_reftype(payload, &mut pos)?;
                let exprs = read_expr_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Passive,
                    reftype,
                    init: ElementInit::Exprs(exprs),
                }
            }
            // active, explicit table, reftype + expr vector
            6 => {
                let (table_index, n) = decode_leb128_u32(payload, pos)?;
                pos += n;
                let offset_expr = super::read_init_expr(payload, &mut pos)?;
                let reftype = read_reftype(payload, &mut pos)?;
                let exprs = read_expr_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index,
                        offset_expr,
                    },
                    reftype,
                    init: ElementInit::Exprs(exprs),
                }
            }
            // declarative, reftype + expr vector
            7 => {
                let reftype = read_reftype(payload, &mut pos)?;
                let exprs = read_expr_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Declarative,
                    reftype,
                    init: ElementInit::Exprs(exprs),
                }
            }
            // flags ≥ 8 are undefined; reject rather than silently misread.
            _ => return Err(ParseError::UnsupportedElementFlag(flag as u8)),
        };
        segments.push(seg);
    }
    Ok(segments)
}

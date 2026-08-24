use crate::parser::{decode_leb128_i32, decode_leb128_i64, decode_leb128_u32, ParseError};
use std::fmt;

use super::ValType;

/// The type of a global variable: its value type and whether it is mutable.
#[derive(Debug, PartialEq, Clone)]
pub struct GlobalType {
    pub valtype: ValType,
    pub mutable: bool,
}

impl fmt::Display for GlobalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.valtype,
            if self.mutable { " mut" } else { "" }
        )
    }
}

/// A global variable: its type plus its typed initializer expression.
#[derive(Debug, PartialEq, Clone)]
pub struct Global {
    pub global_type: GlobalType,
    pub init: ConstExpr,
}

impl fmt::Display for Global {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.global_type, self.init)
    }
}

/// A decoded constant initializer expression value. Covers the constant
/// instructions valid in a global / element / data init_expr.
#[derive(Debug, PartialEq, Clone)]
pub enum ConstExpr {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    GlobalGet(u32),
}

impl fmt::Display for ConstExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstExpr::I32(v) => write!(f, "{}", v),
            ConstExpr::I64(v) => write!(f, "{}", v),
            ConstExpr::F32(v) => write!(f, "{}", v),
            ConstExpr::F64(v) => write!(f, "{}", v),
            ConstExpr::GlobalGet(i) => write!(f, "global.get {}", i),
        }
    }
}

/// Interprets the raw bytes of an init_expr (as produced by `read_init_expr`,
/// i.e. a single const instruction followed by `0x0B`) into a typed value.
pub fn parse_const_expr(bytes: &[u8]) -> Result<ConstExpr, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::UnexpectedEof);
    }
    let op = bytes[0];
    let mut pos = 1;

    let expr = match op {
        0x41 => {
            let (v, n) = decode_leb128_i32(bytes, pos)?;
            pos += n;
            ConstExpr::I32(v)
        }
        0x42 => {
            let (v, n) = decode_leb128_i64(bytes, pos)?;
            pos += n;
            ConstExpr::I64(v)
        }
        0x43 => {
            if pos + 4 > bytes.len() {
                return Err(ParseError::UnexpectedEof);
            }
            let v =
                f32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            pos += 4;
            ConstExpr::F32(v)
        }
        0x44 => {
            if pos + 8 > bytes.len() {
                return Err(ParseError::UnexpectedEof);
            }
            let v = f64::from_le_bytes([
                bytes[pos],
                bytes[pos + 1],
                bytes[pos + 2],
                bytes[pos + 3],
                bytes[pos + 4],
                bytes[pos + 5],
                bytes[pos + 6],
                bytes[pos + 7],
            ]);
            pos += 8;
            ConstExpr::F64(v)
        }
        0x23 => {
            let (v, n) = decode_leb128_u32(bytes, pos)?;
            pos += n;
            ConstExpr::GlobalGet(v)
        }
        _ => return Err(ParseError::UnsupportedInitExpr(op)),
    };

    // A single-value const expr must be terminated immediately by `end` (0x0B).
    match bytes.get(pos) {
        Some(0x0B) => Ok(expr),
        Some(&other) => Err(ParseError::UnsupportedInitExpr(other)),
        None => Err(ParseError::UnexpectedEof),
    }
}

pub fn decode_global_section(payload: &[u8]) -> Result<Vec<Global>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut globals = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let global_type = super::read_global_type(payload, &mut pos)?;
        let init_bytes = super::read_init_expr(payload, &mut pos)?;
        let init = parse_const_expr(&init_bytes)?;
        globals.push(Global { global_type, init });
    }
    Ok(globals)
}

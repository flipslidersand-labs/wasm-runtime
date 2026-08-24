use crate::parser::{decode_leb128_u32, ParseError};
use std::fmt;

use super::ValType;

/// A single local variable declaration: `count` locals of the same `valtype`.
#[derive(Debug, PartialEq, Clone)]
pub struct LocalDecl {
    pub count: u32,
    pub valtype: ValType,
}

/// The body of a single function: its local declarations and raw expression bytes.
#[derive(Debug, PartialEq, Clone)]
pub struct FuncBody {
    pub locals: Vec<LocalDecl>,
    pub expr: Vec<u8>,
}

impl fmt::Display for FuncBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let locals = self
            .locals
            .iter()
            .map(|l| format!("{} {}", l.count, l.valtype))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "locals=[{}] expr={} bytes", locals, self.expr.len())
    }
}

pub fn decode_code_section(payload: &[u8]) -> Result<Vec<FuncBody>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut bodies = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (size, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        // The body occupies exactly `size` bytes starting here.
        let body_end = pos + size as usize;
        if body_end > payload.len() {
            return Err(ParseError::SizeMismatch);
        }

        let (local_count, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        let mut locals = Vec::with_capacity(local_count as usize);
        for _ in 0..local_count {
            let (cnt, n) = decode_leb128_u32(payload, pos)?;
            pos += n;
            if pos >= body_end {
                return Err(ParseError::SizeMismatch);
            }
            let valtype = ValType::try_from(payload[pos])?;
            pos += 1;
            locals.push(LocalDecl {
                count: cnt,
                valtype,
            });
        }

        // Whatever remains inside the body is the expression byte stream.
        if pos > body_end {
            return Err(ParseError::SizeMismatch);
        }
        let expr = payload[pos..body_end].to_vec();
        pos = body_end;

        bodies.push(FuncBody { locals, expr });
    }

    Ok(bodies)
}

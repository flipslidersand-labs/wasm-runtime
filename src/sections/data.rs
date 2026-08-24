use crate::parser::{decode_leb128_u32, ParseError};
use std::fmt;

use super::global::{parse_const_expr, ConstExpr};

/// How a data segment is placed into memory.
#[derive(Debug, PartialEq, Clone)]
pub enum DataMode {
    /// Copied into `memory[memory_index]` at `offset` during instantiation.
    Active {
        memory_index: u32,
        offset: ConstExpr,
    },
    /// Not copied automatically; referenced by `memory.init`.
    Passive,
}

/// A data segment: its placement mode and raw initializer bytes.
#[derive(Debug, PartialEq, Clone)]
pub struct DataSegment {
    pub mode: DataMode,
    pub bytes: Vec<u8>,
}

impl fmt::Display for DataSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.mode {
            DataMode::Active {
                memory_index,
                offset,
            } => write!(
                f,
                "active mem[{}] offset={} data={} bytes",
                memory_index,
                offset,
                self.bytes.len()
            ),
            DataMode::Passive => write!(f, "passive data={} bytes", self.bytes.len()),
        }
    }
}

pub fn decode_data_section(payload: &[u8]) -> Result<Vec<DataSegment>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut segments = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (flag, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        let mode = match flag {
            // active, memory 0, offset follows
            0 => {
                let ob = super::read_init_expr(payload, &mut pos)?;
                DataMode::Active {
                    memory_index: 0,
                    offset: parse_const_expr(&ob)?,
                }
            }
            // passive
            1 => DataMode::Passive,
            // active, explicit memory index, offset follows
            2 => {
                let (memory_index, n) = decode_leb128_u32(payload, pos)?;
                pos += n;
                let ob = super::read_init_expr(payload, &mut pos)?;
                DataMode::Active {
                    memory_index,
                    offset: parse_const_expr(&ob)?,
                }
            }
            _ => return Err(ParseError::UnsupportedDataFlag(flag as u8)),
        };

        let bytes = super::read_byte_vec(payload, &mut pos)?;
        segments.push(DataSegment { mode, bytes });
    }
    Ok(segments)
}

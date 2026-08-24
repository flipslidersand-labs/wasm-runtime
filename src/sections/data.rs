use crate::parser::{decode_leb128_u32, ParseError};
use std::fmt;

/// How a data segment is placed into memory.
#[derive(Debug, PartialEq, Clone)]
pub enum DataMode {
    /// Copied into `memory[memory_index]` at `offset_expr` during instantiation.
    /// `offset_expr` holds the raw const-expr bytes (including the trailing `0x0B`).
    Active {
        memory_index: u32,
        offset_expr: Vec<u8>,
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
                offset_expr,
            } => write!(
                f,
                "active mem[{}] offset={} data={} bytes",
                memory_index,
                super::fmt_init_expr(offset_expr),
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
            // active, memory 0, offset_expr follows
            0 => {
                let offset_expr = super::read_init_expr(payload, &mut pos)?;
                DataMode::Active {
                    memory_index: 0,
                    offset_expr,
                }
            }
            // passive
            1 => DataMode::Passive,
            // active, explicit memory index, offset_expr follows
            2 => {
                let (memory_index, n) = decode_leb128_u32(payload, pos)?;
                pos += n;
                let offset_expr = super::read_init_expr(payload, &mut pos)?;
                DataMode::Active {
                    memory_index,
                    offset_expr,
                }
            }
            _ => return Err(ParseError::UnsupportedDataFlag(flag as u8)),
        };

        let bytes = super::read_byte_vec(payload, &mut pos)?;
        segments.push(DataSegment { mode, bytes });
    }
    Ok(segments)
}

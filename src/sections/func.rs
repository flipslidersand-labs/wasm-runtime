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

/// What [`scan_expr`] learned about a body beyond its structure.
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub(crate) struct BodyInfo {
    /// The body uses `memory.init` or `data.drop`, which need a datacount section.
    pub uses_data_index: bool,
}

/// Skips one LEB128 value of at most `max` bytes starting at `pos`; returns the
/// position after it. Does not check the value's range.
fn skip_leb(expr: &[u8], pos: usize, max: usize) -> Result<usize, ParseError> {
    for i in 0..max {
        match expr.get(pos + i) {
            Some(b) if b & 0x80 == 0 => return Ok(pos + i + 1),
            Some(_) => {}
            None => return Err(ParseError::UnexpectedEof),
        }
    }
    Err(ParseError::Leb128Overflow)
}

/// Skips `n` fixed bytes.
fn skip_bytes(expr: &[u8], pos: usize, n: usize) -> Result<usize, ParseError> {
    match pos.checked_add(n) {
        Some(end) if end <= expr.len() => Ok(end),
        _ => Err(ParseError::UnexpectedEof),
    }
}

/// Walks a function body as an instruction stream: only each instruction's
/// immediate length and the block structure are read. Nothing is executed or
/// type-checked. Rejects an unknown opcode, a truncated immediate, a body not
/// closed by its final `end`, and bytes after that `end`.
///
/// Known opcodes: the MVP set, sign-extension, reference-type basics, and the
/// `0xFC` prefix (saturating truncation, bulk memory, table ops).
pub(crate) fn scan_expr(expr: &[u8]) -> Result<BodyInfo, ParseError> {
    let mut info = BodyInfo::default();
    let mut pos = 0;
    // The function body itself is the outermost block.
    let mut depth: usize = 1;
    while let Some(&op) = expr.get(pos) {
        pos += 1;
        match op {
            // unreachable, nop, else, return, drop, select, numeric (no immediate)
            0x00 | 0x01 | 0x05 | 0x0F | 0x1A | 0x1B | 0x45..=0xC4 | 0xD1 => {}
            // block, loop, if: blocktype (0x40, a valtype, or an s33 type index)
            0x02..=0x04 => {
                pos = skip_leb(expr, pos, 5)?;
                depth += 1;
            }
            0x0B => {
                depth -= 1;
                if depth == 0 {
                    return if pos == expr.len() {
                        Ok(info)
                    } else {
                        Err(ParseError::SizeMismatch)
                    };
                }
            }
            // br, br_if, call, local.*, global.*, table.get/set, ref.func,
            // memory.size/grow (reserved byte)
            0x0C | 0x0D | 0x10 | 0x20..=0x26 | 0x3F | 0x40 | 0xD2 => {
                pos = skip_leb(expr, pos, 5)?;
            }
            0x0E => {
                let (count, n) = decode_leb128_u32(expr, pos)?;
                pos += n;
                // Every target takes at least one byte; this bounds the loop.
                if count as usize >= expr.len() - pos.min(expr.len()) {
                    return Err(ParseError::UnexpectedEof);
                }
                for _ in 0..=count {
                    pos = skip_leb(expr, pos, 5)?;
                }
            }
            // call_indirect: type index, table index
            0x11 => {
                pos = skip_leb(expr, pos, 5)?;
                pos = skip_leb(expr, pos, 5)?;
            }
            // select t*: a vector of value types
            0x1C => {
                let (count, n) = decode_leb128_u32(expr, pos)?;
                pos = skip_bytes(expr, pos + n, count as usize)?;
            }
            // loads and stores: memarg (align, offset)
            0x28..=0x3E => {
                pos = skip_leb(expr, pos, 5)?;
                pos = skip_leb(expr, pos, 5)?;
            }
            0x41 => pos = skip_leb(expr, pos, 5)?,
            0x42 => pos = skip_leb(expr, pos, 10)?,
            0x43 => pos = skip_bytes(expr, pos, 4)?,
            0x44 => pos = skip_bytes(expr, pos, 8)?,
            // ref.null: a reference type byte
            0xD0 => pos = skip_bytes(expr, pos, 1)?,
            0xFC => {
                let (sub, n) = decode_leb128_u32(expr, pos)?;
                pos += n;
                let idx_count = match sub {
                    0..=7 => 0,
                    8 => {
                        info.uses_data_index = true;
                        2 // memory.init: data index, memory index
                    }
                    9 => {
                        info.uses_data_index = true;
                        1 // data.drop
                    }
                    10 | 12 | 14 => 2,      // memory.copy, table.init, table.copy
                    11 | 13 | 15..=17 => 1, // memory.fill, elem.drop, table.grow/size/fill
                    _ => return Err(ParseError::UnknownOpcode(0xFC)),
                };
                for _ in 0..idx_count {
                    pos = skip_leb(expr, pos, 5)?;
                }
            }
            other => return Err(ParseError::UnknownOpcode(other)),
        }
    }
    Err(ParseError::MissingEnd)
}

pub fn decode_code_section(payload: &[u8]) -> Result<Vec<FuncBody>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut bodies = Vec::with_capacity(super::capped_capacity(count, payload.len()));
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

        let mut locals = Vec::with_capacity(super::capped_capacity(local_count, payload.len()));
        let mut total_locals: u64 = 0;
        for _ in 0..local_count {
            let (cnt, n) = decode_leb128_u32(payload, pos)?;
            pos += n;
            total_locals += u64::from(cnt);
            if total_locals > u64::from(u32::MAX) {
                return Err(ParseError::TooManyLocals);
            }
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
        scan_expr(&expr)?;
        pos = body_end;

        bodies.push(FuncBody { locals, expr });
    }

    super::ensure_consumed(payload, pos)?;
    Ok(bodies)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(expr: &[u8]) -> Result<BodyInfo, ParseError> {
        scan_expr(expr)
    }

    #[test]
    fn accepts_minimal_and_simple_bodies() {
        assert_eq!(scan(&[0x0B]), Ok(BodyInfo::default()));
        // i32.const 42; drop; end
        assert_eq!(scan(&[0x41, 0x2A, 0x1A, 0x0B]), Ok(BodyInfo::default()));
        // i64.const with a multi-byte LEB, f32.const, f64.const
        assert!(scan(&[0x42, 0x80, 0x01, 0x1A, 0x43, 0, 0, 0, 0, 0x1A, 0x0B]).is_ok());
        assert!(scan(&[0x44, 0, 0, 0, 0, 0, 0, 0, 0, 0x1A, 0x0B]).is_ok());
    }

    #[test]
    fn blocktype_may_be_a_type_index_not_an_end() {
        // block (type 11) end end: the 0x0B after 0x02 is the blocktype, not `end`.
        assert_eq!(scan(&[0x02, 0x0B, 0x0B, 0x0B]), Ok(BodyInfo::default()));
        // The same bytes with one `end` too few are not closed (spec binary.101).
        assert_eq!(scan(&[0x02, 0x0B, 0x0B]), Err(ParseError::MissingEnd));
        // block with the empty blocktype and with a value type
        assert!(scan(&[0x02, 0x40, 0x0B, 0x0B]).is_ok());
        assert!(scan(&[0x03, 0x7F, 0x41, 0x00, 0x0B, 0x1A, 0x0B]).is_ok());
    }

    #[test]
    fn walks_memarg_br_table_and_call_indirect() {
        // i32.const 0; i32.load align=2 offset=0; drop
        assert!(scan(&[0x41, 0x00, 0x28, 0x02, 0x00, 0x1A, 0x0B]).is_ok());
        // br_table with 2 targets and a default
        assert!(scan(&[0x41, 0x00, 0x0E, 0x02, 0x00, 0x00, 0x00, 0x0B]).is_ok());
        // call_indirect type 0 table 0
        assert!(scan(&[0x41, 0x00, 0x11, 0x00, 0x00, 0x0B]).is_ok());
    }

    #[test]
    fn prefix_fc_operations_are_walked_and_data_use_is_reported() {
        // i32.trunc_sat_f32_s
        assert_eq!(scan(&[0xFC, 0x00, 0x0B]), Ok(BodyInfo::default()));
        // memory.copy, memory.fill, table.copy
        assert!(scan(&[0xFC, 0x0A, 0x00, 0x00, 0xFC, 0x0B, 0x00, 0xFC, 0x0E, 0, 0, 0x0B]).is_ok());
        // memory.init data 0 mem 0, data.drop 0
        let info = scan(&[0xFC, 0x08, 0x00, 0x00, 0x0B]).unwrap();
        assert!(info.uses_data_index);
        assert!(scan(&[0xFC, 0x09, 0x00, 0x0B]).unwrap().uses_data_index);
        // an unassigned 0xFC sub-opcode
        assert_eq!(
            scan(&[0xFC, 0x63, 0x0B]),
            Err(ParseError::UnknownOpcode(0xFC))
        );
    }

    #[test]
    fn rejects_bodies_that_are_not_closed_by_end() {
        assert_eq!(scan(&[]), Err(ParseError::MissingEnd));
        // i32.const 1; drop; (no end) (spec binary.37 / 38 / 39)
        assert_eq!(scan(&[0x41, 0x01, 0x1A]), Err(ParseError::MissingEnd));
        // a nested block still open at the end of the body
        assert_eq!(scan(&[0x02, 0x40, 0x0B]), Err(ParseError::MissingEnd));
        // bytes after the final end
        assert_eq!(scan(&[0x0B, 0x00]), Err(ParseError::SizeMismatch));
    }

    #[test]
    fn rejects_unknown_opcodes_and_truncated_immediates() {
        // spec binary.126
        assert_eq!(scan(&[0xFF, 0x0B]), Err(ParseError::UnknownOpcode(0xFF)));
        // SIMD prefix is not supported
        assert_eq!(
            scan(&[0xFD, 0x00, 0x0B]),
            Err(ParseError::UnknownOpcode(0xFD))
        );
        // i32.const with no operand; f64.const cut short
        assert_eq!(scan(&[0x41]), Err(ParseError::UnexpectedEof));
        assert_eq!(scan(&[0x44, 0, 0, 0]), Err(ParseError::UnexpectedEof));
        // an unterminated LEB128 immediate
        assert_eq!(
            scan(&[0x41, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x0B]),
            Err(ParseError::Leb128Overflow)
        );
    }

    #[test]
    fn huge_br_table_count_fails_fast() {
        let r = scan(&[0x0E, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x0B]);
        assert_eq!(r, Err(ParseError::UnexpectedEof));
    }

    #[test]
    fn never_panics_on_arbitrary_bytes() {
        // Every first byte x every second byte, with a tail of zeros and of 0xFF.
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let _ = scan(&[a, b]);
                let _ = scan(&[a, b, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                let _ = scan(&[
                    a, b, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ]);
            }
        }
    }

    // ── scan_expr: immediates of previously untested opcodes ──────────────

    #[test]
    fn select_t_skips_exactly_count_type_bytes() {
        // select t* with 0, 1 and 2 value types
        assert!(scan(&[0x1C, 0x00, 0x0B]).is_ok());
        assert!(scan(&[0x1C, 0x01, 0x7F, 0x0B]).is_ok());
        assert!(scan(&[0x1C, 0x02, 0x7F, 0x7E, 0x0B]).is_ok());
        // count 1 swallows the 0x0B, so the body is left without an end
        assert_eq!(scan(&[0x1C, 0x01, 0x0B]), Err(ParseError::MissingEnd));
    }

    #[test]
    fn select_t_count_beyond_remaining_bytes_is_unexpected_eof() {
        assert_eq!(
            scan(&[0x1C, 0x05, 0x7F, 0x0B]),
            Err(ParseError::UnexpectedEof)
        );
        // the vec length itself is missing
        assert_eq!(scan(&[0x1C]), Err(ParseError::UnexpectedEof));
        // a huge count is an EOF
        assert_eq!(
            scan(&[0x1C, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x0B]),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn ref_null_is_null_and_func_immediates() {
        // ref.null funcref; ref.is_null; drop
        assert!(scan(&[0xD0, 0x70, 0xD1, 0x1A, 0x0B]).is_ok());
        // the reftype byte is skipped even if it looks like `end`
        assert_eq!(scan(&[0xD0, 0x0B]), Err(ParseError::MissingEnd));
        assert_eq!(scan(&[0xD0]), Err(ParseError::UnexpectedEof));
        // ref.is_null takes no immediate
        assert!(scan(&[0xD1, 0x0B]).is_ok());
        // ref.func with a one-byte function index
        assert!(scan(&[0xD2, 0x00, 0x1A, 0x0B]).is_ok());
        assert_eq!(scan(&[0xD2]), Err(ParseError::UnexpectedEof));
        assert_eq!(
            scan(&[0xD2, 0x80, 0x80, 0x80, 0x80, 0x80, 0x0B]),
            Err(ParseError::Leb128Overflow)
        );
    }

    #[test]
    fn table_get_set_and_memory_size_grow_take_one_index() {
        for op in [0x25u8, 0x26, 0x3F, 0x40] {
            assert!(scan(&[op, 0x00, 0x0B]).is_ok(), "op {op:#x}");
            // the index swallows the 0x0B
            assert_eq!(scan(&[op, 0x0B]), Err(ParseError::MissingEnd), "op {op:#x}");
            assert_eq!(scan(&[op]), Err(ParseError::UnexpectedEof), "op {op:#x}");
        }
    }

    #[test]
    fn table_get_set_read_a_multi_byte_index() {
        for op in [0x25u8, 0x26] {
            // index 0x80 0x06 is read whole; 0x06 is not seen as an opcode
            assert!(scan(&[op, 0x80, 0x06, 0x0B]).is_ok(), "op {op:#x}");
            assert_eq!(
                scan(&[op, 0x80, 0x80, 0x80, 0x80, 0x80, 0x0B]),
                Err(ParseError::Leb128Overflow),
                "op {op:#x}"
            );
        }
    }

    #[test]
    fn prefix_fc_single_index_operations() {
        // memory.fill, elem.drop, table.grow, table.size, table.fill
        for sub in [11u8, 13, 15, 16, 17] {
            let info = scan(&[0xFC, sub, 0x00, 0x0B]).unwrap();
            assert!(!info.uses_data_index, "sub {sub}");
            // exactly one index: it swallows the 0x0B
            assert_eq!(
                scan(&[0xFC, sub, 0x0B]),
                Err(ParseError::MissingEnd),
                "sub {sub}"
            );
            assert_eq!(
                scan(&[0xFC, sub]),
                Err(ParseError::UnexpectedEof),
                "sub {sub}"
            );
        }
    }

    #[test]
    fn prefix_fc_two_index_operations() {
        // memory.copy, table.init, table.copy
        for sub in [10u8, 12, 14] {
            assert!(scan(&[0xFC, sub, 0x00, 0x00, 0x0B]).is_ok(), "sub {sub}");
            assert_eq!(
                scan(&[0xFC, sub, 0x00, 0x0B]),
                Err(ParseError::MissingEnd),
                "sub {sub}"
            );
        }
    }

    #[test]
    fn prefix_fc_sub_opcode_18_is_rejected() {
        assert_eq!(
            scan(&[0xFC, 18, 0x00, 0x0B]),
            Err(ParseError::UnknownOpcode(0xFC))
        );
        // the sub-opcode is a LEB128: a missing one is an EOF, not unknown
        assert_eq!(scan(&[0xFC]), Err(ParseError::UnexpectedEof));
    }

    #[test]
    fn i64_const_accepts_ten_byte_leb_and_rejects_eleven() {
        // 9 continuation bytes + terminator = 10 bytes
        let mut ok = vec![0x42];
        ok.extend_from_slice(&[0x80; 9]);
        ok.extend_from_slice(&[0x00, 0x1A, 0x0B]);
        assert!(scan(&ok).is_ok());

        // 10 continuation bytes + terminator = 11 bytes
        let mut bad = vec![0x42];
        bad.extend_from_slice(&[0x80; 10]);
        bad.extend_from_slice(&[0x00, 0x1A, 0x0B]);
        assert_eq!(scan(&bad), Err(ParseError::Leb128Overflow));

        // ran out of bytes while still inside the 10-byte window
        let mut short = vec![0x42];
        short.extend_from_slice(&[0x80; 9]);
        assert_eq!(scan(&short), Err(ParseError::UnexpectedEof));
    }

    #[test]
    fn if_else_end_depth_is_tracked() {
        // i32.const 0; if (empty) else end; end
        assert!(scan(&[0x41, 0x00, 0x04, 0x40, 0x05, 0x0B, 0x0B]).is_ok());
        // else does not open a block: the if is still open after one end
        assert_eq!(scan(&[0x04, 0x40, 0x05, 0x0B]), Err(ParseError::MissingEnd));
        // else does not close a block either: the final end is the body's
        assert_eq!(
            scan(&[0x04, 0x40, 0x05, 0x0B, 0x0B, 0x00]),
            Err(ParseError::SizeMismatch)
        );
        // nested if/else inside both arms
        assert!(scan(&[
            0x04, 0x40, 0x04, 0x40, 0x05, 0x0B, 0x05, 0x04, 0x40, 0x05, 0x0B, 0x0B, 0x0B
        ])
        .is_ok());
        // the same nesting with one end too few
        assert_eq!(
            scan(&[0x04, 0x40, 0x04, 0x40, 0x05, 0x0B, 0x05, 0x04, 0x40, 0x05, 0x0B, 0x0B]),
            Err(ParseError::MissingEnd)
        );
    }

    // ── decode_code_section ───────────────────────────────────────────────

    #[test]
    fn code_section_rejects_total_locals_over_u32() {
        // 0xFFFFFFFF i32 + 2 i64 (spec binary.43)
        let payload = [
            0x01, 0x0A, 0x02, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x7F, 0x02, 0x7E, 0x0B,
        ];
        assert_eq!(
            decode_code_section(&payload),
            Err(ParseError::TooManyLocals)
        );
        // four groups of 2^30 (spec binary.44)
        let mut body = vec![0x04];
        for t in [0x7F, 0x7E, 0x7D, 0x7C] {
            body.extend_from_slice(&[0x80, 0x80, 0x80, 0x80, 0x04, t]);
        }
        body.push(0x0B);
        let mut payload = vec![0x01, body.len() as u8];
        payload.extend_from_slice(&body);
        assert_eq!(
            decode_code_section(&payload),
            Err(ParseError::TooManyLocals)
        );
    }

    #[test]
    fn code_section_accepts_the_largest_legal_locals_total() {
        // 0xFFFFFFFF locals in one group is the maximum and stays legal.
        let payload = [0x01, 0x08, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x7F, 0x0B];
        assert!(decode_code_section(&payload).is_ok());
    }

    #[test]
    fn code_section_rejects_a_body_without_end() {
        // body size 4: no locals, i32.const 1, drop (no end)
        let payload = [0x01, 0x04, 0x00, 0x41, 0x01, 0x1A];
        assert_eq!(decode_code_section(&payload), Err(ParseError::MissingEnd));
    }
}

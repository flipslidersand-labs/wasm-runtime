use crate::parser::{decode_leb128_i32, decode_leb128_i64, decode_leb128_u32, ParseError};
use std::fmt;

// ── Value types ──────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
}

impl TryFrom<u8> for ValType {
    type Error = ParseError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            0x7F => Ok(ValType::I32),
            0x7E => Ok(ValType::I64),
            0x7D => Ok(ValType::F32),
            0x7C => Ok(ValType::F64),
            _ => Err(ParseError::UnknownValType(byte)),
        }
    }
}

// ── Type section (id = 1) ────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

pub fn decode_type_section(payload: &[u8]) -> Result<Vec<FuncType>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut types = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        if payload[pos] != 0x60 {
            return Err(ParseError::InvalidFuncType(payload[pos]));
        }
        pos += 1;

        let params = read_val_type_vec(payload, &mut pos)?;
        let results = read_val_type_vec(payload, &mut pos)?;
        types.push(FuncType { params, results });
    }
    Ok(types)
}

fn read_val_type_vec(payload: &[u8], pos: &mut usize) -> Result<Vec<ValType>, ParseError> {
    let (count, n) = decode_leb128_u32(payload, *pos)?;
    *pos += n;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if *pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        out.push(ValType::try_from(payload[*pos])?);
        *pos += 1;
    }
    Ok(out)
}

// ── Import section (id = 2) ──────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RefType {
    FuncRef,
    ExternRef,
}

impl TryFrom<u8> for RefType {
    type Error = ParseError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            0x70 => Ok(RefType::FuncRef),
            0x6F => Ok(RefType::ExternRef),
            _ => Err(ParseError::UnknownRefType(byte)),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

fn read_limits(payload: &[u8], pos: &mut usize) -> Result<Limits, ParseError> {
    if *pos >= payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let flag = payload[*pos];
    *pos += 1;
    let (min, n) = decode_leb128_u32(payload, *pos)?;
    *pos += n;
    let max = match flag {
        0x00 => None,
        0x01 => {
            let (max, n) = decode_leb128_u32(payload, *pos)?;
            *pos += n;
            Some(max)
        }
        _ => return Err(ParseError::UnknownLimitsFlag(flag)),
    };
    Ok(Limits { min, max })
}

#[derive(Debug, PartialEq, Clone)]
pub enum ImportDesc {
    Func(u32),
    Table { reftype: RefType, limits: Limits },
    Memory(Limits),
    Global { valtype: ValType, mutable: bool },
}

#[derive(Debug, PartialEq, Clone)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub desc: ImportDesc,
}

fn read_name(payload: &[u8], pos: &mut usize) -> Result<String, ParseError> {
    let (len, n) = decode_leb128_u32(payload, *pos)?;
    *pos += n;
    let end = *pos + len as usize;
    if end > payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let s = std::str::from_utf8(&payload[*pos..end])
        .map_err(|_| ParseError::InvalidUtf8)?
        .to_string();
    *pos = end;
    Ok(s)
}

pub fn decode_import_section(payload: &[u8]) -> Result<Vec<Import>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut imports = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let module = read_name(payload, &mut pos)?;
        let name = read_name(payload, &mut pos)?;

        if pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let kind = payload[pos];
        pos += 1;

        let desc = match kind {
            0x00 => {
                let (idx, n) = decode_leb128_u32(payload, pos)?;
                pos += n;
                ImportDesc::Func(idx)
            }
            0x01 => {
                if pos >= payload.len() {
                    return Err(ParseError::UnexpectedEof);
                }
                let reftype = RefType::try_from(payload[pos])?;
                pos += 1;
                let limits = read_limits(payload, &mut pos)?;
                ImportDesc::Table { reftype, limits }
            }
            0x02 => {
                let limits = read_limits(payload, &mut pos)?;
                ImportDesc::Memory(limits)
            }
            0x03 => {
                let GlobalType { valtype, mutable } = read_global_type(payload, &mut pos)?;
                ImportDesc::Global { valtype, mutable }
            }
            _ => return Err(ParseError::UnknownImportKind(kind)),
        };

        imports.push(Import { module, name, desc });
    }
    Ok(imports)
}

// ── Custom section (id = 0) ───────────────────────────────────────────────────

/// A custom section: a name followed by an opaque payload. Custom sections
/// carry tooling metadata (e.g. the `name` section, `producers`) and impose no
/// consistency constraints on the module. Multiple may appear in one module.
#[derive(Debug, PartialEq, Clone)]
pub struct CustomSection {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Decodes a custom section: a name string followed by the remaining bytes as an
/// opaque payload (whose meaning depends on `name`). The `name` section body is
/// left undecoded here.
pub fn decode_custom_section(payload: &[u8]) -> Result<CustomSection, ParseError> {
    let mut pos = 0;
    let name = read_name(payload, &mut pos)?;
    let bytes = payload[pos..].to_vec();
    Ok(CustomSection { name, bytes })
}

// ── Name section (custom name == "name") ─────────────────────────────────────

/// Decoded contents of the wasm name section (a custom section whose `name`
/// field is `"name"`). Only subsections 0 (module name) and 1 (function names)
/// are decoded; all others are silently skipped.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct NameSection {
    /// Subsection 0: the module name, if present.
    pub module: Option<String>,
    /// Subsection 1: `(function_index, name)` pairs from the function namemap.
    pub functions: Vec<(u32, String)>,
}

/// Decodes the payload bytes of a `"name"` custom section into a [`NameSection`].
///
/// The payload is a sequence of subsections, each `id:byte size:u32 payload[size]`.
/// Unknown subsection ids are skipped by advancing `size` bytes.
pub fn decode_name_section(bytes: &[u8]) -> Result<NameSection, ParseError> {
    let mut pos = 0;
    let mut ns = NameSection::default();

    while pos < bytes.len() {
        let id = bytes[pos];
        pos += 1;

        let (size, n) = decode_leb128_u32(bytes, pos)?;
        pos += n;

        let sub_end = pos
            .checked_add(size as usize)
            .ok_or(ParseError::UnexpectedEof)?;
        if sub_end > bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }

        let sub = &bytes[pos..sub_end];

        match id {
            // subsection 0: module name — a single namestring
            0 => {
                let mut p = 0;
                ns.module = Some(read_name(sub, &mut p)?);
            }
            // subsection 1: function names — vec(idx:u32 name:namestring)
            1 => {
                let mut p = 0;
                let (count, n) = decode_leb128_u32(sub, p)?;
                p += n;
                for _ in 0..count {
                    let (idx, n) = decode_leb128_u32(sub, p)?;
                    p += n;
                    let name = read_name(sub, &mut p)?;
                    ns.functions.push((idx, name));
                }
            }
            // all other subsection ids (e.g. 2 = local names): skip
            _ => {}
        }

        pos = sub_end;
    }

    Ok(ns)
}

// ── Memory section (id = 5) ──────────────────────────────────────────────────

pub fn decode_memory_section(payload: &[u8]) -> Result<Vec<Limits>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut memories = Vec::with_capacity(count as usize);
    for _ in 0..count {
        memories.push(read_limits(payload, &mut pos)?);
    }
    Ok(memories)
}

// ── Table section (id = 4) ────────────────────────────────────────────────────

/// A table definition: its element reference type and size limits.
#[derive(Debug, PartialEq, Clone)]
pub struct Table {
    pub reftype: RefType,
    pub limits: Limits,
}

pub fn decode_table_section(payload: &[u8]) -> Result<Vec<Table>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut tables = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let reftype = RefType::try_from(payload[pos])?;
        pos += 1;
        let limits = read_limits(payload, &mut pos)?;
        tables.push(Table { reftype, limits });
    }
    Ok(tables)
}

// ── Function section (id = 3) ─────────────────────────────────────────────────

pub fn decode_function_section(payload: &[u8]) -> Result<Vec<u32>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut indices = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (idx, n) = decode_leb128_u32(payload, pos)?;
        pos += n;
        indices.push(idx);
    }
    Ok(indices)
}

// ── Start section (id = 8) ────────────────────────────────────────────────────

/// Decodes the start section: a single function index naming the module's entry
/// point. The payload must contain exactly that index and no trailing bytes.
pub fn decode_start_section(payload: &[u8]) -> Result<u32, ParseError> {
    let (func_index, n) = decode_leb128_u32(payload, 0)?;
    if n != payload.len() {
        return Err(ParseError::SizeMismatch);
    }
    Ok(func_index)
}

// ── DataCount section (id = 12) ───────────────────────────────────────────────

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

// ── Export section (id = 7) ───────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ExportKind {
    Func,
    Table,
    Memory,
    Global,
}

impl TryFrom<u8> for ExportKind {
    type Error = ParseError;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            0x00 => Ok(ExportKind::Func),
            0x01 => Ok(ExportKind::Table),
            0x02 => Ok(ExportKind::Memory),
            0x03 => Ok(ExportKind::Global),
            _ => Err(ParseError::UnknownExportKind(byte)),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

pub fn decode_export_section(payload: &[u8]) -> Result<Vec<Export>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut exports = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let (name_len, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        let end = pos + name_len as usize;
        if end > payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let name = std::str::from_utf8(&payload[pos..end])
            .map_err(|_| ParseError::InvalidUtf8)?
            .to_string();
        pos = end;

        if pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let kind = ExportKind::try_from(payload[pos])?;
        pos += 1;

        let (index, n) = decode_leb128_u32(payload, pos)?;
        pos += n;

        exports.push(Export { name, kind, index });
    }
    Ok(exports)
}

// ── Code section (id = 10) ────────────────────────────────────────────────────

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

// ── Global section (id = 6) ──────────────────────────────────────────────────

/// The type of a global variable: its value type and whether it is mutable.
#[derive(Debug, PartialEq, Clone)]
pub struct GlobalType {
    pub valtype: ValType,
    pub mutable: bool,
}

/// A global variable: its type plus the raw bytes of its initializer expression
/// (including the terminating `0x0B` end opcode).
#[derive(Debug, PartialEq, Clone)]
pub struct Global {
    pub global_type: GlobalType,
    pub init: ConstExpr,
}

/// Reads a `globaltype := valtype mut` at `*pos`, advancing past both bytes.
/// Shared with the import section's global-import descriptor.
fn read_global_type(payload: &[u8], pos: &mut usize) -> Result<GlobalType, ParseError> {
    if *pos >= payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let valtype = ValType::try_from(payload[*pos])?;
    *pos += 1;

    if *pos >= payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let mutable = match payload[*pos] {
        0x00 => false,
        0x01 => true,
        b => return Err(ParseError::InvalidMutability(b)),
    };
    *pos += 1;

    Ok(GlobalType { valtype, mutable })
}

/// Skips a single LEB128-encoded integer immediate (signed or unsigned) without
/// decoding its value, advancing `*pos` past the last continuation byte.
fn skip_leb128(payload: &[u8], pos: &mut usize) -> Result<(), ParseError> {
    loop {
        if *pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let byte = payload[*pos];
        *pos += 1;
        if byte & 0x80 == 0 {
            return Ok(());
        }
    }
}

/// Reads a constant initializer expression, returning its raw bytes including the
/// terminating `0x0B`. Supports the constant instructions valid in a global
/// init_expr / element offset / element expr: `i32/i64/f32/f64.const`,
/// `global.get`, `ref.func` and `ref.null`.
fn read_init_expr(payload: &[u8], pos: &mut usize) -> Result<Vec<u8>, ParseError> {
    let start = *pos;
    loop {
        if *pos >= payload.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let op = payload[*pos];
        *pos += 1;
        match op {
            0x0B => break,                                    // end
            0x41 | 0x42 | 0x23 => skip_leb128(payload, pos)?, // i32/i64.const, global.get
            0x43 => advance(payload, pos, 4)?,                // f32.const
            0x44 => advance(payload, pos, 8)?,                // f64.const
            0xD2 => skip_leb128(payload, pos)?,               // ref.func funcidx
            0xD0 => advance(payload, pos, 1)?,                // ref.null heaptype
            _ => return Err(ParseError::UnsupportedInitExpr(op)),
        }
    }
    Ok(payload[start..*pos].to_vec())
}

fn advance(payload: &[u8], pos: &mut usize, n: usize) -> Result<(), ParseError> {
    if *pos + n > payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    *pos += n;
    Ok(())
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
        let global_type = read_global_type(payload, &mut pos)?;
        let init_bytes = read_init_expr(payload, &mut pos)?;
        let init = parse_const_expr(&init_bytes)?;
        globals.push(Global { global_type, init });
    }
    Ok(globals)
}

// ── Data section (id = 11) ───────────────────────────────────────────────────

/// How a data segment is placed into memory.
#[derive(Debug, PartialEq, Clone)]
pub enum DataMode {
    /// Copied into `memory[memory_index]` at `offset_expr` during instantiation.
    /// `offset_expr` holds the raw const-expr bytes (including the trailing `0x0B`).
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

/// Reads a `count`-prefixed raw byte vector, advancing `*pos` past it.
fn read_byte_vec(payload: &[u8], pos: &mut usize) -> Result<Vec<u8>, ParseError> {
    let (len, n) = decode_leb128_u32(payload, *pos)?;
    *pos += n;
    let end = *pos + len as usize;
    if end > payload.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let bytes = payload[*pos..end].to_vec();
    *pos = end;
    Ok(bytes)
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
                let ob = read_init_expr(payload, &mut pos)?;
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
                let ob = read_init_expr(payload, &mut pos)?;
                DataMode::Active {
                    memory_index,
                    offset: parse_const_expr(&ob)?,
                }
            }
            _ => return Err(ParseError::UnsupportedDataFlag(flag as u8)),
        };

        let bytes = read_byte_vec(payload, &mut pos)?;
        segments.push(DataSegment { mode, bytes });
    }
    Ok(segments)
}

// ── Element section (id = 9) ──────────────────────────────────────────────────

/// How an element segment is placed. `Active` segments copy their elements into
/// `table[table_index]` at `offset_expr` during instantiation; `Passive` and
/// `Declarative` are referenced later (`table.init`) or used only for validation.
/// `offset_expr` holds the raw const-expr bytes (including the trailing `0x0B`).
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
/// 0..=7); flags ≥ 8 are rejected with [`ParseError::UnsupportedElementFlag`].
#[derive(Debug, PartialEq, Clone)]
pub struct ElementSegment {
    pub mode: ElementMode,
    pub reftype: RefType,
    pub init: ElementInit,
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
        out.push(read_init_expr(payload, pos)?);
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
                let ob = read_init_expr(payload, &mut pos)?;
                let func_indices = read_u32_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index: 0,
                        offset: parse_const_expr(&ob)?,
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
                let ob = read_init_expr(payload, &mut pos)?;
                let reftype = read_elemkind(payload, &mut pos)?;
                let func_indices = read_u32_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index,
                        offset: parse_const_expr(&ob)?,
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
                let ob = read_init_expr(payload, &mut pos)?;
                let exprs = read_expr_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index: 0,
                        offset: parse_const_expr(&ob)?,
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
                let ob = read_init_expr(payload, &mut pos)?;
                let reftype = read_reftype(payload, &mut pos)?;
                let exprs = read_expr_vec(payload, &mut pos)?;
                ElementSegment {
                    mode: ElementMode::Active {
                        table_index,
                        offset: parse_const_expr(&ob)?,
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

// ── Display implementations (human-readable, used by `wasm-dump --verbose`) ────

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            ValType::F32 => "f32",
            ValType::F64 => "f64",
        })
    }
}

impl fmt::Display for RefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RefType::FuncRef => "funcref",
            RefType::ExternRef => "externref",
        })
    }
}

impl fmt::Display for FuncType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = join_valtypes(&self.params);
        write!(f, "({}) -> ", params)?;
        if self.results.is_empty() {
            f.write_str("()")
        } else {
            f.write_str(&join_valtypes(&self.results))
        }
    }
}

fn join_valtypes(vs: &[ValType]) -> String {
    vs.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for Limits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.max {
            Some(max) => write!(f, "min={} max={}", self.min, max),
            None => write!(f, "min={}", self.min),
        }
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.reftype, self.limits)
    }
}

impl fmt::Display for ImportDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportDesc::Func(idx) => write!(f, "Func({})", idx),
            ImportDesc::Table { reftype, limits } => write!(f, "Table({} {})", reftype, limits),
            ImportDesc::Memory(limits) => write!(f, "Memory({})", limits),
            ImportDesc::Global { valtype, mutable } => {
                write!(
                    f,
                    "Global({}{})",
                    valtype,
                    if *mutable { " mut" } else { "" }
                )
            }
        }
    }
}

impl fmt::Display for Import {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}  {}", self.module, self.name, self.desc)
    }
}

impl fmt::Display for ExportKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ExportKind::Func => "Func",
            ExportKind::Table => "Table",
            ExportKind::Memory => "Memory",
            ExportKind::Global => "Global",
        })
    }
}

impl fmt::Display for Export {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"  {}({})", self.name, self.kind, self.index)
    }
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

impl fmt::Display for Global {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} = {}", self.global_type, self.init)
    }
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

impl fmt::Display for CustomSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "name=\"{}\" payload={} bytes",
            self.name,
            self.bytes.len()
        )
    }
}

impl fmt::Display for NameSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.module.is_none() && self.functions.is_empty() {
            return writeln!(f, "  (empty)");
        }
        if let Some(m) = &self.module {
            writeln!(f, "  module: \"{}\"", m)?;
        }
        if !self.functions.is_empty() {
            writeln!(f, "  function names:")?;
            for (idx, name) in &self.functions {
                writeln!(f, "    [{}] \"{}\"", idx, name)?;
            }
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Memory section ────────────────────────────────────────────────────────

    #[test]
    fn memory_section_empty() {
        assert_eq!(decode_memory_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn memory_section_single_no_max() {
        // count=1, limits: flag=0x00 min=1
        let payload = [0x01, 0x00, 0x01];
        assert_eq!(
            decode_memory_section(&payload),
            Ok(vec![Limits { min: 1, max: None }])
        );
    }

    #[test]
    fn memory_section_single_with_max() {
        // count=1, limits: flag=0x01 min=1 max=4
        let payload = [0x01, 0x01, 0x01, 0x04];
        assert_eq!(
            decode_memory_section(&payload),
            Ok(vec![Limits {
                min: 1,
                max: Some(4)
            }])
        );
    }

    #[test]
    fn memory_section_multiple() {
        // count=2, [min=0 no max], [min=2 max=8]
        let payload = [0x02, 0x00, 0x00, 0x01, 0x02, 0x08];
        assert_eq!(
            decode_memory_section(&payload),
            Ok(vec![
                Limits { min: 0, max: None },
                Limits {
                    min: 2,
                    max: Some(8)
                },
            ])
        );
    }

    #[test]
    fn memory_section_unknown_limits_flag() {
        let payload = [0x01, 0x02, 0x01];
        assert!(matches!(
            decode_memory_section(&payload),
            Err(ParseError::UnknownLimitsFlag(0x02))
        ));
    }

    // ── Import section ────────────────────────────────────────────────────────

    #[test]
    fn import_section_empty() {
        assert_eq!(decode_import_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn import_section_func_import() {
        // count=1, module="env"(3), name="log"(3), kind=0x00(Func), type_idx=2
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"log");
        payload.extend_from_slice(&[0x00, 0x02]);
        assert_eq!(
            decode_import_section(&payload),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "log".to_string(),
                desc: ImportDesc::Func(2),
            }])
        );
    }

    #[test]
    fn import_section_memory_import_no_max() {
        // count=1, module="env"(3), name="mem"(3), kind=0x02(Memory), limits: flag=0x00 min=1
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"mem");
        payload.extend_from_slice(&[0x02, 0x00, 0x01]);
        assert_eq!(
            decode_import_section(&payload),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "mem".to_string(),
                desc: ImportDesc::Memory(Limits { min: 1, max: None }),
            }])
        );
    }

    #[test]
    fn import_section_memory_import_with_max() {
        // kind=0x02, limits: flag=0x01 min=1 max=4
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"mem");
        payload.extend_from_slice(&[0x02, 0x01, 0x01, 0x04]);
        assert_eq!(
            decode_import_section(&payload),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "mem".to_string(),
                desc: ImportDesc::Memory(Limits {
                    min: 1,
                    max: Some(4)
                }),
            }])
        );
    }

    #[test]
    fn import_section_table_import() {
        // kind=0x01, reftype=0x70(FuncRef), limits: flag=0x00 min=0
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"tbl");
        payload.extend_from_slice(&[0x01, 0x70, 0x00, 0x00]);
        assert_eq!(
            decode_import_section(&payload),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "tbl".to_string(),
                desc: ImportDesc::Table {
                    reftype: RefType::FuncRef,
                    limits: Limits { min: 0, max: None },
                },
            }])
        );
    }

    #[test]
    fn import_section_global_import_immutable() {
        // kind=0x03, valtype=0x7F(i32), mut=0x00
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"glb");
        payload.extend_from_slice(&[0x03, 0x7F, 0x00]);
        assert_eq!(
            decode_import_section(&payload),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "glb".to_string(),
                desc: ImportDesc::Global {
                    valtype: ValType::I32,
                    mutable: false,
                },
            }])
        );
    }

    #[test]
    fn import_section_global_import_mutable() {
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"glb");
        payload.extend_from_slice(&[0x03, 0x7F, 0x01]);
        assert_eq!(
            decode_import_section(&payload),
            Ok(vec![Import {
                module: "env".to_string(),
                name: "glb".to_string(),
                desc: ImportDesc::Global {
                    valtype: ValType::I32,
                    mutable: true,
                },
            }])
        );
    }

    #[test]
    fn import_section_unknown_kind() {
        let mut payload = vec![0x01];
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"env");
        payload.extend_from_slice(&[0x03]);
        payload.extend_from_slice(b"foo");
        payload.push(0xFF);
        assert!(matches!(
            decode_import_section(&payload),
            Err(ParseError::UnknownImportKind(0xFF))
        ));
    }

    // ── Type section ──────────────────────────────────────────────────────────

    #[test]
    fn type_section_empty() {
        // count = 0
        assert_eq!(decode_type_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn type_section_single_func_no_args() {
        // count=1, functype marker=0x60, params=0, results=0
        let payload = [0x01, 0x60, 0x00, 0x00];
        assert_eq!(
            decode_type_section(&payload),
            Ok(vec![FuncType {
                params: vec![],
                results: vec![]
            }])
        );
    }

    #[test]
    fn type_section_i32_add() {
        // (i32, i32) -> i32
        let payload = [
            0x01, // count = 1
            0x60, // functype
            0x02, 0x7F, 0x7F, // params: [i32, i32]
            0x01, 0x7F, // results: [i32]
        ];
        assert_eq!(
            decode_type_section(&payload),
            Ok(vec![FuncType {
                params: vec![ValType::I32, ValType::I32],
                results: vec![ValType::I32],
            }])
        );
    }

    #[test]
    fn type_section_unknown_valtype() {
        let payload = [0x01, 0x60, 0x01, 0x99, 0x00];
        assert!(matches!(
            decode_type_section(&payload),
            Err(ParseError::UnknownValType(0x99))
        ));
    }

    #[test]
    fn type_section_wrong_functype_marker() {
        let payload = [0x01, 0x61, 0x00, 0x00];
        assert!(matches!(
            decode_type_section(&payload),
            Err(ParseError::InvalidFuncType(0x61))
        ));
    }

    // ── Function section ───────────────────────────────────────────────────────

    #[test]
    fn function_section_empty() {
        assert_eq!(decode_function_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn function_section_three_funcs() {
        // count=3, indices=[0,1,0]
        let payload = [0x03, 0x00, 0x01, 0x00];
        assert_eq!(decode_function_section(&payload), Ok(vec![0, 1, 0]));
    }

    // ── Start section ────────────────────────────────────────────────────────

    #[test]
    fn start_section_single_byte_index() {
        assert_eq!(decode_start_section(&[0x00]), Ok(0));
        assert_eq!(decode_start_section(&[0x2A]), Ok(42));
    }

    #[test]
    fn start_section_multibyte_index() {
        // 0x80 0x01 = LEB128 for 128
        assert_eq!(decode_start_section(&[0x80, 0x01]), Ok(128));
    }

    #[test]
    fn start_section_trailing_bytes_rejected() {
        // funcidx=0 but an extra byte follows: not a well-formed start section
        assert_eq!(
            decode_start_section(&[0x00, 0xFF]),
            Err(ParseError::SizeMismatch)
        );
    }

    #[test]
    fn start_section_empty_payload() {
        assert_eq!(decode_start_section(&[]), Err(ParseError::UnexpectedEof));
    }

    // ── DataCount section ────────────────────────────────────────────────────

    #[test]
    fn datacount_section_single_byte() {
        assert_eq!(decode_datacount_section(&[0x00]), Ok(0));
        assert_eq!(decode_datacount_section(&[0x05]), Ok(5));
    }

    #[test]
    fn datacount_section_multibyte() {
        // 0x80 0x01 = 128
        assert_eq!(decode_datacount_section(&[0x80, 0x01]), Ok(128));
    }

    #[test]
    fn datacount_section_trailing_bytes_rejected() {
        assert_eq!(
            decode_datacount_section(&[0x01, 0xFF]),
            Err(ParseError::SizeMismatch)
        );
    }

    #[test]
    fn datacount_section_empty_payload() {
        assert_eq!(
            decode_datacount_section(&[]),
            Err(ParseError::UnexpectedEof)
        );
    }

    // ── Custom section ───────────────────────────────────────────────────────

    #[test]
    fn custom_section_name_and_payload() {
        // name="name"(4 bytes) + payload [0x01,0x02,0x03]
        let mut payload = vec![0x04];
        payload.extend_from_slice(b"name");
        payload.extend_from_slice(&[0x01, 0x02, 0x03]);
        assert_eq!(
            decode_custom_section(&payload),
            Ok(CustomSection {
                name: "name".to_string(),
                bytes: vec![0x01, 0x02, 0x03],
            })
        );
    }

    #[test]
    fn custom_section_empty_payload_after_name() {
        // name="x"(1 byte), no trailing payload bytes
        let payload = [0x01, b'x'];
        assert_eq!(
            decode_custom_section(&payload),
            Ok(CustomSection {
                name: "x".to_string(),
                bytes: vec![],
            })
        );
    }

    #[test]
    fn custom_section_name_length_overruns() {
        // name len=5 but only 2 bytes follow
        let payload = [0x05, b'a', b'b'];
        assert_eq!(
            decode_custom_section(&payload),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn custom_section_invalid_utf8_name() {
        // name len=1 with a non-UTF8 byte
        let payload = [0x01, 0xFF];
        assert_eq!(
            decode_custom_section(&payload),
            Err(ParseError::InvalidUtf8)
        );
    }

    #[test]
    fn display_custom_section() {
        let c = CustomSection {
            name: "producers".to_string(),
            bytes: vec![0xAA, 0xBB],
        };
        assert_eq!(c.to_string(), "name=\"producers\" payload=2 bytes");
    }

    // ── Name section ─────────────────────────────────────────────────────────

    fn leb128_u32(mut val: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (val & 0x7F) as u8;
            val >>= 7;
            if val == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }

    fn name_str(s: &str) -> Vec<u8> {
        let mut v = leb128_u32(s.len() as u32);
        v.extend_from_slice(s.as_bytes());
        v
    }

    #[test]
    fn name_section_empty_bytes() {
        assert_eq!(decode_name_section(&[]), Ok(NameSection::default()));
    }

    #[test]
    fn name_section_module_name() {
        // subsection id=0, size=5 ("mod" => len=3 + 3 bytes = 4 bytes, plus size LEB128)
        // payload of sub: [0x03, b'm', b'o', b'd'] = 4 bytes
        let mut sub = name_str("mod"); // [0x03, 'm', 'o', 'd']
        let size = sub.len() as u8;
        let mut bytes = vec![0x00, size]; // id=0, size=4
        bytes.append(&mut sub);
        assert_eq!(
            decode_name_section(&bytes),
            Ok(NameSection {
                module: Some("mod".to_string()),
                functions: vec![],
            })
        );
    }

    #[test]
    fn name_section_function_names() {
        // subsection id=1: count=2, (0,"add"), (1,"sub")
        let mut sub: Vec<u8> = vec![0x02]; // count=2
        sub.push(0x00); // idx=0
        sub.extend_from_slice(&name_str("add"));
        sub.push(0x01); // idx=1
        sub.extend_from_slice(&name_str("sub"));
        let size = sub.len() as u8;
        let mut bytes = vec![0x01, size];
        bytes.extend_from_slice(&sub);
        assert_eq!(
            decode_name_section(&bytes),
            Ok(NameSection {
                module: None,
                functions: vec![(0, "add".to_string()), (1, "sub".to_string())],
            })
        );
    }

    #[test]
    fn name_section_module_and_functions() {
        // subsection 0 then subsection 1
        let mod_sub = name_str("mymod");
        let mut func_sub: Vec<u8> = vec![0x01, 0x00]; // count=1, idx=0
        func_sub.extend_from_slice(&name_str("main"));

        let mut bytes = vec![0x00, mod_sub.len() as u8];
        bytes.extend_from_slice(&mod_sub);
        bytes.push(0x01);
        bytes.push(func_sub.len() as u8);
        bytes.extend_from_slice(&func_sub);

        assert_eq!(
            decode_name_section(&bytes),
            Ok(NameSection {
                module: Some("mymod".to_string()),
                functions: vec![(0, "main".to_string())],
            })
        );
    }

    #[test]
    fn name_section_unknown_subsection_skipped() {
        // subsection id=2 (local names) with 3 opaque bytes — must be skipped
        // followed by id=1 with one function name
        let mut bytes = vec![0x02, 0x03, 0xAA, 0xBB, 0xCC]; // id=2, size=3, junk
        let mut func_sub: Vec<u8> = vec![0x01, 0x00]; // count=1, idx=0
        func_sub.extend_from_slice(&name_str("f"));
        bytes.push(0x01);
        bytes.push(func_sub.len() as u8);
        bytes.extend_from_slice(&func_sub);
        assert_eq!(
            decode_name_section(&bytes),
            Ok(NameSection {
                module: None,
                functions: vec![(0, "f".to_string())],
            })
        );
    }

    #[test]
    fn name_section_subsection_size_overrun() {
        // id=1, size=10 but only 2 bytes follow
        let bytes = vec![0x01, 0x0A, 0xAA, 0xBB];
        assert_eq!(decode_name_section(&bytes), Err(ParseError::UnexpectedEof));
    }

    // ── Export section ─────────────────────────────────────────────────────────

    #[test]
    fn export_section_empty() {
        assert_eq!(decode_export_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn export_section_single_func() {
        // count=1, name="add"(3 bytes), kind=func(0x00), index=0
        let mut payload = vec![0x01, 0x03];
        payload.extend_from_slice(b"add");
        payload.extend_from_slice(&[0x00, 0x00]);
        assert_eq!(
            decode_export_section(&payload),
            Ok(vec![Export {
                name: "add".to_string(),
                kind: ExportKind::Func,
                index: 0,
            }])
        );
    }

    #[test]
    fn export_section_memory_export() {
        // count=1, name="mem"(3 bytes), kind=memory(0x02), index=0
        let mut payload = vec![0x01, 0x03];
        payload.extend_from_slice(b"mem");
        payload.extend_from_slice(&[0x02, 0x00]);
        assert_eq!(
            decode_export_section(&payload),
            Ok(vec![Export {
                name: "mem".to_string(),
                kind: ExportKind::Memory,
                index: 0,
            }])
        );
    }

    #[test]
    fn export_section_unknown_kind() {
        let mut payload = vec![0x01, 0x01];
        payload.extend_from_slice(b"x");
        payload.extend_from_slice(&[0xFF, 0x00]);
        assert!(matches!(
            decode_export_section(&payload),
            Err(ParseError::UnknownExportKind(0xFF))
        ));
    }

    // ── Code section ─────────────────────────────────────────────────────────

    #[test]
    fn code_section_empty() {
        // count = 0
        assert_eq!(decode_code_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn code_section_no_locals() {
        // count=1, body: size=2, local_count=0, expr=[0x0B] (end)
        let payload = [0x01, 0x02, 0x00, 0x0B];
        assert_eq!(
            decode_code_section(&payload),
            Ok(vec![FuncBody {
                locals: vec![],
                expr: vec![0x0B],
            }])
        );
    }

    #[test]
    fn code_section_with_locals() {
        // count=1
        // body size=6: local_count=1, local(count=2, i32=0x7F),
        //              expr=[0x20,0x00,0x0B] (local.get 0; end)
        let payload = [0x01, 0x06, 0x01, 0x02, 0x7F, 0x20, 0x00, 0x0B];
        assert_eq!(
            decode_code_section(&payload),
            Ok(vec![FuncBody {
                locals: vec![LocalDecl {
                    count: 2,
                    valtype: ValType::I32,
                }],
                expr: vec![0x20, 0x00, 0x0B],
            }])
        );
    }

    #[test]
    fn code_section_two_bodies() {
        // count=2, two empty-local bodies each with a single-byte expr
        let payload = [
            0x02, // count = 2
            0x02, 0x00, 0x0B, // body[0]: size=2, no locals, expr=[0x0B]
            0x02, 0x00, 0x0B, // body[1]: size=2, no locals, expr=[0x0B]
        ];
        assert_eq!(
            decode_code_section(&payload),
            Ok(vec![
                FuncBody {
                    locals: vec![],
                    expr: vec![0x0B],
                },
                FuncBody {
                    locals: vec![],
                    expr: vec![0x0B],
                },
            ])
        );
    }

    #[test]
    fn code_section_size_exceeds_payload() {
        // count=1, body claims size=10 but only 2 bytes follow
        let payload = [0x01, 0x0A, 0x00, 0x0B];
        assert_eq!(decode_code_section(&payload), Err(ParseError::SizeMismatch));
    }

    #[test]
    fn code_section_locals_overrun_body() {
        // count=1, body size=1 but local decl needs a valtype byte past body_end
        // size=1 covers only local_count byte; reading the local's valtype overruns.
        let payload = [0x01, 0x01, 0x01, 0x02, 0x7F];
        assert_eq!(decode_code_section(&payload), Err(ParseError::SizeMismatch));
    }

    // ── Display ──────────────────────────────────────────────────────────────

    #[test]
    fn display_valtype() {
        assert_eq!(ValType::I32.to_string(), "i32");
        assert_eq!(ValType::F64.to_string(), "f64");
    }

    #[test]
    fn display_functype() {
        let ft = FuncType {
            params: vec![ValType::I32, ValType::I32],
            results: vec![ValType::I32],
        };
        assert_eq!(ft.to_string(), "(i32, i32) -> i32");
    }

    #[test]
    fn display_functype_no_result() {
        let ft = FuncType {
            params: vec![],
            results: vec![],
        };
        assert_eq!(ft.to_string(), "() -> ()");
    }

    #[test]
    fn display_limits() {
        assert_eq!(Limits { min: 1, max: None }.to_string(), "min=1");
        assert_eq!(
            Limits {
                min: 1,
                max: Some(4)
            }
            .to_string(),
            "min=1 max=4"
        );
    }

    #[test]
    fn display_import() {
        let imp = Import {
            module: "env".to_string(),
            name: "log".to_string(),
            desc: ImportDesc::Func(2),
        };
        assert_eq!(imp.to_string(), "env::log  Func(2)");
    }

    #[test]
    fn display_import_global_mut() {
        let imp = Import {
            module: "env".to_string(),
            name: "g".to_string(),
            desc: ImportDesc::Global {
                valtype: ValType::I32,
                mutable: true,
            },
        };
        assert_eq!(imp.to_string(), "env::g  Global(i32 mut)");
    }

    #[test]
    fn display_export() {
        let exp = Export {
            name: "add".to_string(),
            kind: ExportKind::Func,
            index: 0,
        };
        assert_eq!(exp.to_string(), "\"add\"  Func(0)");
    }

    #[test]
    fn display_func_body() {
        let body = FuncBody {
            locals: vec![LocalDecl {
                count: 2,
                valtype: ValType::I32,
            }],
            expr: vec![0x20, 0x00, 0x0B],
        };
        assert_eq!(body.to_string(), "locals=[2 i32] expr=3 bytes");
    }

    // ── Global section ───────────────────────────────────────────────────────

    #[test]
    fn global_section_empty() {
        assert_eq!(decode_global_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn global_section_immutable_i32_const() {
        // count=1, valtype=i32(0x7F), mut=0x00 (const),
        // init_expr = i32.const 42 (0x41 0x2A) end (0x0B)
        let payload = [0x01, 0x7F, 0x00, 0x41, 0x2A, 0x0B];
        assert_eq!(
            decode_global_section(&payload),
            Ok(vec![Global {
                global_type: GlobalType {
                    valtype: ValType::I32,
                    mutable: false,
                },
                init: ConstExpr::I32(42),
            }])
        );
    }

    #[test]
    fn global_section_mutable_flag() {
        // count=1, valtype=i64(0x7E), mut=0x01 (mutable),
        // init = i64.const 1 (0x42 0x01) end (0x0B)
        let payload = [0x01, 0x7E, 0x01, 0x42, 0x01, 0x0B];
        assert_eq!(
            decode_global_section(&payload),
            Ok(vec![Global {
                global_type: GlobalType {
                    valtype: ValType::I64,
                    mutable: true,
                },
                init: ConstExpr::I64(1),
            }])
        );
    }

    #[test]
    fn global_section_multibyte_leb_not_mistaken_for_end() {
        // i32.const with a value whose LEB128 encoding contains 0x0B as a
        // continuation byte must not terminate the expr early.
        // 0x8B 0x01 = LEB128 for 139; the 0x8B has high bit set so it is consumed.
        let payload = [0x01, 0x7F, 0x00, 0x41, 0x8B, 0x01, 0x0B];
        assert_eq!(
            decode_global_section(&payload),
            Ok(vec![Global {
                global_type: GlobalType {
                    valtype: ValType::I32,
                    mutable: false,
                },
                init: ConstExpr::I32(139),
            }])
        );
    }

    #[test]
    fn global_section_invalid_mutability() {
        let payload = [0x01, 0x7F, 0x02, 0x41, 0x00, 0x0B];
        assert_eq!(
            decode_global_section(&payload),
            Err(ParseError::InvalidMutability(0x02))
        );
    }

    #[test]
    fn global_section_unsupported_init_expr() {
        // opcode 0xFF is not a valid const instruction
        let payload = [0x01, 0x7F, 0x00, 0xFF, 0x0B];
        assert_eq!(
            decode_global_section(&payload),
            Err(ParseError::UnsupportedInitExpr(0xFF))
        );
    }

    #[test]
    fn display_global() {
        let g = Global {
            global_type: GlobalType {
                valtype: ValType::I32,
                mutable: true,
            },
            init: ConstExpr::I32(42),
        };
        assert_eq!(g.to_string(), "i32 mut = 42");
    }

    // ── Table section ────────────────────────────────────────────────────────

    #[test]
    fn table_section_empty() {
        assert_eq!(decode_table_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn table_section_funcref_min_only() {
        // count=1, reftype=funcref(0x70), limits: flag=0x00 min=1
        let payload = [0x01, 0x70, 0x00, 0x01];
        assert_eq!(
            decode_table_section(&payload),
            Ok(vec![Table {
                reftype: RefType::FuncRef,
                limits: Limits { min: 1, max: None },
            }])
        );
    }

    #[test]
    fn table_section_externref_min_max() {
        // count=1, reftype=externref(0x6F), limits: flag=0x01 min=1 max=16
        let payload = [0x01, 0x6F, 0x01, 0x01, 0x10];
        assert_eq!(
            decode_table_section(&payload),
            Ok(vec![Table {
                reftype: RefType::ExternRef,
                limits: Limits {
                    min: 1,
                    max: Some(16),
                },
            }])
        );
    }

    #[test]
    fn table_section_invalid_reftype() {
        let payload = [0x01, 0x99, 0x00, 0x01];
        assert_eq!(
            decode_table_section(&payload),
            Err(ParseError::UnknownRefType(0x99))
        );
    }

    #[test]
    fn table_section_truncated_before_reftype() {
        // count=1 but no reftype byte follows
        let payload = [0x01];
        assert_eq!(
            decode_table_section(&payload),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn display_table() {
        let t = Table {
            reftype: RefType::FuncRef,
            limits: Limits {
                min: 1,
                max: Some(2),
            },
        };
        assert_eq!(t.to_string(), "funcref min=1 max=2");
    }

    // ── Data section ─────────────────────────────────────────────────────────

    #[test]
    fn data_section_empty() {
        assert_eq!(decode_data_section(&[0x00]), Ok(vec![]));
    }

    #[test]
    fn data_section_active_flag0() {
        // count=1, flag=0, offset_expr=i32.const 0 end (0x41 0x00 0x0B),
        // bytes: len=3 [0xAA,0xBB,0xCC]
        let payload = [0x01, 0x00, 0x41, 0x00, 0x0B, 0x03, 0xAA, 0xBB, 0xCC];
        assert_eq!(
            decode_data_section(&payload),
            Ok(vec![DataSegment {
                mode: DataMode::Active {
                    memory_index: 0,
                    offset: ConstExpr::I32(0),
                },
                bytes: vec![0xAA, 0xBB, 0xCC],
            }])
        );
    }

    #[test]
    fn data_section_passive_flag1() {
        // count=1, flag=1, bytes: len=2 [0x01,0x02]
        let payload = [0x01, 0x01, 0x02, 0x01, 0x02];
        assert_eq!(
            decode_data_section(&payload),
            Ok(vec![DataSegment {
                mode: DataMode::Passive,
                bytes: vec![0x01, 0x02],
            }])
        );
    }

    #[test]
    fn data_section_active_explicit_memidx_flag2() {
        // count=1, flag=2, memidx=1, offset=i32.const 8 end, bytes len=1 [0xFF]
        let payload = [0x01, 0x02, 0x01, 0x41, 0x08, 0x0B, 0x01, 0xFF];
        assert_eq!(
            decode_data_section(&payload),
            Ok(vec![DataSegment {
                mode: DataMode::Active {
                    memory_index: 1,
                    offset: ConstExpr::I32(8),
                },
                bytes: vec![0xFF],
            }])
        );
    }

    #[test]
    fn data_section_unsupported_flag() {
        let payload = [0x01, 0x05, 0x00];
        assert_eq!(
            decode_data_section(&payload),
            Err(ParseError::UnsupportedDataFlag(0x05))
        );
    }

    #[test]
    fn data_section_byte_vec_overrun() {
        // flag=1 passive, bytes claims len=5 but only 2 follow
        let payload = [0x01, 0x01, 0x05, 0xAA, 0xBB];
        assert_eq!(
            decode_data_section(&payload),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn display_data_segment() {
        let active = DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: ConstExpr::I32(0),
            },
            bytes: vec![0xAA, 0xBB, 0xCC],
        };
        assert_eq!(active.to_string(), "active mem[0] offset=0 data=3 bytes");
        let passive = DataSegment {
            mode: DataMode::Passive,
            bytes: vec![0x01, 0x02],
        };
        assert_eq!(passive.to_string(), "passive data=2 bytes");
    }

    // ── Element section ──────────────────────────────────────────────────────

    #[test]
    fn element_section_empty() {
        assert_eq!(decode_element_section(&[0x00]), Ok(vec![]));
    }

    fn active0(offset: ConstExpr, funcs: Vec<u32>) -> ElementSegment {
        ElementSegment {
            mode: ElementMode::Active {
                table_index: 0,
                offset,
            },
            reftype: RefType::FuncRef,
            init: ElementInit::FuncIndices(funcs),
        }
    }

    #[test]
    fn element_section_active_flag0_single_func() {
        // flag=0, offset=i32.const 0 end, funcvec [0]
        let payload = [0x01, 0x00, 0x41, 0x00, 0x0B, 0x01, 0x00];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![active0(ConstExpr::I32(0), vec![0])])
        );
    }

    #[test]
    fn element_section_multiple_func_indices() {
        // flag=0, offset=i32.const 1 end, funcvec [0,2,1]
        let payload = [0x01, 0x00, 0x41, 0x01, 0x0B, 0x03, 0x00, 0x02, 0x01];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![active0(ConstExpr::I32(1), vec![0, 2, 1])])
        );
    }

    #[test]
    fn element_section_two_segments() {
        let payload = [
            0x02, // count = 2
            0x00, 0x41, 0x00, 0x0B, 0x01, 0x00, // seg 0
            0x00, 0x41, 0x04, 0x0B, 0x02, 0x01, 0x02, // seg 1
        ];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![
                active0(ConstExpr::I32(0), vec![0]),
                active0(ConstExpr::I32(4), vec![1, 2]),
            ])
        );
    }

    #[test]
    fn element_section_flag1_passive_funcidx() {
        // flag=1, elemkind=funcref(0x00), funcvec [5]
        let payload = [0x01, 0x01, 0x00, 0x01, 0x05];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Passive,
                reftype: RefType::FuncRef,
                init: ElementInit::FuncIndices(vec![5]),
            }])
        );
    }

    #[test]
    fn element_section_flag2_active_explicit_table() {
        // flag=2, tableidx=1, offset=i32.const 0 end, elemkind=0x00, funcvec [0]
        let payload = [0x01, 0x02, 0x01, 0x41, 0x00, 0x0B, 0x00, 0x01, 0x00];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Active {
                    table_index: 1,
                    offset: ConstExpr::I32(0),
                },
                reftype: RefType::FuncRef,
                init: ElementInit::FuncIndices(vec![0]),
            }])
        );
    }

    #[test]
    fn element_section_flag3_declarative() {
        // flag=3, elemkind=0x00, funcvec [0,1]
        let payload = [0x01, 0x03, 0x00, 0x02, 0x00, 0x01];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Declarative,
                reftype: RefType::FuncRef,
                init: ElementInit::FuncIndices(vec![0, 1]),
            }])
        );
    }

    #[test]
    fn element_section_flag4_active_exprs() {
        // flag=4, offset=i32.const 0 end, exprvec [ref.func 3 end]
        let payload = [0x01, 0x04, 0x41, 0x00, 0x0B, 0x01, 0xD2, 0x03, 0x0B];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Active {
                    table_index: 0,
                    offset: ConstExpr::I32(0),
                },
                reftype: RefType::FuncRef,
                init: ElementInit::Exprs(vec![vec![0xD2, 0x03, 0x0B]]),
            }])
        );
    }

    #[test]
    fn element_section_flag5_passive_exprs_externref() {
        // flag=5, reftype=externref(0x6F), exprvec [ref.null extern end]
        let payload = [0x01, 0x05, 0x6F, 0x01, 0xD0, 0x6F, 0x0B];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Passive,
                reftype: RefType::ExternRef,
                init: ElementInit::Exprs(vec![vec![0xD0, 0x6F, 0x0B]]),
            }])
        );
    }

    #[test]
    fn element_section_flag6_active_explicit_exprs() {
        // flag=6, tableidx=0, offset=i32.const 0 end, reftype=funcref(0x70),
        // exprvec [ref.func 0 end]
        let payload = [
            0x01, 0x06, 0x00, 0x41, 0x00, 0x0B, 0x70, 0x01, 0xD2, 0x00, 0x0B,
        ];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Active {
                    table_index: 0,
                    offset: ConstExpr::I32(0),
                },
                reftype: RefType::FuncRef,
                init: ElementInit::Exprs(vec![vec![0xD2, 0x00, 0x0B]]),
            }])
        );
    }

    #[test]
    fn element_section_flag7_declarative_exprs() {
        // flag=7, reftype=funcref(0x70), exprvec [ref.func 0 end]
        let payload = [0x01, 0x07, 0x70, 0x01, 0xD2, 0x00, 0x0B];
        assert_eq!(
            decode_element_section(&payload),
            Ok(vec![ElementSegment {
                mode: ElementMode::Declarative,
                reftype: RefType::FuncRef,
                init: ElementInit::Exprs(vec![vec![0xD2, 0x00, 0x0B]]),
            }])
        );
    }

    #[test]
    fn element_section_invalid_elemkind() {
        // flag=1 with elemkind byte 0x99 (not funcref)
        let payload = [0x01, 0x01, 0x99];
        assert_eq!(
            decode_element_section(&payload),
            Err(ParseError::UnknownRefType(0x99))
        );
    }

    #[test]
    fn element_section_unsupported_flag() {
        // flag=8 is undefined
        let payload = [0x01, 0x08];
        assert_eq!(
            decode_element_section(&payload),
            Err(ParseError::UnsupportedElementFlag(0x08))
        );
    }

    #[test]
    fn element_section_truncated_func_vec() {
        // flag=0, offset ok, func vec claims 2 indices but only 1 follows
        let payload = [0x01, 0x00, 0x41, 0x00, 0x0B, 0x02, 0x00];
        assert_eq!(
            decode_element_section(&payload),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn display_element_segment_active_funcs() {
        let seg = active0(ConstExpr::I32(0), vec![0, 2, 1]);
        assert_eq!(
            seg.to_string(),
            "active table[0] offset=0 funcref funcs=[0, 2, 1]"
        );
    }

    #[test]
    fn display_element_segment_passive_exprs() {
        let seg = ElementSegment {
            mode: ElementMode::Passive,
            reftype: RefType::ExternRef,
            init: ElementInit::Exprs(vec![vec![0xD0, 0x6F, 0x0B]]),
        };
        assert_eq!(seg.to_string(), "passive externref exprs=1");
    }

    // ── const expr ───────────────────────────────────────────────────────────

    #[test]
    fn const_expr_i32_positive() {
        // i32.const 42 (0x41 0x2A) end (0x0B)
        assert_eq!(
            parse_const_expr(&[0x41, 0x2A, 0x0B]),
            Ok(ConstExpr::I32(42))
        );
    }

    #[test]
    fn const_expr_i32_negative() {
        // i32.const -1 (0x41 0x7F) end
        assert_eq!(
            parse_const_expr(&[0x41, 0x7F, 0x0B]),
            Ok(ConstExpr::I32(-1))
        );
    }

    #[test]
    fn const_expr_i64() {
        // i64.const -128 (0x42 0x80 0x7F) end
        assert_eq!(
            parse_const_expr(&[0x42, 0x80, 0x7F, 0x0B]),
            Ok(ConstExpr::I64(-128))
        );
    }

    #[test]
    fn const_expr_f32() {
        // f32.const 1.0 = 0x3F800000 little-endian
        let bytes = [0x43, 0x00, 0x00, 0x80, 0x3F, 0x0B];
        assert_eq!(parse_const_expr(&bytes), Ok(ConstExpr::F32(1.0)));
    }

    #[test]
    fn const_expr_f64() {
        // f64.const 1.0 = 0x3FF0000000000000 little-endian
        let bytes = [0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F, 0x0B];
        assert_eq!(parse_const_expr(&bytes), Ok(ConstExpr::F64(1.0)));
    }

    #[test]
    fn const_expr_global_get() {
        // global.get 3 (0x23 0x03) end
        assert_eq!(
            parse_const_expr(&[0x23, 0x03, 0x0B]),
            Ok(ConstExpr::GlobalGet(3))
        );
    }

    #[test]
    fn const_expr_missing_end() {
        // i32.const 1 without trailing 0x0B
        assert_eq!(
            parse_const_expr(&[0x41, 0x01]),
            Err(ParseError::UnexpectedEof)
        );
    }

    #[test]
    fn const_expr_unsupported_opcode() {
        assert_eq!(
            parse_const_expr(&[0xFF, 0x0B]),
            Err(ParseError::UnsupportedInitExpr(0xFF))
        );
    }

    #[test]
    fn display_const_expr() {
        assert_eq!(ConstExpr::I32(-5).to_string(), "-5");
        assert_eq!(ConstExpr::GlobalGet(2).to_string(), "global.get 2");
    }
}

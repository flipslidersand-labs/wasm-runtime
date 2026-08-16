use crate::parser::{decode_leb128_u32, ParseError};
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
    pub init_expr: Vec<u8>,
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
/// init_expr: `i32/i64/f32/f64.const` and `global.get`.
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

pub fn decode_global_section(payload: &[u8]) -> Result<Vec<Global>, ParseError> {
    let mut pos = 0;
    let (count, n) = decode_leb128_u32(payload, pos)?;
    pos += n;

    let mut globals = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let global_type = read_global_type(payload, &mut pos)?;
        let init_expr = read_init_expr(payload, &mut pos)?;
        globals.push(Global {
            global_type,
            init_expr,
        });
    }
    Ok(globals)
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

impl fmt::Display for Global {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} init={} bytes",
            self.global_type,
            self.init_expr.len()
        )
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
                init_expr: vec![0x41, 0x2A, 0x0B],
            }])
        );
    }

    #[test]
    fn global_section_mutable_flag() {
        // count=1, valtype=i64(0x7E), mut=0x01 (mutable),
        // init_expr = i64.const 1 (0x42 0x01) end (0x0B)
        let payload = [0x01, 0x7E, 0x01, 0x42, 0x01, 0x0B];
        assert_eq!(
            decode_global_section(&payload),
            Ok(vec![Global {
                global_type: GlobalType {
                    valtype: ValType::I64,
                    mutable: true,
                },
                init_expr: vec![0x42, 0x01, 0x0B],
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
                init_expr: vec![0x41, 0x8B, 0x01, 0x0B],
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
            init_expr: vec![0x41, 0x2A, 0x0B],
        };
        assert_eq!(g.to_string(), "i32 mut init=3 bytes");
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
}

use crate::parser::{decode_leb128_u32, ParseError};

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
                if pos + 1 >= payload.len() {
                    return Err(ParseError::UnexpectedEof);
                }
                let valtype = ValType::try_from(payload[pos])?;
                pos += 1;
                let mutable = match payload[pos] {
                    0x00 => false,
                    0x01 => true,
                    b => return Err(ParseError::InvalidMutability(b)),
                };
                pos += 1;
                ImportDesc::Global { valtype, mutable }
            }
            _ => return Err(ParseError::UnknownImportKind(kind)),
        };

        imports.push(Import { module, name, desc });
    }
    Ok(imports)
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
                desc: ImportDesc::Memory(Limits { min: 1, max: Some(4) }),
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
}

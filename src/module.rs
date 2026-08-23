//! Whole-module aggregation and lightweight cross-section validation.
//!
//! `parse_module` decodes every known section into one [`Module`]; `validate`
//! then checks the consistency constraints that individual section decoders
//! cannot see (e.g. that the function and code sections agree in length).

use crate::parser::{parse_header, section_iter, ParseError, ParseErrorContext};
use crate::sections::{
    decode_code_section, decode_custom_section, decode_data_section, decode_datacount_section,
    decode_element_section, decode_export_section, decode_function_section, decode_global_section,
    decode_import_section, decode_memory_section, decode_start_section, decode_table_section,
    decode_type_section, CustomSection, DataSegment, ElementInit, ElementMode, ElementSegment,
    Export, ExportKind, FuncBody, FuncType, Global, Import, ImportDesc, Limits, Table,
};
use std::fmt;

/// A decoded wasm module: every known section collected in one place.
/// Absent sections are represented as empty vectors.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Module {
    pub version: u32,
    /// Custom sections (id 0): tooling metadata. May appear multiple times.
    pub customs: Vec<CustomSection>,
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    /// Function section: one type index per locally-defined function.
    pub functions: Vec<u32>,
    pub tables: Vec<Table>,
    pub memories: Vec<Limits>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    /// Start section: index of the module's entry-point function, if present.
    pub start: Option<u32>,
    /// Element section: table-initializer segments (flag 0 only).
    pub elements: Vec<ElementSegment>,
    /// Code section: one body per locally-defined function.
    pub code: Vec<FuncBody>,
    pub data: Vec<DataSegment>,
    /// DataCount section: declared number of data segments, if present.
    pub data_count: Option<u32>,
}

/// Decodes the module header and every known section into a [`Module`].
pub fn parse_module(bytes: &[u8]) -> Result<Module, ParseError> {
    let version = parse_header(bytes)?;
    let mut module = Module {
        version,
        ..Module::default()
    };

    for result in section_iter(bytes) {
        let hdr = result?;
        let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
        match hdr.id {
            0 => module.customs.push(decode_custom_section(payload)?),
            1 => module.types = decode_type_section(payload)?,
            2 => module.imports = decode_import_section(payload)?,
            3 => module.functions = decode_function_section(payload)?,
            4 => module.tables = decode_table_section(payload)?,
            5 => module.memories = decode_memory_section(payload)?,
            6 => module.globals = decode_global_section(payload)?,
            7 => module.exports = decode_export_section(payload)?,
            8 => module.start = Some(decode_start_section(payload)?),
            9 => module.elements = decode_element_section(payload)?,
            10 => module.code = decode_code_section(payload)?,
            11 => module.data = decode_data_section(payload)?,
            12 => module.data_count = Some(decode_datacount_section(payload)?),
            _ => {} // unknown section id: skipped
        }
    }

    Ok(module)
}

/// Like [`parse_module`] but returns [`ParseErrorContext`] on failure, which
/// includes the byte offset and section id where the error occurred.
pub fn parse_module_with_context(bytes: &[u8]) -> Result<Module, ParseErrorContext> {
    let version = parse_header(bytes).map_err(|e| ParseErrorContext::new(e).with_offset(0))?;
    let mut module = Module {
        version,
        ..Module::default()
    };

    for result in section_iter(bytes) {
        let hdr = result.map_err(ParseErrorContext::new)?;
        let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
        let ctx = |e| {
            ParseErrorContext::new(e)
                .with_offset(hdr.offset)
                .with_section(hdr.id)
        };
        match hdr.id {
            0 => module
                .customs
                .push(decode_custom_section(payload).map_err(ctx)?),
            1 => module.types = decode_type_section(payload).map_err(ctx)?,
            2 => module.imports = decode_import_section(payload).map_err(ctx)?,
            3 => module.functions = decode_function_section(payload).map_err(ctx)?,
            4 => module.tables = decode_table_section(payload).map_err(ctx)?,
            5 => module.memories = decode_memory_section(payload).map_err(ctx)?,
            6 => module.globals = decode_global_section(payload).map_err(ctx)?,
            7 => module.exports = decode_export_section(payload).map_err(ctx)?,
            8 => module.start = Some(decode_start_section(payload).map_err(ctx)?),
            9 => module.elements = decode_element_section(payload).map_err(ctx)?,
            10 => module.code = decode_code_section(payload).map_err(ctx)?,
            11 => module.data = decode_data_section(payload).map_err(ctx)?,
            12 => module.data_count = Some(decode_datacount_section(payload).map_err(ctx)?),
            _ => {}
        }
    }

    Ok(module)
}

/// A cross-section consistency violation. Distinct from [`ParseError`], which
/// covers malformed bytes; a `ValidationError` means the bytes decoded fine but
/// the module is internally inconsistent.
#[derive(Debug, PartialEq, Clone)]
pub enum ValidationError {
    /// The function section and code section disagree on the function count.
    FuncCodeCountMismatch { functions: usize, code: usize },
    /// A type index (from a function or an imported function) is out of range.
    TypeIndexOutOfRange { index: u32, type_count: usize },
    /// An export references an index beyond its index space.
    ExportIndexOutOfRange {
        name: String,
        kind: ExportKind,
        index: u32,
        space: usize,
    },
    /// An element segment initializes a table that does not exist.
    ElementTableIndexOutOfRange { index: u32, table_space: usize },
    /// An element segment references a function beyond the function space.
    ElementFuncIndexOutOfRange { index: u32, func_space: usize },
    /// The start section names a function beyond the function space.
    StartFuncIndexOutOfRange { index: u32, func_space: usize },
    /// The datacount section disagrees with the actual data segment count.
    DataCountMismatch { declared: u32, actual: usize },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::FuncCodeCountMismatch { functions, code } => write!(
                f,
                "function section has {} entries but code section has {}",
                functions, code
            ),
            ValidationError::TypeIndexOutOfRange { index, type_count } => write!(
                f,
                "type index {} out of range (only {} types)",
                index, type_count
            ),
            ValidationError::ExportIndexOutOfRange {
                name,
                kind,
                index,
                space,
            } => write!(
                f,
                "export \"{}\" ({}) index {} out of range (space size {})",
                name, kind, index, space
            ),
            ValidationError::ElementTableIndexOutOfRange { index, table_space } => write!(
                f,
                "element table index {} out of range (only {} tables)",
                index, table_space
            ),
            ValidationError::ElementFuncIndexOutOfRange { index, func_space } => write!(
                f,
                "element func index {} out of range (func space {})",
                index, func_space
            ),
            ValidationError::StartFuncIndexOutOfRange { index, func_space } => write!(
                f,
                "start func index {} out of range (func space {})",
                index, func_space
            ),
            ValidationError::DataCountMismatch { declared, actual } => write!(
                f,
                "datacount section declares {} segments but data section has {}",
                declared, actual
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Number of imports of each kind (imports occupy the low end of each space).
#[derive(Default)]
struct ImportCounts {
    funcs: usize,
    tables: usize,
    memories: usize,
    globals: usize,
}

impl Module {
    /// Runs lightweight cross-section validation. Returns the first violation.
    pub fn validate(&self) -> Result<(), ValidationError> {
        // 1. function count must equal code body count.
        if self.functions.len() != self.code.len() {
            return Err(ValidationError::FuncCodeCountMismatch {
                functions: self.functions.len(),
                code: self.code.len(),
            });
        }

        let type_count = self.types.len();
        let mut counts = ImportCounts::default();

        // 2a. imported function type indices must be in range; tally import kinds.
        for import in &self.imports {
            match &import.desc {
                ImportDesc::Func(type_index) => {
                    if *type_index as usize >= type_count {
                        return Err(ValidationError::TypeIndexOutOfRange {
                            index: *type_index,
                            type_count,
                        });
                    }
                    counts.funcs += 1;
                }
                ImportDesc::Table { .. } => counts.tables += 1,
                ImportDesc::Memory(_) => counts.memories += 1,
                ImportDesc::Global { .. } => counts.globals += 1,
            }
        }

        // 2b. locally-defined function type indices must be in range.
        for &type_index in &self.functions {
            if type_index as usize >= type_count {
                return Err(ValidationError::TypeIndexOutOfRange {
                    index: type_index,
                    type_count,
                });
            }
        }

        // 3. export indices must fall within their (imports + defined) space.
        let func_space = counts.funcs + self.functions.len();
        let table_space = counts.tables + self.tables.len();
        let memory_space = counts.memories + self.memories.len();
        let global_space = counts.globals + self.globals.len();

        for export in &self.exports {
            let space = match export.kind {
                ExportKind::Func => func_space,
                ExportKind::Table => table_space,
                ExportKind::Memory => memory_space,
                ExportKind::Global => global_space,
            };
            if export.index as usize >= space {
                return Err(ValidationError::ExportIndexOutOfRange {
                    name: export.name.clone(),
                    kind: export.kind,
                    index: export.index,
                    space,
                });
            }
        }

        // 4. active element segments must target an existing table; funcidx-form
        //    element lists must reference funcs within the function space.
        //    (expr-form element lists are not range-checked here.)
        for element in &self.elements {
            if let ElementMode::Active { table_index, .. } = &element.mode {
                if *table_index as usize >= table_space {
                    return Err(ValidationError::ElementTableIndexOutOfRange {
                        index: *table_index,
                        table_space,
                    });
                }
            }
            if let ElementInit::FuncIndices(indices) = &element.init {
                for &func_index in indices {
                    if func_index as usize >= func_space {
                        return Err(ValidationError::ElementFuncIndexOutOfRange {
                            index: func_index,
                            func_space,
                        });
                    }
                }
            }
        }

        // 5. the start function, if any, must fall within the function space.
        if let Some(index) = self.start {
            if index as usize >= func_space {
                return Err(ValidationError::StartFuncIndexOutOfRange { index, func_space });
            }
        }

        // 6. a declared data count, if present, must match the data section.
        if let Some(declared) = self.data_count {
            if declared as usize != self.data.len() {
                return Err(ValidationError::DataCountMismatch {
                    declared,
                    actual: self.data.len(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::{ConstExpr, ValType};

    fn func_type() -> FuncType {
        FuncType {
            params: vec![],
            results: vec![],
        }
    }

    fn empty_body() -> FuncBody {
        FuncBody {
            locals: vec![],
            expr: vec![0x0B],
        }
    }

    #[test]
    fn valid_minimal_module() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            exports: vec![Export {
                name: "f".to_string(),
                kind: ExportKind::Func,
                index: 0,
            }],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn func_code_count_mismatch() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0, 0],
            code: vec![empty_body()],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::FuncCodeCountMismatch {
                functions: 2,
                code: 1,
            })
        );
    }

    #[test]
    fn function_type_index_out_of_range() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![5],
            code: vec![empty_body()],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::TypeIndexOutOfRange {
                index: 5,
                type_count: 1,
            })
        );
    }

    #[test]
    fn imported_func_type_index_out_of_range() {
        let module = Module {
            types: vec![func_type()],
            imports: vec![Import {
                module: "env".to_string(),
                name: "f".to_string(),
                desc: ImportDesc::Func(9),
            }],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::TypeIndexOutOfRange {
                index: 9,
                type_count: 1,
            })
        );
    }

    #[test]
    fn export_index_counts_imports() {
        // 1 imported func + 1 defined func => func space = 2; export index 1 is valid.
        let module = Module {
            types: vec![func_type()],
            imports: vec![Import {
                module: "env".to_string(),
                name: "g".to_string(),
                desc: ImportDesc::Func(0),
            }],
            functions: vec![0],
            code: vec![empty_body()],
            exports: vec![Export {
                name: "local".to_string(),
                kind: ExportKind::Func,
                index: 1,
            }],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn export_index_out_of_range() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            exports: vec![Export {
                name: "missing".to_string(),
                kind: ExportKind::Func,
                index: 3,
            }],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::ExportIndexOutOfRange {
                name: "missing".to_string(),
                kind: ExportKind::Func,
                index: 3,
                space: 1,
            })
        );
    }

    #[test]
    fn export_global_out_of_range() {
        let module = Module {
            globals: vec![Global {
                global_type: crate::sections::GlobalType {
                    valtype: ValType::I32,
                    mutable: false,
                },
                init: ConstExpr::I32(0),
            }],
            exports: vec![Export {
                name: "g".to_string(),
                kind: ExportKind::Global,
                index: 5,
            }],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::ExportIndexOutOfRange {
                name: "g".to_string(),
                kind: ExportKind::Global,
                index: 5,
                space: 1,
            })
        );
    }

    fn elem_seg(func_indices: Vec<u32>) -> ElementSegment {
        ElementSegment {
            mode: ElementMode::Active {
                table_index: 0,
                offset: ConstExpr::I32(0),
            },
            reftype: crate::sections::RefType::FuncRef,
            init: ElementInit::FuncIndices(func_indices),
        }
    }

    #[test]
    fn element_func_index_in_range() {
        // 1 defined func (space 1) + 1 table => element referencing func 0 is valid.
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            tables: vec![Table {
                reftype: crate::sections::RefType::FuncRef,
                limits: Limits { min: 1, max: None },
            }],
            elements: vec![elem_seg(vec![0])],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn element_func_index_out_of_range() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            tables: vec![Table {
                reftype: crate::sections::RefType::FuncRef,
                limits: Limits { min: 1, max: None },
            }],
            elements: vec![elem_seg(vec![5])],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::ElementFuncIndexOutOfRange {
                index: 5,
                func_space: 1,
            })
        );
    }

    #[test]
    fn element_table_index_out_of_range() {
        // element targets table 0 but the module declares no tables.
        let module = Module {
            elements: vec![elem_seg(vec![])],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::ElementTableIndexOutOfRange {
                index: 0,
                table_space: 0,
            })
        );
    }

    #[test]
    fn parse_module_aggregates_element_section() {
        // header + table(1 funcref) + element(flag0, offset i32.const 0, funcs [0])
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x04, 0x04, 0x01, 0x70, 0x00, 0x01]); // table
        bytes.extend_from_slice(&[0x09, 0x07, 0x01, 0x00, 0x41, 0x00, 0x0B, 0x01, 0x00]); // element
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.elements, vec![elem_seg(vec![0])]);
    }

    #[test]
    fn start_func_index_in_range() {
        // 1 defined func (space 1); start naming func 0 is valid.
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            start: Some(0),
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn start_func_index_out_of_range() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            start: Some(3),
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::StartFuncIndexOutOfRange {
                index: 3,
                func_space: 1,
            })
        );
    }

    #[test]
    fn parse_module_aggregates_start_section() {
        // header + type + func + start(func 0) + code
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type
        bytes.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]); // function
        bytes.extend_from_slice(&[0x08, 0x01, 0x00]); // start: func 0
        bytes.extend_from_slice(&[0x0A, 0x04, 0x01, 0x02, 0x00, 0x0B]); // code
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.start, Some(0));
        assert_eq!(module.validate(), Ok(()));
    }

    fn passive_data() -> DataSegment {
        DataSegment {
            mode: crate::sections::DataMode::Passive,
            bytes: vec![0xAA],
        }
    }

    #[test]
    fn datacount_matches_data_section() {
        let module = Module {
            data: vec![passive_data()],
            data_count: Some(1),
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn datacount_mismatch() {
        let module = Module {
            data: vec![passive_data()],
            data_count: Some(2),
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::DataCountMismatch {
                declared: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn parse_module_aggregates_datacount_section() {
        // header + datacount(1) + data(1 passive segment [0xAA])
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x0C, 0x01, 0x01]); // datacount = 1
        bytes.extend_from_slice(&[0x0B, 0x04, 0x01, 0x01, 0x01, 0xAA]); // data: 1 passive seg
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.data_count, Some(1));
        assert_eq!(module.data.len(), 1);
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn parse_module_aggregates_custom_sections() {
        // header + two custom sections ("a" with [0x01], "bc" with no payload)
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x00, 0x03, 0x01, b'a', 0x01]); // custom "a" payload [0x01]
        bytes.extend_from_slice(&[0x00, 0x03, 0x02, b'b', b'c']); // custom "bc" empty payload
        let module = parse_module(&bytes).unwrap();
        assert_eq!(
            module.customs,
            vec![
                CustomSection {
                    name: "a".to_string(),
                    bytes: vec![0x01],
                },
                CustomSection {
                    name: "bc".to_string(),
                    bytes: vec![],
                },
            ]
        );
        // custom sections carry no consistency constraints.
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn parse_module_roundtrip() {
        // header + type section (1 empty func type) + function section (1, type 0)
        // + code section (1 empty body)
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type
        bytes.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]); // function
        bytes.extend_from_slice(&[0x0A, 0x04, 0x01, 0x02, 0x00, 0x0B]); // code
        let module = parse_module(&bytes).unwrap();
        assert_eq!(module.version, 1);
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.functions, vec![0]);
        assert_eq!(module.code.len(), 1);
        assert_eq!(module.validate(), Ok(()));
    }
}

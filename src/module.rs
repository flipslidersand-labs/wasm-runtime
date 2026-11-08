//! Whole-module aggregation and lightweight cross-section validation.
//!
//! `parse_module` decodes every known section into one [`Module`]; `validate`
//! then checks the consistency constraints that individual section decoders
//! cannot see (e.g. that the function and code sections agree in length).

use crate::parser::{parse_header, section_iter, ParseError, ParseErrorContext, SectionHeader};
use crate::sections::{
    decode_code_section, decode_custom_section, decode_data_section, decode_datacount_section,
    decode_element_section, decode_export_section, decode_function_section, decode_global_section,
    decode_import_section, decode_memory_section, decode_start_section, decode_table_section,
    decode_type_section, scan_expr, CustomSection, DataMode, DataSegment, ElementInit, ElementMode,
    ElementSegment, Export, ExportKind, FuncBody, FuncType, Global, Import, ImportDesc, Limits,
    Table,
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
    /// Element section: table-initializer segments (all encodings, flag 0..=7).
    pub elements: Vec<ElementSegment>,
    /// Code section: one body per locally-defined function.
    pub code: Vec<FuncBody>,
    pub data: Vec<DataSegment>,
    /// DataCount section: declared number of data segments, if present.
    pub data_count: Option<u32>,
}

/// A single section encountered during parsing.
///
/// Passed to the callback in [`parse_module_with_events`] for each section in
/// the binary, in the order they appear.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionEvent {
    /// Raw section id (0 = custom, 1 = type, …, 12 = datacount).
    pub id: u8,
    /// Human-readable name matching the id.
    pub name: &'static str,
    /// Payload size in bytes (does not include the id or LEB128 size bytes).
    pub size: u32,
    /// Byte offset of the payload start within the module bytes.
    pub offset: usize,
}

/// Short lowercase name for a wasm section id (0 = custom, 1 = type, …, 12 =
/// datacount; unrecognized ids map to "unknown").
pub fn section_name(id: u8) -> &'static str {
    match id {
        0 => "custom",
        1 => "type",
        2 => "import",
        3 => "func",
        4 => "table",
        5 => "memory",
        6 => "global",
        7 => "export",
        8 => "start",
        9 => "element",
        10 => "code",
        11 => "data",
        12 => "datacount",
        _ => "unknown",
    }
}

/// Decodes one section into `module`. The only place that maps a section id to
/// its decoder and `Module` field. `map_err` converts a decoder's [`ParseError`]
/// into the caller's error type (identity, or wrapping with offset/section id).
fn apply_section<E>(
    module: &mut Module,
    hdr: &SectionHeader,
    payload: &[u8],
    map_err: impl Fn(ParseError) -> E,
) -> Result<(), E> {
    match hdr.id {
        0 => module
            .customs
            .push(decode_custom_section(payload).map_err(map_err)?),
        1 => module.types = decode_type_section(payload).map_err(map_err)?,
        2 => module.imports = decode_import_section(payload).map_err(map_err)?,
        3 => module.functions = decode_function_section(payload).map_err(map_err)?,
        4 => module.tables = decode_table_section(payload).map_err(map_err)?,
        5 => module.memories = decode_memory_section(payload).map_err(map_err)?,
        6 => module.globals = decode_global_section(payload).map_err(map_err)?,
        7 => module.exports = decode_export_section(payload).map_err(map_err)?,
        8 => module.start = Some(decode_start_section(payload).map_err(map_err)?),
        9 => module.elements = decode_element_section(payload).map_err(map_err)?,
        10 => module.code = decode_code_section(payload).map_err(map_err)?,
        11 => module.data = decode_data_section(payload).map_err(map_err)?,
        12 => module.data_count = Some(decode_datacount_section(payload).map_err(map_err)?),
        _ => {} // unknown section id: skipped
    }
    Ok(())
}

/// How [`parse_impl`] turns a [`ParseError`] into the caller's error type at
/// each of its three failure sites.
trait MapParseError: Sized {
    fn header(e: ParseError) -> Self;
    fn iter(e: ParseError, at: Option<(usize, u8)>) -> Self;
    fn section(e: ParseError, hdr: &SectionHeader) -> Self;
}

impl MapParseError for ParseError {
    fn header(e: ParseError) -> Self {
        e
    }
    fn iter(e: ParseError, _at: Option<(usize, u8)>) -> Self {
        e
    }
    fn section(e: ParseError, _hdr: &SectionHeader) -> Self {
        e
    }
}

impl MapParseError for ParseErrorContext {
    fn header(e: ParseError) -> Self {
        ParseErrorContext::new(e).with_offset(0)
    }
    fn iter(e: ParseError, at: Option<(usize, u8)>) -> Self {
        match at {
            // Offset of the failing section's id byte (its payload start is
            // not known when the header itself is malformed).
            Some((offset, id)) => ParseErrorContext::new(e)
                .with_offset(offset)
                .with_section(id),
            // Not reached today: every error `SectionIter` returns records its
            // location first. Kept as a safe fallback.
            None => ParseErrorContext::new(e),
        }
    }
    fn section(e: ParseError, hdr: &SectionHeader) -> Self {
        ParseErrorContext::new(e)
            .with_offset(hdr.offset)
            .with_section(hdr.id)
    }
}

/// The single parse loop behind every public `parse_module*` function.
fn parse_impl<E: MapParseError>(
    bytes: &[u8],
    mut on_section: impl FnMut(SectionEvent),
) -> Result<Module, E> {
    let version = parse_header(bytes).map_err(E::header)?;
    let mut module = Module {
        version,
        ..Module::default()
    };

    let mut iter = section_iter(bytes);
    while let Some(result) = iter.next() {
        let hdr = result.map_err(|e| E::iter(e, iter.error_location()))?;
        let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
        on_section(SectionEvent {
            id: hdr.id,
            name: section_name(hdr.id),
            size: hdr.size,
            offset: hdr.offset,
        });
        apply_section(&mut module, &hdr, payload, |e| E::section(e, &hdr))?;
    }

    Ok(module)
}

fn ignore_section(_: SectionEvent) {}

/// Options for [`parse_module_with_options`].
///
/// Build with [`ParseOptions::new`] (or `Default`) and chain setters. Fields
/// are private so new options can be added without breaking callers.
#[non_exhaustive]
pub struct ParseOptions<F = fn(SectionEvent)> {
    on_section: F,
}

impl ParseOptions {
    /// Options with no section callback.
    pub fn new() -> Self {
        ParseOptions {
            on_section: ignore_section,
        }
    }
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl<F> ParseOptions<F> {
    /// Sets the callback invoked once per section, in order, before the
    /// section is decoded (same timing as [`parse_module_with_events`]).
    pub fn on_section<G: FnMut(SectionEvent)>(self, on_section: G) -> ParseOptions<G> {
        ParseOptions { on_section }
    }
}

impl<F> fmt::Debug for ParseOptions<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseOptions").finish_non_exhaustive()
    }
}

/// Decodes a module with the given [`ParseOptions`], returning
/// [`ParseErrorContext`] on failure.
///
/// Combines [`parse_module_with_events`] (section callback) and
/// [`parse_module_with_context`] (offset / section id on errors). The plain
/// [`ParseError`] is available as [`ParseErrorContext::error`].
pub fn parse_module_with_options<F: FnMut(SectionEvent)>(
    bytes: &[u8],
    options: ParseOptions<F>,
) -> Result<Module, ParseErrorContext> {
    parse_impl(bytes, options.on_section)
}

/// Decodes the module header and every known section into a [`Module`].
///
/// `on_section` is called once per section, in order, before the section is
/// decoded.  The callback receives a [`SectionEvent`] describing the section's
/// id, name, payload size, and payload offset.  Use `|_| {}` to ignore events.
pub fn parse_module_with_events(
    bytes: &[u8],
    on_section: impl FnMut(SectionEvent),
) -> Result<Module, ParseError> {
    parse_impl(bytes, on_section)
}

/// Like [`parse_module`] but returns [`ParseErrorContext`] on failure.
///
/// Header errors carry offset 0 (no section id). Errors while decoding a
/// section's payload carry that section's offset and id. Errors raised by the
/// section iterator itself (malformed section headers) are wrapped without an
/// offset or section id.
pub fn parse_module_with_context(bytes: &[u8]) -> Result<Module, ParseErrorContext> {
    parse_impl(bytes, ignore_section)
}

/// Decodes the module header and every known section into a [`Module`].
pub fn parse_module(bytes: &[u8]) -> Result<Module, ParseError> {
    parse_module_with_events(bytes, |_| {})
}

/// A cross-section consistency violation. Distinct from [`ParseError`], which
/// covers malformed bytes; a `ValidationError` means the bytes decoded fine but
/// the module is internally inconsistent.
#[derive(Debug, PartialEq, Clone)]
#[non_exhaustive]
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
    /// An active data segment targets a memory that does not exist.
    DataMemoryIndexOutOfRange { index: u32, memory_space: usize },
    /// An element segment references a function beyond the function space.
    ElementFuncIndexOutOfRange { index: u32, func_space: usize },
    /// The start section names a function beyond the function space.
    StartFuncIndexOutOfRange { index: u32, func_space: usize },
    /// The datacount section disagrees with the actual data segment count.
    DataCountMismatch { declared: u32, actual: usize },
    /// A function uses `memory.init` or `data.drop`, which need a datacount section.
    DataCountRequired,
    /// A memory or table declares `min` greater than `max`.
    LimitsMinGreaterThanMax {
        kind: &'static str,
        min: u32,
        max: u32,
    },
    /// A memory declares `min` or `max` above the 65536-page (4 GiB) limit.
    MemoryLimitExceeded { field: &'static str, value: u32 },
}

/// Maximum memory size in 64 KiB pages (4 GiB) for 32-bit memories.
const MAX_MEMORY_PAGES: u32 = 65536;

/// Which kind of limits is being checked (avoids string-compare dispatch).
#[derive(Clone, Copy, PartialEq)]
enum LimitsKind {
    Table,
    Memory,
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
                crate::sections::display_name(name),
                kind,
                index,
                space
            ),
            ValidationError::ElementTableIndexOutOfRange { index, table_space } => write!(
                f,
                "element table index {} out of range (only {} tables)",
                index, table_space
            ),
            ValidationError::DataMemoryIndexOutOfRange {
                index,
                memory_space,
            } => write!(
                f,
                "data memory index {} out of range (only {} memories)",
                index, memory_space
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
            ValidationError::DataCountRequired => write!(
                f,
                "memory.init or data.drop is used but there is no datacount section"
            ),
            ValidationError::LimitsMinGreaterThanMax { kind, min, max } => write!(
                f,
                "{} size minimum {} must not be greater than maximum {}",
                kind, min, max
            ),
            ValidationError::MemoryLimitExceeded { field, value } => write!(
                f,
                "memory size {} {} must not exceed {} pages",
                field, value, MAX_MEMORY_PAGES
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

        // 2c. limits of imported and locally-defined tables/memories need min <= max.
        let imported_limits = self.imports.iter().filter_map(|i| match &i.desc {
            ImportDesc::Table { limits, .. } => Some((LimitsKind::Table, limits)),
            ImportDesc::Memory(limits) => Some((LimitsKind::Memory, limits)),
            _ => None,
        });
        let local_limits = self
            .tables
            .iter()
            .map(|t| (LimitsKind::Table, &t.limits))
            .chain(self.memories.iter().map(|m| (LimitsKind::Memory, m)));
        for (kind, limits) in imported_limits.chain(local_limits) {
            if kind == LimitsKind::Memory {
                if limits.min > MAX_MEMORY_PAGES {
                    return Err(ValidationError::MemoryLimitExceeded {
                        field: "minimum",
                        value: limits.min,
                    });
                }
                if let Some(max) = limits.max {
                    if max > MAX_MEMORY_PAGES {
                        return Err(ValidationError::MemoryLimitExceeded {
                            field: "maximum",
                            value: max,
                        });
                    }
                }
            }
            if let Some(max) = limits.max {
                if limits.min > max {
                    return Err(ValidationError::LimitsMinGreaterThanMax {
                        kind: match kind {
                            LimitsKind::Table => "table",
                            LimitsKind::Memory => "memory",
                        },
                        min: limits.min,
                        max,
                    });
                }
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

        // 4b. active data segments must target an existing memory.
        for segment in &self.data {
            if let DataMode::Active { memory_index, .. } = &segment.mode {
                if *memory_index as usize >= memory_space {
                    return Err(ValidationError::DataMemoryIndexOutOfRange {
                        index: *memory_index,
                        memory_space,
                    });
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

        // 7. memory.init and data.drop refer to data segments by index, which the
        //    binary format only allows when a datacount section is present.
        if self.data_count.is_none()
            && self
                .code
                .iter()
                .any(|b| matches!(scan_expr(&b.expr), Ok(info) if info.uses_data_index))
        {
            return Err(ValidationError::DataCountRequired);
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

    fn active_data(memory_index: u32) -> DataSegment {
        DataSegment {
            mode: crate::sections::DataMode::Active {
                memory_index,
                offset: ConstExpr::I32(0),
            },
            bytes: vec![],
        }
    }

    #[test]
    fn data_memory_index_no_memory_is_invalid() {
        // flag 0 data segment (implicit memory 0) with no memory defined.
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x0B, 0x06, 0x01, 0x00, 0x41, 0x00, 0x0B, 0x00]);
        let module = parse_module(&bytes).unwrap();
        assert_eq!(
            module.validate(),
            Err(ValidationError::DataMemoryIndexOutOfRange {
                index: 0,
                memory_space: 0,
            })
        );
    }

    #[test]
    fn data_memory_index_beyond_defined_memory() {
        let module = Module {
            memories: vec![Limits { min: 1, max: None }],
            data: vec![active_data(1)],
            ..Module::default()
        };
        assert_eq!(
            module.validate(),
            Err(ValidationError::DataMemoryIndexOutOfRange {
                index: 1,
                memory_space: 1,
            })
        );
    }

    #[test]
    fn data_memory_index_valid_with_defined_memory() {
        let module = Module {
            memories: vec![Limits { min: 1, max: None }],
            data: vec![active_data(0)],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn data_memory_index_counts_imported_memory() {
        let module = Module {
            imports: vec![Import {
                module: "env".to_string(),
                name: "m".to_string(),
                desc: ImportDesc::Memory(Limits { min: 1, max: None }),
            }],
            data: vec![active_data(0)],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
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

    fn module_with_body(expr: Vec<u8>, data_count: Option<u32>) -> Module {
        Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![FuncBody {
                locals: vec![],
                expr,
            }],
            data_count,
            ..Module::default()
        }
    }

    #[test]
    fn data_drop_without_datacount_is_rejected() {
        // data.drop 0; end (spec binary.57)
        let module = module_with_body(vec![0xFC, 0x09, 0x00, 0x0B], None);
        assert_eq!(module.validate(), Err(ValidationError::DataCountRequired));
    }

    #[test]
    fn memory_init_without_datacount_is_rejected() {
        // memory.init data 0 mem 0; end (spec binary.56)
        let module = module_with_body(vec![0xFC, 0x08, 0x00, 0x00, 0x0B], None);
        assert_eq!(module.validate(), Err(ValidationError::DataCountRequired));
    }

    #[test]
    fn data_drop_with_datacount_or_other_bulk_ops_without_it_is_valid() {
        let with = module_with_body(vec![0xFC, 0x09, 0x00, 0x0B], Some(0));
        assert_eq!(with.validate(), Ok(()));
        // memory.fill needs no data index, so no datacount is required
        let fill = module_with_body(vec![0xFC, 0x0B, 0x00, 0x0B], None);
        assert_eq!(fill.validate(), Ok(()));
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

    #[test]
    fn parse_module_oob_section_size_returns_err_not_panic() {
        // header(8) + section id=1, declared size=0xFF(255) but only 1 byte of
        // payload actually follows. Regression test for issue #111: this used to
        // panic with a slice-index-out-of-range instead of returning ParseError.
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.push(1); // section id
        bytes.push(0xFF); // declared size (LEB128 single byte, 255)
        bytes.push(0x00); // only 1 byte of actual payload present
        assert_eq!(parse_module(&bytes), Err(ParseError::UnexpectedEof));
    }

    #[test]
    fn parse_module_with_context_oob_section_size_returns_err_not_panic() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        bytes.push(1);
        bytes.push(0xFF);
        bytes.push(0x00);
        let err = parse_module_with_context(&bytes).unwrap_err();
        assert_eq!(err.error, ParseError::UnexpectedEof);
    }

    fn limits_table(min: u32, max: Option<u32>) -> Table {
        Table {
            reftype: crate::sections::RefType::FuncRef,
            limits: Limits { min, max },
        }
    }

    #[test]
    fn limits_min_greater_than_max_memory_rejected() {
        // memory section bytes: 05 04 01 01 02 01 (min=2, max=1)
        let bytes = [
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x05, 0x04, 0x01, 0x01, 0x02, 0x01,
        ];
        let module = parse_module(&bytes).unwrap();
        assert_eq!(
            module.validate(),
            Err(ValidationError::LimitsMinGreaterThanMax {
                kind: "memory",
                min: 2,
                max: 1
            })
        );
    }

    #[test]
    fn limits_min_greater_than_max_table_rejected() {
        // table section bytes: 04 05 01 70 01 05 01 (funcref, min=5, max=1)
        let bytes = [
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x04, 0x05, 0x01, 0x70, 0x01, 0x05,
            0x01,
        ];
        let module = parse_module(&bytes).unwrap();
        assert_eq!(
            module.validate(),
            Err(ValidationError::LimitsMinGreaterThanMax {
                kind: "table",
                min: 5,
                max: 1
            })
        );
    }

    #[test]
    fn limits_min_greater_than_max_imports_rejected() {
        let mem = Module {
            imports: vec![Import {
                module: "m".to_string(),
                name: "mem".to_string(),
                desc: ImportDesc::Memory(Limits {
                    min: 3,
                    max: Some(2),
                }),
            }],
            ..Module::default()
        };
        assert_eq!(
            mem.validate(),
            Err(ValidationError::LimitsMinGreaterThanMax {
                kind: "memory",
                min: 3,
                max: 2
            })
        );
        let tbl = Module {
            imports: vec![Import {
                module: "m".to_string(),
                name: "tbl".to_string(),
                desc: ImportDesc::Table {
                    reftype: crate::sections::RefType::FuncRef,
                    limits: Limits {
                        min: 4,
                        max: Some(3),
                    },
                },
            }],
            ..Module::default()
        };
        assert_eq!(
            tbl.validate(),
            Err(ValidationError::LimitsMinGreaterThanMax {
                kind: "table",
                min: 4,
                max: 3
            })
        );
    }

    #[test]
    fn limits_min_equal_max_accepted() {
        let module = Module {
            tables: vec![limits_table(1, Some(1))],
            memories: vec![Limits {
                min: 2,
                max: Some(2),
            }],
            imports: vec![Import {
                module: "m".to_string(),
                name: "mem".to_string(),
                desc: ImportDesc::Memory(Limits {
                    min: 0,
                    max: Some(0),
                }),
            }],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    fn mem_module(min: u32, max: Option<u32>) -> Module {
        Module {
            memories: vec![Limits { min, max }],
            ..Module::default()
        }
    }

    #[test]
    fn memory_limits_at_65536_accepted() {
        assert_eq!(mem_module(65536, Some(65536)).validate(), Ok(()));
        assert_eq!(mem_module(65536, None).validate(), Ok(()));
    }

    #[test]
    fn memory_min_over_65536_rejected() {
        assert_eq!(
            mem_module(65537, None).validate(),
            Err(ValidationError::MemoryLimitExceeded {
                field: "minimum",
                value: 65537
            })
        );
    }

    #[test]
    fn memory_max_over_65536_rejected() {
        assert_eq!(
            mem_module(0, Some(65537)).validate(),
            Err(ValidationError::MemoryLimitExceeded {
                field: "maximum",
                value: 65537
            })
        );
    }

    #[test]
    fn memory_import_over_65536_rejected() {
        let m = Module {
            imports: vec![Import {
                module: "m".to_string(),
                name: "mem".to_string(),
                desc: ImportDesc::Memory(Limits {
                    min: 0,
                    max: Some(65537),
                }),
            }],
            ..Module::default()
        };
        assert_eq!(
            m.validate(),
            Err(ValidationError::MemoryLimitExceeded {
                field: "maximum",
                value: 65537
            })
        );
    }

    /// Pins the check order: the 65536-page range check runs before the
    /// min > max check, so min=65537,max=1 reports MemoryLimitExceeded.
    #[test]
    fn memory_limit_exceeded_reported_before_min_greater_than_max() {
        assert_eq!(
            mem_module(65537, Some(1)).validate(),
            Err(ValidationError::MemoryLimitExceeded {
                field: "minimum",
                value: 65537
            })
        );
    }

    #[test]
    fn table_limits_over_65536_not_affected() {
        let module = Module {
            tables: vec![limits_table(70000, Some(80000))],
            ..Module::default()
        };
        assert_eq!(module.validate(), Ok(()));
    }

    #[test]
    fn memory_limit_exceeded_display() {
        let e = ValidationError::MemoryLimitExceeded {
            field: "maximum",
            value: 65537,
        };
        assert_eq!(
            e.to_string(),
            "memory size maximum 65537 must not exceed 65536 pages"
        );
    }
}

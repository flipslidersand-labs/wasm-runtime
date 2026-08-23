pub use crate::module::{parse_module, parse_module_with_context, Module, ValidationError};
pub use crate::parser::{parse_header, section_iter, ParseError, ParseErrorContext, SectionHeader};
pub use crate::sections::{
    ConstExpr, CustomSection, DataMode, DataSegment, ElementInit, ElementMode, ElementSegment,
    Export, ExportKind, FuncBody, FuncType, Global, GlobalType, Import, ImportDesc, Limits,
    LocalDecl, NameSection, RefType, Table, ValType,
};

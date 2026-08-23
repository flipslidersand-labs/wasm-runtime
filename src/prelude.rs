pub use crate::module::{parse_module, Module, ValidationError};
pub use crate::parser::{parse_header, section_iter, ParseError, SectionHeader};
pub use crate::sections::{
    ConstExpr, CustomSection, DataMode, DataSegment, ElementInit, ElementMode, ElementSegment,
    Export, ExportKind, FuncBody, FuncType, Global, GlobalType, Import, ImportDesc, Limits,
    LocalDecl, NameSection, RefType, Table, ValType,
};

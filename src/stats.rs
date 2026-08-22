//! Module statistics: section counts and kind breakdowns.

use crate::module::Module;
use crate::sections::{ExportKind, ImportDesc};
use std::fmt;

/// Per-kind breakdown for imports and exports.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct KindCounts {
    pub func: usize,
    pub table: usize,
    pub memory: usize,
    pub global: usize,
}

impl KindCounts {
    fn total(&self) -> usize {
        self.func + self.table + self.memory + self.global
    }
}

/// High-level statistics derived from a decoded [`Module`].
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ModuleStats {
    pub sections: usize,
    pub types: usize,
    pub imports: KindCounts,
    /// Locally-defined functions (excludes imported functions).
    pub functions: usize,
    pub tables: usize,
    pub memories: usize,
    pub globals: usize,
    pub exports: KindCounts,
    pub elements: usize,
    pub data_segments: usize,
    pub has_start: bool,
    pub custom_sections: usize,
}

impl ModuleStats {
    pub fn from_module(module: &Module) -> Self {
        let mut imports = KindCounts::default();
        for import in &module.imports {
            match &import.desc {
                ImportDesc::Func(_) => imports.func += 1,
                ImportDesc::Table { .. } => imports.table += 1,
                ImportDesc::Memory(_) => imports.memory += 1,
                ImportDesc::Global { .. } => imports.global += 1,
            }
        }

        let mut exports = KindCounts::default();
        for export in &module.exports {
            match export.kind {
                ExportKind::Func => exports.func += 1,
                ExportKind::Table => exports.table += 1,
                ExportKind::Memory => exports.memory += 1,
                ExportKind::Global => exports.global += 1,
            }
        }

        // Count non-empty sections that are present in the module.
        let mut sections = 0usize;
        if !module.customs.is_empty() {
            sections += module.customs.len();
        }
        if !module.types.is_empty() {
            sections += 1;
        }
        if !module.imports.is_empty() {
            sections += 1;
        }
        if !module.functions.is_empty() {
            sections += 1;
        }
        if !module.tables.is_empty() {
            sections += 1;
        }
        if !module.memories.is_empty() {
            sections += 1;
        }
        if !module.globals.is_empty() {
            sections += 1;
        }
        if !module.exports.is_empty() {
            sections += 1;
        }
        if module.start.is_some() {
            sections += 1;
        }
        if !module.elements.is_empty() {
            sections += 1;
        }
        if !module.code.is_empty() {
            sections += 1;
        }
        if !module.data.is_empty() {
            sections += 1;
        }
        if module.data_count.is_some() {
            sections += 1;
        }

        ModuleStats {
            sections,
            types: module.types.len(),
            imports,
            functions: module.functions.len(),
            tables: module.tables.len(),
            memories: module.memories.len(),
            globals: module.globals.len(),
            exports,
            elements: module.elements.len(),
            data_segments: module.data.len(),
            has_start: module.start.is_some(),
            custom_sections: module.customs.len(),
        }
    }
}

impl fmt::Display for ModuleStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Sections : {}", self.sections)?;
        writeln!(f, "Types    : {}", self.types)?;

        let imp = &self.imports;
        if imp.total() == 0 {
            writeln!(f, "Imports  : 0")?;
        } else {
            let mut parts = Vec::new();
            if imp.func > 0 {
                parts.push(format!("func: {}", imp.func));
            }
            if imp.table > 0 {
                parts.push(format!("table: {}", imp.table));
            }
            if imp.memory > 0 {
                parts.push(format!("memory: {}", imp.memory));
            }
            if imp.global > 0 {
                parts.push(format!("global: {}", imp.global));
            }
            writeln!(f, "Imports  : {}  ({})", imp.total(), parts.join(", "))?;
        }

        writeln!(f, "Functions: {} (local)", self.functions)?;
        writeln!(f, "Tables   : {}", self.tables)?;
        writeln!(f, "Memories : {}", self.memories)?;
        writeln!(f, "Globals  : {}", self.globals)?;

        let exp = &self.exports;
        if exp.total() == 0 {
            writeln!(f, "Exports  : 0")?;
        } else {
            let mut parts = Vec::new();
            if exp.func > 0 {
                parts.push(format!("func: {}", exp.func));
            }
            if exp.table > 0 {
                parts.push(format!("table: {}", exp.table));
            }
            if exp.memory > 0 {
                parts.push(format!("memory: {}", exp.memory));
            }
            if exp.global > 0 {
                parts.push(format!("global: {}", exp.global));
            }
            writeln!(f, "Exports  : {}  ({})", exp.total(), parts.join(", "))?;
        }

        writeln!(f, "Elements : {}", self.elements)?;
        writeln!(f, "Data     : {} segment(s)", self.data_segments)?;
        writeln!(
            f,
            "Start    : {}",
            if self.has_start { "yes" } else { "no" }
        )?;
        write!(f, "Custom   : {}", self.custom_sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::Module;
    use crate::sections::{
        Export, ExportKind, FuncBody, FuncType, Global, GlobalType, Import, ImportDesc, Limits,
        ValType,
    };

    fn empty_module() -> Module {
        Module::default()
    }

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
    fn empty_module_stats() {
        let stats = ModuleStats::from_module(&empty_module());
        assert_eq!(stats.sections, 0);
        assert_eq!(stats.types, 0);
        assert_eq!(stats.imports.total(), 0);
        assert_eq!(stats.functions, 0);
        assert_eq!(stats.exports.total(), 0);
        assert!(!stats.has_start);
    }

    #[test]
    fn counts_types_and_funcs() {
        let module = Module {
            types: vec![func_type(), func_type()],
            functions: vec![0, 1],
            code: vec![empty_body(), empty_body()],
            ..Module::default()
        };
        let stats = ModuleStats::from_module(&module);
        assert_eq!(stats.types, 2);
        assert_eq!(stats.functions, 2);
        assert_eq!(stats.sections, 3); // type + func + code
    }

    #[test]
    fn counts_imports_by_kind() {
        let module = Module {
            types: vec![func_type()],
            imports: vec![
                Import {
                    module: "env".to_string(),
                    name: "f".to_string(),
                    desc: ImportDesc::Func(0),
                },
                Import {
                    module: "env".to_string(),
                    name: "m".to_string(),
                    desc: ImportDesc::Memory(Limits { min: 1, max: None }),
                },
                Import {
                    module: "env".to_string(),
                    name: "g".to_string(),
                    desc: ImportDesc::Global {
                        valtype: ValType::I32,
                        mutable: false,
                    },
                },
            ],
            ..Module::default()
        };
        let stats = ModuleStats::from_module(&module);
        assert_eq!(stats.imports.func, 1);
        assert_eq!(stats.imports.memory, 1);
        assert_eq!(stats.imports.global, 1);
        assert_eq!(stats.imports.table, 0);
        assert_eq!(stats.imports.total(), 3);
    }

    #[test]
    fn counts_exports_by_kind() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            memories: vec![Limits { min: 1, max: None }],
            exports: vec![
                Export {
                    name: "main".to_string(),
                    kind: ExportKind::Func,
                    index: 0,
                },
                Export {
                    name: "mem".to_string(),
                    kind: ExportKind::Memory,
                    index: 0,
                },
            ],
            ..Module::default()
        };
        let stats = ModuleStats::from_module(&module);
        assert_eq!(stats.exports.func, 1);
        assert_eq!(stats.exports.memory, 1);
        assert_eq!(stats.exports.total(), 2);
    }

    #[test]
    fn has_start_flag() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            start: Some(0),
            ..Module::default()
        };
        let stats = ModuleStats::from_module(&module);
        assert!(stats.has_start);
    }

    #[test]
    fn display_empty_module() {
        let stats = ModuleStats::from_module(&empty_module());
        let output = format!("{}", stats);
        assert!(output.contains("Sections : 0"));
        assert!(output.contains("Types    : 0"));
        assert!(output.contains("Imports  : 0"));
        assert!(output.contains("Start    : no"));
    }

    #[test]
    fn display_with_imports_and_exports() {
        let module = Module {
            types: vec![func_type()],
            functions: vec![0],
            code: vec![empty_body()],
            imports: vec![Import {
                module: "env".to_string(),
                name: "f".to_string(),
                desc: ImportDesc::Func(0),
            }],
            globals: vec![Global {
                global_type: GlobalType {
                    valtype: ValType::I32,
                    mutable: false,
                },
                init_expr: vec![0x41, 0x00, 0x0B],
            }],
            exports: vec![Export {
                name: "main".to_string(),
                kind: ExportKind::Func,
                index: 1,
            }],
            ..Module::default()
        };
        let stats = ModuleStats::from_module(&module);
        let output = format!("{}", stats);
        assert!(output.contains("func: 1"));
        assert!(output.contains("Globals  : 1"));
    }
}

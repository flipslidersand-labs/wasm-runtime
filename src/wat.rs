//! Converts a decoded [`Module`] to WebAssembly Text Format (WAT).
//!
//! Code bodies are represented as placeholders — full decompilation is out of
//! scope. The output is valid WAT for the sections that are fully decoded.

use crate::module::Module;
use crate::sections::{ExportKind, ImportDesc, ValType};

/// Converts a decoded module to a WAT string.
pub fn module_to_wat(module: &Module) -> String {
    let mut out = String::from("(module\n");

    // Types
    for (i, ft) in module.types.iter().enumerate() {
        let params: Vec<String> = ft
            .params
            .iter()
            .map(|v| format!("(param {})", valtype(v)))
            .collect();
        let results: Vec<String> = ft
            .results
            .iter()
            .map(|v| format!("(result {})", valtype(v)))
            .collect();
        let mut parts = vec![format!("(;{};)", i)];
        parts.extend(params);
        parts.extend(results);
        out.push_str(&format!("  (type (func {}))\n", parts.join(" ")));
    }

    // Imports
    for imp in &module.imports {
        let desc = match &imp.desc {
            ImportDesc::Func(type_idx) => format!("(func (type {}))", type_idx),
            ImportDesc::Table { reftype, limits } => {
                let max = limits.max.map_or(String::new(), |m| format!(" {}", m));
                format!(
                    "(table {} {}{})",
                    limits.min,
                    max.trim(),
                    reftype_str(reftype)
                )
            }
            ImportDesc::Memory(limits) => {
                let max = limits.max.map_or(String::new(), |m| format!(" {}", m));
                format!("(memory {}{})", limits.min, max)
            }
            ImportDesc::Global {
                valtype: vt,
                mutable,
            } => {
                if *mutable {
                    format!("(global (mut {}))", valtype(vt))
                } else {
                    format!("(global {})", valtype(vt))
                }
            }
        };
        out.push_str(&format!(
            "  (import \"{}\" \"{}\" {})\n",
            imp.module, imp.name, desc
        ));
    }

    // Tables (locally defined)
    for table in &module.tables {
        let max = table
            .limits
            .max
            .map_or(String::new(), |m| format!(" {}", m));
        out.push_str(&format!(
            "  (table {} {}{})\n",
            table.limits.min,
            max.trim_start(),
            reftype_str(&table.reftype)
        ));
    }

    // Memories (locally defined)
    for mem in &module.memories {
        let max = mem.max.map_or(String::new(), |m| format!(" {}", m));
        out.push_str(&format!("  (memory {}{})\n", mem.min, max));
    }

    // Globals (locally defined)
    for global in &module.globals {
        let vt = valtype(&global.global_type.valtype);
        let type_str = if global.global_type.mutable {
            format!("(mut {})", vt)
        } else {
            vt.to_string()
        };
        out.push_str(&format!("  (global {} ({}))\n", type_str, global.init));
    }

    // Exports
    for exp in &module.exports {
        let kind = match exp.kind {
            ExportKind::Func => "func",
            ExportKind::Table => "table",
            ExportKind::Memory => "memory",
            ExportKind::Global => "global",
        };
        out.push_str(&format!(
            "  (export \"{}\" ({} {}))\n",
            exp.name, kind, exp.index
        ));
    }

    // Start
    if let Some(idx) = module.start {
        out.push_str(&format!("  (start {})\n", idx));
    }

    // Function bodies (placeholders — full decompilation is out of scope)
    for (i, body) in module.code.iter().enumerate() {
        let type_idx = module.functions.get(i).copied().unwrap_or(0);
        out.push_str(&format!(
            "  (func (type {}) ;; {} bytes\n    ;; body omitted\n  )\n",
            type_idx,
            body.expr.len()
        ));
    }

    // Data segments
    for seg in &module.data {
        match &seg.mode {
            crate::sections::DataMode::Active {
                memory_index,
                offset,
            } => {
                out.push_str(&format!(
                    "  (data (memory {}) (offset {}) \"<{} bytes>\")\n",
                    memory_index,
                    offset,
                    seg.bytes.len()
                ));
            }
            crate::sections::DataMode::Passive => {
                out.push_str(&format!("  (data \"<{} bytes>\")\n", seg.bytes.len()));
            }
        }
    }

    out.push(')');
    out
}

fn valtype(vt: &ValType) -> &'static str {
    match vt {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
    }
}

fn reftype_str(rt: &crate::sections::RefType) -> &'static str {
    match rt {
        crate::sections::RefType::FuncRef => "funcref",
        crate::sections::RefType::ExternRef => "externref",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::parse_module;

    fn minimal_wasm() -> Vec<u8> {
        // header + type (()→()) + func + export "main" + code (empty body)
        let mut b = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        b.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type
        b.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]); // func
        b.extend_from_slice(&[0x07, 0x08, 0x01, 0x04, b'm', b'a', b'i', b'n', 0x00, 0x00]); // export
        b.extend_from_slice(&[0x0A, 0x04, 0x01, 0x02, 0x00, 0x0B]); // code
        b
    }

    #[test]
    fn empty_module_wat() {
        let module = parse_module(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]).unwrap();
        let wat = module_to_wat(&module);
        assert_eq!(wat, "(module\n)");
    }

    #[test]
    fn minimal_module_contains_type() {
        let module = parse_module(&minimal_wasm()).unwrap();
        let wat = module_to_wat(&module);
        assert!(wat.contains("(type (func"));
    }

    #[test]
    fn minimal_module_contains_export() {
        let module = parse_module(&minimal_wasm()).unwrap();
        let wat = module_to_wat(&module);
        assert!(wat.contains("(export \"main\""));
        assert!(wat.contains("(func 0)"));
    }

    #[test]
    fn minimal_module_contains_func_placeholder() {
        let module = parse_module(&minimal_wasm()).unwrap();
        let wat = module_to_wat(&module);
        assert!(wat.contains("(func (type 0)"));
        assert!(wat.contains(";; body omitted"));
    }

    #[test]
    fn module_starts_and_ends_correctly() {
        let module = parse_module(&minimal_wasm()).unwrap();
        let wat = module_to_wat(&module);
        assert!(wat.starts_with("(module\n"));
        assert!(wat.ends_with(')'));
    }

    #[test]
    fn import_func_in_wat() {
        use crate::module::Module;
        use crate::sections::{FuncType, Import, ImportDesc};
        let module = Module {
            types: vec![FuncType {
                params: vec![],
                results: vec![],
            }],
            imports: vec![Import {
                module: "env".to_string(),
                name: "abort".to_string(),
                desc: ImportDesc::Func(0),
            }],
            ..Module::default()
        };
        let wat = module_to_wat(&module);
        assert!(wat.contains("(import \"env\" \"abort\""));
        assert!(wat.contains("(func (type 0))"));
    }

    fn import_wat(desc: crate::sections::ImportDesc) -> String {
        use crate::sections::Import;
        let module = Module {
            imports: vec![Import {
                module: "env".to_string(),
                name: "x".to_string(),
                desc,
            }],
            ..Module::default()
        };
        module_to_wat(&module)
    }

    #[test]
    fn import_table_in_wat() {
        use crate::sections::{ImportDesc, Limits, RefType};
        let wat = import_wat(ImportDesc::Table {
            reftype: RefType::FuncRef,
            limits: Limits { min: 1, max: None },
        });
        assert!(wat.contains("(import \"env\" \"x\" (table 1 funcref))"));
    }

    #[test]
    fn import_memory_in_wat() {
        use crate::sections::{ImportDesc, Limits};
        let wat = import_wat(ImportDesc::Memory(Limits {
            min: 1,
            max: Some(2),
        }));
        assert!(wat.contains("(import \"env\" \"x\" (memory 1 2))"));
    }

    #[test]
    fn import_global_mutable_and_immutable_in_wat() {
        use crate::sections::ImportDesc;
        let m = import_wat(ImportDesc::Global {
            valtype: ValType::I32,
            mutable: true,
        });
        assert!(m.contains("(global (mut i32))"));
        let i = import_wat(ImportDesc::Global {
            valtype: ValType::I64,
            mutable: false,
        });
        assert!(i.contains("(global i64)"));
        assert!(!i.contains("(mut"));
    }

    #[test]
    fn local_table_in_wat() {
        use crate::sections::{Limits, RefType, Table};
        let module = Module {
            tables: vec![Table {
                reftype: RefType::FuncRef,
                limits: Limits { min: 2, max: None },
            }],
            ..Module::default()
        };
        assert!(module_to_wat(&module).contains("  (table 2 funcref)\n"));
    }

    #[test]
    fn local_memory_in_wat() {
        use crate::sections::Limits;
        let module = Module {
            memories: vec![Limits {
                min: 1,
                max: Some(4),
            }],
            ..Module::default()
        };
        assert!(module_to_wat(&module).contains("  (memory 1 4)\n"));
    }

    #[test]
    fn local_mutable_global_in_wat() {
        use crate::sections::{ConstExpr, Global, GlobalType};
        let module = Module {
            globals: vec![
                Global {
                    global_type: GlobalType {
                        valtype: ValType::I32,
                        mutable: true,
                    },
                    init: ConstExpr::I32(7),
                },
                Global {
                    global_type: GlobalType {
                        valtype: ValType::I64,
                        mutable: false,
                    },
                    init: ConstExpr::I64(9),
                },
            ],
            ..Module::default()
        };
        let wat = module_to_wat(&module);
        assert!(wat.contains("(global (mut i32) (7))"));
        assert!(wat.contains("(global i64 (9))"));
    }

    #[test]
    fn data_segments_in_wat() {
        use crate::sections::{ConstExpr, DataMode, DataSegment};
        let module = Module {
            data: vec![
                DataSegment {
                    mode: DataMode::Active {
                        memory_index: 0,
                        offset: ConstExpr::I32(16),
                    },
                    bytes: vec![1, 2, 3],
                },
                DataSegment {
                    mode: DataMode::Passive,
                    bytes: vec![1, 2],
                },
            ],
            ..Module::default()
        };
        let wat = module_to_wat(&module);
        assert!(wat.contains("(data (memory 0) (offset 16) \"<3 bytes>\")"));
        assert!(wat.contains("  (data \"<2 bytes>\")"));
    }
}

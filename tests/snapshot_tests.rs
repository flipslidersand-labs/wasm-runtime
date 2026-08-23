use insta::assert_snapshot;
use wasm_runtime::sections::{
    ConstExpr, CustomSection, DataMode, DataSegment, ElementInit, ElementMode, ElementSegment,
    Export, ExportKind, FuncBody, FuncType, Global, GlobalType, Import, ImportDesc, Limits,
    LocalDecl, NameSection, RefType, Table, ValType,
};

// ── 基本型 ────────────────────────────────────────────────

#[test]
fn valtype_display() {
    assert_snapshot!("i32", format!("{}", ValType::I32));
    assert_snapshot!("i64", format!("{}", ValType::I64));
    assert_snapshot!("f32", format!("{}", ValType::F32));
    assert_snapshot!("f64", format!("{}", ValType::F64));
}

#[test]
fn reftype_display() {
    assert_snapshot!("funcref", format!("{}", RefType::FuncRef));
    assert_snapshot!("externref", format!("{}", RefType::ExternRef));
}

// ── FuncType ──────────────────────────────────────────────

#[test]
fn functype_no_params_no_results() {
    let ft = FuncType {
        params: vec![],
        results: vec![],
    };
    assert_snapshot!(format!("{}", ft));
}

#[test]
fn functype_with_params_and_results() {
    let ft = FuncType {
        params: vec![ValType::I32, ValType::I64],
        results: vec![ValType::F32],
    };
    assert_snapshot!(format!("{}", ft));
}

// ── Limits / Table ────────────────────────────────────────

#[test]
fn limits_no_max() {
    assert_snapshot!(format!("{}", Limits { min: 1, max: None }));
}

#[test]
fn limits_with_max() {
    assert_snapshot!(format!(
        "{}",
        Limits {
            min: 0,
            max: Some(4)
        }
    ));
}

#[test]
fn table_funcref() {
    let t = Table {
        reftype: RefType::FuncRef,
        limits: Limits { min: 1, max: None },
    };
    assert_snapshot!(format!("{}", t));
}

// ── Import / ImportDesc ───────────────────────────────────

#[test]
fn import_func() {
    let i = Import {
        module: "env".to_string(),
        name: "log".to_string(),
        desc: ImportDesc::Func(2),
    };
    assert_snapshot!(format!("{}", i));
}

#[test]
fn import_memory() {
    let i = Import {
        module: "env".to_string(),
        name: "mem".to_string(),
        desc: ImportDesc::Memory(Limits {
            min: 1,
            max: Some(4),
        }),
    };
    assert_snapshot!(format!("{}", i));
}

#[test]
fn import_global_mutable() {
    let i = Import {
        module: "env".to_string(),
        name: "g".to_string(),
        desc: ImportDesc::Global {
            valtype: ValType::I32,
            mutable: true,
        },
    };
    assert_snapshot!(format!("{}", i));
}

// ── Export ────────────────────────────────────────────────

#[test]
fn export_func() {
    let e = Export {
        name: "main".to_string(),
        kind: ExportKind::Func,
        index: 0,
    };
    assert_snapshot!(format!("{}", e));
}

#[test]
fn export_memory() {
    let e = Export {
        name: "memory".to_string(),
        kind: ExportKind::Memory,
        index: 0,
    };
    assert_snapshot!(format!("{}", e));
}

// ── FuncBody ──────────────────────────────────────────────

#[test]
fn funcbody_no_locals() {
    let b = FuncBody {
        locals: vec![],
        expr: vec![0x0B],
    };
    assert_snapshot!(format!("{}", b));
}

#[test]
fn funcbody_with_locals() {
    let b = FuncBody {
        locals: vec![
            LocalDecl {
                count: 2,
                valtype: ValType::I32,
            },
            LocalDecl {
                count: 1,
                valtype: ValType::F64,
            },
        ],
        expr: vec![0x01, 0x01, 0x0B],
    };
    assert_snapshot!(format!("{}", b));
}

// ── Global ────────────────────────────────────────────────

#[test]
fn global_immutable_i32() {
    let g = Global {
        global_type: GlobalType {
            valtype: ValType::I32,
            mutable: false,
        },
        init_expr: vec![0x41, 0x2A, 0x0B], // i32.const 42
    };
    assert_snapshot!(format!("{}", g));
}

#[test]
fn global_mutable_i64() {
    let g = Global {
        global_type: GlobalType {
            valtype: ValType::I64,
            mutable: true,
        },
        init_expr: vec![0x42, 0x00, 0x0B], // i64.const 0
    };
    assert_snapshot!(format!("{}", g));
}

// ── DataSegment ───────────────────────────────────────────

#[test]
fn data_passive() {
    let d = DataSegment {
        mode: DataMode::Passive,
        bytes: vec![0x48, 0x65, 0x6C, 0x6C, 0x6F],
    };
    assert_snapshot!(format!("{}", d));
}

#[test]
fn data_active() {
    let d = DataSegment {
        mode: DataMode::Active {
            memory_index: 0,
            offset_expr: vec![0x41, 0x00, 0x0B], // i32.const 0
        },
        bytes: vec![0x01, 0x02],
    };
    assert_snapshot!(format!("{}", d));
}

// ── ElementSegment ────────────────────────────────────────

#[test]
fn element_active_func_indices() {
    let e = ElementSegment {
        mode: ElementMode::Active {
            table_index: 0,
            offset_expr: vec![0x41, 0x00, 0x0B],
        },
        reftype: RefType::FuncRef,
        init: ElementInit::FuncIndices(vec![0, 1, 2]),
    };
    assert_snapshot!(format!("{}", e));
}

#[test]
fn element_passive() {
    let e = ElementSegment {
        mode: ElementMode::Passive,
        reftype: RefType::FuncRef,
        init: ElementInit::FuncIndices(vec![]),
    };
    assert_snapshot!(format!("{}", e));
}

// ── CustomSection ─────────────────────────────────────────

#[test]
fn custom_section_display() {
    let c = CustomSection {
        name: "producers".to_string(),
        bytes: vec![0u8; 16],
    };
    assert_snapshot!(format!("{}", c));
}

// ── NameSection ───────────────────────────────────────────

#[test]
fn name_section_empty() {
    let ns = NameSection {
        module: None,
        functions: vec![],
    };
    assert_snapshot!(format!("{}", ns));
}

#[test]
fn name_section_with_entries() {
    let ns = NameSection {
        module: Some("my_module".to_string()),
        functions: vec![(0, "main".to_string()), (1, "helper".to_string())],
    };
    assert_snapshot!(format!("{}", ns));
}

// ── ConstExpr ─────────────────────────────────────────────

#[test]
fn const_expr_i32() {
    assert_snapshot!(format!("{}", ConstExpr::I32(42)));
}

#[test]
fn const_expr_global_get() {
    assert_snapshot!(format!("{}", ConstExpr::GlobalGet(3)));
}

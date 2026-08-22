use wasm_runtime::module::parse_module;

const EMPTY: &[u8] = include_bytes!("fixtures/empty.wasm");
const MINIMAL: &[u8] = include_bytes!("fixtures/minimal.wasm");
const GLOBALS: &[u8] = include_bytes!("fixtures/globals.wasm");

#[test]
fn empty_wasm_parses() {
    let module = parse_module(EMPTY).expect("empty.wasm should parse");
    assert_eq!(module.version, 1);
    assert!(module.types.is_empty());
    assert!(module.functions.is_empty());
}

#[test]
fn empty_wasm_validates() {
    let module = parse_module(EMPTY).unwrap();
    assert!(module.validate().is_ok());
}

#[test]
fn minimal_wasm_section_counts() {
    let module = parse_module(MINIMAL).expect("minimal.wasm should parse");
    assert_eq!(module.types.len(), 1);
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.code.len(), 1);
    assert!(module.imports.is_empty());
}

#[test]
fn minimal_wasm_export_name() {
    let module = parse_module(MINIMAL).unwrap();
    assert_eq!(module.exports[0].name, "main");
}

#[test]
fn minimal_wasm_validates() {
    let module = parse_module(MINIMAL).unwrap();
    assert!(module.validate().is_ok());
}

#[test]
fn globals_wasm_section_counts() {
    let module = parse_module(GLOBALS).expect("globals.wasm should parse");
    assert_eq!(module.types.len(), 2);
    assert_eq!(module.imports.len(), 2);
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.globals.len(), 1);
    assert_eq!(module.exports.len(), 2);
    assert_eq!(module.code.len(), 1);
}

#[test]
fn globals_wasm_import_modules() {
    let module = parse_module(GLOBALS).unwrap();
    assert!(module.imports.iter().all(|i| i.module == "env"));
}

#[test]
fn globals_wasm_validates() {
    let module = parse_module(GLOBALS).unwrap();
    assert!(module.validate().is_ok());
}

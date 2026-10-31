//! セクションの宣言サイズが、デコードした内容より大きい（末尾に余る）場合の拒否。

use wasm_runtime::module::parse_module;
use wasm_runtime::parser::ParseError;

fn make_wasm(sections: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    for (id, payload) in sections {
        out.push(*id);
        out.push(payload.len() as u8);
        out.extend_from_slice(payload);
    }
    out
}

#[test]
fn type_section_with_trailing_bytes_is_rejected() {
    // count=1 の functype (params=0, results=0) の後ろに余分な 3 バイト。
    let bytes = make_wasm(&[(1, &[0x01, 0x60, 0x00, 0x00, 0x60, 0x00, 0x00])]);
    assert_eq!(parse_module(&bytes).unwrap_err(), ParseError::SizeMismatch);
}

#[test]
fn function_section_with_trailing_bytes_is_rejected() {
    let bytes = make_wasm(&[(3, &[0x01, 0x00, 0xFF])]);
    assert_eq!(parse_module(&bytes).unwrap_err(), ParseError::SizeMismatch);
}

#[test]
fn memory_section_with_trailing_bytes_is_rejected() {
    let bytes = make_wasm(&[(5, &[0x01, 0x00, 0x01, 0x00])]);
    assert_eq!(parse_module(&bytes).unwrap_err(), ParseError::SizeMismatch);
}

#[test]
fn exactly_sized_sections_are_accepted() {
    let bytes = make_wasm(&[
        (1, &[0x01, 0x60, 0x00, 0x00]),
        (3, &[0x01, 0x00]),
        (5, &[0x01, 0x00, 0x01]),
    ]);
    assert!(parse_module(&bytes).is_ok());
}

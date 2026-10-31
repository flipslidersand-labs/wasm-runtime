//! セクションの順序と重複の検査（仕様: 非 custom セクションは決められた順に高々 1 回）。

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
fn duplicate_start_section_is_rejected() {
    let bytes = make_wasm(&[(8, &[0x00]), (8, &[0x00])]);
    assert_eq!(
        parse_module(&bytes).unwrap_err(),
        ParseError::SectionOutOfOrder(8)
    );
}

#[test]
fn out_of_order_sections_are_rejected() {
    // code (10) の後に type (1) は置けない。
    let bytes = make_wasm(&[(10, &[0x00]), (1, &[0x00])]);
    assert_eq!(
        parse_module(&bytes).unwrap_err(),
        ParseError::SectionOutOfOrder(1)
    );
}

#[test]
fn custom_sections_may_appear_anywhere_and_repeat() {
    let custom: &[u8] = &[0x01, b'x'];
    let bytes = make_wasm(&[
        (0, custom),
        (1, &[0x00]),
        (0, custom),
        (10, &[0x00]),
        (0, custom),
    ]);
    assert!(parse_module(&bytes).is_ok());
}

#[test]
fn datacount_comes_before_code_and_data() {
    let ok = make_wasm(&[(12, &[0x00]), (10, &[0x00]), (11, &[0x00])]);
    assert!(parse_module(&ok).is_ok());
    // datacount (12) が code (10) の後ろにあると順序違反。
    let bad = make_wasm(&[(10, &[0x00]), (12, &[0x00])]);
    assert_eq!(
        parse_module(&bad).unwrap_err(),
        ParseError::SectionOutOfOrder(12)
    );
}

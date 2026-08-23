// Tests for Issues #56, #57, #58, #59:
// #56 — 未カバー ParseError バリアント補完テスト
// #57 — ParseError / ValidationError Display メッセージ内容テスト
// #58 — section_iter 切り詰め・不正サイズエラーテスト
// #59 — parse_module エラー伝播テスト

use wasm_runtime::{
    module::{parse_module, ValidationError},
    parser::{section_iter, ParseError},
    sections::{
        decode_code_section, decode_custom_section, decode_data_section, decode_global_section,
        decode_table_section, ExportKind,
    },
};

// ── ヘルパー ─────────────────────────────────────────────

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]
}

fn make_wasm(sections: &[(u8, &[u8])]) -> Vec<u8> {
    let mut b = header();
    for (id, payload) in sections {
        b.push(*id);
        b.push(payload.len() as u8);
        b.extend_from_slice(payload);
    }
    b
}

// ── #56: 未カバーの ParseError バリアント ────────────────

#[test]
fn parse_error_unknown_ref_type() {
    // table section: count=1, reftype=0xFF (invalid), limits_flag=0, min=1
    let payload = [0x01, 0xFF, 0x00, 0x01];
    assert_eq!(
        decode_table_section(&payload),
        Err(ParseError::UnknownRefType(0xFF))
    );
}

#[test]
fn parse_error_invalid_mutability() {
    // global section: count=1, valtype=i32(0x7F), mutability=0x02 (invalid)
    let payload = [0x01, 0x7F, 0x02, 0x41, 0x00, 0x0B];
    assert_eq!(
        decode_global_section(&payload),
        Err(ParseError::InvalidMutability(0x02))
    );
}

#[test]
fn parse_error_invalid_utf8() {
    // custom section: name_len=2, name=[\xFF\xFE] (invalid UTF-8)
    let payload = [0x02, 0xFF, 0xFE];
    assert_eq!(
        decode_custom_section(&payload),
        Err(ParseError::InvalidUtf8)
    );
}

#[test]
fn parse_error_size_mismatch_code_body_too_large() {
    // code section: count=1, body_size=10, but only 2 bytes follow
    let payload = [0x01, 0x0A, 0x00, 0x0B];
    assert_eq!(decode_code_section(&payload), Err(ParseError::SizeMismatch));
}

#[test]
fn parse_error_unsupported_init_expr() {
    // global section: count=1, valtype=i32, mut=false, opcode=0xAA (unsupported)
    let payload = [0x01, 0x7F, 0x00, 0xAA, 0x0B];
    assert_eq!(
        decode_global_section(&payload),
        Err(ParseError::UnsupportedInitExpr(0xAA))
    );
}

#[test]
fn parse_error_unsupported_data_flag() {
    // data section: count=1, flag=0x03 (only 0/1/2 are valid)
    let payload = [0x01, 0x03];
    assert_eq!(
        decode_data_section(&payload),
        Err(ParseError::UnsupportedDataFlag(0x03))
    );
}

// ── #57: ParseError Display メッセージ内容テスト ────────

#[test]
fn display_unexpected_eof() {
    let msg = format!("{}", ParseError::UnexpectedEof);
    assert!(
        msg.contains("unexpected") && msg.contains("end"),
        "got: {}",
        msg
    );
}

#[test]
fn display_invalid_magic() {
    let msg = format!("{}", ParseError::InvalidMagic([0xDE, 0xAD, 0xBE, 0xEF]));
    assert!(msg.contains("magic"), "got: {}", msg);
}

#[test]
fn display_invalid_version() {
    let msg = format!("{}", ParseError::InvalidVersion([0x02, 0x00, 0x00, 0x00]));
    assert!(msg.contains("version"), "got: {}", msg);
}

#[test]
fn display_leb128_overflow() {
    let msg = format!("{}", ParseError::Leb128Overflow);
    assert!(
        msg.contains("LEB128") && msg.contains("overflow"),
        "got: {}",
        msg
    );
}

#[test]
fn display_unknown_val_type() {
    let msg = format!("{}", ParseError::UnknownValType(0xAB));
    assert!(
        msg.contains("value type") && msg.contains("0xAB"),
        "got: {}",
        msg
    );
}

#[test]
fn display_invalid_func_type() {
    let msg = format!("{}", ParseError::InvalidFuncType(0x61));
    assert!(
        msg.contains("0x60") || msg.contains("functype"),
        "got: {}",
        msg
    );
}

#[test]
fn display_unknown_export_kind() {
    let msg = format!("{}", ParseError::UnknownExportKind(0x05));
    assert!(msg.contains("export kind"), "got: {}", msg);
}

#[test]
fn display_unknown_import_kind() {
    let msg = format!("{}", ParseError::UnknownImportKind(0x05));
    assert!(msg.contains("import kind"), "got: {}", msg);
}

#[test]
fn display_unknown_ref_type() {
    let msg = format!("{}", ParseError::UnknownRefType(0xFF));
    assert!(msg.contains("reftype"), "got: {}", msg);
}

#[test]
fn display_unknown_limits_flag() {
    let msg = format!("{}", ParseError::UnknownLimitsFlag(0x03));
    assert!(msg.contains("limits flag"), "got: {}", msg);
}

#[test]
fn display_invalid_mutability() {
    let msg = format!("{}", ParseError::InvalidMutability(0x02));
    assert!(msg.contains("mutability"), "got: {}", msg);
}

#[test]
fn display_invalid_utf8() {
    let msg = format!("{}", ParseError::InvalidUtf8);
    assert!(msg.contains("UTF-8"), "got: {}", msg);
}

#[test]
fn display_size_mismatch() {
    let msg = format!("{}", ParseError::SizeMismatch);
    assert!(
        msg.contains("size") || msg.contains("mismatch"),
        "got: {}",
        msg
    );
}

#[test]
fn display_unsupported_init_expr() {
    let msg = format!("{}", ParseError::UnsupportedInitExpr(0xAA));
    assert!(
        msg.contains("init_expr") || msg.contains("opcode"),
        "got: {}",
        msg
    );
}

#[test]
fn display_unsupported_data_flag() {
    let msg = format!("{}", ParseError::UnsupportedDataFlag(0x03));
    assert!(msg.contains("data") && msg.contains("flag"), "got: {}", msg);
}

#[test]
fn display_unsupported_element_flag() {
    let msg = format!("{}", ParseError::UnsupportedElementFlag(0x08));
    assert!(
        msg.contains("element") && msg.contains("flag"),
        "got: {}",
        msg
    );
}

// ── #57: ValidationError Display メッセージ内容テスト ───

#[test]
fn display_validation_func_code_mismatch() {
    let msg = format!(
        "{}",
        ValidationError::FuncCodeCountMismatch {
            functions: 2,
            code: 1
        }
    );
    assert!(
        msg.contains("function") && msg.contains("code"),
        "got: {}",
        msg
    );
    assert!(msg.contains("2") && msg.contains("1"), "got: {}", msg);
}

#[test]
fn display_validation_type_index_out_of_range() {
    let msg = format!(
        "{}",
        ValidationError::TypeIndexOutOfRange {
            index: 5,
            type_count: 1
        }
    );
    assert!(msg.contains("type") && msg.contains("5"), "got: {}", msg);
}

#[test]
fn display_validation_export_index_out_of_range() {
    let msg = format!(
        "{}",
        ValidationError::ExportIndexOutOfRange {
            name: "foo".to_string(),
            kind: ExportKind::Func,
            index: 9,
            space: 2,
        }
    );
    assert!(msg.contains("foo") && msg.contains("9"), "got: {}", msg);
}

#[test]
fn display_validation_element_table_out_of_range() {
    let msg = format!(
        "{}",
        ValidationError::ElementTableIndexOutOfRange {
            index: 1,
            table_space: 0
        }
    );
    assert!(msg.contains("table") && msg.contains("1"), "got: {}", msg);
}

#[test]
fn display_validation_element_func_out_of_range() {
    let msg = format!(
        "{}",
        ValidationError::ElementFuncIndexOutOfRange {
            index: 3,
            func_space: 1
        }
    );
    assert!(msg.contains("func") && msg.contains("3"), "got: {}", msg);
}

#[test]
fn display_validation_start_func_out_of_range() {
    let msg = format!(
        "{}",
        ValidationError::StartFuncIndexOutOfRange {
            index: 2,
            func_space: 1
        }
    );
    assert!(msg.contains("start") && msg.contains("2"), "got: {}", msg);
}

#[test]
fn display_validation_datacount_mismatch() {
    let msg = format!(
        "{}",
        ValidationError::DataCountMismatch {
            declared: 3,
            actual: 1
        }
    );
    assert!(msg.contains("3") && msg.contains("1"), "got: {}", msg);
}

// ── #58: section_iter 切り詰め・不正サイズテスト ─────────

#[test]
fn section_iter_id_only_no_size() {
    // ヘッダ + section id のみ (サイズバイトなし)
    let mut bytes = header();
    bytes.push(0x01); // id=1, size missing
    let results: Vec<_> = section_iter(&bytes).collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], Err(ParseError::UnexpectedEof));
}

#[test]
fn section_iter_incomplete_leb128_size() {
    // ヘッダ + id + マルチバイト LEB128 の途中で終わる
    let mut bytes = header();
    bytes.extend_from_slice(&[0x01, 0x80]); // id=1, size=0x80 (more bit set, no next byte)
    let results: Vec<_> = section_iter(&bytes).collect();
    // Iterator does not halt on error, so we get at least one UnexpectedEof item.
    assert!(
        results.contains(&Err(ParseError::UnexpectedEof)),
        "expected at least one UnexpectedEof, got: {:?}",
        results
    );
}

#[test]
fn section_iter_empty_module_yields_none() {
    // ヘッダのみ — セクションなし → イテレータは None を返す
    let bytes = header();
    let results: Vec<_> = section_iter(&bytes).collect();
    assert!(results.is_empty());
}

#[test]
fn section_iter_zero_size_section_is_valid() {
    // サイズ=0 のセクション (空ペイロード) は有効
    let mut bytes = header();
    bytes.extend_from_slice(&[0x00, 0x00]); // id=0 (custom), size=0
    let results: Vec<_> = section_iter(&bytes).collect();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
}

#[test]
fn section_iter_two_byte_leb128_size() {
    // 2バイト LEB128 のサイズ (128以上) を持つセクションを正しくパースできる
    let type_payload = vec![0x01u8, 0x60, 0x00, 0x00]; // 4 bytes
    let mut bytes = header();
    bytes.push(0x01); // id=1 (type)
    bytes.push(0x84); // LEB128 low byte: 4, more bit set
    bytes.push(0x00); // LEB128 high byte: 0 → value=4
    bytes.extend_from_slice(&type_payload);
    let results: Vec<_> = section_iter(&bytes).collect();
    assert_eq!(results.len(), 1);
    let hdr = results[0].as_ref().unwrap();
    assert_eq!(hdr.id, 1);
    assert_eq!(hdr.size, 4);
}

#[test]
fn section_iter_multiple_sections_with_id_truncation() {
    // 1つ目は正常、2つ目は id だけで終わる
    let mut bytes = header();
    bytes.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // valid type section
    bytes.push(0x03); // 2つ目のセクション id だけ
    let results: Vec<_> = section_iter(&bytes).collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert_eq!(results[1], Err(ParseError::UnexpectedEof));
}

// ── #59: parse_module エラー伝播テスト ──────────────────

#[test]
fn parse_module_propagates_unknown_export_kind() {
    // export section: count=1, name="f"(1 byte), kind=0x05 (invalid)
    let bad_export = &[0x01, 0x01, b'f', 0x05, 0x00];
    let bytes = make_wasm(&[(7, bad_export)]);
    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::UnknownExportKind(0x05))
    );
}

#[test]
fn parse_module_propagates_unknown_val_type() {
    // type section: count=1, func marker=0x60, 1 param=0xAB (invalid valtype)
    let bad_type = &[0x01, 0x60, 0x01, 0xAB, 0x00];
    let bytes = make_wasm(&[(1, bad_type)]);
    assert_eq!(parse_module(&bytes), Err(ParseError::UnknownValType(0xAB)));
}

#[test]
fn parse_module_propagates_invalid_utf8() {
    // custom section: name_len=1, name=0xFF (invalid UTF-8)
    let bad_custom = &[0x01, 0xFF];
    let bytes = make_wasm(&[(0, bad_custom)]);
    assert_eq!(parse_module(&bytes), Err(ParseError::InvalidUtf8));
}

#[test]
fn parse_module_propagates_code_size_mismatch() {
    // type + func + code section with mismatched body size
    let type_payload = &[0x01, 0x60, 0x00, 0x00]; // 1 type: () -> ()
    let func_payload = &[0x01, 0x00]; // 1 func: type 0
    let bad_code = &[0x01, 0x0A, 0x00, 0x0B]; // body_size=10, only 2 bytes
    let bytes = make_wasm(&[(1, type_payload), (3, func_payload), (10, bad_code)]);
    assert_eq!(parse_module(&bytes), Err(ParseError::SizeMismatch));
}

#[test]
fn parse_module_propagates_unsupported_data_flag() {
    let bad_data = &[0x01, 0x03]; // flag=3 (unsupported)
    let bytes = make_wasm(&[(11, bad_data)]);
    assert_eq!(
        parse_module(&bytes),
        Err(ParseError::UnsupportedDataFlag(0x03))
    );
}

#[test]
fn parse_module_invalid_magic_returns_error() {
    let bad = [0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00];
    assert_eq!(
        parse_module(&bad),
        Err(ParseError::InvalidMagic([0xFF, 0xFF, 0xFF, 0xFF]))
    );
}

#[test]
fn parse_module_empty_bytes_returns_eof() {
    assert_eq!(parse_module(&[]), Err(ParseError::UnexpectedEof));
}

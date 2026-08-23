// Tests for Issue #60: ParseError context wrapper (offset + section_id)

use wasm_runtime::{
    module::parse_module_with_context,
    parser::{ParseError, ParseErrorContext},
};

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

// ── ParseErrorContext struct ──────────────────────────────────────────────

#[test]
fn context_new_has_no_location() {
    let ctx = ParseErrorContext::new(ParseError::UnexpectedEof);
    assert!(ctx.offset.is_none());
    assert!(ctx.section_id.is_none());
    assert_eq!(ctx.error, ParseError::UnexpectedEof);
}

#[test]
fn context_with_offset() {
    let ctx = ParseErrorContext::new(ParseError::SizeMismatch).with_offset(42);
    assert_eq!(ctx.offset, Some(42));
    assert!(ctx.section_id.is_none());
}

#[test]
fn context_with_section() {
    let ctx = ParseErrorContext::new(ParseError::InvalidUtf8).with_section(7);
    assert_eq!(ctx.section_id, Some(7));
    assert!(ctx.offset.is_none());
}

#[test]
fn context_with_both() {
    let ctx = ParseErrorContext::new(ParseError::UnknownValType(0xAB))
        .with_offset(100)
        .with_section(1);
    assert_eq!(ctx.offset, Some(100));
    assert_eq!(ctx.section_id, Some(1));
}

// ── Display ──────────────────────────────────────────────────────────────

#[test]
fn display_no_context_falls_back_to_underlying_error() {
    let ctx = ParseErrorContext::new(ParseError::UnexpectedEof);
    let s = format!("{}", ctx);
    assert!(s.contains("unexpected"), "got: {}", s);
}

#[test]
fn display_with_offset_includes_offset() {
    let ctx = ParseErrorContext::new(ParseError::UnexpectedEof).with_offset(42);
    let s = format!("{}", ctx);
    assert!(s.contains("42"), "got: {}", s);
    assert!(s.contains("unexpected"), "got: {}", s);
}

#[test]
fn display_with_section_includes_section_id() {
    let ctx = ParseErrorContext::new(ParseError::SizeMismatch).with_section(10);
    let s = format!("{}", ctx);
    assert!(s.contains("10"), "got: {}", s);
    assert!(s.contains("size") || s.contains("mismatch"), "got: {}", s);
}

#[test]
fn display_with_both_includes_offset_and_section() {
    let ctx = ParseErrorContext::new(ParseError::InvalidUtf8)
        .with_offset(20)
        .with_section(0);
    let s = format!("{}", ctx);
    assert!(s.contains("20"), "got: {}", s);
    assert!(s.contains("0"), "got: {}", s);
    assert!(s.contains("UTF-8"), "got: {}", s);
}

// ── parse_module_with_context ─────────────────────────────────────────────

#[test]
fn valid_module_returns_ok() {
    let bytes = header();
    assert!(parse_module_with_context(&bytes).is_ok());
}

#[test]
fn invalid_magic_includes_offset_zero() {
    let bad = [0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00];
    let err = parse_module_with_context(&bad).unwrap_err();
    assert!(matches!(err.error, ParseError::InvalidMagic(_)));
    assert_eq!(err.offset, Some(0));
    assert!(err.section_id.is_none());
}

#[test]
fn section_error_includes_section_id() {
    // export section with invalid kind byte 0x05
    let bad_export = &[0x01, 0x01, b'f', 0x05, 0x00];
    let bytes = make_wasm(&[(7, bad_export)]);
    let err = parse_module_with_context(&bytes).unwrap_err();
    assert_eq!(err.error, ParseError::UnknownExportKind(0x05));
    assert_eq!(err.section_id, Some(7));
    assert!(err.offset.is_some(), "offset should be set");
}

#[test]
fn section_error_display_contains_section_id_and_message() {
    let bad_type = &[0x01, 0x60, 0x01, 0xAB, 0x00];
    let bytes = make_wasm(&[(1, bad_type)]);
    let err = parse_module_with_context(&bytes).unwrap_err();
    let s = format!("{}", err);
    assert!(
        s.contains("1") && (s.contains("value type") || s.contains("0xAB")),
        "got: {}",
        s
    );
}

#[test]
fn section_error_offset_matches_payload_start() {
    // type section starts at byte 10 (8 header + 1 id + 1 size)
    let type_payload = &[0x01u8, 0x60, 0x01, 0xAB, 0x00];
    let bytes = make_wasm(&[(1, type_payload)]);
    let err = parse_module_with_context(&bytes).unwrap_err();
    // offset = 10 for a single section starting right after the 8-byte header
    assert_eq!(err.offset, Some(10));
}

#[test]
fn error_source_is_underlying_parse_error() {
    use std::error::Error;
    let ctx = ParseErrorContext::new(ParseError::Leb128Overflow);
    assert!(ctx.source().is_some());
}

// Tests for Issue #196: section events and contextual errors in one call.

use wasm_runtime::module::{
    parse_module, parse_module_with_context, parse_module_with_events, parse_module_with_options,
    ParseOptions, SectionEvent,
};
use wasm_runtime::parser::ParseError;

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

fn minimal_wasm() -> Vec<u8> {
    make_wasm(&[
        (1, &[0x01, 0x60, 0x00, 0x00]),  // type: () -> ()
        (3, &[0x01, 0x00]),              // func: type[0]
        (10, &[0x01, 0x02, 0x00, 0x0B]), // code: 1 body, end
    ])
}

// Valid type section followed by a function section (id 3) that declares two
// entries but holds only one: decoding its payload fails with UnexpectedEof.
fn bad_second_section() -> Vec<u8> {
    make_wasm(&[(1, &[0x01, 0x60, 0x00, 0x00]), (3, &[0x02, 0x00])])
}

fn bad_payload() -> Vec<u8> {
    make_wasm(&[(1, &[0x01, 0x60, 0x00, 0x00]), (2, &[0x01])])
}

#[test]
fn events_and_context_together_on_payload_error() {
    let bytes = bad_payload();
    let mut events: Vec<SectionEvent> = Vec::new();
    let err = parse_module_with_options(&bytes, ParseOptions::new().on_section(|e| events.push(e)))
        .unwrap_err();

    let expected = parse_module_with_context(&bytes).unwrap_err();
    assert_eq!(err.error, expected.error);
    assert_eq!(err.offset, expected.offset);
    assert_eq!(err.section_id, expected.section_id);
    assert_eq!(err.section_id, Some(2));
    assert!(err.offset.is_some());

    // Events fired for both sections, including the one that failed.
    let ids: Vec<u8> = events.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(err.offset, Some(events[1].offset));
}

#[test]
fn section_iter_error_has_location_and_fires_no_events() {
    // Section id 1 whose size LEB128 has the continuation bit set but is cut
    // off: the section header itself is malformed. The location is that of
    // the section's id byte (offset 8, right after the module header).
    let mut bytes = header();
    bytes.extend_from_slice(&[0x01, 0x80]);

    let mut events: Vec<SectionEvent> = Vec::new();
    let err = parse_module_with_options(&bytes, ParseOptions::new().on_section(|e| events.push(e)))
        .unwrap_err();
    assert_eq!(err.offset, Some(8));
    assert_eq!(err.section_id, Some(1));
    assert!(events.is_empty());

    let expected = parse_module_with_context(&bytes).unwrap_err();
    assert_eq!(err.error, expected.error);
    assert_eq!(err.offset, expected.offset);
    assert_eq!(err.section_id, expected.section_id);
}

#[test]
fn section_out_of_order_error_has_location() {
    // type section (id 1, empty) then a second type section: out of order.
    let bytes = make_wasm(&[(1, &[0x00]), (1, &[0x00])]);
    let err = parse_module_with_context(&bytes).unwrap_err();
    assert_eq!(err.error, ParseError::SectionOutOfOrder(1));
    // header 8 + first section (id, size, 1 payload byte) = 11
    assert_eq!(err.offset, Some(11));
    assert_eq!(err.section_id, Some(1));
}

#[test]
fn section_size_exceeds_input_error_has_location() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x0A, 0x05, 0x00]);
    let err = parse_module_with_context(&bytes).unwrap_err();
    assert_eq!(err.error, ParseError::UnexpectedEof);
    assert_eq!(err.offset, Some(8));
    assert_eq!(err.section_id, Some(10));
}

#[test]
fn header_error_has_offset_zero() {
    let err = parse_module_with_options(&[0x00, 0x61], ParseOptions::new()).unwrap_err();
    assert_eq!(err.offset, Some(0));
    assert_eq!(err.section_id, None);
}

#[test]
fn default_options_match_parse_module() {
    for bytes in [
        header(),
        minimal_wasm(),
        bad_payload(),
        bad_second_section(),
        vec![0x00, 0x61, 0x73],
    ] {
        let plain = parse_module(&bytes);
        let opts = parse_module_with_options(&bytes, ParseOptions::default()).map_err(|e| e.error);
        assert_eq!(plain, opts);
    }
}

#[test]
fn options_events_match_parse_module_with_events() {
    for bytes in [minimal_wasm(), bad_payload()] {
        let mut a: Vec<SectionEvent> = Vec::new();
        let mut b: Vec<SectionEvent> = Vec::new();
        let ra = parse_module_with_events(&bytes, |e| a.push(e));
        let rb = parse_module_with_options(&bytes, ParseOptions::new().on_section(|e| b.push(e)));
        assert_eq!(ra, rb.map_err(|e| e.error));
        assert_eq!(a, b);
    }
}

#[test]
fn existing_wrappers_unchanged() {
    let bytes = bad_payload();
    assert!(matches!(
        parse_module(&bytes),
        Err(ParseError::UnexpectedEof)
    ));
    let ctx = parse_module_with_context(&bytes).unwrap_err();
    assert_eq!(ctx.error, ParseError::UnexpectedEof);
    assert_eq!(ctx.section_id, Some(2));
    assert_eq!(ctx.offset, Some(16));
    assert_eq!(
        parse_module(&minimal_wasm()).unwrap(),
        parse_module_with_context(&minimal_wasm()).unwrap()
    );
}

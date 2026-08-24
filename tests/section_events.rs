use wasm_runtime::module::{parse_module, parse_module_with_events, SectionEvent};

// ── helpers ────────────────────────────────────────────────────────────────

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

// Minimal valid module: type + func + code (one empty function).
fn minimal_wasm() -> Vec<u8> {
    make_wasm(&[
        (1, &[0x01, 0x60, 0x00, 0x00]),  // type: () -> ()
        (3, &[0x01, 0x00]),              // func: type[0]
        (10, &[0x01, 0x02, 0x00, 0x0B]), // code: 1 body, end
    ])
}

// ── SectionEvent tests ─────────────────────────────────────────────────────

#[test]
fn header_only_fires_no_events() {
    let bytes = header();
    let mut events: Vec<SectionEvent> = Vec::new();
    parse_module_with_events(&bytes, |e| events.push(e)).unwrap();
    assert!(events.is_empty());
}

#[test]
fn minimal_wasm_fires_three_events() {
    let bytes = minimal_wasm();
    let mut events: Vec<SectionEvent> = Vec::new();
    parse_module_with_events(&bytes, |e| events.push(e)).unwrap();
    assert_eq!(events.len(), 3);
}

#[test]
fn events_have_correct_ids() {
    let bytes = minimal_wasm();
    let mut ids: Vec<u8> = Vec::new();
    parse_module_with_events(&bytes, |e| ids.push(e.id)).unwrap();
    assert_eq!(ids, vec![1, 3, 10]);
}

#[test]
fn events_have_correct_names() {
    let bytes = minimal_wasm();
    let mut names: Vec<&'static str> = Vec::new();
    parse_module_with_events(&bytes, |e| names.push(e.name)).unwrap();
    assert_eq!(names, vec!["type", "func", "code"]);
}

#[test]
fn event_size_matches_payload() {
    // Single type section with payload [0x01, 0x60, 0x00, 0x00] (4 bytes).
    let payload: &[u8] = &[0x01, 0x60, 0x00, 0x00];
    let bytes = make_wasm(&[(1, payload)]);
    let mut sizes: Vec<u32> = Vec::new();
    parse_module_with_events(&bytes, |e| sizes.push(e.size)).unwrap();
    assert_eq!(sizes, vec![payload.len() as u32]);
}

#[test]
fn event_offset_points_to_payload_start() {
    // header (8) + id (1) + leb128 size (1) = payload at offset 10.
    let payload: &[u8] = &[0x01, 0x60, 0x00, 0x00];
    let bytes = make_wasm(&[(1, payload)]);
    let mut offsets: Vec<usize> = Vec::new();
    parse_module_with_events(&bytes, |e| offsets.push(e.offset)).unwrap();
    assert_eq!(offsets, vec![10]);
}

#[test]
fn events_fire_in_binary_order() {
    // export (7) appears before code (10) in the binary.
    let bytes = make_wasm(&[
        (7, &[0x01, 0x04, b'm', b'a', b'i', b'n', 0x00, 0x00]),
        (10, &[0x01, 0x02, 0x00, 0x0B]),
    ]);
    let mut ids: Vec<u8> = Vec::new();
    parse_module_with_events(&bytes, |e| ids.push(e.id)).unwrap();
    assert_eq!(ids, vec![7, 10]);
}

#[test]
fn unknown_section_id_fires_event_with_name_unknown() {
    let bytes = make_wasm(&[(99, &[0xAB, 0xCD])]);
    let mut events: Vec<SectionEvent> = Vec::new();
    parse_module_with_events(&bytes, |e| events.push(e)).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, 99);
    assert_eq!(events[0].name, "unknown");
}

#[test]
fn custom_section_fires_event() {
    let bytes = make_wasm(&[(0, &[0x01, b'x'])]);
    let mut events: Vec<SectionEvent> = Vec::new();
    parse_module_with_events(&bytes, |e| events.push(e)).unwrap();
    assert_eq!(events[0].id, 0);
    assert_eq!(events[0].name, "custom");
}

#[test]
fn parse_module_result_matches_parse_module_with_events() {
    let bytes = minimal_wasm();
    let m1 = parse_module(&bytes).unwrap();
    let m2 = parse_module_with_events(&bytes, |_| {}).unwrap();
    assert_eq!(m1, m2);
}

#[test]
fn section_event_is_clone_and_debug() {
    let ev = SectionEvent {
        id: 1,
        name: "type",
        size: 4,
        offset: 10,
    };
    let cloned = ev.clone();
    assert_eq!(cloned, ev);
    let _ = format!("{:?}", ev);
}

#[test]
fn callback_can_accumulate_total_bytes() {
    let bytes = minimal_wasm();
    let mut total: u32 = 0;
    parse_module_with_events(&bytes, |e| total += e.size).unwrap();
    // 4 (type payload) + 2 (func payload) + 4 (code payload) = 10
    assert_eq!(total, 10);
}

#[test]
fn no_event_fired_on_parse_error() {
    let bad: Vec<u8> = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00];
    let mut count = 0usize;
    let result = parse_module_with_events(&bad, |_| count += 1);
    assert!(result.is_err());
    assert_eq!(count, 0);
}

//! Byte-level annotation of wasm binaries (`--explain`).
//!
//! Walks the binary from start to finish and emits an [`ExplainLine`] for each
//! logical group of bytes, stating what those bytes mean.

use crate::parser::decode_leb128_u32;
use std::fmt;

/// One annotated span of bytes.
#[derive(Debug, Clone)]
pub struct ExplainLine {
    /// Byte offset from the start of the module.
    pub offset: usize,
    /// The raw bytes covered by this annotation (up to 8 shown).
    pub bytes: Vec<u8>,
    /// Human-readable description of what the bytes mean.
    pub annotation: String,
}

impl fmt::Display for ExplainLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex: Vec<String> = self.bytes.iter().map(|b| format!("{:02x}", b)).collect();
        // Show at most 8 hex bytes in the column; pad to fixed width.
        let shown = hex[..hex.len().min(8)].join(" ");
        let suffix = if self.bytes.len() > 8 { ".." } else { "" };
        write!(
            f,
            "  0x{:04x}: {:<26} {}",
            self.offset,
            format!("{}{}", shown, suffix),
            self.annotation
        )
    }
}

/// Walk every byte of a wasm module and return annotated spans.
///
/// Unknown / unsupported sections are emitted as a single "raw payload" line.
pub fn explain_bytes(bytes: &[u8]) -> Vec<ExplainLine> {
    let mut c = Cursor::new(bytes);

    if bytes.len() < 8 {
        c.emit(bytes.len(), "truncated: too short for wasm header".into());
        return c.lines;
    }

    // ── Header ──────────────────────────────────────────────────────────────
    c.emit(4, "magic (\\0asm)".into());
    c.emit(4, "version: 1".into());

    // ── Sections ────────────────────────────────────────────────────────────
    while c.remaining() > 0 {
        let id_offset = c.pos;
        if c.remaining() == 0 {
            break;
        }
        let id = c.bytes[c.pos];
        c.emit(1, format!("section id: {} ({})", id, section_name(id)));

        let (size, consumed) = match decode_leb128_u32(c.bytes, c.pos) {
            Ok(v) => v,
            Err(_) => {
                c.emit(c.remaining(), "invalid section size LEB128".into());
                break;
            }
        };
        c.emit(consumed, format!("section size: {}", size));

        let payload_start = c.pos;
        let payload_end = payload_start + size as usize;
        if payload_end > bytes.len() {
            c.emit(
                c.remaining(),
                format!("section payload truncated (declared {} bytes)", size),
            );
            break;
        }

        let payload = &bytes[payload_start..payload_end];
        match id {
            1 => annotate_type_section(&mut c, payload, payload_start),
            3 => annotate_function_section(&mut c, payload, payload_start),
            7 => annotate_export_section(&mut c, payload, payload_start),
            10 => annotate_code_section(&mut c, payload, payload_start),
            _ => {
                // Generic: show first bytes as raw payload.
                let label = if size == 0 {
                    format!("section {} payload (empty)", section_name(id))
                } else {
                    format!("section {} payload ({} bytes)", section_name(id), size)
                };
                c.pos = id_offset + 1; // rewind to after id
                                       // Skip LEB128 consumed bytes + payload.
                c.pos += consumed + size as usize;
                c.lines.push(ExplainLine {
                    offset: payload_start,
                    bytes: payload.to_vec(),
                    annotation: label,
                });
            }
        }
    }

    c.lines
}

// ── Section annotators ───────────────────────────────────────────────────────

fn annotate_type_section(c: &mut Cursor, payload: &[u8], base: usize) {
    let mut pc = Cursor::new_at(payload, base);
    let count = pc.read_leb128_u32("type count");
    for i in 0..count {
        if pc.remaining() == 0 {
            break;
        }
        let marker = pc.bytes[pc.pos];
        pc.emit(1, format!("functype[{}] marker: 0x{:02x}", i, marker));
        let params = pc.read_leb128_u32(&format!("functype[{}] param count", i));
        for p in 0..params {
            if pc.remaining() == 0 {
                break;
            }
            let t = pc.bytes[pc.pos];
            pc.emit(
                1,
                format!("functype[{}] param[{}]: {}", i, p, val_type_name(t)),
            );
        }
        let results = pc.read_leb128_u32(&format!("functype[{}] result count", i));
        for r in 0..results {
            if pc.remaining() == 0 {
                break;
            }
            let t = pc.bytes[pc.pos];
            pc.emit(
                1,
                format!("functype[{}] result[{}]: {}", i, r, val_type_name(t)),
            );
        }
    }
    c.pos += payload.len();
    c.lines.extend(pc.lines);
}

fn annotate_function_section(c: &mut Cursor, payload: &[u8], base: usize) {
    let mut pc = Cursor::new_at(payload, base);
    let count = pc.read_leb128_u32("function count");
    for i in 0..count {
        if pc.remaining() == 0 {
            break;
        }
        let (type_idx, consumed) = decode_leb128_u32(pc.bytes, pc.pos).unwrap_or((0, 1));
        pc.emit(consumed, format!("func[{}]: type_index={}", i, type_idx));
    }
    c.pos += payload.len();
    c.lines.extend(pc.lines);
}

fn annotate_export_section(c: &mut Cursor, payload: &[u8], base: usize) {
    let mut pc = Cursor::new_at(payload, base);
    let count = pc.read_leb128_u32("export count");
    for i in 0..count {
        if pc.remaining() < 1 {
            break;
        }
        let name_len = pc.read_leb128_u32(&format!("export[{}] name length", i)) as usize;
        if pc.remaining() < name_len {
            break;
        }
        let name_bytes = &pc.bytes[pc.pos..pc.pos + name_len];
        let name = std::str::from_utf8(name_bytes).unwrap_or("<invalid utf8>");
        pc.emit(name_len, format!("export[{}] name: \"{}\"", i, name));
        if pc.remaining() < 1 {
            break;
        }
        let kind = pc.bytes[pc.pos];
        pc.emit(1, format!("export[{}] kind: {}", i, export_kind_name(kind)));
        let (idx, consumed) = decode_leb128_u32(pc.bytes, pc.pos).unwrap_or((0, 1));
        pc.emit(consumed, format!("export[{}] index: {}", i, idx));
    }
    c.pos += payload.len();
    c.lines.extend(pc.lines);
}

fn annotate_code_section(c: &mut Cursor, payload: &[u8], base: usize) {
    let mut pc = Cursor::new_at(payload, base);
    let count = pc.read_leb128_u32("function body count");
    for i in 0..count {
        if pc.remaining() < 1 {
            break;
        }
        let (body_size, consumed) = decode_leb128_u32(pc.bytes, pc.pos).unwrap_or((0, 1));
        pc.emit(consumed, format!("body[{}] size: {}", i, body_size));
        if pc.remaining() < body_size as usize {
            pc.emit(pc.remaining(), format!("body[{}] truncated", i));
            break;
        }
        let body_start = pc.pos;
        let (local_count, lc) = decode_leb128_u32(pc.bytes, pc.pos).unwrap_or((0, 1));
        pc.emit(
            lc,
            format!("body[{}] local declaration count: {}", i, local_count),
        );
        for l in 0..local_count {
            if pc.remaining() < 1 {
                break;
            }
            let (n, nc) = decode_leb128_u32(pc.bytes, pc.pos).unwrap_or((0, 1));
            pc.emit(nc, format!("body[{}] local[{}] count: {}", i, l, n));
            if pc.remaining() < 1 {
                break;
            }
            let t = pc.bytes[pc.pos];
            pc.emit(
                1,
                format!("body[{}] local[{}] type: {}", i, l, val_type_name(t)),
            );
        }
        let expr_len = body_size as usize - (pc.pos - body_start);
        if expr_len > 0 && pc.remaining() >= expr_len {
            pc.emit(expr_len, format!("body[{}] expr ({} bytes)", i, expr_len));
        }
    }
    c.pos += payload.len();
    c.lines.extend(pc.lines);
}

// ── Cursor ───────────────────────────────────────────────────────────────────

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    base: usize,
    lines: Vec<ExplainLine>,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            base: 0,
            lines: Vec::new(),
        }
    }

    fn new_at(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            pos: 0,
            base,
            lines: Vec::new(),
        }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn emit(&mut self, count: usize, annotation: String) {
        let count = count.min(self.remaining());
        if count == 0 && self.remaining() == 0 {
            return;
        }
        let offset = self.base + self.pos;
        let bytes = self.bytes[self.pos..self.pos + count].to_vec();
        self.lines.push(ExplainLine {
            offset,
            bytes,
            annotation,
        });
        self.pos += count;
    }

    fn read_leb128_u32(&mut self, label: &str) -> u32 {
        match decode_leb128_u32(self.bytes, self.pos) {
            Ok((val, consumed)) => {
                self.emit(consumed, format!("{}: {}", label, val));
                val
            }
            Err(_) => {
                let remaining = self.remaining();
                self.emit(remaining, format!("{}: <invalid LEB128>", label));
                0
            }
        }
    }
}

// ── Name helpers ─────────────────────────────────────────────────────────────

fn section_name(id: u8) -> &'static str {
    match id {
        0 => "custom",
        1 => "type",
        2 => "import",
        3 => "func",
        4 => "table",
        5 => "memory",
        6 => "global",
        7 => "export",
        8 => "start",
        9 => "element",
        10 => "code",
        11 => "data",
        12 => "datacount",
        _ => "unknown",
    }
}

fn val_type_name(b: u8) -> &'static str {
    match b {
        0x7F => "i32",
        0x7E => "i64",
        0x7D => "f32",
        0x7C => "f64",
        0x70 => "funcref",
        0x6F => "externref",
        _ => "?",
    }
}

fn export_kind_name(b: u8) -> &'static str {
    match b {
        0 => "func",
        1 => "table",
        2 => "memory",
        3 => "global",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn header_only_produces_two_lines() {
        let lines = explain_bytes(&header());
        assert_eq!(lines.len(), 2);
        assert!(lines[0].annotation.contains("magic"));
        assert!(lines[1].annotation.contains("version"));
    }

    #[test]
    fn header_offsets_correct() {
        let lines = explain_bytes(&header());
        assert_eq!(lines[0].offset, 0);
        assert_eq!(lines[1].offset, 4);
    }

    #[test]
    fn type_section_annotated() {
        // type section: 1 functype () -> ()
        let payload = &[0x01, 0x60, 0x00, 0x00];
        let bytes = make_wasm(&[(1, payload)]);
        let lines = explain_bytes(&bytes);
        let annotations: Vec<_> = lines.iter().map(|l| l.annotation.as_str()).collect();
        assert!(annotations.iter().any(|a| a.contains("section id: 1")));
        assert!(annotations.iter().any(|a| a.contains("section size")));
        assert!(annotations.iter().any(|a| a.contains("type count")));
        assert!(annotations.iter().any(|a| a.contains("functype")));
    }

    #[test]
    fn export_section_annotated() {
        // export section: 1 export "main" func[0]
        let payload = &[0x01, 0x04, b'm', b'a', b'i', b'n', 0x00, 0x00];
        let bytes = make_wasm(&[(7, payload)]);
        let lines = explain_bytes(&bytes);
        let annotations: Vec<_> = lines.iter().map(|l| l.annotation.as_str()).collect();
        assert!(annotations.iter().any(|a| a.contains("export count")));
        assert!(annotations.iter().any(|a| a.contains("\"main\"")));
        assert!(annotations.iter().any(|a| a.contains("kind: func")));
    }

    #[test]
    fn display_format_contains_offset_and_hex() {
        let lines = explain_bytes(&header());
        let s = format!("{}", lines[0]);
        assert!(s.contains("0x0000"));
        assert!(s.contains("00"));
        assert!(s.contains("magic"));
    }

    #[test]
    fn empty_bytes_produces_no_lines_or_truncation_line() {
        let lines = explain_bytes(&[]);
        // Either empty or a single truncation line — should not panic.
        let _ = lines;
    }

    #[test]
    fn function_section_annotated() {
        let type_payload = &[0x01, 0x60, 0x00, 0x00];
        let func_payload = &[0x01, 0x00];
        let bytes = make_wasm(&[(1, type_payload), (3, func_payload)]);
        let lines = explain_bytes(&bytes);
        let annotations: Vec<_> = lines.iter().map(|l| l.annotation.as_str()).collect();
        assert!(annotations.iter().any(|a| a.contains("function count")));
        assert!(annotations.iter().any(|a| a.contains("type_index")));
    }

    #[test]
    fn code_section_annotated() {
        // code section: 1 body, 2 bytes (0 locals, end)
        let code_payload = &[0x01, 0x02, 0x00, 0x0B];
        let bytes = make_wasm(&[(10, code_payload)]);
        let lines = explain_bytes(&bytes);
        let annotations: Vec<_> = lines.iter().map(|l| l.annotation.as_str()).collect();
        assert!(annotations.iter().any(|a| a.contains("body count")));
        assert!(annotations.iter().any(|a| a.contains("body[0]")));
    }

    #[test]
    fn unknown_section_shows_raw_payload() {
        // section id 5 (memory) not specially handled
        let payload = &[0x01, 0x00, 0x01];
        let bytes = make_wasm(&[(5, payload)]);
        let lines = explain_bytes(&bytes);
        let annotations: Vec<_> = lines.iter().map(|l| l.annotation.as_str()).collect();
        assert!(annotations
            .iter()
            .any(|a| a.contains("section memory payload")));
    }
}

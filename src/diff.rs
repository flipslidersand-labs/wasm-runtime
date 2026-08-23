//! Section-level diff between two wasm binaries.

use crate::parser::{section_iter, ParseError};
use std::fmt;

/// Status of a section when comparing two wasm binaries.
#[derive(Debug, PartialEq, Clone)]
pub enum DiffStatus {
    /// Present in both with identical byte size.
    Equal { size: u32 },
    /// Present in both but with different byte sizes.
    Changed { size_a: u32, size_b: u32 },
    /// Present only in `a`.
    OnlyInA { size: u32 },
    /// Present only in `b`.
    OnlyInB { size: u32 },
}

/// One entry in the section-level diff result.
#[derive(Debug, PartialEq, Clone)]
pub struct SectionDiff {
    pub id: u8,
    pub name: &'static str,
    pub status: DiffStatus,
}

impl fmt::Display for SectionDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let marker = match &self.status {
            DiffStatus::Equal { .. } => "=",
            DiffStatus::Changed { .. } => "~",
            DiffStatus::OnlyInA { .. } => "-",
            DiffStatus::OnlyInB { .. } => "+",
        };
        let detail = match &self.status {
            DiffStatus::Equal { size } => format!("({} bytes)", size),
            DiffStatus::Changed { size_a, size_b } => {
                format!("({} → {} bytes)", size_a, size_b)
            }
            DiffStatus::OnlyInA { size } => format!("({} bytes, removed in b)", size),
            DiffStatus::OnlyInB { size } => format!("({} bytes, added in b)", size),
        };
        write!(f, "[{}] {:>10}  {}", marker, self.name, detail)
    }
}

/// Compare two wasm byte slices at section granularity.
///
/// Returns a list of [`SectionDiff`] entries, one per unique section id that
/// appears in either binary. Sections that appear in both with the same byte
/// size are reported as [`DiffStatus::Equal`]. Only the size is compared, not
/// the decoded contents — this is intentionally lightweight.
pub fn diff_modules(a: &[u8], b: &[u8]) -> Result<Vec<SectionDiff>, ParseError> {
    let map_a = collect_sections(a)?;
    let map_b = collect_sections(b)?;

    // Gather all distinct section ids from both sides, in id order.
    let mut ids: Vec<u8> = {
        let mut set: std::collections::BTreeSet<u8> = Default::default();
        set.extend(map_a.keys());
        set.extend(map_b.keys());
        set.into_iter().collect()
    };
    ids.sort_unstable();

    let mut result = Vec::new();
    for id in ids {
        let size_a = map_a.get(&id).copied();
        let size_b = map_b.get(&id).copied();
        let status = match (size_a, size_b) {
            (Some(sa), Some(sb)) if sa == sb => DiffStatus::Equal { size: sa },
            (Some(sa), Some(sb)) => DiffStatus::Changed {
                size_a: sa,
                size_b: sb,
            },
            (Some(sa), None) => DiffStatus::OnlyInA { size: sa },
            (None, Some(sb)) => DiffStatus::OnlyInB { size: sb },
            (None, None) => unreachable!(),
        };
        result.push(SectionDiff {
            id,
            name: section_name(id),
            status,
        });
    }

    Ok(result)
}

/// Returns a human-readable summary line of the diff result.
pub fn diff_summary(diffs: &[SectionDiff]) -> String {
    let equal = diffs
        .iter()
        .filter(|d| matches!(d.status, DiffStatus::Equal { .. }))
        .count();
    let changed = diffs
        .iter()
        .filter(|d| matches!(d.status, DiffStatus::Changed { .. }))
        .count();
    let added = diffs
        .iter()
        .filter(|d| matches!(d.status, DiffStatus::OnlyInB { .. }))
        .count();
    let removed = diffs
        .iter()
        .filter(|d| matches!(d.status, DiffStatus::OnlyInA { .. }))
        .count();
    format!(
        "{} equal, {} changed, {} added, {} removed",
        equal, changed, added, removed
    )
}

/// Collects `{section_id → size}` from a wasm byte slice.
/// If the same id appears multiple times (e.g. custom sections), the last
/// size wins — for diff purposes this is sufficient.
fn collect_sections(bytes: &[u8]) -> Result<std::collections::BTreeMap<u8, u32>, ParseError> {
    let mut map = std::collections::BTreeMap::new();
    for result in section_iter(bytes) {
        let hdr = result?;
        map.insert(hdr.id, hdr.size);
    }
    Ok(map)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Vec<u8> {
        vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]
    }

    fn wasm_with_sections(sections: &[(u8, &[u8])]) -> Vec<u8> {
        let mut b = header();
        for (id, payload) in sections {
            b.push(*id);
            b.push(payload.len() as u8);
            b.extend_from_slice(payload);
        }
        b
    }

    #[test]
    fn identical_binaries_all_equal() {
        let a = wasm_with_sections(&[(1, &[0x01, 0x60, 0x00, 0x00])]);
        let diffs = diff_modules(&a, &a).unwrap();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0].status, DiffStatus::Equal { .. }));
    }

    #[test]
    fn empty_vs_empty_no_diffs() {
        let a = header();
        let diffs = diff_modules(&a, &a).unwrap();
        assert!(diffs.is_empty());
    }

    #[test]
    fn section_added_in_b() {
        let a = header();
        let b = wasm_with_sections(&[(1, &[0x01, 0x60, 0x00, 0x00])]);
        let diffs = diff_modules(&a, &b).unwrap();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0].status, DiffStatus::OnlyInB { .. }));
        assert_eq!(diffs[0].id, 1);
    }

    #[test]
    fn section_removed_in_b() {
        let a = wasm_with_sections(&[(1, &[0x01, 0x60, 0x00, 0x00])]);
        let b = header();
        let diffs = diff_modules(&a, &b).unwrap();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0].status, DiffStatus::OnlyInA { .. }));
    }

    #[test]
    fn section_changed_size() {
        let a = wasm_with_sections(&[(10, &[0x01, 0x02, 0x00, 0x0B])]);
        let b = wasm_with_sections(&[(10, &[0x02, 0x02, 0x00, 0x0B, 0x02, 0x00, 0x0B])]);
        let diffs = diff_modules(&a, &b).unwrap();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0].status, DiffStatus::Changed { .. }));
    }

    #[test]
    fn multiple_sections_mixed_status() {
        // a: type + code; b: type(same) + export(new)
        let type_payload = &[0x01u8, 0x60, 0x00, 0x00] as &[u8];
        let a = wasm_with_sections(&[(1, type_payload), (10, &[0x01, 0x02, 0x00, 0x0B])]);
        let b = wasm_with_sections(&[
            (1, type_payload),
            (7, &[0x01, 0x04, b'm', b'a', b'i', b'n', 0x00, 0x00]),
        ]);
        let diffs = diff_modules(&a, &b).unwrap();

        let equal = diffs
            .iter()
            .filter(|d| matches!(d.status, DiffStatus::Equal { .. }))
            .count();
        let added = diffs
            .iter()
            .filter(|d| matches!(d.status, DiffStatus::OnlyInB { .. }))
            .count();
        let removed = diffs
            .iter()
            .filter(|d| matches!(d.status, DiffStatus::OnlyInA { .. }))
            .count();
        assert_eq!(equal, 1); // type section
        assert_eq!(added, 1); // export
        assert_eq!(removed, 1); // code
    }

    #[test]
    fn summary_string() {
        let a = header();
        let b = wasm_with_sections(&[(1, &[0x01, 0x60, 0x00, 0x00])]);
        let diffs = diff_modules(&a, &b).unwrap();
        let s = diff_summary(&diffs);
        assert!(s.contains("1 added"));
        assert!(s.contains("0 equal"));
    }

    #[test]
    fn display_equal_shows_size() {
        let d = SectionDiff {
            id: 1,
            name: "type",
            status: DiffStatus::Equal { size: 4 },
        };
        let s = format!("{}", d);
        assert!(s.contains("[=]"), "expected [=] marker, got: {}", s);
        assert!(s.contains("4 bytes"), "expected size in output, got: {}", s);
        assert!(s.contains("type"), "expected section name, got: {}", s);
    }

    #[test]
    fn display_changed() {
        let d = SectionDiff {
            id: 1,
            name: "type",
            status: DiffStatus::Changed {
                size_a: 4,
                size_b: 8,
            },
        };
        let s = format!("{}", d);
        assert!(s.contains("[~]"));
        assert!(s.contains("4"));
        assert!(s.contains("8"));
    }

    #[test]
    fn equal_size_propagated_from_diff() {
        let payload = &[0x01u8, 0x60, 0x00, 0x00] as &[u8];
        let a = wasm_with_sections(&[(1, payload)]);
        let diffs = diff_modules(&a, &a).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].status, DiffStatus::Equal { size: 4 });
        let s = format!("{}", diffs[0]);
        assert!(s.contains("4 bytes"), "expected actual size, got: {}", s);
    }

    #[test]
    fn section_names_correct() {
        assert_eq!(section_name(1), "type");
        assert_eq!(section_name(10), "code");
        assert_eq!(section_name(0), "custom");
        assert_eq!(section_name(255), "unknown");
    }
}

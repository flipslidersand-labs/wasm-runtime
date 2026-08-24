use crate::parser::{decode_leb128_u32, ParseError};
use std::fmt;

/// A custom section: a name followed by an opaque payload. Custom sections
/// carry tooling metadata (e.g. the `name` section, `producers`) and impose no
/// consistency constraints on the module. Multiple may appear in one module.
#[derive(Debug, PartialEq, Clone)]
pub struct CustomSection {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl fmt::Display for CustomSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "name=\"{}\" payload={} bytes",
            self.name,
            self.bytes.len()
        )
    }
}

/// Decodes a custom section: a name string followed by the remaining bytes as an
/// opaque payload (whose meaning depends on `name`). The `name` section body is
/// left undecoded here.
pub fn decode_custom_section(payload: &[u8]) -> Result<CustomSection, ParseError> {
    let mut pos = 0;
    let name = super::read_name(payload, &mut pos)?;
    let bytes = payload[pos..].to_vec();
    Ok(CustomSection { name, bytes })
}

// ── Name section (custom name == "name") ─────────────────────────────────────

/// Decoded contents of the wasm name section (a custom section whose `name`
/// field is `"name"`). Only subsections 0 (module name) and 1 (function names)
/// are decoded; all others are silently skipped.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct NameSection {
    /// Subsection 0: the module name, if present.
    pub module: Option<String>,
    /// Subsection 1: `(function_index, name)` pairs from the function namemap.
    pub functions: Vec<(u32, String)>,
}

impl fmt::Display for NameSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.module.is_none() && self.functions.is_empty() {
            return writeln!(f, "  (empty)");
        }
        if let Some(m) = &self.module {
            writeln!(f, "  module: \"{}\"", m)?;
        }
        if !self.functions.is_empty() {
            writeln!(f, "  function names:")?;
            for (idx, name) in &self.functions {
                writeln!(f, "    [{}] \"{}\"", idx, name)?;
            }
        }
        Ok(())
    }
}

/// Decodes the payload bytes of a `"name"` custom section into a [`NameSection`].
///
/// The payload is a sequence of subsections, each `id:byte size:u32 payload[size]`.
/// Unknown subsection ids are skipped by advancing `size` bytes.
pub fn decode_name_section(bytes: &[u8]) -> Result<NameSection, ParseError> {
    let mut pos = 0;
    let mut ns = NameSection::default();

    while pos < bytes.len() {
        let id = bytes[pos];
        pos += 1;

        let (size, n) = decode_leb128_u32(bytes, pos)?;
        pos += n;

        let sub_end = pos
            .checked_add(size as usize)
            .ok_or(ParseError::UnexpectedEof)?;
        if sub_end > bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }

        let sub = &bytes[pos..sub_end];

        match id {
            // subsection 0: module name — a single namestring
            0 => {
                let mut p = 0;
                ns.module = Some(super::read_name(sub, &mut p)?);
            }
            // subsection 1: function names — vec(idx:u32 name:namestring)
            1 => {
                let mut p = 0;
                let (count, n) = decode_leb128_u32(sub, p)?;
                p += n;
                for _ in 0..count {
                    let (idx, n) = decode_leb128_u32(sub, p)?;
                    p += n;
                    let name = super::read_name(sub, &mut p)?;
                    ns.functions.push((idx, name));
                }
            }
            // all other subsection ids (e.g. 2 = local names): skip
            _ => {}
        }

        pos = sub_end;
    }

    Ok(ns)
}

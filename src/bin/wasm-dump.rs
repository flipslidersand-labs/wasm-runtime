use std::io::Read;
use std::process;
use std::time::Instant;
use wasm_runtime::{
    diff::{diff_modules, diff_summary},
    explain::{explain_bytes_with_limit, MAX_EXPLAIN_LINES},
    module::{parse_module, section_name},
    parser::{parse_header, section_iter, ParseError},
    sections::{
        decode_code_section, decode_custom_section, decode_data_section, decode_datacount_section,
        decode_element_section, decode_export_section, decode_function_section,
        decode_global_section, decode_import_section, decode_memory_section, decode_name_section,
        decode_start_section, decode_table_section, decode_type_section, display_name,
    },
    stats::ModuleStats,
    wat::module_to_wat,
};

struct Flags {
    verbose: bool,
    validate: bool,
    stats: bool,
    wat: bool,
    timing: bool,
    explain: bool,
    max_size: u64,
    /// Line cap for `--explain`; 0 on the command line means unlimited (stored as `usize::MAX`).
    explain_max_lines: usize,
}

/// Default input size limit: 256 MiB.
const DEFAULT_MAX_SIZE: u64 = 256 * 1024 * 1024;

fn parse_max_size(v: Option<&String>) -> Result<u64, String> {
    match v.and_then(|s| s.parse::<u64>().ok()) {
        None => Err("--max-size requires a byte count (positive integer)".to_string()),
        Some(0) => Err("--max-size must be at least 1".to_string()),
        Some(n) => Ok(n),
    }
}

fn parse_explain_max_lines(v: Option<&String>) -> Result<usize, String> {
    let s = v.ok_or_else(|| {
        "--explain-max-lines requires a line count (non-negative integer, 0 = unlimited)"
            .to_string()
    })?;
    match s.parse::<usize>() {
        Ok(0) => Ok(usize::MAX),
        Ok(n) => Ok(n),
        Err(e) if *e.kind() == std::num::IntErrorKind::PosOverflow => Err(format!(
            "--explain-max-lines value is out of range (max {})",
            usize::MAX
        )),
        Err(_) => Err(
            "--explain-max-lines requires a line count (non-negative integer, 0 = unlimited)"
                .to_string(),
        ),
    }
}

fn print_usage() {
    eprintln!(
        "Usage: wasm-dump [--verbose|-v] [--validate] [--stats] [--timing] [--wat] [--explain] [--explain-max-lines <N>] [--max-size <bytes>] <file.wasm>"
    );
    eprintln!("       wasm-dump --diff [--max-size <bytes>] <a.wasm> <b.wasm>");
    eprintln!("  --explain-max-lines <N>: line cap for --explain (default 100000; 0 = unlimited, may use lots of memory)");
}

/// Read a file, refusing inputs larger than `max` bytes.
/// Reads at most `max + 1` bytes so non-regular files (e.g. /dev/zero) are bounded too.
fn read_limited(path: &str, max: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read '{}': {}", path, e))?;
    let too_big = || format!("input '{}' exceeds the size limit of {} bytes", path, max);
    if let Ok(meta) = file.metadata() {
        if meta.is_file() && meta.len() > max {
            return Err(too_big());
        }
    }
    let mut buf = Vec::new();
    file.take(max.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| format!("cannot read '{}': {}", path, e))?;
    if buf.len() as u64 > max {
        return Err(too_big());
    }
    Ok(buf)
}

/// Parse CLI arguments into (Flags, path).
/// Returns Err with a usage message on invalid input.
fn parse_flags(args: &[String]) -> Result<(Flags, String), String> {
    let mut flags = Flags {
        verbose: false,
        validate: false,
        stats: false,
        wat: false,
        timing: false,
        explain: false,
        max_size: DEFAULT_MAX_SIZE,
        explain_max_lines: MAX_EXPLAIN_LINES,
    };
    let mut path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--verbose" | "-v" => flags.verbose = true,
            "--validate" => flags.validate = true,
            "--stats" => flags.stats = true,
            "--wat" => flags.wat = true,
            "--timing" => flags.timing = true,
            "--explain" => flags.explain = true,
            "--explain-max-lines" => {
                i += 1;
                flags.explain_max_lines = parse_explain_max_lines(args.get(i))?;
            }
            "--max-size" => {
                i += 1;
                flags.max_size = parse_max_size(args.get(i))?;
            }
            s if s.starts_with('-') => {
                return Err(format!("unknown flag: {}", s));
            }
            s => {
                if path.is_some() {
                    return Err("too many positional arguments".to_string());
                }
                path = Some(s.to_string());
            }
        }
        i += 1;
    }

    match path {
        Some(p) => Ok((flags, p)),
        None => Err("no input file specified".to_string()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // --diff takes two file arguments; handle it before the standard flag dispatch.
    if args.get(1).map(|s| s.as_str()) == Some("--diff") {
        let mut max_size = DEFAULT_MAX_SIZE;
        let mut rest: Vec<&String> = Vec::new();
        let mut j = 2;
        while j < args.len() {
            if args[j] == "--max-size" {
                j += 1;
                max_size = match parse_max_size(args.get(j)) {
                    Ok(v) => v,
                    Err(msg) => {
                        eprintln!("error: {}", msg);
                        print_usage();
                        process::exit(1);
                    }
                };
            } else {
                rest.push(&args[j]);
            }
            j += 1;
        }
        match rest.as_slice() {
            [path_a, path_b] => {
                run_diff(path_a, path_b, max_size);
                return;
            }
            _ => {
                eprintln!("Usage: wasm-dump --diff [--max-size <bytes>] <a.wasm> <b.wasm>");
                process::exit(1);
            }
        }
    }

    let (flags, path) = match parse_flags(&args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            print_usage();
            process::exit(1);
        }
    };

    let bytes = match read_limited(&path, flags.max_size) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    let version = match parse_header(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    println!("magic: 0x6D736100, version: {}", version);
    println!("sections:");

    let mut decode_errors = 0usize;
    // Per-section `--verbose` detail, collected during the listing pass so each
    // section is decoded exactly once.
    let mut details: Vec<(u8, u32, String)> = Vec::new();
    for result in section_iter(&bytes) {
        let hdr = match result {
            Ok(h) => h,
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        };

        let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
        print!(
            "  [{:2}] {:<8} size={}",
            hdr.id,
            section_name(hdr.id),
            hdr.size
        );
        let section = describe_section(hdr.id, payload, flags.verbose);
        print!("{}", section.summary);
        decode_errors += section.errors;
        println!();
        if flags.verbose {
            details.push((hdr.id, hdr.size, section.detail));
        }
    }

    if flags.verbose {
        print_verbose(&details);
    }

    // --explain is a byte-level walkthrough, so decode errors do not change
    // its exit status. Everything else reports them with exit 1 (after the
    // remaining requested output has been produced).
    let fail_on_decode_errors = decode_errors > 0 && !flags.explain;
    if fail_on_decode_errors {
        eprintln!("error: {} section(s) failed to decode", decode_errors);
    }

    // Parse module once when any of stats / wat / validate / timing are requested.
    let need_module = flags.stats || flags.wat || flags.validate || flags.timing;
    let (module, parse_elapsed_us) = if need_module {
        let t0 = Instant::now();
        let m = match parse_module(&bytes) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        };
        let elapsed = t0.elapsed().as_micros() as u64;
        (Some(m), Some(elapsed))
    } else {
        (None, None)
    };

    if flags.stats {
        print!("{}", ModuleStats::from_module(module.as_ref().unwrap()));
    }

    if flags.wat {
        println!("{}", module_to_wat(module.as_ref().unwrap()));
    }

    if flags.validate {
        match module.as_ref().unwrap().validate() {
            Ok(()) => println!("\nvalidation: OK"),
            Err(e) => {
                eprintln!("\nvalidation error: {}", e);
                process::exit(1);
            }
        }
    }

    if flags.timing {
        if let Some(us) = parse_elapsed_us {
            if us < 1_000 {
                println!("\nparse time: {} µs", us);
            } else {
                println!("\nparse time: {:.3} ms", us as f64 / 1_000.0);
            }
        }
    }

    if flags.explain {
        println!("\nexplain:");
        for line in explain_bytes_with_limit(&bytes, flags.explain_max_lines) {
            println!("{}", line);
        }
    }

    if fail_on_decode_errors {
        process::exit(1);
    }
}

fn run_diff(path_a: &str, path_b: &str, max_size: u64) {
    let read = |p: &str| {
        read_limited(p, max_size).unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            process::exit(1);
        })
    };
    let a = read(path_a);
    let b = read(path_b);

    let diffs = match diff_modules(&a, &b) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    println!("diff: {} vs {}", path_a, path_b);
    for d in &diffs {
        println!("  {}", d);
    }
    println!("\n{}", diff_summary(&diffs));
}

/// One section's decoded view: the `(..)` suffix of the listing line, the
/// `--verbose` body (built only when requested), and how many decode errors
/// occurred.
#[derive(Default)]
struct Described {
    summary: String,
    detail: String,
    errors: usize,
}

impl Described {
    fn fail(&mut self, e: ParseError) {
        self.summary.push_str(&format!("  (decode error: {})", e));
        self.detail.push_str(&format!("  <decode error: {}>\n", e));
        self.errors += 1;
    }

    /// A section holding a list: summary `(N noun)`, detail `[i] item` lines.
    fn list<T: std::fmt::Display>(
        &mut self,
        res: Result<Vec<T>, ParseError>,
        noun: &str,
        verbose: bool,
    ) {
        match res {
            Ok(items) => {
                self.summary
                    .push_str(&format!("  ({} {})", items.len(), noun));
                if verbose {
                    for (i, item) in items.iter().enumerate() {
                        self.detail.push_str(&format!("  [{}] {}\n", i, item));
                    }
                }
            }
            Err(e) => self.fail(e),
        }
    }
}

/// The single section-id -> decoder dispatch of this tool: decodes `payload`
/// once and renders both the listing summary and the verbose detail.
fn describe_section(id: u8, payload: &[u8], verbose: bool) -> Described {
    let mut d = Described::default();
    match id {
        0 => match decode_custom_section(payload) {
            Ok(c) => {
                d.summary
                    .push_str(&format!("  (name \"{}\")", display_name(&c.name)));
                d.detail.push_str(&format!("  {}\n", c));
                if c.name == "name" {
                    match decode_name_section(&c.bytes) {
                        Ok(ns) => {
                            if !ns.functions.is_empty() {
                                d.summary
                                    .push_str(&format!("  ({} func names)", ns.functions.len()));
                            }
                            d.detail.push_str(&ns.to_string());
                        }
                        Err(e) => {
                            d.summary.push_str(&format!("  (decode error: {})", e));
                            d.detail
                                .push_str(&format!("  <name section decode error: {}>\n", e));
                            d.errors += 1;
                        }
                    }
                }
            }
            Err(e) => d.fail(e),
        },
        1 => d.list(decode_type_section(payload), "types", verbose),
        2 => d.list(decode_import_section(payload), "imports", verbose),
        3 => match decode_function_section(payload) {
            Ok(indices) => {
                d.summary.push_str(&format!("  ({} funcs)", indices.len()));
                for (i, idx) in indices.iter().enumerate() {
                    d.detail.push_str(&format!("  [{}] type[{}]\n", i, idx));
                }
            }
            Err(e) => d.fail(e),
        },
        4 => d.list(decode_table_section(payload), "tables", verbose),
        5 => d.list(decode_memory_section(payload), "mems", verbose),
        6 => d.list(decode_global_section(payload), "globals", verbose),
        7 => d.list(decode_export_section(payload), "exports", verbose),
        8 => match decode_start_section(payload) {
            Ok(idx) => {
                d.summary.push_str(&format!("  (func {})", idx));
                d.detail.push_str(&format!("  start: func[{}]\n", idx));
            }
            Err(e) => d.fail(e),
        },
        9 => d.list(decode_element_section(payload), "elems", verbose),
        10 => d.list(decode_code_section(payload), "funcs", verbose),
        11 => d.list(decode_data_section(payload), "data", verbose),
        12 => match decode_datacount_section(payload) {
            Ok(count) => {
                d.summary.push_str(&format!("  (count {})", count));
                d.detail.push_str(&format!("  datacount: {}\n", count));
            }
            Err(e) => d.fail(e),
        },
        _ => {}
    }
    if !verbose {
        d.detail.clear();
    }
    d
}

/// Prints each section's collected `--verbose` detail.
fn print_verbose(details: &[(u8, u32, String)]) {
    for (id, size, detail) in details {
        println!("\nSection[{}] {} ({} bytes)", id, section_title(*id), size);
        print!("{}", detail);
    }
}

/// Capitalized title used in the `--verbose` per-section header, derived from
/// the shared lowercase [`section_name`].
fn section_title(id: u8) -> String {
    let name = section_name(id);
    let mut title = String::with_capacity(name.len());
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        title.extend(first.to_uppercase());
    }
    title.push_str(chars.as_str());
    title
}

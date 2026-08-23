use std::process;
use wasm_runtime::{
    diff::{diff_modules, diff_summary},
    module::parse_module,
    parser::{parse_header, section_iter, ParseError},
    sections::{
        decode_code_section, decode_custom_section, decode_data_section, decode_datacount_section,
        decode_element_section, decode_export_section, decode_function_section,
        decode_global_section, decode_import_section, decode_memory_section, decode_name_section,
        decode_start_section, decode_table_section, decode_type_section,
    },
    stats::ModuleStats,
    wat::module_to_wat,
};

struct Flags {
    verbose: bool,
    validate: bool,
    stats: bool,
    wat: bool,
}

/// Parse CLI arguments into (Flags, path).
/// Returns Err with a usage message on invalid input.
fn parse_flags(args: &[String]) -> Result<(Flags, String), String> {
    let mut flags = Flags {
        verbose: false,
        validate: false,
        stats: false,
        wat: false,
    };
    let mut path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--verbose" | "-v" => flags.verbose = true,
            "--validate" => flags.validate = true,
            "--stats" => flags.stats = true,
            "--wat" => flags.wat = true,
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
        match args.as_slice() {
            [_, _, path_a, path_b] => {
                run_diff(path_a, path_b);
                return;
            }
            _ => {
                eprintln!("Usage: wasm-dump --diff <a.wasm> <b.wasm>");
                process::exit(1);
            }
        }
    }

    let (flags, path) = match parse_flags(&args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("Usage: wasm-dump [--verbose|-v] [--validate] [--stats] [--wat] <file.wasm>");
            eprintln!("       wasm-dump --diff <a.wasm> <b.wasm>");
            process::exit(1);
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", path, e);
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
        match hdr.id {
            0 => {
                if let Ok(c) = decode_custom_section(payload) {
                    print!("  (name \"{}\")", c.name);
                    if c.name == "name" {
                        if let Ok(ns) = decode_name_section(&c.bytes) {
                            if !ns.functions.is_empty() {
                                print!("  ({} func names)", ns.functions.len());
                            }
                        }
                    }
                }
            }
            1 => print_count(decode_type_section(payload).map(|v| v.len()), "types"),
            2 => print_count(decode_import_section(payload).map(|v| v.len()), "imports"),
            3 => print_count(decode_function_section(payload).map(|v| v.len()), "funcs"),
            4 => print_count(decode_table_section(payload).map(|v| v.len()), "tables"),
            5 => print_count(decode_memory_section(payload).map(|v| v.len()), "mems"),
            6 => print_count(decode_global_section(payload).map(|v| v.len()), "globals"),
            7 => print_count(decode_export_section(payload).map(|v| v.len()), "exports"),
            8 => {
                if let Ok(idx) = decode_start_section(payload) {
                    print!("  (func {})", idx);
                }
            }
            9 => print_count(decode_element_section(payload).map(|v| v.len()), "elems"),
            10 => print_count(decode_code_section(payload).map(|v| v.len()), "funcs"),
            11 => print_count(decode_data_section(payload).map(|v| v.len()), "data"),
            12 => {
                if let Ok(count) = decode_datacount_section(payload) {
                    print!("  (count {})", count);
                }
            }
            _ => {}
        }
        println!();
    }

    if flags.verbose {
        print_verbose(&bytes);
    }

    // Parse module once when any of stats / wat / validate are requested.
    let need_module = flags.stats || flags.wat || flags.validate;
    let module = if need_module {
        match parse_module(&bytes) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
    } else {
        None
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
}

fn run_diff(path_a: &str, path_b: &str) {
    let read = |p: &str| {
        std::fs::read(p).unwrap_or_else(|e| {
            eprintln!("error: cannot read '{}': {}", p, e);
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

fn print_count(res: Result<usize, ParseError>, noun: &str) {
    if let Ok(n) = res {
        print!("  ({} {})", n, noun);
    }
}

/// Prints each section's decoded contents in a human-readable form.
fn print_verbose(bytes: &[u8]) {
    for result in section_iter(bytes) {
        let hdr = match result {
            Ok(h) => h,
            Err(_) => return,
        };
        let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];

        println!(
            "\nSection[{}] {} ({} bytes)",
            hdr.id,
            section_title(hdr.id),
            hdr.size
        );

        match hdr.id {
            0 => match decode_custom_section(payload) {
                Ok(c) if c.name == "name" => {
                    println!("  {}", c);
                    match decode_name_section(&c.bytes) {
                        Ok(ns) => print!("{}", ns),
                        Err(e) => println!("  <name section decode error: {}>", e),
                    }
                }
                Ok(c) => println!("  {}", c),
                Err(e) => println!("  <decode error: {}>", e),
            },
            1 => print_entries(decode_type_section(payload)),
            2 => print_entries(decode_import_section(payload)),
            3 => match decode_function_section(payload) {
                Ok(indices) => {
                    for (i, idx) in indices.iter().enumerate() {
                        println!("  [{}] type[{}]", i, idx);
                    }
                }
                Err(e) => println!("  <decode error: {}>", e),
            },
            4 => print_entries(decode_table_section(payload)),
            5 => print_entries(decode_memory_section(payload)),
            6 => print_entries(decode_global_section(payload)),
            7 => print_entries(decode_export_section(payload)),
            8 => match decode_start_section(payload) {
                Ok(idx) => println!("  start: func[{}]", idx),
                Err(e) => println!("  <decode error: {}>", e),
            },
            9 => print_entries(decode_element_section(payload)),
            10 => print_entries(decode_code_section(payload)),
            11 => print_entries(decode_data_section(payload)),
            12 => match decode_datacount_section(payload) {
                Ok(count) => println!("  datacount: {}", count),
                Err(e) => println!("  <decode error: {}>", e),
            },
            _ => {}
        }
    }
}

fn print_entries<T: std::fmt::Display>(res: Result<Vec<T>, ParseError>) {
    match res {
        Ok(items) => {
            for (i, item) in items.iter().enumerate() {
                println!("  [{}] {}", i, item);
            }
        }
        Err(e) => println!("  <decode error: {}>", e),
    }
}

/// Short lowercase name used in the compact section list.
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

/// Capitalized title used in the `--verbose` per-section header.
fn section_title(id: u8) -> &'static str {
    match id {
        0 => "Custom",
        1 => "Type",
        2 => "Import",
        3 => "Function",
        4 => "Table",
        5 => "Memory",
        6 => "Global",
        7 => "Export",
        8 => "Start",
        9 => "Element",
        10 => "Code",
        11 => "Data",
        12 => "DataCount",
        _ => "Unknown",
    }
}

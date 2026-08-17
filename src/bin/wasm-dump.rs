use std::process;
use wasm_runtime::{
    module::parse_module,
    parser::{parse_header, section_iter, ParseError},
    sections::{
        decode_code_section, decode_data_section, decode_element_section, decode_export_section,
        decode_function_section, decode_global_section, decode_import_section,
        decode_memory_section, decode_start_section, decode_table_section, decode_type_section,
    },
};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let (verbose, validate, path) = match args.as_slice() {
        [_, flag, path] if flag == "--verbose" || flag == "-v" => (true, false, path.clone()),
        [_, flag, path] if flag == "--validate" => (false, true, path.clone()),
        [_, path] => (false, false, path.clone()),
        _ => {
            eprintln!("Usage: wasm-dump [--verbose|-v | --validate] <file.wasm>");
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
            _ => {}
        }
        println!();
    }

    if verbose {
        print_verbose(&bytes);
    }

    if validate {
        match parse_module(&bytes).map(|m| m.validate()) {
            Ok(Ok(())) => println!("\nvalidation: OK"),
            Ok(Err(e)) => {
                eprintln!("\nvalidation error: {}", e);
                process::exit(1);
            }
            Err(e) => {
                eprintln!("\nerror: {}", e);
                process::exit(1);
            }
        }
    }
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

use std::process;
use wasm_runtime::{
    parser::{parse_header, section_iter},
    sections::{
        decode_code_section, decode_export_section, decode_function_section, decode_type_section,
    },
};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let (verbose, path) = match args.as_slice() {
        [_, flag, path] if flag == "--verbose" || flag == "-v" => (true, path.clone()),
        [_, path] => (false, path.clone()),
        _ => {
            eprintln!("Usage: wasm-dump [--verbose] <file.wasm>");
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

    let mut type_payload: Option<(usize, u32)> = None;
    let mut func_payload: Option<(usize, u32)> = None;
    let mut export_payload: Option<(usize, u32)> = None;

    for result in section_iter(&bytes) {
        let hdr = match result {
            Ok(h) => h,
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        };

        let name = section_name(hdr.id);
        print!("  [{:2}] {:<8} size={}", hdr.id, name, hdr.size);

        match hdr.id {
            1 => {
                let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
                if let Ok(types) = decode_type_section(payload) {
                    print!("  ({} types)", types.len());
                }
                type_payload = Some((hdr.offset, hdr.size));
            }
            3 => {
                let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
                if let Ok(funcs) = decode_function_section(payload) {
                    print!("  ({} funcs)", funcs.len());
                }
                func_payload = Some((hdr.offset, hdr.size));
            }
            7 => {
                let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
                if let Ok(exports) = decode_export_section(payload) {
                    print!("  ({} exports)", exports.len());
                }
                export_payload = Some((hdr.offset, hdr.size));
            }
            10 => {
                let payload = &bytes[hdr.offset..hdr.offset + hdr.size as usize];
                if let Ok(bodies) = decode_code_section(payload) {
                    print!("  ({} funcs)", bodies.len());
                }
            }
            _ => {}
        }
        println!();
    }

    if verbose {
        print_verbose(&bytes, type_payload, func_payload);
    }

    if let Some((off, sz)) = export_payload {
        let payload = &bytes[off..off + sz as usize];
        if let Ok(exports) = decode_export_section(payload) {
            println!("\nexports:");
            for e in &exports {
                println!("  {:<16} {:?}  [{}]", e.name, e.kind, e.index);
            }
        }
    }
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

fn print_verbose(
    bytes: &[u8],
    type_payload: Option<(usize, u32)>,
    func_payload: Option<(usize, u32)>,
) {
    let types =
        type_payload.and_then(|(off, sz)| decode_type_section(&bytes[off..off + sz as usize]).ok());
    let funcs = func_payload
        .and_then(|(off, sz)| decode_function_section(&bytes[off..off + sz as usize]).ok());

    if let Some(ref ts) = types {
        println!("\ntypes:");
        for (i, ft) in ts.iter().enumerate() {
            let params: Vec<_> = ft.params.iter().map(|v| format!("{:?}", v)).collect();
            let results: Vec<_> = ft.results.iter().map(|v| format!("{:?}", v)).collect();
            println!(
                "  [{}] ({}) -> ({})",
                i,
                params.join(", "),
                results.join(", ")
            );
        }
    }

    if let (Some(ref ts), Some(ref fs)) = (types, funcs) {
        println!("\nfunctions:");
        for (i, &type_idx) in fs.iter().enumerate() {
            if let Some(ft) = ts.get(type_idx as usize) {
                let params: Vec<_> = ft.params.iter().map(|v| format!("{:?}", v)).collect();
                let results: Vec<_> = ft.results.iter().map(|v| format!("{:?}", v)).collect();
                println!(
                    "  [{}] type[{}]: ({}) -> ({})",
                    i,
                    type_idx,
                    params.join(", "),
                    results.join(", ")
                );
            } else {
                println!("  [{}] type[{}]", i, type_idx);
            }
        }
    }
}

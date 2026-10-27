//! 公式 WebAssembly testsuite への適合性を測る (#150)。
//!
//! 環境変数 `SPEC_TESTSUITE_DIR`（testsuite の checkout）と `wasm-tools` がある場合だけ実行する。
//! どちらかが無ければ skip する（通常の `cargo test` を壊さないため）。
//!
//! - 正しいモジュール(`module`)は parse + validate が通ること
//! - 壊れたバイナリ(`assert_malformed` の binary)は parse か validate が失敗すること
//!
//! 失敗は `tests/spec_expected_failures.txt` に分類つきで記録し、集合が変わったらテストが落ちる
//! （悪化も、改善の記録漏れも検知する）。更新は `UPDATE_SPEC_BASELINE=1 cargo test --test spec_conformance`。
//!
//! 注意: testsuite の main は MVP より後の拡張を含む。失敗がそのままバグとは限らない。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use wasm_runtime::module::parse_module;

const FILES: &[&str] = &[
    "binary", "custom", "data", "elem", "func", "global", "imports", "memory", "start", "table",
    "type",
];
const BASELINE: &str = "tests/spec_expected_failures.txt";

#[derive(Default)]
struct Tally {
    valid_total: usize,
    valid_ok: usize,
    malformed_total: usize,
    malformed_rejected: usize,
}

fn have_wasm_tools() -> bool {
    Command::new("wasm-tools")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn classify_valid_failure(msg: &str) -> &'static str {
    // MVP より後の拡張（reference types / typed function references / extended const / exception handling）
    let post_mvp = [
        "reftype",
        "unknown value type",
        "init_expr opcode",
        "export kind",
        "import kind",
    ];
    if post_mvp.iter().any(|p| msg.contains(p)) {
        "post-mvp"
    } else {
        "unclassified"
    }
}

fn run_file(
    suite: &Path,
    out_root: &Path,
    name: &str,
    tally: &mut Tally,
    fails: &mut BTreeMap<String, String>,
) {
    let out = out_root.join(name);
    std::fs::create_dir_all(&out).unwrap();
    let json_path = out.join(format!("{name}.json"));
    let status = Command::new("wasm-tools")
        .arg("json-from-wast")
        .arg(suite.join(format!("{name}.wast")))
        .arg("-o")
        .arg(&json_path)
        .arg("--wasm-dir")
        .arg(&out)
        .output()
        .expect("wasm-tools を実行できない");
    assert!(status.status.success(), "json-from-wast が失敗: {name}");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
    for cmd in doc["commands"].as_array().unwrap() {
        let ty = cmd["type"].as_str().unwrap_or("");
        let file = cmd["filename"].as_str().unwrap_or("");
        let is_valid = ty == "module" && file.ends_with(".wasm");
        let is_malformed = ty == "assert_malformed" && cmd["module_type"] == "binary";
        if !is_valid && !is_malformed {
            continue;
        }
        let bytes = std::fs::read(out.join(file)).unwrap();
        let result = parse_module(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|m| m.validate().map_err(|e| e.to_string()));
        let key = format!("{name}:{file}");
        if is_valid {
            tally.valid_total += 1;
            match result {
                Ok(()) => tally.valid_ok += 1,
                Err(msg) => {
                    fails.insert(
                        key,
                        format!("valid {} # {}", classify_valid_failure(&msg), msg),
                    );
                }
            }
        } else {
            tally.malformed_total += 1;
            match result {
                Err(_) => tally.malformed_rejected += 1,
                Ok(()) => {
                    let text = cmd["text"].as_str().unwrap_or("");
                    fails.insert(key, format!("malformed mvp-gap-candidate # {text}"));
                }
            }
        }
    }
}

fn render(fails: &BTreeMap<String, String>) -> String {
    fails.iter().map(|(k, v)| format!("{k} {v}\n")).collect()
}

#[test]
fn spec_conformance() {
    let Ok(dir) = std::env::var("SPEC_TESTSUITE_DIR") else {
        eprintln!("skip: SPEC_TESTSUITE_DIR が未設定");
        return;
    };
    if !have_wasm_tools() {
        eprintln!("skip: wasm-tools が無い");
        return;
    }
    let suite = PathBuf::from(dir);
    let out_root = std::env::temp_dir().join("wasm-runtime-spec");
    let mut tally = Tally::default();
    let mut fails = BTreeMap::new();
    for name in FILES {
        run_file(&suite, &out_root, name, &mut tally, &mut fails);
    }
    eprintln!(
        "valid accepted {}/{} / malformed rejected {}/{}",
        tally.valid_ok, tally.valid_total, tally.malformed_rejected, tally.malformed_total
    );
    let rendered = render(&fails);
    if std::env::var("UPDATE_SPEC_BASELINE").is_ok() {
        std::fs::write(BASELINE, &rendered).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(BASELINE).unwrap_or_default();
    assert_eq!(
        rendered, expected,
        "失敗の集合が baseline と違う。悪化か、改善の記録漏れ。意図した変更なら UPDATE_SPEC_BASELINE=1 で更新する"
    );
}

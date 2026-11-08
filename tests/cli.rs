use assert_cmd::Command;
use predicates::prelude::*;

fn wasm_dump() -> Command {
    Command::cargo_bin("wasm-dump").unwrap()
}

const MINIMAL_WASM: &str = "tests/fixtures/minimal.wasm";
const EMPTY_WASM: &str = "tests/fixtures/empty.wasm";
const GLOBALS_WASM: &str = "tests/fixtures/globals.wasm";

// ── 正常系 ───────────────────────────────────────────────

#[test]
fn default_output_exits_zero() {
    wasm_dump().arg(MINIMAL_WASM).assert().success();
}

#[test]
fn default_output_contains_magic() {
    wasm_dump()
        .arg(MINIMAL_WASM)
        .assert()
        .success()
        .stdout(predicate::str::contains("magic: 0x6D736100, version: 1"));
}

#[test]
fn default_output_lists_sections() {
    wasm_dump()
        .arg(MINIMAL_WASM)
        .assert()
        .success()
        .stdout(predicate::str::contains("sections:"))
        .stdout(predicate::str::contains("type"))
        .stdout(predicate::str::contains("export"))
        .stdout(predicate::str::contains("code"));
}

#[test]
fn empty_wasm_exits_zero() {
    wasm_dump().arg(EMPTY_WASM).assert().success();
}

// ── --verbose ────────────────────────────────────────────

#[test]
fn verbose_flag_exits_zero() {
    wasm_dump()
        .args(["--verbose", MINIMAL_WASM])
        .assert()
        .success();
}

#[test]
fn verbose_flag_shows_section_details() {
    wasm_dump()
        .args(["--verbose", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["));
}

#[test]
fn short_verbose_flag_works() {
    wasm_dump()
        .args(["-v", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["));
}

// ── --validate ───────────────────────────────────────────

#[test]
fn validate_valid_wasm_exits_zero() {
    wasm_dump()
        .args(["--validate", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("validation: OK"));
}

#[test]
fn validate_empty_wasm_exits_zero() {
    wasm_dump()
        .args(["--validate", EMPTY_WASM])
        .assert()
        .success();
}

#[test]
fn validate_globals_wasm_exits_zero() {
    wasm_dump()
        .args(["--validate", GLOBALS_WASM])
        .assert()
        .success();
}

// ── フラグ組み合わせ (#72) ────────────────────────────────

#[test]
fn verbose_and_validate_both_work() {
    wasm_dump()
        .args(["--verbose", "--validate", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["))
        .stdout(predicate::str::contains("validation: OK"));
}

#[test]
fn verbose_and_stats_both_work() {
    wasm_dump()
        .args(["--verbose", "--stats", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["))
        .stdout(predicate::str::contains("Functions:"));
}

#[test]
fn stats_and_validate_both_work() {
    wasm_dump()
        .args(["--stats", "--validate", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Functions:"))
        .stdout(predicate::str::contains("validation: OK"));
}

#[test]
fn all_flags_combined() {
    wasm_dump()
        .args(["--verbose", "--stats", "--validate", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["))
        .stdout(predicate::str::contains("Functions:"))
        .stdout(predicate::str::contains("validation: OK"));
}

#[test]
fn flags_before_path_any_order() {
    // flags can appear in different orders
    wasm_dump()
        .args(["--validate", "--verbose", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["))
        .stdout(predicate::str::contains("validation: OK"));
}

#[test]
fn path_before_flags() {
    // file path can come before flags
    wasm_dump()
        .args([MINIMAL_WASM, "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Section["));
}

#[test]
fn unknown_flag_exits_nonzero() {
    wasm_dump()
        .args(["--unknown", MINIMAL_WASM])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown flag"));
}

#[test]
fn too_many_positional_arguments_exits_nonzero() {
    wasm_dump()
        .args([MINIMAL_WASM, "extra_positional_arg"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("too many positional arguments"));
}

// ── --timing (#63) ───────────────────────────────────────

#[test]
fn timing_alone_exits_zero_and_shows_parse_time() {
    wasm_dump()
        .args(["--timing", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("parse time:"));
}

#[test]
fn timing_shows_unit() {
    // output should contain either "µs" or "ms"
    let output = wasm_dump()
        .args(["--timing", MINIMAL_WASM])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("µs") || stdout.contains("ms"),
        "expected time unit in output, got: {}",
        stdout
    );
}

#[test]
fn stats_and_timing_both_work() {
    wasm_dump()
        .args(["--stats", "--timing", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Functions:"))
        .stdout(predicate::str::contains("parse time:"));
}

#[test]
fn timing_and_validate_both_work() {
    wasm_dump()
        .args(["--timing", "--validate", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("parse time:"))
        .stdout(predicate::str::contains("validation: OK"));
}

#[test]
fn all_flags_including_timing() {
    wasm_dump()
        .args(["--stats", "--timing", "--validate", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("Functions:"))
        .stdout(predicate::str::contains("validation: OK"))
        .stdout(predicate::str::contains("parse time:"));
}

// ── --wat (#107) ─────────────────────────────────────────

#[test]
fn wat_flag_exits_zero_and_starts_with_module() {
    wasm_dump()
        .args(["--wat", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("(module"));
}

#[test]
fn wat_flag_shows_function_types() {
    wasm_dump()
        .args(["--wat", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("(type (func"));
}

// ── --explain (#107) ─────────────────────────────────────

#[test]
fn explain_flag_exits_zero_and_shows_header() {
    wasm_dump()
        .args(["--explain", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("explain:"))
        .stdout(predicate::str::contains("magic"));
}

#[test]
fn explain_flag_shows_section_ids() {
    wasm_dump()
        .args(["--explain", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("section id:"));
}

// ── --explain-max-lines (#212) ───────────────────────────

#[test]
fn explain_max_lines_small_limit_truncates() {
    wasm_dump()
        .args(["--explain", "--explain-max-lines", "2", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "output truncated: line limit (2) reached",
        ));
}

#[test]
fn explain_max_lines_zero_is_unlimited() {
    wasm_dump()
        .args(["--explain", "--explain-max-lines", "0", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("section id:"))
        .stdout(predicate::str::contains("output truncated").not());
}

#[test]
fn explain_default_limit_does_not_truncate_small_module() {
    wasm_dump()
        .args(["--explain", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("output truncated").not());
}

#[test]
fn explain_max_lines_invalid_value_exits_one() {
    wasm_dump()
        .args(["--explain", "--explain-max-lines", "abc", MINIMAL_WASM])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "--explain-max-lines requires a line count",
        ));
}

#[test]
fn explain_max_lines_overflow_reports_out_of_range() {
    wasm_dump()
        .args([
            "--explain",
            "--explain-max-lines",
            "99999999999999999999999999",
            MINIMAL_WASM,
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "--explain-max-lines value is out of range",
        ));
}

#[test]
fn explain_max_lines_missing_value_exits_one() {
    wasm_dump()
        .args(["--explain-max-lines"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "--explain-max-lines requires a line count",
        ));
}

#[test]
fn explain_max_lines_without_explain_is_harmless() {
    wasm_dump()
        .args(["--explain-max-lines", "5", MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("explain:").not());
}

// ── 異常系 ───────────────────────────────────────────────

#[test]
fn missing_file_exits_nonzero() {
    wasm_dump()
        .arg("nonexistent.wasm")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn no_args_exits_nonzero_with_usage() {
    wasm_dump()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn invalid_magic_exits_nonzero() {
    // Write a temp file with invalid magic
    let dir = std::env::temp_dir();
    let path = dir.join("bad_magic.wasm");
    std::fs::write(&path, b"\xFF\xFF\xFF\xFF\x01\x00\x00\x00").unwrap();
    wasm_dump()
        .arg(path.to_str().unwrap())
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

// ── デコードエラー (#168) ─────────────────────────────────

/// pid 付きの tmp ファイル。Drop で削除する。
struct TmpWasm(std::path::PathBuf);

impl Drop for TmpWasm {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// type セクションの functype マーカーが 0xFF の壊れたモジュールを tmp に書く。
fn write_bad_type_section(name: &str) -> TmpWasm {
    let path = std::env::temp_dir().join(format!("{}_{}.wasm", name, std::process::id()));
    std::fs::write(&path, b"\x00\x61\x73\x6D\x01\x00\x00\x00\x01\x02\x01\xFF").unwrap();
    TmpWasm(path)
}

#[test]
fn default_output_decode_error_exits_one() {
    let tmp = write_bad_type_section("bad_type_default_168");
    wasm_dump()
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("type"))
        .stdout(predicate::str::contains("decode error"));
}

#[test]
fn verbose_decode_error_exits_one() {
    let tmp = write_bad_type_section("bad_type_verbose_168");
    wasm_dump()
        .arg("--verbose")
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("expected 0x60 functype marker"))
        .stderr(predicate::str::contains("1 section(s) failed to decode"));
}

#[test]
fn explain_decode_error_keeps_exit_zero() {
    let tmp = write_bad_type_section("bad_type_explain_168");
    wasm_dump()
        .arg("--explain")
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("decode error"));
}

// ── --diff ───────────────────────────────────────────────

#[test]
fn diff_identical_files_reports_equal() {
    wasm_dump()
        .args(["--diff", MINIMAL_WASM, MINIMAL_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("diff:"))
        .stdout(predicate::str::contains("equal"))
        .stdout(predicate::str::contains("0 changed, 0 added, 0 removed"));
}

#[test]
fn diff_different_files_reports_differences() {
    wasm_dump()
        .args(["--diff", MINIMAL_WASM, GLOBALS_WASM])
        .assert()
        .success()
        .stdout(predicate::str::contains("diff:"))
        .stdout(predicate::str::contains("0 changed, 0 added, 0 removed").not());
}

#[test]
fn diff_invalid_version_exits_nonzero() {
    let path = std::env::temp_dir().join("diff_bad_version.wasm");
    std::fs::write(&path, b"\x00asm\x02\x00\x00\x00").unwrap();
    wasm_dump()
        .args(["--diff", path.to_str().unwrap(), MINIMAL_WASM])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn diff_with_single_arg_exits_nonzero_with_usage() {
    wasm_dump()
        .args(["--diff", MINIMAL_WASM])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage: wasm-dump --diff"));
}

#[test]
fn diff_nonexistent_file_exits_nonzero_with_cannot_read() {
    wasm_dump()
        .args(["--diff", MINIMAL_WASM, "tests/fixtures/does_not_exist.wasm"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

// ── 入力サイズ上限 (#177) ─────────────────────────────────

fn write_sized(name: &str, len: usize) -> TmpWasm {
    let path = std::env::temp_dir().join(format!("{}_{}.wasm", name, std::process::id()));
    let mut data = b"\x00asm\x01\x00\x00\x00".to_vec();
    data.resize(len, 0);
    std::fs::write(&path, data).unwrap();
    TmpWasm(path)
}

#[test]
fn max_size_exceeded_exits_one() {
    let tmp = write_sized("max_size_single_177", 64);
    wasm_dump()
        .args(["--max-size", "16"])
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

#[test]
fn max_size_at_limit_is_accepted() {
    let tmp = write_sized("max_size_exact_177", 8);
    wasm_dump()
        .args(["--max-size", "8"])
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .success();
}

#[test]
fn max_size_invalid_value_exits_one() {
    wasm_dump()
        .args(["--max-size", "abc", MINIMAL_WASM])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("--max-size"));
}

#[test]
fn max_size_zero_is_rejected() {
    wasm_dump()
        .args(["--max-size", "0", MINIMAL_WASM])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("--max-size must be at least 1"))
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn diff_max_size_invalid_matches_single_output() {
    wasm_dump()
        .args(["--diff", "--max-size", "0", MINIMAL_WASM, MINIMAL_WASM])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("--max-size must be at least 1"))
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn max_size_one_over_limit_is_rejected() {
    let tmp = write_sized("max_size_plus1_177", 9);
    wasm_dump()
        .args(["--max-size", "8"])
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

#[cfg(unix)]
#[test]
fn max_size_stdin_pipe_exact_is_accepted() {
    wasm_dump()
        .args(["--max-size", "8", "/dev/stdin"])
        .write_stdin(b"\x00asm\x01\x00\x00\x00".to_vec())
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn max_size_stdin_pipe_plus_one_is_rejected() {
    wasm_dump()
        .args(["--max-size", "8", "/dev/stdin"])
        .write_stdin(b"\x00asm\x01\x00\x00\x00\x00".to_vec())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

#[test]
fn diff_max_size_exceeded_first_file() {
    let tmp = write_sized("max_size_diff_a_177", 64);
    wasm_dump()
        .args(["--diff", "--max-size", "16"])
        .arg(tmp.0.to_str().unwrap())
        .arg(MINIMAL_WASM)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

#[test]
fn diff_max_size_exceeded_second_file() {
    let tmp = write_sized("max_size_diff_b_177", 64);
    wasm_dump()
        .args(["--diff", "--max-size", "16", MINIMAL_WASM])
        .arg(tmp.0.to_str().unwrap())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

#[cfg(unix)]
#[test]
fn max_size_non_regular_file_is_bounded() {
    wasm_dump()
        .args(["--max-size", "1024", "/dev/zero"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

#[cfg(unix)]
#[test]
fn diff_max_size_non_regular_file_is_bounded() {
    wasm_dump()
        .args(["--diff", "--max-size", "1024", "/dev/zero", MINIMAL_WASM])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exceeds the size limit"));
}

// ── 制御文字を含む名前のエスケープ (#176) ────────────────

fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() < 128);
    let mut v = vec![id, payload.len() as u8];
    v.extend_from_slice(payload);
    v
}

fn name_bytes(s: &str) -> Vec<u8> {
    let mut v = vec![s.len() as u8];
    v.extend_from_slice(s.as_bytes());
    v
}

/// ESC / 改行を名前に含む wasm を組み立てる。
fn hostile_names_wasm() -> Vec<u8> {
    let mut m = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    // import: "mod\x1b[31m" :: "imp\x1b]0;pwn\x07" func type 0
    let mut imp = vec![1];
    imp.extend(name_bytes("mod\x1b[31m"));
    imp.extend(name_bytes("imp\x1b]0;pwn\x07"));
    imp.extend([0x00, 0x00]);
    m.extend(section(2, &imp));
    // export: "exp\x1b[2J\nfake" func 0
    let mut exp = vec![1];
    exp.extend(name_bytes("exp\x1b[2J\nfake"));
    exp.extend([0x00, 0x00]);
    m.extend(section(7, &exp));
    // custom: "cus\x1b[1mtom"
    let mut cus = name_bytes("cus\x1b[1mtom");
    cus.extend([1, 2, 3]);
    m.extend(section(0, &cus));
    // name section: module name + function name with ESC
    let mut ns = name_bytes("name");
    let mut sub0 = name_bytes("modname\x1b[0m");
    ns.push(0);
    ns.push(sub0.len() as u8);
    ns.append(&mut sub0);
    let mut sub1 = vec![1, 0];
    sub1.extend(name_bytes("fn\x1b[7mname"));
    ns.push(1);
    ns.push(sub1.len() as u8);
    ns.extend(sub1);
    m.extend(section(0, &ns));
    m
}

#[test]
fn control_chars_in_names_are_escaped_in_output() {
    let path = std::env::temp_dir().join(format!("wasm-dump-176-{}.wasm", std::process::id()));
    std::fs::write(&path, hostile_names_wasm()).unwrap();
    let mut outputs = Vec::new();
    for flags in [
        vec![],
        vec!["--verbose"],
        vec!["--explain"],
        vec!["--stats"],
        vec!["--wat"],
    ] {
        let out = wasm_dump().args(&flags).arg(&path).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "exit code for {:?}", flags);
        outputs.push((flags, out));
    }
    std::fs::remove_file(&path).ok();
    for (flags, out) in &outputs {
        for b in [0x1bu8, 0x07] {
            assert!(
                !out.stdout.contains(&b),
                "raw byte {:#04x} in stdout for {:?}:\n{}",
                b,
                flags,
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }
    let verbose = String::from_utf8_lossy(&outputs[1].1.stdout).to_string();
    for want in [
        "cus\\u{1b}[1mtom",
        "imp\\u{1b}]0;pwn\\u{7}",
        "exp\\u{1b}[2J\\u{a}fake",
        "modname\\u{1b}[0m",
        "fn\\u{1b}[7mname",
    ] {
        assert!(verbose.contains(want), "missing {want:?} in:\n{verbose}");
    }
}

#[test]
fn control_chars_in_validation_error_are_escaped_on_stderr() {
    let mut m = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    let mut exp = vec![1];
    exp.extend(name_bytes("bad\x1b[2J\x07\nfake"));
    exp.extend([0x00, 0x05]); // func index 5: out of range
    m.extend(section(7, &exp));
    let path = std::env::temp_dir().join(format!("wasm-dump-176v-{}.wasm", std::process::id()));
    std::fs::write(&path, m).unwrap();
    let out = wasm_dump().arg("--validate").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("out of range"),
        "unexpected stderr:\n{stderr}"
    );
    for b in [0x1bu8, 0x07] {
        assert!(
            !out.stderr.contains(&b),
            "raw {b:#04x} in stderr:\n{stderr}"
        );
        assert!(!out.stdout.contains(&b), "raw {b:#04x} in stdout");
    }
    assert!(stderr.contains("bad\\u{1b}[2J\\u{7}\\u{a}fake"), "{stderr}");
}

#[test]
fn backslash_quote_c1_and_bidi_in_names_are_escaped() {
    let mut m = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    let mut exp = vec![2];
    // literal backslash+n (2 chars) vs real newline must be distinguishable
    exp.extend(name_bytes("a\\nb\"q"));
    exp.extend([0x00, 0x00]);
    exp.extend(name_bytes("c1\u{85}bidi\u{202e}x\u{2066}"));
    exp.extend([0x00, 0x00]);
    m.extend(section(7, &exp));
    let path = std::env::temp_dir().join(format!("wasm-dump-184-{}.wasm", std::process::id()));
    std::fs::write(&path, m).unwrap();
    let out = wasm_dump().arg("--verbose").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    for want in ["a\\\\nb\\\"q", "c1\\u{85}bidi\\u{202e}x\\u{2066}"] {
        assert!(stdout.contains(want), "missing {want:?} in:\n{stdout}");
    }
    for c in ['\u{85}', '\u{202e}', '\u{2066}'] {
        assert!(!stdout.contains(c), "raw {c:?} in stdout:\n{stdout}");
    }
}

#[test]
fn bidi_marks_and_line_separators_in_names_are_escaped() {
    let mut m = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    let mut exp = vec![1];
    exp.extend(name_bytes("m\u{200e}\u{200f}\u{61c}\u{2028}\u{2029}x"));
    exp.extend([0x00, 0x00]);
    m.extend(section(7, &exp));
    let path = std::env::temp_dir().join(format!("wasm-dump-190-{}.wasm", std::process::id()));
    std::fs::write(&path, m).unwrap();
    let out = wasm_dump().arg("--verbose").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("m\\u{200e}\\u{200f}\\u{61c}\\u{2028}\\u{2029}x"),
        "{stdout}"
    );
    for c in ['\u{200e}', '\u{200f}', '\u{61c}', '\u{2028}', '\u{2029}'] {
        assert!(!stdout.contains(c), "raw {c:?} in stdout:\n{stdout}");
    }
}

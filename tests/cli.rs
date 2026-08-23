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

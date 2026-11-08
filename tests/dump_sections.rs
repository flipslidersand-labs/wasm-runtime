//! Characterization tests for `wasm-dump` section listing (`--verbose` and
//! decode-error reporting). They pin stdout, stderr and exit codes for every
//! section kind so the dispatch code can be refactored without changing output.

use assert_cmd::Command;
use std::path::PathBuf;

const HEADER: [u8; 8] = [0, 0x61, 0x73, 0x6d, 1, 0, 0, 0];

fn sec(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![id, payload.len() as u8];
    v.extend_from_slice(payload);
    v
}

fn build(sections: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut v = HEADER.to_vec();
    for (id, p) in sections {
        v.extend(sec(*id, p));
    }
    v
}

fn name(s: &str) -> Vec<u8> {
    let mut v = vec![s.len() as u8];
    v.extend_from_slice(s.as_bytes());
    v
}

/// One well-formed section of every kind, plus a plain and a `name` custom section.
fn all_good() -> Vec<u8> {
    let mut plain = name("abc");
    plain.extend([1, 2]);
    let mut names = name("name");
    names.extend([1, 4, 1, 0, 1, b'f']);
    build(&[
        (1, vec![1, 0x60, 1, 0x7f, 1, 0x7f]),
        (2, vec![1, 3, b'e', b'n', b'v', 1, b'f', 0, 0]),
        (3, vec![1, 0]),
        (4, vec![1, 0x70, 0, 1]),
        (5, vec![1, 0, 1]),
        (6, vec![1, 0x7f, 0, 0x41, 0, 0x0b]),
        (7, vec![1, 1, b'm', 2, 0]),
        (8, vec![0]),
        (9, vec![1, 0, 0x41, 0, 0x0b, 1, 0]),
        (12, vec![1]),
        (10, vec![1, 4, 0, 0x20, 0, 0x0b]),
        (11, vec![1, 0, 0x41, 0, 0x0b, 2, b'h', b'i']),
        (0, plain),
        (0, names),
    ])
}

/// Every section kind with a truncated (undecodable) payload.
fn all_bad() -> Vec<u8> {
    let ids = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 12, 10, 11, 0];
    build(&ids.iter().map(|&i| (i, vec![0xff])).collect::<Vec<_>>())
}

/// A custom `name` section whose body fails to decode.
fn bad_name() -> Vec<u8> {
    let mut p = name("name");
    p.extend([1, 9, 0xff]);
    build(&[(0, p)])
}

struct Out {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run(tag: &str, bytes: &[u8], flags: &[&str]) -> Out {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "wasm-dump-char-{}-{}.wasm",
        std::process::id(),
        tag
    ));
    std::fs::write(&path, bytes).unwrap();
    let o = Command::cargo_bin("wasm-dump")
        .unwrap()
        .arg(&path)
        .args(flags)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Out {
        stdout: String::from_utf8(o.stdout).unwrap(),
        stderr: String::from_utf8(o.stderr).unwrap(),
        code: o.status.code().unwrap(),
    }
}

fn listing(full: &str) -> &str {
    full.split("\nSection[").next().unwrap()
}

const GOOD_VERBOSE: &str = r#"magic: 0x6D736100, version: 1
sections:
  [ 1] type     size=6  (1 types)
  [ 2] import   size=9  (1 imports)
  [ 3] func     size=2  (1 funcs)
  [ 4] table    size=4  (1 tables)
  [ 5] memory   size=3  (1 mems)
  [ 6] global   size=6  (1 globals)
  [ 7] export   size=5  (1 exports)
  [ 8] start    size=1  (func 0)
  [ 9] element  size=7  (1 elems)
  [12] datacount size=1  (count 1)
  [10] code     size=6  (1 funcs)
  [11] data     size=8  (1 data)
  [ 0] custom   size=6  (name "abc")
  [ 0] custom   size=11  (name "name")  (1 func names)

Section[1] Type (6 bytes)
  [0] (i32) -> i32

Section[2] Import (9 bytes)
  [0] env::f  Func(0)

Section[3] Func (2 bytes)
  [0] type[0]

Section[4] Table (4 bytes)
  [0] funcref min=1

Section[5] Memory (3 bytes)
  [0] min=1

Section[6] Global (6 bytes)
  [0] i32 = 0

Section[7] Export (5 bytes)
  [0] "m"  Memory(0)

Section[8] Start (1 bytes)
  start: func[0]

Section[9] Element (7 bytes)
  [0] active table[0] offset=0 funcref funcs=[0]

Section[12] Datacount (1 bytes)
  datacount: 1

Section[10] Code (6 bytes)
  [0] locals=[] expr=3 bytes

Section[11] Data (8 bytes)
  [0] active mem[0] offset=0 data=2 bytes

Section[0] Custom (6 bytes)
  name="abc" payload=2 bytes

Section[0] Custom (11 bytes)
  name="name" payload=6 bytes
  function names:
    [0] "f"
"#;

const BAD_VERBOSE: &str = r#"magic: 0x6D736100, version: 1
sections:
  [ 1] type     size=1  (decode error: unexpected end of input)
  [ 2] import   size=1  (decode error: unexpected end of input)
  [ 3] func     size=1  (decode error: unexpected end of input)
  [ 4] table    size=1  (decode error: unexpected end of input)
  [ 5] memory   size=1  (decode error: unexpected end of input)
  [ 6] global   size=1  (decode error: unexpected end of input)
  [ 7] export   size=1  (decode error: unexpected end of input)
  [ 8] start    size=1  (decode error: unexpected end of input)
  [ 9] element  size=1  (decode error: unexpected end of input)
  [12] datacount size=1  (decode error: unexpected end of input)
  [10] code     size=1  (decode error: unexpected end of input)
  [11] data     size=1  (decode error: unexpected end of input)
  [ 0] custom   size=1  (decode error: unexpected end of input)

Section[1] Type (1 bytes)
  <decode error: unexpected end of input>

Section[2] Import (1 bytes)
  <decode error: unexpected end of input>

Section[3] Func (1 bytes)
  <decode error: unexpected end of input>

Section[4] Table (1 bytes)
  <decode error: unexpected end of input>

Section[5] Memory (1 bytes)
  <decode error: unexpected end of input>

Section[6] Global (1 bytes)
  <decode error: unexpected end of input>

Section[7] Export (1 bytes)
  <decode error: unexpected end of input>

Section[8] Start (1 bytes)
  <decode error: unexpected end of input>

Section[9] Element (1 bytes)
  <decode error: unexpected end of input>

Section[12] Datacount (1 bytes)
  <decode error: unexpected end of input>

Section[10] Code (1 bytes)
  <decode error: unexpected end of input>

Section[11] Data (1 bytes)
  <decode error: unexpected end of input>

Section[0] Custom (1 bytes)
  <decode error: unexpected end of input>
"#;

const BADNAME_VERBOSE: &str = r#"magic: 0x6D736100, version: 1
sections:
  [ 0] custom   size=8  (name "name")  (decode error: unexpected end of input)

Section[0] Custom (8 bytes)
  name="name" payload=3 bytes
  <name section decode error: unexpected end of input>
"#;

#[test]
fn good_verbose_output_is_pinned() {
    let o = run("good-v", &all_good(), &["-v"]);
    assert_eq!(o.stdout, GOOD_VERBOSE);
    assert_eq!(o.stderr, "");
    assert_eq!(o.code, 0);
}

#[test]
fn good_default_output_is_the_verbose_listing() {
    let o = run("good-d", &all_good(), &[]);
    assert_eq!(o.stdout, listing(GOOD_VERBOSE));
    assert_eq!((o.stderr.as_str(), o.code), ("", 0));
}

#[test]
fn bad_verbose_reports_every_section_once_and_exits_one() {
    let o = run("bad-v", &all_bad(), &["-v"]);
    assert_eq!(o.stdout, BAD_VERBOSE);
    assert_eq!(o.stderr, "error: 13 section(s) failed to decode\n");
    assert_eq!(o.code, 1);
}

#[test]
fn bad_default_output_counts_each_failure_once() {
    let o = run("bad-d", &all_bad(), &[]);
    assert_eq!(o.stdout, listing(BAD_VERBOSE));
    assert_eq!(o.stderr, "error: 13 section(s) failed to decode\n");
    assert_eq!(o.code, 1);
}

#[test]
fn bad_explain_keeps_exit_zero_and_no_summary() {
    let o = run("bad-e", &all_bad(), &["-v", "--explain"]);
    assert!(o.stdout.starts_with(BAD_VERBOSE));
    assert!(o.stdout.contains("\nexplain:\n"));
    assert_eq!((o.stderr.as_str(), o.code), ("", 0));
}

#[test]
fn bad_name_section_is_counted_once() {
    let o = run("bn-v", &bad_name(), &["-v"]);
    assert_eq!(o.stdout, BADNAME_VERBOSE);
    assert_eq!(o.stderr, "error: 1 section(s) failed to decode\n");
    assert_eq!(o.code, 1);

    let o = run("bn-d", &bad_name(), &[]);
    assert_eq!(o.stdout, listing(BADNAME_VERBOSE));
    assert_eq!(o.stderr, "error: 1 section(s) failed to decode\n");
    assert_eq!(o.code, 1);
}

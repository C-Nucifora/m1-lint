//! CLI argument-parsing acceptance tests.
//!
//! These run the compiled `m1-lint` binary to verify the command-line surface,
//! in particular that the GNU `--flag=value` form is accepted alongside the
//! space-separated `--flag value` form (#69) — matching m1-fmt/m1-typecheck.

use std::io::Write;
use std::process::Command;

/// Write `contents` to a uniquely-named temp file and return its path.
fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("m1lint-cli-{}-{}", std::process::id(), name));
    let mut f = std::fs::File::create(&dir).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    dir
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_m1-lint"))
}

#[test]
fn format_equals_json_parses() {
    // `--format=json` must be accepted just like `--format json`.
    let file = temp_file("fmt-eq.m1scr", "x = a == b;\n");
    let out = bin()
        .arg("--format=json")
        .arg(&file)
        .output()
        .expect("run m1-lint");
    let _ = std::fs::remove_file(&file);
    assert!(
        out.status.code() != Some(2),
        "--format=json should parse (exit 2 = arg error). stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"version\":2"),
        "JSON output expected, got: {stdout}"
    );
}

#[test]
fn format_space_json_still_parses() {
    // Regression guard: the space-separated form keeps working.
    let file = temp_file("fmt-sp.m1scr", "x = a == b;\n");
    let out = bin()
        .arg("--format")
        .arg("json")
        .arg(&file)
        .output()
        .expect("run m1-lint");
    let _ = std::fs::remove_file(&file);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"version\":2"), "got: {stdout}");
}

#[test]
fn select_equals_code_parses() {
    // `--select=L010` must be accepted just like `--select L010`.
    let file = temp_file("sel-eq.m1scr", "x = 1;\n");
    let out = bin()
        .arg("--select=L010")
        .arg(&file)
        .output()
        .expect("run m1-lint");
    let _ = std::fs::remove_file(&file);
    assert_ne!(
        out.status.code(),
        Some(2),
        "--select=L010 should parse, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unknown_equals_flag_still_rejected() {
    // A genuinely unknown `--flag=value` must still be an arg error (exit 2),
    // not silently accepted.
    let file = temp_file("unk.m1scr", "x = 1;\n");
    let out = bin()
        .arg("--bogus=1")
        .arg(&file)
        .output()
        .expect("run m1-lint");
    let _ = std::fs::remove_file(&file);
    assert_eq!(out.status.code(), Some(2), "unknown flag must exit 2");
}

/// Write raw `bytes` to a uniquely-named temp file and return its path. Used to
/// plant a non-UTF-8 (`0xB0`) MoTeC byte that strict `read_to_string` rejects.
fn temp_file_bytes(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("m1lint-cli-{}-{}", std::process::id(), name));
    let mut f = std::fs::File::create(&dir).unwrap();
    f.write_all(bytes).unwrap();
    dir
}

#[test]
fn windows_1252_file_is_linted_and_batch_continues() {
    // A `.m1scr` with a Windows-1252 `°` (0xB0) in a comment is valid MoTeC but
    // not valid UTF-8. Passed FIRST in the batch, it must (a) be decoded and
    // linted rather than aborting the run, and (b) not prevent the later dirty
    // file's L002 trailing-whitespace diagnostic from being reported (#66).
    let deg = temp_file_bytes("deg.m1scr", b"// yaw \xb0/s\n[\n]\n");
    let dirty = temp_file("trailing.m1scr", "Math.Constant Y = 2\t \n");
    let out = bin().arg(&deg).arg(&dirty).output().expect("run m1-lint");
    let _ = std::fs::remove_file(&deg);
    let _ = std::fs::remove_file(&dirty);

    let stderr = String::from_utf8_lossy(&out.stderr);
    // The decode must not surface as a read error...
    assert!(
        !stderr.contains("could not read"),
        "0xB0 file should be decoded, not reported unreadable. stderr: {stderr}"
    );
    // ...the later dirty file must still be linted (its L002 appears)...
    assert!(
        stderr.contains("L002"),
        "later file's L002 must be reported (no abort). stderr: {stderr}"
    );
    // ...and the run must not abort with the usage/arg code.
    assert_ne!(
        out.status.code(),
        Some(2),
        "decode path must not exit 2. stderr: {stderr}"
    );
}

#[test]
fn unreadable_file_continues_and_exits_one() {
    // A genuinely unreadable path (does not exist) placed FIRST must report an
    // error, keep linting the later dirty file (its L002 still shows), and exit
    // with the lint-failure code 1 — not abort the batch with 2 (#66).
    let mut missing = std::env::temp_dir();
    missing.push(format!(
        "m1lint-cli-{}-nonexistent.m1scr",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing); // ensure it does not exist
    let dirty = temp_file("trailing-after-missing.m1scr", "Math.Constant Y = 2\t \n");
    let out = bin()
        .arg(&missing)
        .arg(&dirty)
        .output()
        .expect("run m1-lint");
    let _ = std::fs::remove_file(&dirty);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("could not read"),
        "missing file should report a read error. stderr: {stderr}"
    );
    assert!(
        stderr.contains("L002"),
        "later file must still be linted after an unreadable one. stderr: {stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "an unreadable file must exit 1 (lint failure), not 2 (abort). stderr: {stderr}"
    );
}

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

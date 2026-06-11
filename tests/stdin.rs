//! stdin input (#119): a lone `-` (or no file arguments) lints stdin,
//! `--stdin-filename` names it in output — mirroring m1-fmt's CLI surface.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_m1-lint"))
}

fn run_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = bin()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A run that rejects its arguments exits without reading stdin; the write
    // then sees EPIPE, which is fine — we only care about the child's output.
    let _ = child.stdin.as_mut().unwrap().write_all(input.as_bytes());
    child.wait_with_output().unwrap()
}

// `a == b` trips L004 (eq-operator-preferred), which is also fixable.
const INPUT: &str = "x = a == b;\n";

#[test]
fn dash_reads_stdin_and_reports_under_stdin_name() {
    let out = run_with_stdin(&["-"], INPUT);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("<stdin>"), "got:\n{stderr}");
    assert!(stderr.contains("L004"), "got:\n{stderr}");
    // L004 is warning severity: a clean exit.
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn no_files_reads_stdin() {
    let out = run_with_stdin(&[], INPUT);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("L004"), "got:\n{stderr}");
}

#[test]
fn stdin_filename_names_the_output() {
    let out = run_with_stdin(&["-", "--stdin-filename", "Foo.m1scr"], INPUT);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Foo.m1scr"), "got:\n{stderr}");
    assert!(!stderr.contains("<stdin>"));
}

#[test]
fn fix_writes_the_fixed_source_to_stdout() {
    let out = run_with_stdin(&["-", "--fix"], INPUT);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "x = a eq b;\n");
}

#[test]
fn fix_passes_through_clean_input() {
    let clean = "x = a eq b;\n";
    let out = run_with_stdin(&["-", "--fix"], clean);
    assert_eq!(String::from_utf8_lossy(&out.stdout), clean);
}

#[test]
fn json_format_uses_the_stdin_name() {
    let out = run_with_stdin(&["-", "--format", "json"], INPUT);
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
    assert_eq!(v["files"][0]["path"], "<stdin>");
}

#[test]
fn fix_with_machine_format_on_stdin_is_rejected() {
    // Both would claim stdout; refuse rather than interleave.
    let out = run_with_stdin(&["-", "--fix", "--format", "json"], INPUT);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn diff_previews_without_writing() {
    let out = run_with_stdin(&["-", "--diff"], INPUT);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("-x = a == b;"), "got:\n{stdout}");
    assert!(stdout.contains("+x = a eq b;"));
    // Non-empty diff exits 1, matching the file path behaviour.
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn dash_mixed_with_files_is_rejected() {
    let out = run_with_stdin(&["-", "also-a-file.m1scr"], INPUT);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn error_severity_finding_on_stdin_exits_one() {
    // L006 float-eq-comparison is error severity.
    let out = run_with_stdin(&["-"], "local f = 1.5;\nif (f == 2.5)\n{\n}\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("L006"), "got:\n{stderr}");
    assert_eq!(out.status.code(), Some(1));
}

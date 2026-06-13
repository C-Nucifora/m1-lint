//! Autofix acceptance tests: fix(in) == out, and fix is idempotent.

use std::path::Path;

use m1_lint::registry::Registry;
use m1_lint::runner::Runner;

fn run_fix(stem: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures_fix");
    let input = std::fs::read_to_string(dir.join(format!("{stem}.in.m1scr"))).unwrap();
    let expected = std::fs::read_to_string(dir.join(format!("{stem}.out.m1scr"))).unwrap();

    let runner = Runner::new(Registry::default());
    let fixed = runner.fix_source(&input).unwrap().unwrap_or(input.clone());
    assert_eq!(fixed, expected, "fix mismatch for {stem}");

    // Idempotency: fixing the output yields no further change.
    assert_eq!(
        runner.fix_source(&expected).unwrap(),
        None,
        "not idempotent: {stem}"
    );
}

#[test]
fn fix_eq_op() {
    run_fix("eq_op");
}
#[test]
fn fix_logical() {
    run_fix("logical");
}
#[test]
fn fix_spacing() {
    run_fix("spacing");
}
#[test]
fn fix_trailing() {
    run_fix("trailing");
}
#[test]
fn fix_comment() {
    run_fix("comment");
}
#[test]
fn fix_l018_semicolon() {
    run_fix("l018_semicolon");
}
#[test]
fn fix_l021_one_stmt() {
    run_fix("l021_one_stmt");
}
#[test]
fn fix_l022_kw_paren() {
    run_fix("l022_kw_paren");
}
#[test]
fn fix_l023_call_paren() {
    run_fix("l023_call_paren");
}
#[test]
fn fix_l024_ternary() {
    run_fix("l024_ternary");
}
#[test]
fn fix_l026_top_indent() {
    run_fix("l026_top_indent");
}

// L027 (file-final-blank-line) is opt-in; run_fix uses Registry::default()
// which excludes it. Test it here with an explicit opt-in registry using
// the public rule struct.
#[test]
fn fix_l027_final_blank_line() {
    use m1_lint::rules::l027_file_final_blank_line::FileFinalBlankLine;

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures_fix");
    let input = std::fs::read_to_string(dir.join("l027_final_blank.in.m1scr")).unwrap();
    let expected = std::fs::read_to_string(dir.join("l027_final_blank.out.m1scr")).unwrap();

    let mut reg = Registry::empty();
    reg.register(Box::new(FileFinalBlankLine));
    let runner = Runner::new(reg);

    let fixed = runner.fix_source(&input).unwrap().unwrap_or(input.clone());
    assert_eq!(fixed, expected, "fix mismatch for l027_final_blank");

    assert_eq!(
        runner.fix_source(&expected).unwrap(),
        None,
        "not idempotent: l027_final_blank"
    );
}

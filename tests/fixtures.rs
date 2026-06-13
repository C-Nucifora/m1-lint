//! Fixture-based acceptance tests.
//!
//! Each fixture is a pair `<stem>.m1scr` (input) and `<stem>.diag` (expected
//! diagnostics, one `line:col:code:message-fragment` per line). The harness
//! asserts every expected diagnostic is present (by line, code, and message
//! fragment).

use m1_lint::registry::Registry;
use m1_lint::runner::Runner;
use std::path::Path;

fn runner() -> Runner {
    Runner::new(Registry::default())
}

fn run_fixture(stem: &str) {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source_path = fixture_dir.join(format!("{}.m1scr", stem));
    let diag_path = fixture_dir.join(format!("{}.diag", stem));

    let source = std::fs::read_to_string(&source_path)
        .unwrap_or_else(|_| panic!("fixture source not found: {}", source_path.display()));

    let expected_raw = std::fs::read_to_string(&diag_path)
        .unwrap_or_else(|_| panic!("fixture diag not found: {}", diag_path.display()));

    let result = runner().run_source(&source);

    for line in expected_raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        assert_eq!(parts.len(), 4, "malformed fixture line: {}", line);
        let exp_line: u32 = parts[0].parse().expect("line number");
        let exp_code = parts[2];
        let exp_msg_frag = parts[3];

        let found = result.diagnostics.iter().any(|d| {
            d.inner.range.start.line + 1 == exp_line
                && d.code.to_string() == exp_code
                && d.inner.message.contains(exp_msg_frag)
        });

        assert!(
            found,
            "expected diagnostic {}:{} '{}' not found for fixture '{}'.\nActual:\n{:#?}",
            exp_line, exp_code, exp_msg_frag, stem, result.diagnostics
        );
    }
}

#[test]
fn fixture_l001_long_line() {
    run_fixture("l001_long_line");
}
#[test]
fn fixture_l002_trailing_ws() {
    run_fixture("l002_trailing_ws");
}
#[test]
fn fixture_l003_no_final_newline() {
    run_fixture("l003_no_final_newline");
}
#[test]
fn fixture_l004_eq_eq() {
    run_fixture("l004_eq_eq");
}
#[test]
fn fixture_l005_logical_ops() {
    run_fixture("l005_logical_ops");
}
#[test]
fn fixture_l006_float_eq() {
    run_fixture("l006_float_eq");
}
#[test]
fn fixture_l007_op_spacing() {
    run_fixture("l007_op_spacing");
}
#[test]
fn fixture_l008_nesting() {
    run_fixture("l008_nesting");
}
#[test]
fn fixture_l009_complex() {
    run_fixture("l009_complex");
}
#[test]
fn fixture_l019_cognitive() {
    run_fixture("l019_cognitive");
}
#[test]
fn fixture_l012_unused() {
    run_fixture("l012_unused");
}
#[test]
fn fixture_l010_spaces() {
    run_fixture("l010_spaces");
}
#[test]
fn fixture_l011_comment() {
    run_fixture("l011_comment");
}
#[test]
fn fixture_l014_expand_undef() {
    run_fixture("l014_expand_undef");
}
#[test]
fn fixture_l015_local_no_init() {
    run_fixture("l015_local_no_init");
}
#[test]
fn fixture_l016_local_naming() {
    run_fixture("l016_local_naming");
}
#[test]
fn fixture_l018_semicolon_space() {
    run_fixture("l018_semicolon_space");
}
#[test]
fn fixture_l020_object_naming() {
    run_fixture("l020_object_naming");
}
#[test]
fn fixture_l021_one_stmt() {
    run_fixture("l021_one_stmt");
}
#[test]
fn fixture_l022_kw_paren() {
    run_fixture("l022_kw_paren");
}
#[test]
fn fixture_l023_call_paren() {
    run_fixture("l023_call_paren");
}
#[test]
fn fixture_l024_ternary_parens() {
    run_fixture("l024_ternary_parens");
}
#[test]
fn fixture_l025_local_scope() {
    run_fixture("l025_local_scope");
}
#[test]
fn fixture_l026_top_indent() {
    run_fixture("l026_top_indent");
}

// L017 (magic-number) and L027 (file-final-blank-line) are opt-in rules not
// included in Registry::default(), so they cannot use run_fixture. They are
// exercised here with an explicit opt-in registry using the public rule structs.

#[test]
fn fixture_l017_magic_number() {
    use m1_lint::diagnostic::LintCode;
    use m1_lint::rules::l017_magic_number::MagicNumber;

    let mut reg = Registry::empty();
    reg.register(Box::new(MagicNumber));
    let runner = Runner::new(reg);
    let src = "Energy = Power * 0.05;\n";
    let result = runner.run_source(src);
    let found = result.diagnostics.iter().any(|d| {
        d.code == LintCode::L017
            && d.inner.range.start.line + 1 == 1
            && d.inner.message.contains("magic number `0.05`")
    });
    assert!(
        found,
        "expected L017 for magic number 0.05.\nActual:\n{:#?}",
        result.diagnostics
    );
}

#[test]
fn fixture_l027_final_blank_line() {
    use m1_lint::diagnostic::LintCode;
    use m1_lint::rules::l027_file_final_blank_line::FileFinalBlankLine;

    let mut reg = Registry::empty();
    reg.register(Box::new(FileFinalBlankLine));
    let runner = Runner::new(reg);
    let src = "x = 1;\n";
    let result = runner.run_source(src);
    let found = result
        .diagnostics
        .iter()
        .any(|d| d.code == LintCode::L027 && d.inner.message.contains("end with a blank line"));
    assert!(
        found,
        "expected L027 for missing final blank line.\nActual:\n{:#?}",
        result.diagnostics
    );
}

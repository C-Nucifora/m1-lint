use m1_lint::config::Config;
use m1_lint::diagnostic::LintCode;
use m1_lint::registry::Registry;
use m1_lint::runner::Runner;
use std::io::Write;
use std::process::{Command, Stdio};

fn runner(codes: &[LintCode]) -> Runner {
    let config = Config {
        enabled: codes.iter().copied().collect(),
        ..Config::default()
    };
    Runner::new(Registry::from_config(&config))
}

#[test]
fn fixes_respect_allow_annotations_and_keep_independent_edits() {
    let lint = runner(&[LintCode::L004]);
    for protected in [
        "// @m1:allow(L004)\nOut = a == b;\n",
        "Out = a == b; // @m1:allow(L004)\n",
        "/* @m1:allow(L004) */\nOut = a == b;\n",
        "// @m1:allow\nOut = a == b;\n",
        "// @m1:allow(L004)\nif (Flag) {\nOut = a == b;\n}\n",
    ] {
        assert!(
            lint.run_source(protected).diagnostics.is_empty(),
            "{protected}"
        );
        assert_eq!(
            lint.fix_source_stable(protected).unwrap(),
            None,
            "{protected}"
        );
        let source = format!("{protected}Other = c == d;\n");
        let expected = format!("{protected}Other = c eq d;\n");
        assert_eq!(lint.fix_source_stable(&source).unwrap().unwrap(), expected);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Test.m1scr");
        std::fs::write(&path, &source).unwrap();
        assert!(lint.fix_file(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    }
    let source = "// @m1:allow(L002)\nOut = a == b;  \nOther = c == d;  \n";
    let mixed = runner(&[LintCode::L002, LintCode::L004]);
    assert_eq!(
        mixed.fix_source_stable(source).unwrap().unwrap(),
        "// @m1:allow(L002)\nOut = a eq b;  \nOther = c eq d;\n"
    );
}

#[test]
fn unicode_ranges_are_scalar_columns_in_json_and_sarif() {
    let source = "/* é😀 */ Out=1;  \n// é😀";
    let runner = runner(&[LintCode::L002, LintCode::L003, LintCode::L007]);
    let result = runner.run_source(source);
    for (code, line, column, byte) in [
        (LintCode::L002, 0, 15, 19),
        (LintCode::L003, 1, 5, source.len()),
        (LintCode::L007, 0, 12, 16),
    ] {
        let diagnostic = result.diagnostics.iter().find(|d| d.code == code).unwrap();
        assert_eq!(diagnostic.inner.range.start.line, line);
        assert_eq!(diagnostic.inner.range.start.column, column);
        assert_eq!(diagnostic.inner.byte_range.start, byte);
    }
    let files = vec![("Unicode.m1scr".into(), result)];
    let json: serde_json::Value =
        serde_json::from_str(&m1_lint::report::render_json(&files)).unwrap();
    let trailing = json["files"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "L002")
        .unwrap();
    assert_eq!(trailing["range"]["start"]["column"], 15);
    let sarif: serde_json::Value =
        serde_json::from_str(&m1_lint::report::render_sarif(&files)).unwrap();
    assert_eq!(sarif["runs"][0]["columnKind"], "unicodeCodePoints");
    let trailing = sarif["runs"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["ruleId"] == "L002")
        .unwrap();
    assert_eq!(
        trailing["locations"][0]["physicalLocation"]["region"]["startColumn"],
        16
    );
}

#[test]
fn incomplete_inputs_cannot_replace_a_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let valid = dir.path().join("Valid.m1scr");
    let missing = dir.path().join("Missing.m1scr");
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    std::fs::write(&valid, "Out = a eq 1.5;\n").unwrap();
    for inputs in [vec![&missing], vec![&valid, &missing], vec![&empty]] {
        std::fs::write(&baseline, "keep previous complete baseline").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
            .args(["--jobs", "1", "--write-baseline"])
            .arg(&baseline)
            .args(inputs)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(
            std::fs::read_to_string(&baseline).unwrap(),
            "keep previous complete baseline"
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .args(["--jobs", "1", "--select", "L006", "--write-baseline"])
        .arg(&baseline)
        .arg(&valid)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(std::fs::read_to_string(baseline).unwrap().contains("L006"));
}

#[test]
fn diff_rejects_machine_output_for_stdin_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Test.m1scr");
    for source in ["Out=1;\n", "Out = 1;\n"] {
        std::fs::write(&file, source).unwrap();
        for format in ["json", "sarif"] {
            for input in [file.as_os_str(), std::ffi::OsStr::new("-")] {
                let mut child = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
                    .args(["--diff", "--format", format])
                    .arg(input)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
                let output = child.wait_with_output().unwrap();
                assert!(!output.status.success());
                assert!(output.stdout.is_empty());
                assert!(String::from_utf8_lossy(&output.stderr).contains("--format human"));
            }
        }
    }
}

#[test]
fn cli_fixes_keep_suppressed_statements_for_files_and_stdin() {
    let source = "// @m1:allow(L004)\nOut = a == b;\nOther = c == d;\n";
    let expected = "// @m1:allow(L004)\nOut = a == b;\nOther = c eq d;\n";
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Test.m1scr");
    std::fs::write(&file, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .args(["--jobs", "1", "--select", "L004", "--fix"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(file).unwrap(), expected);
    let mut child = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .args(["--jobs", "1", "--select", "L004", "--fix", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
#[cfg(unix)]
fn incomplete_directory_discovery_preserves_the_previous_baseline() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("private");
    std::fs::create_dir(&private).unwrap();
    for path in [
        dir.path().join("Visible.m1scr"),
        private.join("Hidden.m1scr"),
    ] {
        std::fs::write(path, "Out = value eq 1.5;\n").unwrap();
    }
    let baseline = dir.path().join("baseline.json");
    std::fs::write(&baseline, "prior complete baseline").unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o0)).unwrap();
    let inaccessible = std::fs::read_dir(&private).is_err();
    let output = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .args(["--jobs", "1", "--select", "L006", "--write-baseline"])
        .arg(&baseline)
        .arg(dir.path())
        .output();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    if !inaccessible {
        eprintln!("permission-denied discovery requires an unprivileged test user");
        return;
    }
    let output = output.unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("private"));
    assert_eq!(
        std::fs::read_to_string(baseline).unwrap(),
        "prior complete baseline"
    );
}

#[test]
fn discovery_depth_limit_cannot_silently_truncate_a_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    std::fs::write(&baseline, "prior complete baseline").unwrap();
    std::fs::write(dir.path().join("Visible.m1scr"), "Out = value eq 1.5;\n").unwrap();
    let mut nested = dir.path().to_path_buf();
    for _ in 0..65 {
        nested.push("d");
    }
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("Hidden.m1scr"), "Out = value eq 1.5;\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .args(["--jobs", "1", "--select", "L006", "--write-baseline"])
        .arg(&baseline)
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("depth 64"));
    assert_eq!(
        std::fs::read_to_string(baseline).unwrap(),
        "prior complete baseline"
    );
}

#[test]
#[cfg(unix)]
fn discovery_still_skips_descendant_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let scripts = dir.path().join("scripts");
    std::fs::create_dir(&scripts).unwrap();
    std::fs::write(scripts.join("Visible.m1scr"), "Out = value eq 1.5;\n").unwrap();
    std::os::unix::fs::symlink(dir.path(), scripts.join("cycle")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("absent"), scripts.join("Missing.m1scr")).unwrap();
    let baseline = dir.path().join("baseline.json");
    let output = Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .args(["--jobs", "1", "--select", "L006", "--write-baseline"])
        .arg(&baseline)
        .arg(&scripts)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(baseline).unwrap()).unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), 1);
}

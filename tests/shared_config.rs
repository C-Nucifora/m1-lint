//! Cross-tool configuration belongs in the public loader used by CLI and MCP.

use m1_core::Severity;
use m1_lint::config::Config;
use m1_lint::diagnostic::LintCode;
use m1_workspace::config::M1ToolsConfig;
use std::process::Command;

#[test]
fn shared_diagnostics_partition_select_ignore_and_severity() {
    let shared = M1ToolsConfig::from_toml_str(
        r#"[diagnostics]
select = ["L004", "L007", "T002"]
ignore = ["L007", "T002"]
[diagnostics.severity]
L004 = "error"
T002 = "warning"
"#,
    )
    .unwrap();
    let mut config = Config::default();
    config.apply_tools_config(&shared).unwrap();
    assert_eq!(config.enabled, [LintCode::L004].into_iter().collect());
    assert_eq!(config.severity_overrides.len(), 1);
    assert_eq!(
        config.severity_overrides.get(&LintCode::L004),
        Some(&Severity::Error)
    );
}

#[test]
fn other_tools_ignore_and_severity_entries_leave_default_lint_rules_enabled() {
    let shared = M1ToolsConfig::from_toml_str(
        "[diagnostics]\nignore = [\"T002\"]\n\
         [diagnostics.severity]\nT002 = \"error\"\n",
    )
    .unwrap();
    let mut config = Config::default();
    config.apply_tools_config(&shared).unwrap();
    assert_eq!(config.enabled, Config::default().enabled);
    assert!(config.severity_overrides.is_empty());
}

#[test]
fn type_only_shared_selection_disables_lint_until_a_later_overlay() {
    let shared = M1ToolsConfig::from_toml_str(
        "[diagnostics]\nselect = [\"T002\"]\nignore = [\"T002\"]\n\
         [diagnostics.severity]\nT002 = \"error\"\n",
    )
    .unwrap();
    let mut config = Config::default();
    config.apply_tools_config(&shared).unwrap();
    assert!(config.enabled.is_empty());
    assert!(config.severity_overrides.is_empty());
    config.apply_toml_str("select = [\"L004\"]\n").unwrap();
    assert_eq!(config.enabled, [LintCode::L004].into_iter().collect());
}

#[test]
fn shared_lint_entries_and_lint_specific_inputs_remain_strict() {
    for body in [
        "[diagnostics]\nselect = [\"L999\"]\n",
        "[diagnostics]\nignore = [\"L999\"]\n",
        "[diagnostics.severity]\nL999 = \"error\"\n",
        "[diagnostics.severity]\nL004 = \"fatal\"\n",
    ] {
        let shared = M1ToolsConfig::from_toml_str(body).unwrap();
        assert!(
            Config::default().apply_tools_config(&shared).is_err(),
            "{body}"
        );
    }
    for code in ["T002", "L999"] {
        for key in ["select", "ignore"] {
            let body = format!("{key} = [\"{code}\"]\n");
            assert!(Config::from_toml_str(&body).is_err(), "{body}");
        }
        let body = format!("[severity]\n{code} = \"error\"\n");
        assert!(Config::from_toml_str(&body).is_err(), "{body}");
    }
}

fn cli(shared: &str, tool: &str, flags: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("m1-tools.toml"), shared).unwrap();
    // Isolate these cases from discovered user-global lint configuration.
    std::fs::write(dir.path().join(".m1lint.toml"), tool).unwrap();
    std::fs::write(dir.path().join("Probe.m1scr"), "Out = 1 == 2;\n").unwrap();
    Command::new(env!("CARGO_BIN_EXE_m1-lint"))
        .current_dir(dir.path())
        .args(["--jobs", "1", "--format", "json"])
        .args(flags)
        .arg("Probe.m1scr")
        .output()
        .unwrap()
}

#[test]
fn cli_accepts_shared_codes_and_preserves_overlay_precedence() {
    for (shared, tool, flags, severity) in [
        (
            "[diagnostics]\nselect = [\"L004\"]\nignore = [\"L007\"]\n\
             [diagnostics.severity]\nL004 = \"warning\"\n",
            "",
            &[][..],
            Some("warning"),
        ),
        (
            "[diagnostics]\nselect = [\"L004\", \"T002\"]\n",
            "",
            &[][..],
            Some("warning"),
        ),
        (
            "[diagnostics]\nselect = [\"L004\"]\nignore = [\"L007\", \"T002\"]\n",
            "",
            &[][..],
            Some("warning"),
        ),
        (
            "[diagnostics]\nselect = [\"L004\"]\n\
             [diagnostics.severity]\nL004 = \"error\"\nT002 = \"warning\"\n",
            "",
            &[][..],
            Some("error"),
        ),
        ("[diagnostics]\nselect = [\"T002\"]\n", "", &[][..], None),
        (
            "[diagnostics]\nselect = [\"T002\"]\n",
            "select = [\"L004\"]\n",
            &[][..],
            Some("warning"),
        ),
        (
            "[diagnostics]\nselect = [\"T002\"]\n",
            "",
            &["--select", "L004"][..],
            Some("warning"),
        ),
    ] {
        let output = cli(shared, tool, flags);
        assert_eq!(
            output.status.code(),
            Some(i32::from(severity == Some("error"))),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let diagnostics = json["files"][0]["diagnostics"].as_array().unwrap();
        if let Some(severity) = severity {
            assert_eq!(diagnostics.len(), 1, "{json}");
            assert_eq!(diagnostics[0]["code"], "L004");
            assert_eq!(diagnostics[0]["severity"], severity);
        } else {
            assert!(diagnostics.is_empty(), "{json}");
        }
    }
}

#[test]
fn cli_rejects_invalid_lint_entries_in_each_config_layer() {
    for shared in [
        "[diagnostics]\nselect = [\"L999\"]\n",
        "[diagnostics]\nignore = [\"L999\"]\n",
        "[diagnostics.severity]\nL999 = \"error\"\n",
        "[diagnostics.severity]\nL004 = \"fatal\"\n",
    ] {
        assert_eq!(cli(shared, "", &[]).status.code(), Some(2), "{shared}");
    }
    for tool in [
        "select = [\"T002\"]\n",
        "ignore = [\"T002\"]\n",
        "[severity]\nT002 = \"error\"\n",
    ] {
        assert_eq!(cli("", tool, &[]).status.code(), Some(2), "{tool}");
    }
    for flag in ["--select", "--ignore"] {
        for code in ["T002", "L999"] {
            assert_eq!(cli("", "", &[flag, code]).status.code(), Some(2));
        }
    }
}

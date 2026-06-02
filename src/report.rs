//! Rendering of lint results: the v1 human format and the v2 JSON format.

use std::fmt::Write as _;

use m1_core::Severity;

use crate::runner::RunResult;

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

/// Human-readable lines for one file (matches v1 output, returned as a String
/// so the caller chooses the stream). Line/column are 1-based here.
pub fn render_human(path: &str, result: &RunResult) -> String {
    let mut out = String::new();
    for d in &result.syntax_errors {
        let _ = writeln!(
            out,
            "{path}:{}:{}: error[syntax]: {}",
            d.range.start.line + 1,
            d.range.start.column + 1,
            d.message
        );
    }
    for d in &result.diagnostics {
        let _ = writeln!(
            out,
            "{path}:{}:{}: {}[{}]: {}",
            d.inner.range.start.line + 1,
            d.inner.range.start.column + 1,
            severity_str(d.inner.severity),
            d.code,
            d.inner.message
        );
    }
    out
}

fn esc(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// One file's portion of the JSON document. Line/column are 0-based bytes
/// (matching `m1_core::Position`; m1-lsp does UTF-16 conversion).
pub fn render_json(files: &[(String, RunResult)]) -> String {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut out = String::from("{\"version\":2,\"files\":[");
    for (fi, (path, result)) in files.iter().enumerate() {
        if fi > 0 {
            out.push(',');
        }
        out.push_str("{\"path\":");
        esc(path, &mut out);
        out.push_str(",\"syntax_errors\":[");
        for (i, d) in result.syntax_errors.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            errors += 1;
            out.push_str("{\"code\":\"syntax\",\"severity\":");
            esc(severity_str(d.severity), &mut out);
            out.push_str(",\"message\":");
            esc(&d.message, &mut out);
            range_json(&mut out, &d.range, &d.byte_range);
            out.push('}');
        }
        out.push_str("],\"diagnostics\":[");
        for (i, d) in result.diagnostics.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match d.inner.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                _ => {}
            }
            out.push_str("{\"code\":");
            esc(&d.code.to_string(), &mut out);
            out.push_str(",\"name\":");
            esc(d.code.name(), &mut out);
            out.push_str(",\"severity\":");
            esc(severity_str(d.inner.severity), &mut out);
            out.push_str(",\"message\":");
            esc(&d.inner.message, &mut out);
            range_json(&mut out, &d.inner.range, &d.inner.byte_range);
            let _ = write!(out, ",\"fixable\":{}", d.code.fixable());
            out.push('}');
        }
        out.push_str("]}");
    }
    let _ = write!(
        out,
        "],\"summary\":{{\"errors\":{errors},\"warnings\":{warnings},\"files\":{}}}}}",
        files.len()
    );
    out
}

/// The full rule catalogue as JSON (schema version 1):
/// `{"version":1,"rules":[{"code","name","fixable"},…]}`.
///
/// Sourced directly from [`crate::diagnostic::LintCode`] — the single source of
/// truth for the rule set — so external consumers (editor plugins, docs) can
/// enumerate the rules without copying the list and drifting out of sync.
pub fn render_rules_json() -> String {
    use crate::diagnostic::LintCode;
    let mut out = String::from("{\"version\":1,\"rules\":[");
    for (i, code) in LintCode::all_codes().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"code\":");
        esc(&code.to_string(), &mut out);
        out.push_str(",\"name\":");
        esc(code.name(), &mut out);
        let _ = write!(out, ",\"fixable\":{}", code.fixable());
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// The rule catalogue as human-readable lines (`Lxxx  name  (fixable)`).
pub fn render_rules_human() -> String {
    use crate::diagnostic::LintCode;
    let mut out = String::new();
    for code in LintCode::all_codes() {
        let _ = writeln!(
            out,
            "{code}  {}{}",
            code.name(),
            if code.fixable() { "  (fixable)" } else { "" }
        );
    }
    out
}

fn range_json(out: &mut String, range: &m1_core::Range, byte: &std::ops::Range<usize>) {
    let _ = write!(
        out,
        ",\"range\":{{\"start\":{{\"line\":{},\"column\":{}}},\"end\":{{\"line\":{},\"column\":{}}}}},\"byte_range\":{{\"start\":{},\"end\":{}}}",
        range.start.line,
        range.start.column,
        range.end.line,
        range.end.column,
        byte.start,
        byte.end
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    #[test]
    fn json_has_expected_shape() {
        let runner = Runner::new(Registry::default());
        let result = runner.run_source("x = a == b;\n");
        let json = render_json(&[("Demo.m1scr".to_string(), result)]);
        assert!(json.starts_with("{\"version\":2,"));
        assert!(json.contains("\"code\":\"L004\""));
        assert!(json.contains("\"name\":\"eq-operator-preferred\""));
        assert!(json.contains("\"fixable\":true"));
        assert!(json.contains("\"summary\":{\"errors\":0,\"warnings\":1,\"files\":1}"));
    }

    #[test]
    fn escapes_strings() {
        let mut s = String::new();
        esc("a\"b\\c\n", &mut s);
        assert_eq!(s, "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn rules_json_covers_every_code() {
        use crate::diagnostic::LintCode;
        let json = render_rules_json();
        assert!(json.starts_with("{\"version\":1,\"rules\":["));
        // Every code, name and fixable flag is present and matches LintCode.
        for code in LintCode::all_codes() {
            assert!(
                json.contains(&format!("\"code\":\"{code}\"")),
                "missing {code}"
            );
            assert!(json.contains(&format!("\"name\":\"{}\"", code.name())));
        }
        // L004 is fixable, L001 is not.
        assert!(
            json.contains("\"code\":\"L004\",\"name\":\"eq-operator-preferred\",\"fixable\":true")
        );
        assert!(json.contains("\"code\":\"L001\",\"name\":\"line-too-long\",\"fixable\":false"));
    }

    #[test]
    fn rules_human_lists_each_code() {
        use crate::diagnostic::LintCode;
        let text = render_rules_human();
        for code in LintCode::all_codes() {
            assert!(text.contains(&code.to_string()), "missing {code}");
        }
        assert!(text.contains("(fixable)"));
    }
}

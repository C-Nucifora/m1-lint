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

/// SARIF 2.1.0 output (`--format sarif`, #109) — the interchange format GitHub
/// code scanning and other CI systems ingest natively. One run, one
/// reportingDescriptor per rule (with help URI), one result per finding;
/// syntax errors are reported under the synthetic `syntax` rule id.
pub fn render_sarif(files: &[(String, RunResult)]) -> String {
    use crate::diagnostic::LintCode;
    use serde_json::json;

    fn level(s: Severity) -> &'static str {
        match s {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info | Severity::Hint => "note",
        }
    }

    let mut rules: Vec<serde_json::Value> = LintCode::all_codes()
        .iter()
        .map(|c| {
            json!({
                "id": c.to_string(),
                "name": c.name(),
                "helpUri": format!("https://github.com/C-Nucifora/m1-lint#{}", c.name()),
                "properties": {"fixable": c.fixable()},
            })
        })
        .collect();
    rules.push(json!({"id": "syntax", "name": "syntax-error"}));

    let mut results: Vec<serde_json::Value> = Vec::new();
    for (path, r) in files {
        for d in &r.syntax_errors {
            results.push(json!({
                "ruleId": "syntax",
                "level": "error",
                "message": {"text": d.message},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": path},
                    "region": {
                        "startLine": d.range.start.line + 1,
                        "startColumn": d.range.start.column + 1,
                    },
                }}],
            }));
        }
        for d in &r.diagnostics {
            results.push(json!({
                "ruleId": d.code.to_string(),
                "level": level(d.inner.severity),
                "message": {"text": d.inner.message},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": path},
                    "region": {
                        "startLine": d.inner.range.start.line + 1,
                        "startColumn": d.inner.range.start.column + 1,
                        "endLine": d.inner.range.end.line + 1,
                        "endColumn": d.inner.range.end.column + 1,
                    },
                }}],
            }));
        }
    }

    json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {
                "name": "m1-lint",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/C-Nucifora/m1-lint",
                "rules": rules,
            }},
            "results": results,
        }],
    })
    .to_string()
}

/// `--explain CODE` (#108): the full rationale for one rule at the terminal —
/// what it checks, why (with the manual reference where one exists), and what
/// `--fix` does. Kept adjacent to the rule registry so a new rule without an
/// explanation fails the `every_code_has_an_explanation` test.
pub fn explain(code: crate::diagnostic::LintCode) -> &'static str {
    use crate::diagnostic::LintCode::*;
    match code {
        L001 => {
            "L001 line-too-long\n\nFlags lines longer than max_line_length (default 88, a tool convention shared\nwith m1-fmt; the manual sets no numeric limit). Long lines hide trailing logic\nand diff badly. Not auto-fixable: breaking a line well needs m1-fmt's wrapper.\nConfig: max_line_length / --max-line-length."
        }
        L002 => {
            "L002 trailing-whitespace\n\nFlags spaces or tabs at end of line. Invisible churn in diffs; the manual's\nlayout section keeps lines clean. --fix deletes the trailing run (CRLF-safe)."
        }
        L003 => {
            "L003 missing-final-newline\n\nFlags a file whose last line has no terminating newline. POSIX tools and\nclean diffs expect one. --fix appends it."
        }
        L004 => {
            "L004 eq-operator-preferred\n\nM1 prefers the word operators: `eq`/`neq` over `==`/`!=` (the C spellings are\naccepted by the compiler but flagged by M1 Build itself). --fix rewrites,\npadding with spaces where an operand is glued so tokens never merge."
        }
        L005 => {
            "L005 logical-operator-preferred\n\n`and`/`or`/`not` over `&&`/`||`/`!`, as the manual's examples are written.\n--fix rewrites (double negation `!!` is left alone)."
        }
        L006 => {
            "L006 float-eq-comparison\n\nError-severity: equality comparison against a float literal or float-typed\nlocal. Floating point equality is not supported on M1 (manual: Floating Point\nComparison) — use ranged comparisons. Not auto-fixable: the right tolerance\nis a human decision. The typechecker's T002 is the project-aware sibling."
        }
        L007 => {
            "L007 operator-spacing\n\nManual layout: binary operators are surrounded by single spaces. Flags\nmissing spaces around arithmetic/bitwise/relational/assignment operators\n(continuation-line leading operators exempt). --fix inserts the spaces."
        }
        L008 => {
            "L008 nesting-too-deep\n\nFlags if/when nesting deeper than max_nesting_depth (default 4). Deep\nnesting reads badly on dash displays of diff tools alike; restructure with\nearly returns or split functions. Config: max_nesting_depth."
        }
        L009 => {
            "L009 cyclomatic-complexity\n\nMcCabe complexity per when-block/file scope (default ceiling 40 — loose, as\nL019 cognitive complexity is the primary gate). Config: max_complexity."
        }
        L010 => {
            "L010 indentation-style\n\nThe manual mandates tab indentation; spaces-style projects can configure\n[format] indent_style = \"spaces\" (shared with m1-fmt). Flags the wrong\nleading character (block-comment interiors and open-paren continuations\nexempt). Fix by running m1-fmt."
        }
        L011 => {
            "L011 comment-style\n\n`//` line comments need a space after the slashes (`// note`, not `//note`),\nmatching the manual's examples. Bare `//`, `///` doc and `////` rules are\nexempt. --fix inserts the space."
        }
        L012 => {
            "L012 unused-local\n\nA `local` whose name never appears again in the file. Either dead code or a\ntypo in the use site. Conservative: any textual reuse counts as a use."
        }
        L014 => {
            "L014 expand-undefined-variable\n\nA `$(VAR)` interpolation with no enclosing `expand (VAR = ...)` binding —\nthe expansion would fail at build time. Catches renamed-but-not-updated\ncounters."
        }
        L015 => {
            "L015 local-missing-initializer\n\nA `local` (or `static local`) declared without `= value`. M1 zero-initialises,\nbut the manual's examples always initialise explicitly — and an explicit\nvalue documents intent."
        }
        L016 => {
            "L016 local-variable-naming\n\nManual, Naming Local Variables: begin with a lowercase letter, no spaces.\nThe case split (locals lower, objects upper — L020) is what makes locals\nvisually distinct from channels."
        }
        L017 => {
            "L017 magic-number\n\nOpt-in (--select L017): unnamed numeric literals inside expressions. The\nmanual recommends named constants; real vehicle code uses many legitimate\nscale factors, so this ships off by default."
        }
        L018 => {
            "L018 semicolon-spacing\n\nManual layout: no space before `;`. --fix deletes the gap."
        }
        L019 => {
            "L019 cognitive-complexity\n\nSonar-style cognitive complexity per when-block/file (default ceiling 15):\nnesting penalised, else-if chains flat. The primary complexity gate.\nConfig: max_cognitive_complexity."
        }
        L020 => {
            "L020 object-naming\n\nManual p.64, Naming Objects: object names begin with an uppercase letter\n(spaces between constituents are fine). Flags an assignment to a\nlowercase-leading object that is not a declared local. Locals are L016's\n(lowercase) side of the same convention."
        }
        L021 => {
            "L021 one-statement-per-line\n\nManual p.65, the first layout rule: one statement (and one declaration) per\nline. Flags the second and later statements sharing a line."
        }
        L022 => {
            "L022 keyword-paren-spacing\n\nManual p.65: put a space between a keyword and a parenthesis — `if (a)`,\nnot `if(a)`; also when/is/expand. --fix inserts the space."
        }
        L023 => {
            "L023 call-paren-spacing\n\nManual p.65: don't put a space between a function and a parenthesis —\n`Func(a)`, not `Func (a)`. Only a same-line gap flags (wrapping an argument\nlist to the next line is a layout choice). --fix deletes the gap."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    #[test]
    fn json_has_expected_shape() {
        let runner = Runner::new(Registry::default());
        let result = runner.run_source("X = a == b;\n");
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

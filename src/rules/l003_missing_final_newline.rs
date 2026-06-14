//! L003 — missing-final-newline

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Range, Severity};

/// L003 — flags files that do not end with a newline.
pub struct MissingFinalNewline;

impl Rule for MissingFinalNewline {
    fn code(&self) -> LintCode {
        LintCode::L003
    }
    fn name(&self) -> &'static str {
        "missing-final-newline"
    }

    fn check_file(&self, source: &str, _lines: &[&str], diags: &mut Vec<LintDiagnostic>) {
        if source.is_empty() || source.ends_with('\n') {
            return;
        }
        let len = source.len();
        let line_count = source.lines().count() as u32;
        let last_line_len = source.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
        let pos = m1_core::Position {
            line: line_count.saturating_sub(1),
            column: last_line_len,
        };
        let range = Range {
            start: pos,
            end: pos,
        };

        // Point at a zero-width EOF span, matching the insertion point of the
        // fixer below (and L027's end-of-file span). The old `(len - 1)..len`
        // assumed a single-byte final char: when the file ends in a multi-byte
        // char with no trailing newline, `len - 1` falls inside the final
        // codepoint, producing a mid-codepoint (non-char-boundary) span.
        diags.push(LintDiagnostic::new(
            LintCode::L003,
            range,
            len..len,
            Severity::Warning,
            "file does not end with a newline".to_string(),
        ));
    }

    fn fix_file(&self, source: &str, _lines: &[&str], edits: &mut Vec<crate::fix::Edit>) {
        if !source.is_empty() && !source.ends_with('\n') {
            let len = source.len();
            edits.push(crate::fix::Edit {
                byte_range: len..len,
                replacement: "\n".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn runner() -> Runner {
        let mut r = Registry::empty();
        r.register(Box::new(MissingFinalNewline));
        Runner::new(r)
    }

    #[test]
    fn no_diagnostic_with_final_newline() {
        let result = runner().run_source("x = 1;\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn diagnostic_without_final_newline() {
        let result = runner().run_source("x = 1;");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L003);
    }

    #[test]
    fn span_is_zero_width_at_eof_with_multibyte_tail() {
        // A file ending in a multi-byte char with no trailing newline (the
        // Windows-1252-decoded MoTeC corpus shape, e.g. a unit comment ending
        // in '°' = U+00B0 = bytes 0xC2 0xB0). The diagnostic must point at a
        // zero-width EOF span on a char boundary, not at `len - 1` which falls
        // inside the final codepoint and yields a mid-codepoint offset.
        let src = "x = 1; // yaw °";
        let len = src.len();
        assert!(
            !src.is_char_boundary(len - 1),
            "test fixture has a multi-byte tail"
        );

        let result = runner().run_source(src);
        assert_eq!(result.diagnostics.len(), 1);
        let byte_range = &result.diagnostics[0].inner.byte_range;
        assert_eq!(*byte_range, len..len);
        assert!(src.is_char_boundary(byte_range.start));
        assert!(src.is_char_boundary(byte_range.end));
    }

    #[test]
    fn no_diagnostic_on_empty_file() {
        let result = runner().run_source("");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn fixes_missing_newline() {
        let mut r = Registry::empty();
        r.register(Box::new(MissingFinalNewline));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("x = 1;").unwrap();
        assert_eq!(out.as_deref(), Some("x = 1;\n"));
    }
}

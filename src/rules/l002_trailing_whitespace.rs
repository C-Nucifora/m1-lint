//! L002 — trailing-whitespace

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Range, Severity};

/// L002 — flags lines ending in space or tab characters.
pub struct TrailingWhitespace;

impl Rule for TrailingWhitespace {
    fn code(&self) -> LintCode {
        LintCode::L002
    }
    fn name(&self) -> &'static str {
        "trailing-whitespace"
    }

    fn check_file(&self, _source: &str, lines: &[&str], diags: &mut Vec<LintDiagnostic>) {
        let mut byte_offset = 0usize;

        for (line_idx, line) in lines.iter().enumerate() {
            // Lines come from `split('\n')`, so a CRLF file leaves a lone `\r` at
            // the end of every line. That `\r` is a line-ending artifact, not
            // trailing whitespace — strip one before measuring, or every CRLF
            // line is a false positive (#67). (L001 already `trim_end()`s for the
            // same reason.) Crucially the `\r` stays OUT of the violation span so
            // the fixer never deletes it and silently rewrites CRLF→LF.
            let content = line.strip_suffix('\r').unwrap_or(line);
            let trimmed_len = content.trim_end().len();
            if trimmed_len < content.len() {
                let start = m1_core::Position {
                    line: line_idx as u32,
                    column: trimmed_len as u32,
                };
                let end = m1_core::Position {
                    line: line_idx as u32,
                    column: content.len() as u32,
                };
                let range = Range { start, end };
                let byte_start = byte_offset + trimmed_len;
                let byte_end = byte_offset + content.len();

                diags.push(LintDiagnostic::new(
                    LintCode::L002,
                    range,
                    byte_start..byte_end,
                    Severity::Warning,
                    "trailing whitespace".to_string(),
                ));
            }
            // Advance by the FULL line length (including any `\r`) + the `\n`, so
            // byte offsets stay correct on CRLF files.
            byte_offset += line.len() + 1;
        }
    }

    fn fix_file(&self, _source: &str, lines: &[&str], edits: &mut Vec<crate::fix::Edit>) {
        let mut byte_offset = 0usize;
        for line in lines {
            // Strip a CRLF `\r` before measuring so the fix span excludes it —
            // otherwise `trim_end()` swallows the `\r` and `--fix` silently
            // converts CRLF→LF on every line (#67).
            let content = line.strip_suffix('\r').unwrap_or(line);
            let trimmed = content.trim_end().len();
            if trimmed < content.len() {
                edits.push(crate::fix::Edit {
                    byte_range: (byte_offset + trimmed)..(byte_offset + content.len()),
                    replacement: String::new(),
                });
            }
            byte_offset += line.len() + 1;
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
        r.register(Box::new(TrailingWhitespace));
        Runner::new(r)
    }

    #[test]
    fn no_diagnostic_clean_lines() {
        let result = runner().run_source("x = 1;\ny = 2;\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn detects_trailing_space() {
        let result = runner().run_source("x = 1;   \n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L002);
        assert_eq!(result.diagnostics[0].inner.range.start.column, 6);
    }

    #[test]
    fn detects_trailing_tab() {
        let result = runner().run_source("x = 1;\t\n");
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn fixes_trailing_whitespace() {
        let mut r = Registry::empty();
        r.register(Box::new(TrailingWhitespace));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("x = 1;  \n").unwrap();
        assert_eq!(out.as_deref(), Some("x = 1;\n"));
    }

    #[test]
    fn crlf_lines_are_not_trailing_whitespace() {
        // A lone `\r` from a CRLF line ending must NOT be flagged as trailing
        // whitespace — it is a line-ending artifact, not a violation (#67).
        let result = runner().run_source("x = 1;\r\ny = 2;\r\n");
        assert!(
            result.diagnostics.iter().all(|d| d.code != LintCode::L002),
            "CRLF carriage returns must not trigger L002, got {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn crlf_with_real_trailing_space_still_flagged_but_spares_cr() {
        // A genuine trailing space before the CRLF is still a violation, but the
        // span must stop before the `\r` so the fix preserves the line ending.
        let result = runner().run_source("x = 1; \r\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L002);
        // The flagged span is exactly the single space at column 6, not the `\r`.
        assert_eq!(result.diagnostics[0].inner.range.start.column, 6);
        assert_eq!(result.diagnostics[0].inner.range.end.column, 7);
    }

    #[test]
    fn fix_preserves_crlf_line_endings() {
        // `--fix` must not silently convert CRLF→LF: with no real trailing
        // whitespace there is nothing to fix, so the file is left byte-identical (#67).
        let mut r = Registry::empty();
        r.register(Box::new(TrailingWhitespace));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(
            fixer.fix_source("x = 1;\r\ny = 2;\r\n").unwrap(),
            None,
            "no real trailing whitespace, so nothing to fix and CRLF preserved"
        );
    }

    #[test]
    fn fix_removes_only_space_keeping_crlf() {
        // With a real trailing space the fixer removes only the space and keeps
        // the `\r\n` intact (#67).
        let mut r = Registry::empty();
        r.register(Box::new(TrailingWhitespace));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("x = 1; \r\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = 1;\r\n"));
    }
}

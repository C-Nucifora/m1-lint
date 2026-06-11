//! L027 — file-final-blank-line
//!
//! Manual p.65 ("Code Layout and Format"): *All functions and methods to end
//! with a blank line.* M1 has no in-file function syntax — each `.m1scr` file
//! *is* a method/function body (scripts map to methods via the project) — so
//! the rule reads at file scope: the script must end with a blank line, i.e.
//! the source ends in `\n\n`.
//!
//! **Opt-in** (`off_by_default`), like L017: neither real corpus follows this
//! manual bullet, and m1-fmt's trailing-newline normalisation collapses the
//! file end to exactly one `\n`, which would undo the fix on every format.
//! Teams enabling L027 should pair it with the formatter option that preserves
//! a final blank line so the two tools agree.
//!
//! Composition with L003: when the file lacks even a final newline, only L003
//! fires (and its fixer appends `\n`); L027 fires once a final newline exists
//! but the blank line does not. This keeps the two fixers from emitting
//! conflicting insertions at the same byte offset.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Range, Severity};

/// L027 — flags scripts that do not end with a blank line.
pub struct FileFinalBlankLine;

fn needs_blank_line(source: &str) -> bool {
    // Defer no-final-newline entirely to L003; skip empty/blank-only files.
    !source.trim().is_empty() && source.ends_with('\n') && !source.ends_with("\n\n")
}

impl Rule for FileFinalBlankLine {
    fn code(&self) -> LintCode {
        LintCode::L027
    }
    fn name(&self) -> &'static str {
        "file-final-blank-line"
    }

    fn check_file(&self, source: &str, _lines: &[&str], diags: &mut Vec<LintDiagnostic>) {
        if !needs_blank_line(source) {
            return;
        }
        let len = source.len();
        let pos = m1_core::byte_to_position(source, len);
        diags.push(LintDiagnostic::new(
            LintCode::L027,
            Range {
                start: pos,
                end: pos,
            },
            len..len,
            Severity::Warning,
            "function/method script must end with a blank line (manual p.65)",
        ));
    }

    fn fix_file(&self, source: &str, _lines: &[&str], edits: &mut Vec<crate::fix::Edit>) {
        if needs_blank_line(source) {
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
        r.register(Box::new(FileFinalBlankLine));
        Runner::new(r)
    }

    fn count(source: &str) -> usize {
        runner()
            .run_source(source)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L027)
            .count()
    }

    #[test]
    fn final_blank_line_is_clean() {
        assert_eq!(count("x = 1;\n\n"), 0);
    }

    #[test]
    fn missing_blank_line_flagged() {
        assert_eq!(count("x = 1;\n"), 1);
    }

    #[test]
    fn no_final_newline_is_l003s_territory() {
        // L003 owns the missing-newline case; L027 stays quiet so the two
        // fixers never insert at the same offset.
        assert_eq!(count("x = 1;"), 0);
    }

    #[test]
    fn empty_and_blank_files_are_clean() {
        assert_eq!(count(""), 0);
        assert_eq!(count("\n"), 0);
        assert_eq!(count("\n\n"), 0);
    }

    #[test]
    fn fix_appends_the_blank_line() {
        let mut r = Registry::empty();
        r.register(Box::new(FileFinalBlankLine));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("x = 1;\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = 1;\n\n"));
    }

    #[test]
    fn fix_is_idempotent() {
        let mut r = Registry::empty();
        r.register(Box::new(FileFinalBlankLine));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(fixer.fix_source("x = 1;\n\n").unwrap(), None);
    }
}

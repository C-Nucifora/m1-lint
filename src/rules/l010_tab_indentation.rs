//! L010 — indentation-style
//!
//! The M1 Build Development Manual mandates **"Indentation by tab character
//! only"**, so by default this rule flags lines *indented with spaces*. Teams
//! that prefer spaces set `indent-style = "spaces"` in `.m1lint.toml`, which
//! flips the rule to flag tab indentation instead.
//!
//! Only the indentation *character* is judged — the first character of a line's
//! leading whitespace. Tabs for indentation followed by spaces for *alignment*
//! (the common continuation-line idiom) are fine under the tab style; likewise a
//! stray tab after leading spaces is fine under the space style. Blank lines are
//! ignored.

use crate::config::IndentStyle;
use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Range, Severity};

/// L010 — flags indentation that uses the wrong character for the configured
/// [`IndentStyle`] (default: tabs required, so spaces are flagged).
pub struct Indentation {
    pub style: IndentStyle,
}

impl Rule for Indentation {
    fn code(&self) -> LintCode {
        LintCode::L010
    }
    fn name(&self) -> &'static str {
        "indentation-style"
    }

    fn check_file(&self, _source: &str, lines: &[&str], diags: &mut Vec<LintDiagnostic>) {
        let (bad, message) = match self.style {
            IndentStyle::Tab => (' ', "line is indented with spaces; use tabs"),
            IndentStyle::Spaces => ('\t', "line is indented with a tab; use spaces"),
        };
        let mut byte_offset = 0usize;
        // Track whether each line *starts* inside a `/* … */` block comment; the
        // ` * …` continuation indentation there is comment layout, not code.
        let mut in_block_comment = false;
        // Track the open `(`/`[` depth entering each line. A line that begins while
        // a bracket is still open is a *continuation*: its leading whitespace is
        // alignment under the open paren, not block indentation, so the
        // indent-character check would be a false positive there (#86).
        let mut bracket_depth: i32 = 0;
        for (line_idx, line) in lines.iter().enumerate() {
            let starts_in_comment = in_block_comment;
            let is_continuation = bracket_depth > 0;
            scan_line(line, &mut in_block_comment, &mut bracket_depth);
            // Skip blank / whitespace-only lines (L002's job), block-comment
            // interior lines, and open-paren continuation lines.
            if !starts_in_comment && !is_continuation && !line.trim().is_empty() {
                let indent_len = line.len() - line.trim_start().len();
                let indent = &line[..indent_len];
                // Judge only the indentation character (the first one): tabs +
                // trailing alignment spaces are fine under the tab style.
                if indent.starts_with(bad) {
                    let start = m1_core::Position {
                        line: line_idx as u32,
                        column: 0,
                    };
                    let end = m1_core::Position {
                        line: line_idx as u32,
                        column: indent_len as u32,
                    };
                    diags.push(LintDiagnostic::new(
                        LintCode::L010,
                        Range { start, end },
                        byte_offset..(byte_offset + indent_len),
                        Severity::Warning,
                        message.to_string(),
                    ));
                }
            }
            byte_offset += line.len() + 1;
        }
    }
}

/// Scan one line, advancing the `/* … */` block-comment state across lines and
/// the running `(`/`[` bracket `depth`. Brackets inside strings, `//` line
/// comments, and block comments are ignored, so they can't spuriously open or
/// close a continuation. Strings are treated as single-line (the M1 grammar does
/// not allow them to span lines), so the in-string state is local to the line.
fn scan_line(line: &str, in_block: &mut bool, depth: &mut i32) {
    let b = line.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    while i < b.len() {
        if *in_block {
            if i + 1 < b.len() && b[i] == b'*' && b[i + 1] == b'/' {
                *in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            match b[i] {
                b'\\' => i += 2, // skip an escaped character
                b'"' => {
                    in_string = false;
                    i += 1;
                }
                _ => i += 1,
            }
            continue;
        }
        if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'*' {
            *in_block = true;
            i += 2;
            continue;
        }
        if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'/' {
            return; // rest of the line is a line comment
        }
        match b[i] {
            b'"' => in_string = true,
            b'(' | b'[' => *depth += 1,
            b')' | b']' => *depth = (*depth - 1).max(0),
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn runner(style: IndentStyle) -> Runner {
        let mut r = Registry::empty();
        r.register(Box::new(Indentation { style }));
        Runner::new(r)
    }

    #[test]
    fn default_tab_style_flags_space_indentation() {
        let result = runner(IndentStyle::Tab).run_source("if (a)\n{\n    x = 1;\n}\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L010);
    }

    #[test]
    fn default_tab_style_accepts_tab_indentation() {
        let result = runner(IndentStyle::Tab).run_source("if (a)\n{\n\tx = 1;\n}\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn spaces_style_flags_tab_indentation() {
        let result = runner(IndentStyle::Spaces).run_source("\tx = 1;\n");
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn spaces_style_accepts_space_indentation() {
        let result = runner(IndentStyle::Spaces).run_source("    x = 1;\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn tab_indent_with_alignment_spaces_is_ok() {
        // Tabs for indentation, spaces only for alignment after them — the common
        // continuation-line idiom — is fine under the default tab style.
        let result = runner(IndentStyle::Tab).run_source("\t\tfoo and\n\t\t    bar;\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn space_aligned_continuation_under_open_paren_is_not_flagged() {
        // #86: line 2 uses spaces to align the continuation under the open `(`.
        // Those spaces are alignment, not block indentation, so under the tab
        // style they must NOT be flagged — replacing them with tabs would break
        // the alignment.
        let src =
            "if (Demo.Count > 100 and\n    Demo.Mode eq Color.Red)\n{\n\tDemo.Result = 1;\n}\n";
        let result = runner(IndentStyle::Tab).run_source(src);
        assert!(
            result.diagnostics.is_empty(),
            "alignment-space continuation wrongly flagged: {:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn space_indentation_after_paren_closes_is_still_flagged() {
        // Once the `(` is closed, a space-indented body line is real (wrong)
        // indentation and must still be flagged — the continuation exemption
        // applies only while a paren is open.
        let src = "if (a)\n{\n    x = 1;\n}\n";
        let result = runner(IndentStyle::Tab).run_source(src);
        assert_eq!(
            result.diagnostics.len(),
            1,
            "got: {:#?}",
            result.diagnostics
        );
        assert_eq!(result.diagnostics[0].inner.range.start.line, 2);
    }

    #[test]
    fn paren_in_string_or_comment_does_not_open_a_continuation() {
        // A `(` inside a string or `//` comment must not be counted as opening a
        // continuation, or the following space-indented body would be wrongly
        // exempted.
        let src = "x = \"(\";\n    y = 1;\n";
        let result = runner(IndentStyle::Tab).run_source(src);
        assert_eq!(
            result.diagnostics.len(),
            1,
            "space-indented line after a string-paren should still flag: {:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn block_comment_interior_is_ignored() {
        // The ` * ...` continuation lines of a block comment are comment layout,
        // not code indentation, and must not be flagged under the tab style.
        let src = "\tx = 1;\n\t/*\n\t * note\n\t * more\n\t */\n\ty = 2;\n";
        let result = runner(IndentStyle::Tab).run_source(src);
        assert!(
            result.diagnostics.is_empty(),
            "got: {:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn blank_lines_are_ignored() {
        // A line of only spaces is not an indentation violation here.
        let result = runner(IndentStyle::Tab).run_source("\tx = 1;\n    \n\ty = 2;\n");
        assert!(result.diagnostics.is_empty());
    }
}

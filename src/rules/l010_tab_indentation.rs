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
        for (line_idx, line) in lines.iter().enumerate() {
            let starts_in_comment = in_block_comment;
            update_block_comment_state(line, &mut in_block_comment);
            // Skip blank / whitespace-only lines (L002's job) and block-comment
            // interior lines.
            if !starts_in_comment && !line.trim().is_empty() {
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

/// Advance `in_block` across one line by scanning for `/*` and `*/` (ignoring
/// `//` line comments, which can't open a block). Good enough for the well-formed
/// comments the parser already accepts.
fn update_block_comment_state(line: &str, in_block: &mut bool) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if *in_block {
            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                *in_block = false;
                i += 2;
                continue;
            }
        } else if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            *in_block = true;
            i += 2;
            continue;
        } else if bytes[i] == b'/' && bytes[i + 1] == b'/' {
            return; // rest of line is a line comment
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

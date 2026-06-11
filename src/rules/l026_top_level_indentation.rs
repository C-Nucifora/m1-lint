//! L026 — top-level-indentation
//!
//! Manual p.65 ("Code Layout and Format"): *All code begins in the first
//! column.* Top-level statements — direct children of the source file — must
//! start at column 1; only nested code is indented.
//!
//! CST-based on purpose: a text scan would flag block-comment interiors (the
//! conventional ` * ` continuation lines are indented by one space throughout
//! the real corpora). Only the line where a top-level *statement* starts is
//! checked, so the continuation lines of a wrapped statement are exempt (the
//! manual separately mandates those be indented one tab stop). Comments are
//! not code and are never flagged.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L026 — flags top-level statements that do not begin in the first column.
pub struct TopLevelIndentation;

/// The byte range of the whitespace run preceding `node` on its own line, if
/// the node is the first thing on that line and the run is non-empty.
fn leading_ws_range(node: &Node, source: &str) -> Option<std::ops::Range<usize>> {
    let start = node.byte_range().start;
    let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let prefix = &source[line_start..start];
    if prefix.is_empty() || !prefix.chars().all(|c| c == ' ' || c == '\t') {
        // Column 1 already, or other code precedes the node on this line
        // (then the indentation isn't what is wrong — L021's territory).
        return None;
    }
    Some(line_start..start)
}

fn is_top_level_code(node: &Node) -> bool {
    node.parent().is_some_and(|p| p.kind() == Kind::SourceFile)
        && !matches!(node.kind(), Kind::LineComment | Kind::BlockComment)
}

impl Rule for TopLevelIndentation {
    fn code(&self) -> LintCode {
        LintCode::L026
    }
    fn name(&self) -> &'static str {
        "top-level-indentation"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        if !is_top_level_code(node) || node.is_error() || node.is_missing() {
            return;
        }
        if let Some(ws) = leading_ws_range(node, source) {
            let start = m1_core::byte_to_position(source, ws.start);
            let end = m1_core::byte_to_position(source, ws.end);
            diags.push(LintDiagnostic::new(
                LintCode::L026,
                m1_core::Range { start, end },
                ws,
                Severity::Warning,
                "top-level code must begin in the first column (manual p.65)",
            ));
        }
    }

    fn fix_node(&self, node: &Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if !is_top_level_code(node) || node.is_error() || node.is_missing() {
            return;
        }
        if let Some(ws) = leading_ws_range(node, source) {
            edits.push(crate::fix::Edit {
                byte_range: ws,
                replacement: String::new(),
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
        r.register(Box::new(TopLevelIndentation));
        Runner::new(r)
    }

    fn codes(source: &str) -> usize {
        runner()
            .run_source(source)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L026)
            .count()
    }

    #[test]
    fn first_column_code_is_clean() {
        assert_eq!(codes("local x = 1;\nif (a)\n{\n\tx = 2;\n}\n"), 0);
    }

    #[test]
    fn indented_top_level_statement_flagged() {
        assert_eq!(codes("\tlocal x = 1;\n"), 1);
        assert_eq!(codes("    x = 1;\n"), 1);
    }

    #[test]
    fn nested_code_is_exempt() {
        // The indented assignment is inside the if block — only depth 0 counts.
        assert_eq!(codes("if (a)\n{\n\tx = 1;\n}\n"), 0);
    }

    #[test]
    fn block_comment_interiors_are_exempt() {
        // Conventional ` * ` continuation lines must not fire (corpus-wide style).
        assert_eq!(codes("/*\n * heading\n */\nx = 1;\n"), 0);
    }

    #[test]
    fn indented_comments_are_exempt() {
        // Comments aren't code; the manual rule covers code only.
        assert_eq!(codes("\t// note\nx = 1;\n"), 0);
    }

    #[test]
    fn continuation_lines_are_exempt() {
        // Only the line where the statement starts is checked.
        assert_eq!(codes("x = a +\n\tb;\n"), 0);
    }

    #[test]
    fn fix_strips_leading_whitespace() {
        let mut r = Registry::empty();
        r.register(Box::new(TopLevelIndentation));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("\tlocal x = 1;\n    y = 2;\n").unwrap();
        assert_eq!(out.as_deref(), Some("local x = 1;\ny = 2;\n"));
    }

    #[test]
    fn fix_leaves_nested_code_alone() {
        let mut r = Registry::empty();
        r.register(Box::new(TopLevelIndentation));
        let fixer = crate::fix::Fixer::new(&r);
        let src = "if (a)\n{\n\tx = 1;\n}\n";
        let out = fixer.fix_source(src).unwrap();
        assert_eq!(out, None, "clean source must produce no edits");
    }
}

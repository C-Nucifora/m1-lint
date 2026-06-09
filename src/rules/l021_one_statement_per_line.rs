//! L021 — one-statement-per-line
//!
//! Manual p.65, the first two layout rules: "Write only one statement per
//! line" and "Write only one declaration per line". A statement that starts
//! on the same line its preceding sibling ends on (`a = 1; b = 2;`) flags —
//! once per offending statement, anchored on the second one.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

pub struct OneStatementPerLine;

fn is_statement(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::AssignmentStatement
            | Kind::ExpressionStatement
            | Kind::LocalDeclaration
            | Kind::IfStatement
            | Kind::WhenStatement
            | Kind::ExpandStatement
    )
}

impl Rule for OneStatementPerLine {
    fn code(&self) -> LintCode {
        LintCode::L021
    }
    fn name(&self) -> &'static str {
        "one-statement-per-line"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        // Visit container nodes and compare consecutive statement children, so
        // each offending pair is reported exactly once.
        if node.kind() != Kind::SourceFile && node.kind() != Kind::Block {
            return;
        }
        let mut prev_end_line: Option<u32> = None;
        for child in node.named_children() {
            if !is_statement(child.kind()) {
                continue;
            }
            let range = child.range();
            if prev_end_line == Some(range.start.line) {
                diags.push(LintDiagnostic::new(
                    LintCode::L021,
                    range,
                    child.byte_range(),
                    Severity::Warning,
                    "write only one statement per line".to_string(),
                ));
            }
            prev_end_line = Some(range.end.line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn count(src: &str) -> usize {
        let mut r = Registry::empty();
        r.register(Box::new(OneStatementPerLine));
        Runner::new(r)
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L021)
            .count()
    }

    #[test]
    fn flags_two_statements_on_one_line() {
        assert_eq!(count("a = 1; b = 2;\n"), 1);
    }

    #[test]
    fn flags_each_extra_statement() {
        assert_eq!(count("a = 1; b = 2; c = 3;\n"), 2);
    }

    #[test]
    fn one_per_line_is_fine() {
        assert_eq!(count("a = 1;\nb = 2;\n"), 0);
    }

    #[test]
    fn flags_inside_blocks() {
        assert_eq!(count("if (a)\n{\n\tx = 1; y = 2;\n}\n"), 1);
    }

    #[test]
    fn multiline_statement_then_next_line_is_fine() {
        // A statement spanning lines doesn't make the NEXT line's statement an
        // offender unless they actually share a line.
        assert_eq!(count("x = a +\n    b;\ny = 1;\n"), 0);
        assert_eq!(count("x = a +\n    b; y = 1;\n"), 1);
    }
}

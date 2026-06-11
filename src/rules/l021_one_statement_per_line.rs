//! L021 — one-statement-per-line
//!
//! Manual p.65, the first two layout rules: "Write only one statement per
//! line" and "Write only one declaration per line". A statement that starts
//! on the same line its preceding sibling ends on (`a = 1; b = 2;`) flags —
//! once per offending statement, anchored on the second one.
//!
//! `--fix` moves each offending statement onto its own line at the shared
//! line's indentation (#130). Pure whitespace between the statements is
//! replaced; a comment in the gap stays put (the newline is inserted just
//! before the statement instead).

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

    fn fix_node(&self, node: &Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if node.kind() != Kind::SourceFile && node.kind() != Kind::Block {
            return;
        }
        let mut prev: Option<Node> = None;
        for child in node.named_children() {
            if !is_statement(child.kind()) {
                continue;
            }
            if let Some(p) = &prev
                && p.range().end.line == child.range().start.line
            {
                let start = child.byte_range().start;
                // Indentation of the shared line: the statement moves to a new
                // line at the same depth.
                let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let indent: String = source[line_start..]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                // Replace only the trailing whitespace run of the gap: a pure
                // whitespace gap is replaced whole, while a comment in the gap
                // stays on the first line (which also gains no trailing
                // blanks). The token-safety check ignores comments, so the
                // rule itself must not delete one.
                let gap = p.byte_range().end..start;
                let gap_text = &source[gap.clone()];
                let keep = gap_text
                    .rfind(|c: char| c != ' ' && c != '\t')
                    .map(|i| i + gap_text[i..].chars().next().unwrap().len_utf8())
                    .unwrap_or(0);
                edits.push(crate::fix::Edit {
                    byte_range: gap.start + keep..gap.end,
                    replacement: format!("\n{indent}"),
                });
            }
            prev = Some(child);
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

    fn fix(src: &str) -> Option<String> {
        let mut r = Registry::empty();
        r.register(Box::new(OneStatementPerLine));
        Runner::new(r).fix_source_stable(src).unwrap()
    }

    #[test]
    fn fix_splits_two_statements() {
        assert_eq!(fix("a = 1; b = 2;\n").as_deref(), Some("a = 1;\nb = 2;\n"));
    }

    #[test]
    fn fix_splits_three_statements_in_one_pass_run() {
        assert_eq!(
            fix("a = 1; b = 2; c = 3;\n").as_deref(),
            Some("a = 1;\nb = 2;\nc = 3;\n")
        );
    }

    #[test]
    fn fix_preserves_block_indentation() {
        // Statements inside a tab-indented block stay at the block's depth.
        assert_eq!(
            fix("if (a)\n{\n\tx = 1; y = 2;\n}\n").as_deref(),
            Some("if (a)\n{\n\tx = 1;\n\ty = 2;\n}\n")
        );
    }

    #[test]
    fn fix_keeps_an_intervening_comment_on_the_first_line() {
        // The comment between the statements must not be deleted (the token
        // safety check ignores comments, so the rule itself must care).
        assert_eq!(
            fix("a = 1; /* note */ b = 2;\n").as_deref(),
            Some("a = 1; /* note */\nb = 2;\n")
        );
    }

    #[test]
    fn fix_result_is_clean_and_stable() {
        let fixed = fix("a = 1; b = 2;\n").unwrap();
        assert_eq!(count(&fixed), 0);
        assert_eq!(fix(&fixed), None);
    }

    #[test]
    fn clean_source_yields_no_fix() {
        assert_eq!(fix("a = 1;\nb = 2;\n"), None);
    }
}

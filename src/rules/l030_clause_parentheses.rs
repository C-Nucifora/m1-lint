//! L030 — clause-parentheses
//!
//! Manual p.65, *Code Layout and Format*: "Use parentheses to clarify clauses
//! in an expression" — its worked example parenthesizes each comparison
//! sub-clause of a compound boolean before joining them: `if ((a > b) and
//! (b < c))`. This is distinct from L024 (ternary-condition-parens, manual p.67)
//! and the L022 keyword-paren spacing rule: it concerns the *operands* of a
//! logical `and`/`or`, not the condition as a whole.
//!
//! The rule visits each `BinaryExpression` joined by `and`/`or` and flags any
//! operand that is itself a relational/equality comparison but is not already
//! wrapped in parentheses. `--fix` wraps that operand in `(` `)` — wrapping a
//! complete comparison subexpression never changes the parse.
//!
//! Opt-in (off by default): the convention is recommended, but the real corpora
//! routinely write `a > b and b < c` unparenthesized, so a default-on rule would
//! drown the output. Enable with `--select L030` (like L017/L027/L029).

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct ClauseParentheses;

/// Whether `kind` is a relational or equality comparison operator token — the
/// operators whose operands form a "clause" the manual wants parenthesized.
/// Detected by token [`Kind`] (not text) so the keyword spellings `eq`/`neq`
/// are covered alongside the symbolic forms.
fn is_comparison_op(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Lt
            | Kind::Gt
            | Kind::LtEq
            | Kind::GtEq
            | Kind::EqEq
            | Kind::BangEq
            | Kind::Eq
            | Kind::Neq
    )
}

/// Whether a `BinaryExpression`'s operator is the logical `and`/`or` that joins
/// clauses. Symbolic `&&`/`||` are L005's concern (the project prefers the
/// keyword spellings) and parenthesizing them here would entrench the symbolic
/// form, so only the keyword operators are treated as clause joiners.
fn is_clause_join(node: &Node) -> bool {
    node.kind() == Kind::BinaryExpression
        && node
            .child_by_field(Field::Operator)
            .is_some_and(|op| matches!(op.kind(), Kind::And | Kind::Or))
}

/// Whether `operand` is a bare (unparenthesized) comparison clause — a
/// `BinaryExpression` whose operator is relational/equality. Such an operand
/// reads more clearly wrapped in parentheses per the manual.
fn is_bare_comparison(operand: &Node) -> bool {
    operand.kind() == Kind::BinaryExpression
        && operand
            .child_by_field(Field::Operator)
            .is_some_and(|op| is_comparison_op(op.kind()))
}

/// The bare comparison operands (left and/or right) of a clause-joining
/// `and`/`or` expression that should be parenthesized.
fn bare_clause_operands<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    if !is_clause_join(node) {
        return Vec::new();
    }
    [Field::Left, Field::Right]
        .into_iter()
        .filter_map(|f| node.child_by_field(f))
        .filter(is_bare_comparison)
        .collect()
}

impl Rule for ClauseParentheses {
    fn code(&self) -> LintCode {
        LintCode::L030
    }
    fn name(&self) -> &'static str {
        "clause-parentheses"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        for operand in bare_clause_operands(node) {
            diags.push(LintDiagnostic::new(
                LintCode::L030,
                operand.range(),
                operand.byte_range(),
                Severity::Warning,
                "wrap this comparison clause in parentheses".to_string(),
            ));
        }
    }

    fn fix_node(&self, node: &m1_core::Node, _source: &str, edits: &mut Vec<crate::fix::Edit>) {
        for operand in bare_clause_operands(node) {
            let r = operand.byte_range();
            edits.push(crate::fix::Edit {
                byte_range: r.start..r.start,
                replacement: "(".to_string(),
            });
            edits.push(crate::fix::Edit {
                byte_range: r.end..r.end,
                replacement: ")".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn registry() -> Registry {
        let mut r = Registry::empty();
        r.register(Box::new(ClauseParentheses));
        r
    }

    fn count(src: &str) -> usize {
        Runner::new(registry())
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L030)
            .count()
    }

    fn fixed(src: &str) -> Option<String> {
        let r = registry();
        crate::fix::Fixer::new(&r).fix_source(src).unwrap()
    }

    #[test]
    fn flags_both_unparenthesized_comparison_clauses() {
        // The manual's own example wraps each comparison: `((a > b) and (b < c))`.
        assert_eq!(count("x = a > b and b < c;\n"), 2);
    }

    #[test]
    fn flags_only_the_bare_clause() {
        // Left is already wrapped; only the right comparison is bare.
        assert_eq!(count("x = (a > b) and b < c;\n"), 1);
    }

    #[test]
    fn fully_parenthesized_clauses_are_clean() {
        assert_eq!(count("x = (a > b) and (b < c);\n"), 0);
    }

    #[test]
    fn equality_keyword_operands_count_as_clauses() {
        assert_eq!(count("x = a eq 1 or b neq 2;\n"), 2);
    }

    #[test]
    fn non_comparison_operands_are_not_clauses() {
        // A bare identifier / boolean operand is not a comparison clause, so the
        // rule leaves it alone — it only targets relational/equality sub-clauses.
        assert_eq!(count("x = a and b;\n"), 0);
        assert_eq!(count("x = flag or other;\n"), 0);
    }

    #[test]
    fn symbolic_logical_join_is_left_to_l005() {
        // `&&`/`||` are L005's concern; wrapping them here would entrench the
        // symbolic form the project discourages.
        assert_eq!(count("x = a > b && b < c;\n"), 0);
    }

    #[test]
    fn arithmetic_operands_are_not_flagged() {
        // The join operator must be logical and/or, not arithmetic.
        assert_eq!(count("x = a + b;\n"), 0);
    }

    #[test]
    fn fix_wraps_each_bare_clause() {
        assert_eq!(
            fixed("x = a > b and b < c;\n").as_deref(),
            Some("x = (a > b) and (b < c);\n")
        );
    }

    #[test]
    fn fix_leaves_already_parenthesized_clause_alone() {
        assert_eq!(
            fixed("x = (a > b) and b < c;\n").as_deref(),
            Some("x = (a > b) and (b < c);\n")
        );
    }

    #[test]
    fn already_clean_source_needs_no_fix() {
        assert_eq!(fixed("x = (a > b) and (b < c);\n"), None);
    }
}

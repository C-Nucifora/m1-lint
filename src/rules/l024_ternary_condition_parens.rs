//! L024 — ternary-condition-parens
//!
//! Manual p.67, *Ternary conditional ? :*: "Put the condition in parentheses"
//! — `(condition) ? a : b`. The manual's bullet is unconditional (its own
//! example parenthesizes a bare identifier), so every unparenthesized
//! condition flags; teams that prefer the bare style disable the rule in
//! config. `--fix` wraps the condition in `(` `)` — wrapping the complete
//! condition subexpression never changes the parse.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct TernaryConditionParens;

/// The unparenthesized `condition` of a ternary, if this node is one.
fn bare_condition<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    if node.kind() != Kind::TernaryExpression {
        return None;
    }
    let cond = node.child_by_field(Field::Condition)?;
    (cond.kind() != Kind::ParenthesizedExpression).then_some(cond)
}

impl Rule for TernaryConditionParens {
    fn code(&self) -> LintCode {
        LintCode::L024
    }
    fn name(&self) -> &'static str {
        "ternary-condition-parens"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if let Some(cond) = bare_condition(node) {
            diags.push(LintDiagnostic::new(
                LintCode::L024,
                cond.range(),
                cond.byte_range(),
                Severity::Warning,
                "put the ternary condition in parentheses".to_string(),
            ));
        }
    }

    fn fix_node(&self, node: &m1_core::Node, _source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if let Some(cond) = bare_condition(node) {
            let r = cond.byte_range();
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
        r.register(Box::new(TernaryConditionParens));
        r
    }

    fn count(src: &str) -> usize {
        Runner::new(registry())
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L024)
            .count()
    }

    fn fixed(src: &str) -> Option<String> {
        let r = registry();
        crate::fix::Fixer::new(&r).fix_source(src).unwrap()
    }

    #[test]
    fn flags_unparenthesized_condition() {
        assert_eq!(count("i = i eq 200 ? 0 : i + 1;\n"), 1);
    }

    #[test]
    fn flags_bare_identifier_condition_too() {
        // The manual's bullet is unconditional — its own example wraps a bare
        // `condition` identifier.
        assert_eq!(count("x = c ? 1 : 2;\n"), 1);
    }

    #[test]
    fn parenthesized_condition_is_fine() {
        assert_eq!(count("i = (i eq 200) ? 0 : i + 1;\n"), 0);
        assert_eq!(count("x = (c) ? 1 : 2;\n"), 0);
    }

    #[test]
    fn partially_parenthesized_comparison_still_flags() {
        // `(word & 0x01) neq 0` — the *comparison* is the condition and it is
        // not parenthesized, even though its left operand is.
        assert_eq!(count("x = (w & 0x01) neq 0 ? 1 : 2;\n"), 1);
    }

    #[test]
    fn fix_wraps_the_condition() {
        assert_eq!(
            fixed("i = i eq 200 ? 0 : i + 1;\n").as_deref(),
            Some("i = (i eq 200) ? 0 : i + 1;\n")
        );
        assert_eq!(
            fixed("x = c ? 1 : 2;\n").as_deref(),
            Some("x = (c) ? 1 : 2;\n")
        );
    }

    #[test]
    fn fix_handles_nested_ternaries() {
        assert_eq!(
            fixed("x = a ? 1 : b ? 2 : 3;\n").as_deref(),
            Some("x = (a) ? 1 : (b) ? 2 : 3;\n")
        );
    }

    #[test]
    fn already_clean_source_needs_no_fix() {
        assert_eq!(fixed("x = (c) ? 1 : 2;\n"), None);
    }
}

//! L004 — eq-operator-preferred
//!
//! Flags use of `==` and `!=`; the project prefers `eq` and `neq`.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L004 — flags `==` / `!=` in binary expressions.
pub struct EqOperatorPreferred;

impl Rule for EqOperatorPreferred {
    fn code(&self) -> LintCode {
        LintCode::L004
    }
    fn name(&self) -> &'static str {
        "eq-operator-preferred"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::BinaryExpression {
            return;
        }

        for child in node.children() {
            let msg = match child.kind() {
                Kind::EqEq => "use `eq` instead of `==`",
                Kind::BangEq => "use `neq` instead of `!=`",
                _ => continue,
            };
            diags.push(LintDiagnostic::new(
                LintCode::L004,
                child.range(),
                child.byte_range(),
                Severity::Warning,
                msg,
            ));
        }
    }

    fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if node.kind() != Kind::BinaryExpression {
            return;
        }
        for child in node.children() {
            let repl = match child.kind() {
                Kind::EqEq => "eq",
                Kind::BangEq => "neq",
                _ => continue,
            };
            // A keyword (`eq`/`neq`) glued to an operand would merge into one
            // identifier (`a==b` -> `aeqb`), a semantics change the fixer
            // rejects, leaving the glued form permanently un-fixable. Pad the
            // replacement so the operand byte on a glued side is split off (#76).
            edits.push(crate::rules::keyword_operator_edit(
                source,
                child.byte_range(),
                repl,
            ));
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
        r.register(Box::new(EqOperatorPreferred));
        Runner::new(r)
    }

    #[test]
    fn flags_double_eq() {
        let source = "x = a == b;\n";
        let result = runner().run_source(source);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L004);
        assert!(result.diagnostics[0].inner.message.contains("eq"));
    }

    #[test]
    fn flags_bang_eq() {
        let source = "x = a != b;\n";
        let result = runner().run_source(source);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L004);
        assert!(result.diagnostics[0].inner.message.contains("neq"));
    }

    #[test]
    fn no_false_positive_eq_keyword() {
        // The correct form — should not be flagged.
        let source = "x = a eq b;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L004));
    }

    #[test]
    fn no_false_positive_clean_code() {
        let source = "x = a + b;\ny = c eq d;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L004));
    }

    #[test]
    fn fixes_eq_eq() {
        let mut r = Registry::empty();
        r.register(Box::new(EqOperatorPreferred));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("x = a == b;\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = a eq b;\n"));
    }

    #[test]
    fn fixes_glued_eq_eq() {
        // A glued `a==b` must become `a eq b` (not `aeqb`, which would be a
        // single identifier and a semantics change): the keyword replacement
        // inserts the separating spaces the symbolic operator no longer needs.
        let mut r = Registry::empty();
        r.register(Box::new(EqOperatorPreferred));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(
            fixer.fix_source("x = a==b;\n").unwrap().as_deref(),
            Some("x = a eq b;\n")
        );
        assert_eq!(
            fixer.fix_source("x = a!=b;\n").unwrap().as_deref(),
            Some("x = a neq b;\n")
        );
        // A half-glued form (space on only one side) gets just the missing space.
        assert_eq!(
            fixer.fix_source("x = a ==b;\n").unwrap().as_deref(),
            Some("x = a eq b;\n")
        );
        assert_eq!(
            fixer.fix_source("x = a== b;\n").unwrap().as_deref(),
            Some("x = a eq b;\n")
        );
    }
}

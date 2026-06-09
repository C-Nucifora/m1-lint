//! L023 — call-paren-spacing
//!
//! Manual p.65: "Don't put a space between a function and a parenthesis" —
//! `Func (a)` should be `Func(a)`. Converse of L022. `--fix` deletes the gap.
//!
//! Only a same-line gap flags: a call whose argument list opens on the next
//! line is a wrapping choice, not a keyword-spacing slip.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

pub struct CallParenSpacing;

/// The byte range of the same-line whitespace between a call's callee and its
/// argument list's `(`, when non-empty.
fn gap(node: &Node, source: &str) -> Option<std::ops::Range<usize>> {
    if node.kind() != Kind::CallExpression {
        return None;
    }
    let callee = node
        .named_children()
        .into_iter()
        .find(|c| matches!(c.kind(), Kind::Identifier | Kind::MemberExpression))?;
    let args = node
        .child_nodes()
        .find(|c| c.kind() == Kind::ArgumentList)?;
    let (start, end) = (callee.byte_range().end, args.byte_range().start);
    if start >= end {
        return None;
    }
    let between = &source[start..end];
    if !between.is_empty() && between.chars().all(|c| c == ' ' || c == '\t') {
        Some(start..end)
    } else {
        None
    }
}

impl Rule for CallParenSpacing {
    fn code(&self) -> LintCode {
        LintCode::L023
    }
    fn name(&self) -> &'static str {
        "call-paren-spacing"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        if let Some(range) = gap(node, source) {
            let start = m1_core::byte_to_position(source, range.start);
            let end = m1_core::byte_to_position(source, range.end);
            diags.push(LintDiagnostic::new(
                LintCode::L023,
                m1_core::Range { start, end },
                range,
                Severity::Warning,
                "remove the space between the function name and `(`".to_string(),
            ));
        }
    }

    fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if let Some(range) = gap(node, source) {
            edits.push(crate::fix::Edit {
                byte_range: range,
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

    fn count(src: &str) -> usize {
        let mut r = Registry::empty();
        r.register(Box::new(CallParenSpacing));
        Runner::new(r)
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L023)
            .count()
    }

    #[test]
    fn flags_space_before_call_paren() {
        assert_eq!(count("x = Calculate.Max (1, 2);\n"), 1);
    }

    #[test]
    fn tight_call_is_fine() {
        assert_eq!(count("x = Calculate.Max(1, 2);\n"), 0);
    }

    #[test]
    fn newline_before_arglist_is_not_flagged() {
        assert_eq!(count("x = Calculate.Max\n(1, 2);\n"), 0);
    }

    #[test]
    fn fix_removes_the_gap() {
        let mut r = Registry::empty();
        r.register(Box::new(CallParenSpacing));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(
            fixer
                .fix_source("x = Calculate.Max (1, 2);\n")
                .unwrap()
                .as_deref(),
            Some("x = Calculate.Max(1, 2);\n")
        );
    }
}

//! L009 — cyclomatic-complexity
//!
//! The cyclomatic complexity of a `when` block (or the top-level source file)
//! must not exceed the configured ceiling (`max_complexity`, default 40 —
//! loose, as L019 cognitive complexity is the primary gate).
//!
//! Complexity = 1 + count of decision points within the scope:
//! - each `if` / `else if` (`else if` is itself an `if_statement` node)
//! - each `is` clause and `expand` statement
//! - each `and` / `&&` / `or` / `||` operator
//!
//! Nested `when` blocks are scopes in their own right and are not counted
//! toward their enclosing scope's complexity.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::{Rule, is_complexity_scope};
use m1_core::{Kind, Node, Severity};

fn is_decision_point(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::IfStatement
            | Kind::IsClause
            | Kind::ExpandStatement
            | Kind::And
            | Kind::Or
            | Kind::AmpAmp
            | Kind::PipePipe
    )
}

fn count_complexity(scope: &Node) -> u32 {
    let mut count = 1u32; // base complexity
    count_children(scope, &mut count);
    count
}

/// Count decision points among `node`'s descendants, without descending into
/// nested scopes (which get their own complexity check).
///
/// Iterative (explicit stack) rather than recursive: a deeply nested expression
/// such as `1+1+…+1` parses to a long BinaryExpression chain, and a recursive
/// descent from the `SourceFile` scope would overflow the native stack and abort
/// the process. An explicit stack counts the same decision points at any depth.
fn count_children(node: &Node, count: &mut u32) {
    let mut stack: Vec<Node> = node.children();
    while let Some(child) = stack.pop() {
        if is_decision_point(child.kind()) {
            *count += 1;
        }
        if is_complexity_scope(child.kind()) {
            // A nested scope is checked independently; do not descend.
            continue;
        }
        stack.extend(child.children());
    }
}

/// L009 — flags scopes whose cyclomatic complexity exceeds `max_complexity`.
pub struct CyclomaticComplexity {
    pub max_complexity: u32,
}

impl Rule for CyclomaticComplexity {
    fn code(&self) -> LintCode {
        LintCode::L009
    }
    fn name(&self) -> &'static str {
        "cyclomatic-complexity"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if !is_complexity_scope(node.kind()) {
            return;
        }
        let complexity = count_complexity(node);
        if complexity > self.max_complexity {
            diags.push(LintDiagnostic::new(
                LintCode::L009,
                node.range(),
                node.byte_range(),
                Severity::Warning,
                format!(
                    "cyclomatic complexity {} exceeds maximum of {}",
                    complexity, self.max_complexity
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::registry::Registry;
    use crate::runner::Runner;

    /// The production ceiling (`Config::default().max_complexity`, currently 40),
    /// so these tests exercise the same threshold `build_rule` wires in — not a
    /// hand-picked test-only default.
    const DEFAULT_MAX: u32 = 40;

    fn runner() -> Runner {
        assert_eq!(
            Config::default().max_complexity,
            DEFAULT_MAX,
            "production default changed; update L009 tests to match"
        );
        let mut r = Registry::empty();
        r.register(Box::new(CyclomaticComplexity {
            max_complexity: DEFAULT_MAX,
        }));
        Runner::new(r)
    }

    #[test]
    fn simple_script_low_complexity() {
        let source = "x = 1;\ny = 2;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L009));
    }

    #[test]
    fn flags_high_complexity_top_level() {
        // 41 `if` statements -> complexity 42 > 40 (the default ceiling).
        let mut source = String::new();
        for _ in 0..(DEFAULT_MAX + 1) {
            source.push_str("if (a) { x = 1; }\n");
        }
        let result = runner().run_source(&source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L009));
    }

    #[test]
    fn just_under_ceiling_not_flagged() {
        // 39 `if` statements -> complexity 40 == 40, not > 40, so no L009.
        let mut source = String::new();
        for _ in 0..(DEFAULT_MAX - 1) {
            source.push_str("if (a) { x = 1; }\n");
        }
        let result = runner().run_source(&source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L009));
    }

    #[test]
    fn logical_operators_count() {
        // 40 `and` operators -> complexity 41 > 40 (the default ceiling).
        let mut cond = String::from("a");
        for _ in 0..DEFAULT_MAX {
            cond.push_str(" and a");
        }
        let source = format!("if ({}) {{ x = 1; }}\n", cond);
        let result = runner().run_source(&source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L009));
    }
}

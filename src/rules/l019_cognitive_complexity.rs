//! L019 — cognitive-complexity
//!
//! Sonar-style *cognitive* complexity of a `when` block (or the top-level
//! source file). Unlike cyclomatic complexity (L009), which counts every
//! decision point equally, cognitive complexity models how hard code is to
//! *read*: nesting is penalised, but flat sequences and `else if` chains stay
//! cheap.
//!
//! Scoring within a scope (mirroring L009's scope model: `SourceFile` and each
//! `WhenStatement` are scopes, checked independently — a nested `when` is not
//! counted toward its parent):
//!
//! - A **nesting** construct (`if`, `when`) adds `1 + current_nesting_level`,
//!   and its body is scored one level deeper.
//! - A **continuation** (`else`, `else if`) adds a flat `1` with no nesting
//!   penalty; its body is still scored one level deeper. (`else if` is an
//!   `if_statement` nested inside an `else_clause` in the grammar, so it must be
//!   special-cased or a long chain would be mistaken for deep nesting.)
//! - Each boolean operator (`and`/`or`/`&&`/`||`) in a condition adds a flat `1`.
//! - Each `is` clause of a `when` adds a flat `1`.
//!
//! So three flat `if`s score 3; three *nested* `if`s score 1+2+3 = 6; a four-arm
//! `if`/`else if`/`else if`/`else if` chain scores 4, not 1+2+3+4.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::{Rule, is_complexity_scope};
use m1_core::{Kind, Node, Severity};

fn is_boolean_operator(kind: Kind) -> bool {
    matches!(kind, Kind::And | Kind::Or | Kind::AmpAmp | Kind::PipePipe)
}

/// Cognitive complexity of `scope`, not descending into nested scopes.
fn cognitive_complexity(scope: &Node) -> u32 {
    let mut total = 0u32;
    for child in scope.children() {
        visit(&child, 0, &mut total);
    }
    total
}

/// Visit `node` at the given `nesting` level, accumulating cognitive complexity.
///
/// Iterative (an explicit work-stack of `(node, nesting)` items) rather than
/// recursive: a deeply nested expression such as `1+1+…+1` parses to a long
/// BinaryExpression chain (it lands in the catch-all arm), and a recursive
/// descent would overflow the native stack and abort the process. The explicit
/// stack reproduces the recursive scoring exactly — each child is pushed with
/// the nesting level the recursive version would have passed it. Visit order
/// does not affect the accumulated total.
fn visit(node: &Node, nesting: u32, total: &mut u32) {
    let mut stack: Vec<(Node, u32)> = vec![(*node, nesting)];
    while let Some((node, nesting)) = stack.pop() {
        let kind = node.kind();

        if is_boolean_operator(kind) {
            *total += 1;
            for child in node.children() {
                stack.push((child, nesting));
            }
            continue;
        }

        match kind {
            // A nested scope (a `when` inside this one) is checked on its own.
            Kind::WhenStatement => {}

            Kind::IfStatement => {
                // `else if` is an if_statement whose parent is an else_clause: it
                // is a continuation (flat +1), not a deeper nesting level.
                let is_else_if = node.parent().is_some_and(|p| p.kind() == Kind::ElseClause);
                *total += if is_else_if { 1 } else { 1 + nesting };

                for child in node.children() {
                    match child.kind() {
                        // The body is one level deeper.
                        Kind::Block => push_children(&child, nesting + 1, &mut stack),
                        // The else/else-if branch stays at this level.
                        Kind::ElseClause => stack.push((child, nesting)),
                        // The condition (and its boolean operators) is flat.
                        _ => stack.push((child, nesting)),
                    }
                }
            }

            Kind::ElseClause => {
                for child in node.children() {
                    match child.kind() {
                        // `else { ... }` — a flat +1, body one level deeper.
                        Kind::Block => {
                            *total += 1;
                            push_children(&child, nesting + 1, &mut stack);
                        }
                        // `else if (...)` — handled by the IfStatement arm.
                        Kind::IfStatement => stack.push((child, nesting)),
                        _ => stack.push((child, nesting)),
                    }
                }
            }

            Kind::ExpandStatement => {
                // A compile-time loop is a nesting construct like `if`: it adds
                // `1 + nesting` and its body is scored one level deeper. L009
                // already counts `expand` as a decision point — the two rules
                // must agree on what branches (#134).
                *total += 1 + nesting;
                for child in node.children() {
                    if child.kind() == Kind::Block {
                        push_children(&child, nesting + 1, &mut stack);
                    } else {
                        stack.push((child, nesting));
                    }
                }
            }

            Kind::IsClause => {
                // A `when ... is` arm: a flat +1, body one level deeper.
                *total += 1;
                for child in node.children() {
                    if child.kind() == Kind::Block {
                        push_children(&child, nesting + 1, &mut stack);
                    } else {
                        stack.push((child, nesting));
                    }
                }
            }

            _ => {
                for child in node.children() {
                    stack.push((child, nesting));
                }
            }
        }
    }
}

fn push_children<'a>(node: &Node<'a>, nesting: u32, stack: &mut Vec<(Node<'a>, u32)>) {
    for child in node.children() {
        stack.push((child, nesting));
    }
}

/// L019 — flags scopes whose cognitive complexity exceeds `max_complexity`.
pub struct CognitiveComplexity {
    pub max_complexity: u32,
}

impl Default for CognitiveComplexity {
    fn default() -> Self {
        Self { max_complexity: 15 }
    }
}

impl Rule for CognitiveComplexity {
    fn code(&self) -> LintCode {
        LintCode::L019
    }
    fn name(&self) -> &'static str {
        "cognitive-complexity"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if !is_complexity_scope(node.kind()) {
            return;
        }
        let complexity = cognitive_complexity(node);
        if complexity > self.max_complexity {
            diags.push(LintDiagnostic::new(
                LintCode::L019,
                node.range(),
                node.byte_range(),
                Severity::Warning,
                format!(
                    "cognitive complexity {} exceeds maximum of {}",
                    complexity, self.max_complexity
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn diagnostics(source: &str, max: u32) -> Vec<LintDiagnostic> {
        let mut r = Registry::empty();
        r.register(Box::new(CognitiveComplexity {
            max_complexity: max,
        }));
        Runner::new(r).run_source(source).diagnostics
    }

    fn l019_messages(source: &str, max: u32) -> Vec<String> {
        diagnostics(source, max)
            .into_iter()
            .filter(|d| d.code == LintCode::L019)
            .map(|d| d.inner.message)
            .collect()
    }

    #[test]
    fn simple_script_has_no_complexity() {
        let msgs = l019_messages("x = 1;\ny = 2;\n", 0);
        assert!(
            msgs.is_empty(),
            "no control flow => no diagnostic: {msgs:?}"
        );
    }

    #[test]
    fn flat_if_sequence_is_linear() {
        // 6 flat ifs => 6 (1 each), NOT quadratic.
        let source = "if (a) { x = 1; }\n".repeat(6);
        let msgs = l019_messages(&source, 5);
        assert!(
            msgs.iter().any(|m| m.contains("cognitive complexity 6")),
            "expected complexity 6, got {msgs:?}"
        );
    }

    #[test]
    fn flat_sequence_under_threshold_not_flagged() {
        // 3 flat ifs => 3, under a threshold of 5.
        let source = "if (a) { x = 1; }\n".repeat(3);
        assert!(l019_messages(&source, 5).is_empty());
    }

    #[test]
    fn nested_ifs_are_superlinear() {
        // 3 nested ifs => 1 + 2 + 3 = 6.
        let source = "if (a) {\nif (a) {\nif (a) {\nx = 1;\n}\n}\n}\n";
        let msgs = l019_messages(source, 5);
        assert!(
            msgs.iter().any(|m| m.contains("cognitive complexity 6")),
            "expected complexity 6 for 3 nested ifs, got {msgs:?}"
        );
    }

    #[test]
    fn else_if_chain_stays_flat() {
        // if / else if / else if / else if => 4 (flat), NOT 1+2+3+4 = 10.
        let source = "if (a) { x = 1; }\n\
                      else if (a) { x = 2; }\n\
                      else if (a) { x = 3; }\n\
                      else if (a) { x = 4; }\n";
        let msgs = l019_messages(source, 3);
        assert!(
            msgs.iter().any(|m| m.contains("cognitive complexity 4")),
            "expected complexity 4 for a 4-arm chain, got {msgs:?}"
        );
    }

    #[test]
    fn expand_counts_like_a_nesting_construct() {
        // expand(1) + nested if(1+1) = 3 — the expand body is one level deeper.
        let source = "expand (I = 1 to 4)\n{\nif (a) { x = $(I); }\n}\n";
        let msgs = l019_messages(source, 2);
        assert!(
            msgs.iter().any(|m| m.contains("cognitive complexity 3")),
            "expected complexity 3 (expand 1, nested if 2), got {msgs:?}"
        );
    }

    #[test]
    fn boolean_operators_add_complexity() {
        // if (a and b or c) => if(1) + and(1) + or(1) = 3.
        let msgs = l019_messages("if (a and b or c) { x = 1; }\n", 2);
        assert!(
            msgs.iter().any(|m| m.contains("cognitive complexity 3")),
            "expected complexity 3, got {msgs:?}"
        );
    }
}

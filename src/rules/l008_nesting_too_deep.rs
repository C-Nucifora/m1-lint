//! L008 — nesting-too-deep
//!
//! Control-flow constructs nested more than 4 levels deep are flagged.
//!
//! The M1 grammar's control constructs are `if` and `when`; there is no
//! `for`/`while`. Depth is tracked during one iterative tree walk.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Cst, Kind, Severity};

fn is_control_node(kind: Kind) -> bool {
    matches!(kind, Kind::IfStatement | Kind::WhenStatement)
}

/// L008 — flags control structures nested deeper than `max_depth` levels.
pub struct NestingTooDeep {
    pub max_depth: usize,
}

impl Default for NestingTooDeep {
    fn default() -> Self {
        Self { max_depth: 4 }
    }
}

impl Rule for NestingTooDeep {
    fn code(&self) -> LintCode {
        LintCode::L008
    }
    fn name(&self) -> &'static str {
        "nesting-too-deep"
    }

    fn check_file_cst(
        &self,
        cst: &Cst,
        _source: &str,
        _lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {
        // tree-sitter parent() searches from the root; carry depth instead.
        let cap = self.max_depth.saturating_add(2);
        let mut stack = vec![(cst.root(), 0usize)];
        while let Some((node, ancestors)) = stack.pop() {
            let mut depth = ancestors;
            if is_control_node(node.kind()) {
                depth = ancestors.saturating_add(1).min(cap);
                if depth > self.max_depth {
                    let shown = if depth >= cap {
                        format!("{depth}+")
                    } else {
                        depth.to_string()
                    };
                    diags.push(LintDiagnostic::new(
                        LintCode::L008,
                        node.range(),
                        node.byte_range(),
                        Severity::Warning,
                        format!(
                            "nesting depth {shown} exceeds maximum of {}",
                            self.max_depth
                        ),
                    ));
                }
            }
            stack.extend(
                node.children()
                    .into_iter()
                    .rev()
                    .map(|child| (child, depth)),
            );
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
        r.register(Box::new(NestingTooDeep::default()));
        Runner::new(r)
    }

    /// Build `depth` nested `if (a) { ... }` blocks.
    fn nested_ifs(depth: usize) -> String {
        let mut s = String::new();
        for _ in 0..depth {
            s.push_str("if (a) {\n");
        }
        s.push_str("x = 1;\n");
        for _ in 0..depth {
            s.push_str("}\n");
        }
        s
    }

    #[test]
    fn no_diagnostic_at_depth_4() {
        let source = nested_ifs(4);
        let result = runner().run_source(&source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L008));
    }

    #[test]
    fn diagnostic_at_depth_5() {
        let source = nested_ifs(5);
        let result = runner().run_source(&source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L008));
    }

    #[test]
    fn deep_nesting_lints_quickly_and_still_flags() {
        // Parent lookup from every nested node repeatedly searches from the
        // tree root. The file pass must carry depth without those searches.
        let source = nested_ifs(1500);
        let start = std::time::Instant::now();
        let result = runner().run_source(&source);
        let elapsed = start.elapsed();
        assert!(
            result.diagnostics.iter().any(|d| d.code == LintCode::L008),
            "deep nesting must still flag L008"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "deep-nesting lint should be near-instant, took {elapsed:?}"
        );
    }
    #[test]
    fn counts_control_depth_without_leaking_across_siblings() {
        let source = "if (a) { when (b) { is (1) { if (c) { Out = 1; } } } }\nif (d) { Out = 2; }";
        for (max_depth, expected) in [
            (0, vec!["1", "2+", "2+", "1"]),
            (1, vec!["2", "3+"]),
            (2, vec!["3"]),
            (3, vec![]),
            (usize::MAX, vec![]),
        ] {
            let mut registry = Registry::empty();
            registry.register(Box::new(NestingTooDeep { max_depth }));
            let result = Runner::new(registry).run_source(source);
            assert!(result.syntax_errors.is_empty(), "{source}");
            let messages: Vec<_> = result
                .diagnostics
                .iter()
                .map(|d| d.inner.message.as_str())
                .collect();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|depth| format!("nesting depth {depth} exceeds maximum of {max_depth}"))
                .collect();
            assert_eq!(messages, expected, "limit {max_depth}");
        }
    }
}

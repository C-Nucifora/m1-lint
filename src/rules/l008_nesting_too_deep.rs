//! L008 — nesting-too-deep
//!
//! Control-flow constructs nested more than 4 levels deep are flagged.
//!
//! The M1 grammar's control constructs are `if` and `when`; there is no
//! `for`/`while`. Depth is computed by counting control-node ancestors.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

fn is_control_node(kind: Kind) -> bool {
    matches!(kind, Kind::IfStatement | Kind::WhenStatement)
}

/// Count control-flow ancestors of `node`, but **stop early** once the running
/// count reaches `limit`. L008 only needs to know whether the depth *exceeds*
/// `max_depth`, so walking the entire ancestor chain for every node — which made
/// a file of N nested `if`s O(N·depth) and hang for tens of seconds on deep
/// nesting — is wasted work. Capping the walk at `limit` makes it linear overall
/// while keeping the flag decision and the reported number correct up to the
/// threshold (anything at or beyond `limit` is reported as `limit`, which still
/// reads "exceeds maximum of N").
fn nesting_depth_capped(node: &Node, limit: usize) -> usize {
    let mut depth = 0usize;
    let mut current = node.parent();
    while let Some(parent) = current {
        if is_control_node(parent.kind()) {
            depth += 1;
            if depth >= limit {
                return depth;
            }
        }
        current = parent.parent();
    }
    depth
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

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if !is_control_node(node.kind()) {
            return;
        }
        // We flag when (ancestor control nodes) + 1 > max_depth, i.e. when the
        // ancestor count reaches max_depth. `nesting_depth_capped` early-exits
        // once it has counted `max_depth + 1` ancestors, which is enough to
        // report the exact depth up to `max_depth + 2` while keeping the
        // whole-file walk linear instead of O(N·depth). At or beyond the cap the
        // exact value is irrelevant (it still "exceeds maximum of N") and the
        // message marks it with a `+`.
        let cap = self.max_depth + 1;
        let ancestors = nesting_depth_capped(node, cap);
        let depth = ancestors + 1; // +1 for this node itself
        if depth > self.max_depth {
            let shown = if ancestors >= cap {
                format!("{}+", depth)
            } else {
                depth.to_string()
            };
            diags.push(LintDiagnostic::new(
                LintCode::L008,
                node.range(),
                node.byte_range(),
                Severity::Warning,
                format!(
                    "nesting depth {} exceeds maximum of {}",
                    shown, self.max_depth
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
        // ~1500 nested `if`s used to make `nesting_depth` O(N·depth) and hang
        // for >90s. With the capped ancestor walk the whole-file pass is linear
        // and finishes near-instantly while still flagging L008.
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
}

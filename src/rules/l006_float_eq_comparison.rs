//! L006 — float-eq-comparison
//!
//! Flags equality comparisons (`==`, `!=`, `eq`, `neq`) where at least one
//! immediate operand is a float literal. This is a CST-only heuristic; it does
//! not flag float *variables* (those require type information).

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L006 — flags equality comparisons against float literals.
pub struct FloatEqComparison;

/// Returns true if the node is a float literal.
///
/// Float notation is detected by the presence of a `.` or an exponent marker
/// (`e`/`E`). Hexadecimal literals (`0x..`) are always integers and must be
/// excluded first: their digits include `e`/`E`/`f` (e.g. `0xE5`, `0xBEEF`,
/// `0xFACE`), which would otherwise be misread as an exponent marker and flag a
/// plain integer comparison as a float one.
fn is_float_literal(node: &Node) -> bool {
    if node.kind() != Kind::Number {
        return false;
    }
    let text = node.text();
    // A hex literal is an integer regardless of which hex digits it contains.
    if text.starts_with("0x") || text.starts_with("0X") {
        return false;
    }
    text.contains('.') || text.to_ascii_lowercase().contains('e')
}

/// Returns true if the node is an equality operator token.
///
/// Both the symbolic (`==`, `!=`) and the keyword (`eq`, `neq`) forms are
/// equality comparisons.
fn is_eq_op(kind: Kind) -> bool {
    matches!(kind, Kind::EqEq | Kind::BangEq | Kind::Eq | Kind::Neq)
}

/// The name of a `local` declared with a float type — either an explicit float
/// type annotation (`local <Float> x` / `<Floating Point>`) or a float-literal
/// initializer (`local x = 1.5`). `None` for any other local. This is a
/// syntactic heuristic: it cannot see the type of a local copied from a channel
/// (`local x = Group.Channel;`), which would need the project type model.
fn float_local_name(decl: &Node) -> Option<String> {
    let kids = decl.named_children();
    let name = kids
        .iter()
        .find(|c| c.kind() == Kind::Identifier)?
        .text()
        .to_string();
    let float_anno = kids
        .iter()
        .any(|c| c.kind() == Kind::TypeAnnotation && c.text().contains("Float"));
    // The only `Number` directly under a LocalDeclaration is its initializer.
    let float_init = kids.iter().any(is_float_literal);
    (float_anno || float_init).then_some(name)
}

impl Rule for FloatEqComparison {
    fn code(&self) -> LintCode {
        LintCode::L006
    }
    fn name(&self) -> &'static str {
        "float-eq-comparison"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::BinaryExpression {
            return;
        }
        let children = node.children();
        let has_eq_op = children.iter().any(|c| is_eq_op(c.kind()));
        if !has_eq_op {
            return;
        }
        let has_float = children.iter().any(is_float_literal);
        if has_float {
            diags.push(LintDiagnostic::new(
                LintCode::L006,
                node.range(),
                node.byte_range(),
                Severity::Error,
                "never compare floats with equality operators; use a tolerance check",
            ));
        }
    }

    /// File-scope pass that extends L006 to float-typed *locals* (#88). The
    /// per-node pass only sees float *literals*; here we first collect the locals
    /// declared float (annotation or float-literal init), then flag equality
    /// comparisons against them. Comparisons that already contain a float literal
    /// are left to `check_node` so each is reported exactly once.
    ///
    /// Uses the CST the runner already parsed (`check_file_cst`) rather than
    /// re-parsing the source.
    fn check_file_cst(
        &self,
        cst: &m1_core::Cst,
        _source: &str,
        _lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {
        let root = cst.root();
        let mut float_locals = std::collections::HashSet::new();
        for n in root.descendants() {
            if n.kind() == Kind::LocalDeclaration
                && let Some(name) = float_local_name(&n)
            {
                float_locals.insert(name);
            }
        }
        if float_locals.is_empty() {
            return;
        }
        for n in root.descendants() {
            if n.kind() != Kind::BinaryExpression {
                continue;
            }
            let children = n.children();
            if !children.iter().any(|c| is_eq_op(c.kind())) {
                continue;
            }
            // `check_node` already reports a comparison that has a float literal.
            if children.iter().any(is_float_literal) {
                continue;
            }
            let touches_float_local = n
                .named_children()
                .iter()
                .any(|c| c.kind() == Kind::Identifier && float_locals.contains(c.text()));
            if touches_float_local {
                diags.push(LintDiagnostic::new(
                    LintCode::L006,
                    n.range(),
                    n.byte_range(),
                    Severity::Error,
                    "never compare floats with equality operators; use a tolerance check",
                ));
            }
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
        r.register(Box::new(FloatEqComparison));
        Runner::new(r)
    }

    #[test]
    fn flags_float_eq_eq() {
        let source = "x = a == 1.0;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L006));
    }

    #[test]
    fn flags_float_bang_eq() {
        let source = "x = a != 0.5;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L006));
    }

    #[test]
    fn flags_float_with_eq_keyword() {
        let source = "x = a eq 1.0;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L006));
    }

    #[test]
    fn no_false_positive_int_eq() {
        let source = "x = a == 1;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L006));
    }

    #[test]
    fn no_false_positive_hex_literal_with_e() {
        // A hex literal whose digits include `e`/`E`/`f` (e.g. `0xE5`, `0xBEEF`,
        // `0xFACE`) is an *integer*, not a float — the `e` is a hex digit, not an
        // exponent marker. Equality comparison against it must NOT be flagged.
        for lit in ["0xE5", "0xBEEF", "0xFACE", "0xface", "0x1e", "0xDEAD"] {
            let source = format!("x = mask == {lit};\n");
            let result = runner().run_source(&source);
            assert!(
                result.diagnostics.iter().all(|d| d.code != LintCode::L006),
                "hex literal {lit} must not be treated as a float: {:?}",
                result.diagnostics
            );
        }
    }

    #[test]
    fn still_flags_decimal_exponent_float() {
        // A genuine exponent-notation float (`1e3`, `2.5E-2`) is still a float and
        // must remain flagged — the hex fix must not weaken real-float detection.
        for lit in ["1e3", "2.5E-2", "1.0", "0.5"] {
            let source = format!("x = a == {lit};\n");
            let result = runner().run_source(&source);
            assert!(
                result.diagnostics.iter().any(|d| d.code == LintCode::L006),
                "float literal {lit} should be flagged: {:?}",
                result.diagnostics
            );
        }
    }

    #[test]
    fn no_false_positive_eq_idents() {
        let source = "x = a eq b;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L006));
    }

    #[test]
    fn flags_local_initialized_from_float_literal() {
        // #88: a local whose initializer is a float literal is float-typed; an
        // equality comparison against it is the same bug as a bare float literal.
        let source = "local threshold = 1.5;\nx = speed eq threshold;\n";
        let result = runner().run_source(source);
        assert!(
            result.diagnostics.iter().any(|d| d.code == LintCode::L006),
            "comparison against a float-literal-initialized local should flag: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn flags_local_with_float_type_annotation() {
        let source = "local <Float> speed = 0.0;\nx = speed eq limit;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L006));
    }

    #[test]
    fn no_false_positive_integer_local() {
        // An integer-initialized local compared with an int literal must not flag.
        let source = "local count = 1;\nx = count eq 2;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L006));
    }

    #[test]
    fn float_local_comparison_flagged_exactly_once() {
        // A float literal *and* a float local in the same comparison must produce
        // only one L006 (the check_node + check_file passes must not double-count).
        let source = "local t = 1.5;\nx = t eq 2.0;\n";
        let result = runner().run_source(source);
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|d| d.code == LintCode::L006)
                .count(),
            1,
            "exactly one L006: {:?}",
            result.diagnostics
        );
    }
}

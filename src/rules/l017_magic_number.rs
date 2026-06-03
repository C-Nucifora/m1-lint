//! L017 — magic-number
//!
//! The M1 Build Development Manual's "Code Layout and Format" advises: *"Avoid
//! magic numbers. Rather, refer to a constant which describes the purpose of the
//! number."* This rule flags an unnamed numeric literal used as an operand of a
//! binary arithmetic/comparison expression — the embedded scaling factors and
//! thresholds the advice targets (`x * 0.05`, `speed > 35.0`).
//!
//! Deliberately conservative to avoid noise on legitimate uses:
//! - `0` and `1` (and `0.0`/`1.0`) are never magic.
//! - Hex literals (`0x…`) are bit masks / IDs / addresses, not magic constants.
//! - Only *binary-expression operands* are flagged — a literal that is a whole
//!   assignment value (`Foo = 5;`), a local initializer (`local n = 5;`, the
//!   recommended "name the number" form), a call argument, or an `expand` bound
//!   is not in a binary expression and is left alone.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

pub struct MagicNumber;

impl Rule for MagicNumber {
    fn code(&self) -> LintCode {
        LintCode::L017
    }
    fn name(&self) -> &'static str {
        "magic-number"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::Number {
            return;
        }
        // Only literals that are a direct operand of a binary expression.
        if node.parent().map(|p| p.kind()) != Some(Kind::BinaryExpression) {
            return;
        }
        let text = node.text();
        if is_exempt(text) {
            return;
        }
        diags.push(LintDiagnostic::new(
            LintCode::L017,
            node.range(),
            node.byte_range(),
            Severity::Warning,
            format!("magic number `{text}`; name it with a constant"),
        ));
    }
}

/// Literals that are never "magic": `0`/`1` (in any spelling) and hex literals
/// (bit masks / IDs / addresses).
fn is_exempt(text: &str) -> bool {
    let t = text.trim_end_matches(['u', 'U']);
    if t.starts_with("0x") || t.starts_with("0X") {
        return true;
    }
    matches!(
        t.parse::<f64>(),
        Ok(v) if v == 0.0 || v == 1.0
    )
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn l017_count(src: &str) -> usize {
        // L017 is off by default, so register it explicitly.
        let mut reg = Registry::empty();
        reg.register(Box::new(super::MagicNumber));
        Runner::new(reg)
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L017)
            .count()
    }

    #[test]
    fn flags_scaling_factor_in_arithmetic() {
        assert_eq!(l017_count("Energy = Power * 0.05;\n"), 1);
    }

    #[test]
    fn flags_threshold_in_comparison() {
        assert_eq!(l017_count("if (Speed > 35.0) { Value = 1; }\n"), 1);
    }

    #[test]
    fn exempts_zero_and_one() {
        assert_eq!(l017_count("x = y * 1;\nz = w + 0;\nq = r - 1.0;\n"), 0);
    }

    #[test]
    fn exempts_hex_literals() {
        assert_eq!(l017_count("flags = raw & 0xFF;\n"), 0);
    }

    #[test]
    fn ignores_literal_assignment_value_and_local_initializer() {
        // Naming contexts: not binary-expression operands.
        assert_eq!(l017_count("Foo = 5;\nlocal threshold = 35.0;\n"), 0);
    }

    #[test]
    fn ignores_expand_bounds() {
        assert_eq!(l017_count("expand (i = 0 to 4) { Value = i; }\n"), 0);
    }
}

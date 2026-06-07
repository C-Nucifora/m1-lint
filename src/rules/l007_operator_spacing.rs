//! L007 — operator-spacing
//!
//! Binary and assignment operator tokens must be surrounded by a space on each
//! side. Checked operators: the arithmetic `+`, `-`, `*`, `/`, `%`; the bitwise
//! `&`, `|`, `^`, `<<`, `>>`; the assignment forms `=`/`+=`/`-=`/`*=`/`/=` and
//! the compound forms `%=`/`&=`/`|=`/`^=`/`<<=`/`>>=`; and the relational
//! operators `<`, `>`, `<=`, `>=`. This mirrors exactly the set m1-fmt spaces.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L007 — flags operators not surrounded by spaces.
pub struct OperatorSpacing;

/// The set of operators L007 checks. This is intentionally **not** the same as
/// `m1_core::is_binary_op` / `is_compound_assign` / `is_unary_op`, so those
/// shared predicates are deliberately not reused here:
///
/// - `is_binary_op` also covers the equality operators (`== != eq neq`) and the
///   logical operators (`&& || and or`). L007 must *not* flag spacing on those —
///   L004/L005/L006 own the equality/logical operators, and double-flagging
///   their spacing would change behaviour.
/// - L007 additionally checks the plain assignment `=` (`Kind::Assign`) and the
///   relational operators (`< > <= >=`), which `is_compound_assign` does not
///   cover and `is_binary_op` only partly covers.
///
/// So the L007 set = arithmetic + bitwise/shift + relational + every assignment
/// form (`=`, `+= -= *= /=`, and the compound `%= &= |= ^= <<= >>=`). This
/// mirrors exactly the set m1-fmt spaces; keep them in lock-step.
fn is_checked_operator(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Plus
            | Kind::Minus
            | Kind::Star
            | Kind::Slash
            | Kind::Percent
            // bitwise binary
            | Kind::Amp
            | Kind::Pipe
            | Kind::Caret
            | Kind::LtLt
            | Kind::GtGt
            | Kind::Assign // assignment =
            | Kind::PlusEq
            | Kind::MinusEq
            | Kind::StarEq
            | Kind::SlashEq
            // compound assignment added in the v0.4.0 grammar
            | Kind::PercentEq
            | Kind::AmpEq
            | Kind::PipeEq
            | Kind::CaretEq
            | Kind::LtLtEq
            | Kind::GtGtEq
            | Kind::Lt
            | Kind::Gt
            | Kind::LtEq
            | Kind::GtEq
    )
}

fn has_space_before(source: &[u8], byte_start: usize) -> bool {
    if byte_start == 0 {
        return false;
    }
    let prev = source[byte_start - 1];
    if prev == b' ' {
        return true;
    }
    // m1-fmt wraps long expressions by putting the symbolic operator at the
    // START of the continuation line, after the tab indentation — which the
    // manual sanctions. In that case the operator is effectively spaced from its
    // left operand by the newline, so treat "begins a (possibly indented) line"
    // as having space before. We accept this when every byte from the operator
    // back to the previous `\n` (or start of file) is horizontal whitespace
    // (tab/space). A bare newline immediately before counts too. (#68)
    if prev == b'\n' {
        return true;
    }
    if prev == b'\t' || prev == b' ' {
        let line_start = source[..byte_start - 1]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        return source[line_start..byte_start]
            .iter()
            .all(|&b| b == b' ' || b == b'\t');
    }
    false
}

fn has_space_after(source: &[u8], byte_end: usize) -> bool {
    byte_end < source.len() && source[byte_end] == b' '
}

impl Rule for OperatorSpacing {
    fn code(&self) -> LintCode {
        LintCode::L007
    }
    fn name(&self) -> &'static str {
        "operator-spacing"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        // Operators appear as direct children of binary expressions and
        // assignment statements.
        if !matches!(
            node.kind(),
            Kind::BinaryExpression | Kind::AssignmentStatement
        ) {
            return;
        }
        let source_bytes = source.as_bytes();
        for child in node.children() {
            if !is_checked_operator(child.kind()) {
                continue;
            }
            let br = child.byte_range();
            let missing_before = !has_space_before(source_bytes, br.start);
            let missing_after = !has_space_after(source_bytes, br.end);
            if missing_before || missing_after {
                diags.push(LintDiagnostic::new(
                    LintCode::L007,
                    child.range(),
                    br,
                    Severity::Warning,
                    format!("missing space around operator `{}`", child.text()),
                ));
            }
        }
    }

    fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if !matches!(
            node.kind(),
            Kind::BinaryExpression | Kind::AssignmentStatement
        ) {
            return;
        }
        let bytes = source.as_bytes();
        for child in node.children() {
            if !is_checked_operator(child.kind()) {
                continue;
            }
            let br = child.byte_range();
            if !has_space_before(bytes, br.start) {
                edits.push(crate::fix::Edit {
                    byte_range: br.start..br.start,
                    replacement: " ".into(),
                });
            }
            if !has_space_after(bytes, br.end) {
                edits.push(crate::fix::Edit {
                    byte_range: br.end..br.end,
                    replacement: " ".into(),
                });
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
        r.register(Box::new(OperatorSpacing));
        Runner::new(r)
    }

    #[test]
    fn no_diagnostic_on_spaced_operators() {
        let source = "x = a + b;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L007));
    }

    #[test]
    fn flags_missing_space_before_plus() {
        let source = "x = a+ b;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L007));
    }

    #[test]
    fn flags_missing_space_after_plus() {
        let source = "x = a +b;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L007));
    }

    #[test]
    fn flags_no_space_around_plus() {
        let source = "x = a+b;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L007));
    }

    #[test]
    fn flags_missing_space_around_assignment() {
        let source = "x=1;\n";
        let result = runner().run_source(source);
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L007));
    }

    #[test]
    fn no_diagnostic_on_unary_minus() {
        // A unary negation lives inside a UnaryExpression, so its `-` is not a
        // direct child of a binary/assignment node and must never be flagged for
        // a "missing space before" (it has no left operand). Regression for #16.
        for src in [
            "x = -1.0;\n",
            "z = -x;\n",
            "y = a + -b;\n",
            "w = (-a) * b;\n",
        ] {
            let result = runner().run_source(src);
            assert!(
                result.diagnostics.iter().all(|d| d.code != LintCode::L007),
                "L007 should not fire on unary minus in {src:?}"
            );
        }
    }

    #[test]
    fn still_flags_binary_minus_without_spaces() {
        let result = runner().run_source("x = a-b;\n");
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L007));
    }

    #[test]
    fn fixes_missing_spacing() {
        let mut r = Registry::empty();
        r.register(Box::new(OperatorSpacing));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("x = a+b;\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = a + b;\n"));
    }

    #[test]
    fn flags_and_fixes_compound_assignment_operators() {
        // The compound-assignment operators added in the v0.4.0 grammar
        // (%= &= |= ^= <<= >>=) must be flagged and fixed like every other one.
        for (src, fixed) in [
            ("x%=1;\n", "x %= 1;\n"),
            ("x&=1;\n", "x &= 1;\n"),
            ("x|=1;\n", "x |= 1;\n"),
            ("x^=1;\n", "x ^= 1;\n"),
            ("x<<=1;\n", "x <<= 1;\n"),
            ("x>>=1;\n", "x >>= 1;\n"),
        ] {
            let result = runner().run_source(src);
            assert!(
                result.diagnostics.iter().any(|d| d.code == LintCode::L007),
                "L007 should flag {src:?}"
            );
            let mut r = Registry::empty();
            r.register(Box::new(OperatorSpacing));
            let fixer = crate::fix::Fixer::new(&r);
            assert_eq!(
                fixer.fix_source(src).unwrap().as_deref(),
                Some(fixed),
                "fix for {src:?}"
            );
        }
    }

    #[test]
    fn no_l007_when_operator_begins_continuation_line() {
        // m1-fmt wraps long binary expressions with the symbolic operator at the
        // start of the (tab-indented) continuation line — manual-conformant. L007
        // must NOT flag those, or fmt and lint disagree (#68). The operator has a
        // newline + tab before it and a space after it.
        for src in [
            "x = aaaaaaaa\n\t>= bbbbbbbb;\n",
            "x = aaaaaaaa\n\t< bbbbbbbb;\n",
            "x = aaaaaaaa\n\t== bbbbbbbb;\n",
            "x = aaaaaaaa\n\t+ bbbbbbbb;\n",
            "x = aaaaaaaa\n    >= bbbbbbbb;\n", // space-indented continuation too
        ] {
            let result = runner().run_source(src);
            assert!(
                result.diagnostics.iter().all(|d| d.code != LintCode::L007),
                "L007 must not fire on a line-leading wrapped operator in {src:?}, got {:?}",
                result.diagnostics
            );
        }
    }

    #[test]
    fn fix_leaves_wrapped_operator_untouched() {
        // The fixer must not "repair" a line-leading operator (it is already
        // correctly spaced via the newline), i.e. nothing to fix (#68).
        let mut r = Registry::empty();
        r.register(Box::new(OperatorSpacing));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(
            fixer.fix_source("x = aaaaaaaa\n\t>= bbbbbbbb;\n").unwrap(),
            None
        );
    }

    #[test]
    fn still_flags_operator_glued_to_left_operand_mid_line() {
        // Guard against over-broadening: a mid-line operator with a non-ws byte
        // immediately before it is still flagged.
        let result = runner().run_source("x = a\n\t+b;\n");
        assert!(
            result.diagnostics.iter().any(|d| d.code == LintCode::L007),
            "line-leading operator glued to its right operand must still flag (missing space after)"
        );
    }

    #[test]
    fn flags_bitwise_binary_operators() {
        for src in [
            "x = a&b;\n",
            "x = a|b;\n",
            "x = a^b;\n",
            "x = a<<b;\n",
            "x = a>>b;\n",
        ] {
            let result = runner().run_source(src);
            assert!(
                result.diagnostics.iter().any(|d| d.code == LintCode::L007),
                "L007 should flag {src:?}"
            );
        }
    }
}

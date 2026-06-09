//! L020 — object-naming
//!
//! Manual p.64, "Naming Objects": object names *begin with an uppercase
//! letter* (a space may be used between constituents). Locals are the
//! opposite (L016). This rule flags an assignment whose written object —
//! the head segment of the target path — begins with a lowercase letter and
//! is not a declared local: writing `engine speed = …` names a channel that
//! violates the convention.
//!
//! Two-phase ([`Rule::check_file_cst`]): first collect every declared local
//! (and `expand` loop variable), then visit assignment targets. Reference
//! keywords (`Out`, `This`, …) begin uppercase anyway; identifiers containing
//! a `$(…)` interpolation are skipped (their final spelling is decided at
//! expansion time).

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};
use std::collections::HashSet;

pub struct ObjectNaming;

/// The head identifier of an assignment target: the target itself when it is
/// a bare identifier, or the innermost `object` of a member chain
/// (`Engine.Speed` → `Engine`).
fn target_head<'a>(target: &Node<'a>) -> Option<Node<'a>> {
    let mut n = *target;
    loop {
        match n.kind() {
            Kind::Identifier => return Some(n),
            Kind::MemberExpression => {
                n = n.child_by_field(Field::Object)?;
            }
            _ => return None,
        }
    }
}

impl Rule for ObjectNaming {
    fn code(&self) -> LintCode {
        LintCode::L020
    }
    fn name(&self) -> &'static str {
        "object-naming"
    }

    fn check_file_cst(
        &self,
        cst: &m1_core::Cst,
        _source: &str,
        _lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {
        // Phase 1: every name that is a local (or expand counter) anywhere in
        // the file. M1 locals are file-scoped, so one flat set suffices.
        let mut locals: HashSet<String> = HashSet::new();
        for n in cst.root().descendants() {
            let field = match n.kind() {
                Kind::LocalDeclaration => Field::Name,
                Kind::ExpandStatement => Field::Variable,
                _ => continue,
            };
            if let Some(name) = n.child_by_field(field) {
                locals.insert(name.text().to_string());
            }
        }

        // Phase 2: assignment targets whose head object begins lowercase.
        for n in cst.root().descendants() {
            if n.kind() != Kind::AssignmentStatement {
                continue;
            }
            let Some(head) = n
                .child_by_field(Field::Target)
                .and_then(|t| target_head(&t))
            else {
                continue;
            };
            let name = head.text();
            if name.contains("$(") || locals.contains(name) {
                continue;
            }
            if name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
                diags.push(LintDiagnostic::new(
                    LintCode::L020,
                    head.range(),
                    head.byte_range(),
                    Severity::Warning,
                    format!(
                        "object `{name}` should begin with an uppercase letter (locals begin lowercase)"
                    ),
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
        r.register(Box::new(ObjectNaming));
        Runner::new(r)
    }

    fn codes(src: &str) -> usize {
        runner()
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L020)
            .count()
    }

    #[test]
    fn flags_lowercase_channel_write() {
        assert_eq!(codes("engine speed = 1;\n"), 1);
        assert_eq!(codes("i2t Active = true;\n"), 1);
    }

    #[test]
    fn flags_lowercase_member_head() {
        assert_eq!(codes("engine.Speed = 1;\n"), 1);
    }

    #[test]
    fn uppercase_objects_are_fine() {
        assert_eq!(codes("Engine.Speed = 1;\nValue = 2;\n"), 0);
    }

    #[test]
    fn locals_are_exempt_even_when_written() {
        assert_eq!(codes("local pulseWidth = 1;\npulseWidth = 2;\n"), 0);
    }

    #[test]
    fn expand_counters_and_interpolations_are_exempt() {
        assert_eq!(
            codes("expand (i = 0 to 3)\n{\n\ty = 1;\n}\n"),
            1, // `y` is a lowercase object write inside the block
        );
        assert_eq!(
            codes("local y = 0;\nexpand (i = 0 to 3)\n{\n\ty = 1;\n}\n"),
            0
        );
    }

    #[test]
    fn reference_keywords_are_fine() {
        assert_eq!(codes("Out = 1;\nThis.Debounce = 2;\n"), 0);
    }
}

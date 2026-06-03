//! L015 — local-missing-initializer
//!
//! The M1 Build Development Manual requires every `local` declaration to have an
//! initial value (`local [name] = [value];`) — the variable's data type is
//! inferred from that value, so a declaration without one is a compile error in
//! M1 Build. The grammar permits the initializer to be absent, so this rule flags
//! a `local`/`static local` declaration that has no `= value`.
//!
//! Pure CST analysis (no project needed): a `local_declaration` node missing its
//! `value` field. No false positives.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct LocalMissingInitializer;

impl Rule for LocalMissingInitializer {
    fn code(&self) -> LintCode {
        LintCode::L015
    }
    fn name(&self) -> &'static str {
        "local-missing-initializer"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::LocalDeclaration {
            return;
        }
        if node.child_by_field(Field::Value).is_some() {
            return;
        }
        let name = node
            .child_by_field(Field::Name)
            .map(|n| n.text().to_string())
            .unwrap_or_default();
        diags.push(LintDiagnostic::new(
            LintCode::L015,
            node.range(),
            node.byte_range(),
            Severity::Warning,
            format!("local `{name}` has no initializer; M1 requires `local {name} = <value>;`"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn l015_count(src: &str) -> usize {
        Runner::new(Registry::default())
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L015)
            .count()
    }

    #[test]
    fn flags_local_without_initializer() {
        assert_eq!(l015_count("local x;\nValue = 1;\n"), 1);
    }

    #[test]
    fn flags_typed_local_without_initializer() {
        assert_eq!(l015_count("local <Unsigned Integer> h;\nValue = 1;\n"), 1);
    }

    #[test]
    fn flags_static_local_without_initializer() {
        assert_eq!(l015_count("static local s;\nValue = 1;\n"), 1);
    }

    #[test]
    fn accepts_local_with_initializer() {
        assert_eq!(l015_count("local x = 1;\nValue = x;\n"), 0);
    }

    #[test]
    fn accepts_typed_local_with_initializer() {
        assert_eq!(l015_count("local <boolean> b = false;\nValue = b;\n"), 0);
    }
}

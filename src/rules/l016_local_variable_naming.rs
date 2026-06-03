//! L016 — local-variable-naming
//!
//! The M1 Build Development Manual's "Naming Local Variables" conventions:
//! a local variable name should begin with a lowercase letter (camelCase is
//! optional) and contain no spaces. The lower-initial spelling is what visually
//! distinguishes locals from channels/objects (which begin uppercase), so the
//! manual calls this out as a readability aid. This rule flags a `local`
//! declaration whose name violates either convention.
//!
//! Pure CST analysis (no project needed). No false positives: the `<Type>`
//! annotation (which may contain spaces / uppercase, e.g. `<Unsigned Integer>`)
//! is a separate field and is never inspected.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct LocalVariableNaming;

impl Rule for LocalVariableNaming {
    fn code(&self) -> LintCode {
        LintCode::L016
    }
    fn name(&self) -> &'static str {
        "local-variable-naming"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::LocalDeclaration {
            return;
        }
        let Some(name_node) = node.child_by_field(Field::Name) else {
            return;
        };
        let name = name_node.text();

        let mut problems: Vec<&str> = Vec::new();
        if !name
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(true)
        {
            problems.push("begin with a lowercase letter");
        }
        if name.contains(' ') {
            problems.push("contain no spaces");
        }
        if problems.is_empty() {
            return;
        }

        diags.push(LintDiagnostic::new(
            LintCode::L016,
            name_node.range(),
            name_node.byte_range(),
            Severity::Warning,
            format!("local `{name}` should {}", problems.join(" and ")),
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn l016_count(src: &str) -> usize {
        Runner::new(Registry::default())
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L016)
            .count()
    }

    #[test]
    fn flags_uppercase_initial_local() {
        assert_eq!(l016_count("local FooBar = 1;\nValue = FooBar;\n"), 1);
    }

    #[test]
    fn flags_spaced_local_name() {
        assert_eq!(l016_count("local foo bar = 1;\nValue = foo bar;\n"), 1);
    }

    #[test]
    fn accepts_lowercase_local() {
        assert_eq!(l016_count("local foo = 1;\nValue = foo;\n"), 0);
    }

    #[test]
    fn accepts_camelcase_local() {
        assert_eq!(l016_count("local fooBar = 1;\nValue = fooBar;\n"), 0);
    }

    #[test]
    fn does_not_inspect_the_type_annotation() {
        // `<Unsigned Integer>` has spaces and uppercase but is the type, not the name.
        assert_eq!(
            l016_count("local <Unsigned Integer> h = 0;\nValue = h;\n"),
            0
        );
    }
}

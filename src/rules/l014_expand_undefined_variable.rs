//! L014 — expand-undefined-variable
//!
//! `expand (VAR = lo to hi) { … $(VAR) … }` does compile-time text
//! substitution. M1-Build expands an *undefined* `$(VAR)` to the empty string,
//! which silently corrupts a channel name or makes an assignment vanish. This
//! rule flags any `$(VAR)` interpolation whose variable is not bound by an
//! enclosing `expand` statement.
//!
//! Pure CST analysis (no project needed): for each `interpolation` node, walk up
//! the ancestors and check that some enclosing `expand_statement` binds that
//! variable name. (A `$(VAR)` that folds into a multi-word identifier segment —
//! e.g. `$(NODE) Foo` — is part of an `identifier` token rather than a standalone
//! `interpolation` node; catching those is a possible follow-up.)

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct ExpandUndefinedVariable;

impl Rule for ExpandUndefinedVariable {
    fn code(&self) -> LintCode {
        LintCode::L014
    }
    fn name(&self) -> &'static str {
        "expand-undefined-variable"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::Interpolation {
            return;
        }
        let Some(var) = interpolation_variable(node) else {
            return;
        };
        if bound_by_enclosing_expand(node, var) {
            return;
        }
        diags.push(LintDiagnostic::new(
            LintCode::L014,
            node.range(),
            node.byte_range(),
            Severity::Warning,
            format!("expand variable `{var}` is not defined in any enclosing expand statement"),
        ));
    }
}

/// The variable name inside a `$(VAR)` interpolation node, e.g. `SEG`.
fn interpolation_variable<'a>(node: &Node<'a>) -> Option<&'a str> {
    let text = node.text();
    let inner = text.strip_prefix("$(")?.strip_suffix(')')?;
    let name = inner.trim();
    (!name.is_empty()).then_some(name)
}

/// True if `var` is bound by an `expand (var = …)` ancestor of `node`.
fn bound_by_enclosing_expand(node: &Node, var: &str) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == Kind::ExpandStatement
            && n.child_by_field(Field::Variable).map(|v| v.text().trim()) == Some(var)
        {
            return true;
        }
        current = n.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn l014_count(src: &str) -> usize {
        Runner::new(Registry::default())
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L014)
            .count()
    }

    #[test]
    fn flags_undefined_expand_variable() {
        // `NODE` is bound; `Undefined` is not.
        let src = "expand (NODE = 1 to 3) {\n  x = $(NODE) + $(Undefined);\n}\n";
        assert_eq!(l014_count(src), 1);
    }

    #[test]
    fn accepts_variable_bound_by_enclosing_expand() {
        let src = "expand (SEG = 1 to 6) {\n  x = $(SEG) + 1;\n}\n";
        assert_eq!(l014_count(src), 0);
    }

    #[test]
    fn accepts_variable_bound_by_an_outer_expand() {
        // Nested: the inner body still sees the outer SEG binding.
        let src = "expand (SEG = 1 to 6) {\n  expand (NODE = 1 to 3) {\n    x = $(SEG) + $(NODE);\n  }\n}\n";
        assert_eq!(l014_count(src), 0);
    }

    #[test]
    fn flags_interpolation_outside_any_expand() {
        let src = "x = $(SEG) + 1;\n";
        assert_eq!(l014_count(src), 1);
    }
}

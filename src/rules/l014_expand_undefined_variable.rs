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
//! variable name. A `$(VAR)` that folds into a multi-word identifier segment —
//! e.g. the channel name `Cell $(NODE) Voltage` — is part of an `identifier`
//! token rather than a standalone `interpolation` node; those are also scanned
//! (each embedded `$(…)` checked individually). This folded form is the highest
//! risk: an undefined `$(VAR)` inside a name silently renames the channel.

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

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        match node.kind() {
            // A standalone `$(VAR)` operand.
            Kind::Interpolation => {
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
                    undefined_message(var),
                ));
            }
            // A `$(VAR)` folded into a multi-word identifier segment, e.g. the
            // channel name `Cell $(SEG) Voltage` — one identifier token, so the
            // interpolation is not its own node. This is the silent-rename case.
            Kind::Identifier => {
                let text = node.text();
                let base = node.byte_range().start;
                for (start_off, end_off, name) in folded_interpolations(text) {
                    if bound_by_enclosing_expand(node, name) {
                        continue;
                    }
                    let start = base + start_off;
                    let end = base + end_off;
                    let range = m1_core::Range {
                        start: byte_to_position(source, start),
                        end: byte_to_position(source, end),
                    };
                    diags.push(LintDiagnostic::new(
                        LintCode::L014,
                        range,
                        start..end,
                        Severity::Warning,
                        undefined_message(name),
                    ));
                }
            }
            _ => {}
        }
    }
}

fn undefined_message(var: &str) -> String {
    format!("expand variable `{var}` is not defined in any enclosing expand statement")
}

/// The variable name inside a `$(VAR)` interpolation node, e.g. `SEG`.
fn interpolation_variable<'a>(node: &Node<'a>) -> Option<&'a str> {
    let text = node.text();
    let inner = text.strip_prefix("$(")?.strip_suffix(')')?;
    let name = inner.trim();
    (!name.is_empty()).then_some(name)
}

/// Every `$(name)` substring folded inside an identifier segment, as
/// `(start, end, trimmed_name)` byte offsets within `text`. Empty `$()` is
/// skipped (nothing to bind).
fn folded_interpolations(text: &str) -> Vec<(usize, usize, &str)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$'
            && bytes[i + 1] == b'('
            && let Some(close_rel) = text[i + 2..].find(')')
        {
            let inner_end = i + 2 + close_rel;
            let end = inner_end + 1; // past the ')'
            let name = text[i + 2..inner_end].trim();
            if !name.is_empty() {
                out.push((i, end, name));
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

/// Byte offset → LSP-style `Position` (column is a byte offset within the line,
/// matching the rest of the toolchain's position convention).
fn byte_to_position(source: &str, byte: usize) -> m1_core::Position {
    let upto = byte.min(source.len());
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (idx, b) in source.as_bytes()[..upto].iter().enumerate() {
        if *b == b'\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    m1_core::Position {
        line,
        column: (upto - line_start) as u32,
    }
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

    #[test]
    fn flags_unbound_interpolation_folded_into_identifier() {
        // `$(SEG)` folds into the channel name `Cell $(SEG) Voltage` (one
        // identifier token, not a standalone interpolation node). Unbound here:
        // it expands to the empty string and silently renames the channel.
        let src = "Cell $(SEG) Voltage = 0;\n";
        assert_eq!(l014_count(src), 1);
    }

    #[test]
    fn accepts_folded_interpolation_bound_by_expand() {
        let src = "expand (SEG = 1 to 6) {\n  Cell $(SEG) Voltage = 0;\n}\n";
        assert_eq!(l014_count(src), 0);
    }

    #[test]
    fn flags_each_unbound_folded_interpolation_separately() {
        // Two folded interpolations in one name, neither bound.
        let src = "Cell $(SEG) Probe $(NODE) Temp = 0;\n";
        assert_eq!(l014_count(src), 2);
    }
}

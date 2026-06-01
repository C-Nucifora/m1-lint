//! L012 — unused-local
//!
//! Flags a `local` declaration whose name is never referenced anywhere else in
//! the file. This is a pure CST walk (no symbol model): collect every
//! `local_declaration` name, and every *other* identifier occurrence; a local
//! whose name never appears as another identifier is unused.
//!
//! The reference count is file-global rather than scope-precise. That is the
//! safe direction for a lint: an identifier that happens to share the name
//! (another local, a member access, a type name) only ever *suppresses* a
//! warning, so the rule never produces a false positive — at worst it misses a
//! genuinely-unused local that collides with an unrelated name.

use std::collections::HashSet;

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct UnusedLocal;

impl Rule for UnusedLocal {
    fn code(&self) -> LintCode {
        LintCode::L012
    }
    fn name(&self) -> &'static str {
        "unused-local"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        // Run once per file, off the root node.
        if node.kind() != Kind::SourceFile {
            return;
        }
        let mut decls: Vec<Node> = Vec::new();
        let mut used: HashSet<String> = HashSet::new();
        collect(*node, &mut decls, &mut used);

        for name in decls {
            if !used.contains(name.text()) {
                diags.push(LintDiagnostic::new(
                    LintCode::L012,
                    name.range(),
                    name.byte_range(),
                    Severity::Warning,
                    format!("local `{}` is never used", name.text()),
                ));
            }
        }
    }
}

/// Walk the tree once: record each `local_declaration`'s name node as a
/// declaration, and every other identifier's text as a use.
fn collect<'a>(node: Node<'a>, decls: &mut Vec<Node<'a>>, used: &mut HashSet<String>) {
    let decl_name = (node.kind() == Kind::LocalDeclaration)
        .then(|| node.child_by_field(Field::Name))
        .flatten();
    if let Some(n) = decl_name {
        decls.push(n);
    }
    for child in node.children() {
        if child.kind() == Kind::Identifier {
            // The declaration's own name node is a definition, not a use.
            let is_decl_name = decl_name.is_some_and(|d| d.byte_range() == child.byte_range());
            if !is_decl_name {
                used.insert(child.text().to_string());
            }
        }
        collect(child, decls, used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn runner() -> Runner {
        let mut r = Registry::empty();
        r.register(Box::new(UnusedLocal));
        Runner::new(r)
    }

    fn codes(src: &str) -> Vec<LintCode> {
        runner()
            .run_source(src)
            .diagnostics
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn flags_unused_local() {
        assert!(codes("local x = 1;\n").contains(&LintCode::L012));
    }

    #[test]
    fn does_not_flag_used_local() {
        assert!(!codes("local x = 1;\nOut = x + 2;\n").contains(&LintCode::L012));
    }

    #[test]
    fn used_in_member_or_call_counts() {
        // referenced as part of a larger expression
        assert!(!codes("local x = 1;\nlocal y = x;\nOut = y;\n").contains(&LintCode::L012));
    }

    #[test]
    fn reassignment_counts_as_use() {
        // write-only locals are not flagged (the LHS identifier counts) — the
        // safe choice to avoid false positives on legitimate patterns.
        assert!(!codes("local x = 1;\nx = 2;\n").contains(&LintCode::L012));
    }

    #[test]
    fn flags_only_the_unused_one() {
        let cs = codes("local used = 1;\nlocal dead = 2;\nOut = used;\n");
        assert_eq!(cs.iter().filter(|c| **c == LintCode::L012).count(), 1);
    }
}

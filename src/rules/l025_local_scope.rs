//! L025 — local-scope-too-wide
//!
//! Manual p.67, *Code Layout and Format*: "Declare local variables in the most
//! constrained scope". Flags a `local` declaration when every reference to it
//! lives inside a single nested block strictly deeper than the declaring
//! block — the declaration can move into that block.
//!
//! Exemptions (legitimate M1 patterns that must not flag):
//! - `static local` — persistence across executions is the point; moving the
//!   declaration is a semantic question, not a layout one.
//! - declarations whose initializer contains a call — moving the declaration
//!   into a conditional block changes *when* the initializer is evaluated
//!   (e.g. a stateful `Timer.…` read is sampled on fewer paths).
//! - uses inside an `expand` body — the body is text-substituted per
//!   iteration, so the expand statement itself is treated as the use site.
//! - a name declared more than once in the file — without a symbol model the
//!   uses cannot be attributed to one declaration, so the rule stays silent
//!   (the conservative direction, matching L012's philosophy).
//!
//! No autofix: moving a declaration is a semantic edit.

use std::collections::HashMap;

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Field, Kind, Node, Severity};

pub struct LocalScopeTooWide;

impl Rule for LocalScopeTooWide {
    fn code(&self) -> LintCode {
        LintCode::L025
    }
    fn name(&self) -> &'static str {
        "local-scope-too-wide"
    }

    fn check_file_cst(
        &self,
        cst: &m1_core::Cst,
        _source: &str,
        _lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {
        let root = cst.root();
        let mut decls: Vec<(Node, Node)> = Vec::new(); // (declaration, name)
        let mut decl_count: HashMap<String, usize> = HashMap::new();
        let mut uses: HashMap<String, Vec<Node>> = HashMap::new();
        collect(root, &mut decls, &mut decl_count, &mut uses);

        for (decl, name) in decls {
            let text = name.text();
            // Re-declared names can't be attributed without a symbol model.
            if decl_count.get(text).copied().unwrap_or(0) > 1 {
                continue;
            }
            // `static local`: persistence is the point — exempt.
            if decl.children().iter().any(|c| c.kind() == Kind::Static) {
                continue;
            }
            // Initializer with a call: moving it changes when it is sampled.
            if decl
                .child_by_field(Field::Value)
                .is_some_and(|v| contains_call(v))
            {
                continue;
            }
            let Some(use_sites) = uses.get(text) else {
                continue; // never used: L012's finding, not ours
            };
            let decl_chain = block_chain(&decl);
            // Lowest common ancestor block of every use = the longest common
            // prefix of their block chains.
            let mut lca = block_chain(&use_sites[0]);
            for u in &use_sites[1..] {
                let chain = block_chain(u);
                let common = lca
                    .iter()
                    .zip(chain.iter())
                    .take_while(|(a, b)| a.byte_range() == b.byte_range())
                    .count();
                lca.truncate(common);
            }
            // Flag only when the deepest block containing every use sits
            // strictly inside the declaring block.
            let same_branch = lca.len() > decl_chain.len()
                && decl_chain
                    .iter()
                    .zip(lca.iter())
                    .all(|(a, b)| a.byte_range() == b.byte_range());
            if same_branch {
                let target_line = lca.last().map(|b| b.range().start.line + 1).unwrap_or(1);
                diags.push(LintDiagnostic::new(
                    LintCode::L025,
                    name.range(),
                    name.byte_range(),
                    Severity::Warning,
                    format!(
                        "local `{text}` is only used inside the nested block starting on line {target_line} — declare it in the most constrained scope"
                    ),
                ));
            }
        }
    }
}

/// Walk the tree once (explicit stack — no recursion on user-shaped input):
/// record each `local_declaration` (with its name node) and every identifier
/// use, keyed by text. The declaration's own name node is a definition, not a
/// use, and the `property` of a member expression (`Foo.sum`) is not a local
/// reference.
fn collect<'a>(
    root: Node<'a>,
    decls: &mut Vec<(Node<'a>, Node<'a>)>,
    decl_count: &mut HashMap<String, usize>,
    uses: &mut HashMap<String, Vec<Node<'a>>>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let decl_name = (node.kind() == Kind::LocalDeclaration)
            .then(|| node.child_by_field(Field::Name))
            .flatten();
        if let Some(name) = decl_name {
            decls.push((node, name));
            *decl_count.entry(name.text().to_string()).or_default() += 1;
        }
        let property = (node.kind() == Kind::MemberExpression)
            .then(|| node.child_by_field(Field::Property))
            .flatten();
        for child in node.children() {
            if child.kind() == Kind::Identifier {
                let is_decl_name = decl_name.is_some_and(|d| d.byte_range() == child.byte_range());
                let is_property = property.is_some_and(|p| p.byte_range() == child.byte_range());
                if !is_decl_name && !is_property {
                    uses.entry(child.text().to_string())
                        .or_default()
                        .push(child);
                }
            }
            stack.push(child);
        }
    }
}

/// Root-first list of the `Block` nodes enclosing `node`. Blocks inside an
/// `expand` body are discarded — the body repeats per iteration, so for scope
/// purposes a use inside it happens where the expand statement stands.
fn block_chain<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut chain = Vec::new(); // innermost-first while walking up
    let mut cur = *node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            Kind::Block => chain.push(parent),
            Kind::ExpandStatement => chain.clear(),
            _ => {}
        }
        cur = parent;
    }
    chain.reverse();
    chain
}

/// Whether the expression contains any call (iteratively, like `collect`).
fn contains_call(node: Node) -> bool {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == Kind::CallExpression {
            return true;
        }
        stack.extend(n.children());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn msgs(src: &str) -> Vec<String> {
        let mut r = Registry::empty();
        r.register(Box::new(LocalScopeTooWide));
        Runner::new(r)
            .run_source(src)
            .diagnostics
            .into_iter()
            .filter(|d| d.code == LintCode::L025)
            .map(|d| d.inner.message)
            .collect()
    }

    #[test]
    fn flags_local_used_only_in_a_nested_block() {
        let src = "local sum = 0;\nif (a)\n{\n\tsum = sum + 1;\n\tOut = sum;\n}\n";
        let m = msgs(src);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("local `sum`"), "{m:?}");
        assert!(m[0].contains("line 3"), "{m:?}");
    }

    #[test]
    fn use_at_declaration_level_keeps_it_quiet() {
        let src = "local sum = 0;\nOut = sum;\nif (a)\n{\n\tsum = 1;\n}\n";
        assert!(msgs(src).is_empty());
    }

    #[test]
    fn uses_in_two_sibling_blocks_keep_it_quiet() {
        // The if and else arms are different blocks: the declaration already
        // sits in the most constrained scope covering both.
        let src = "local m = 0;\nif (a)\n{\n\tm = 1;\n}\nelse\n{\n\tm = 2;\n}\n";
        assert!(msgs(src).is_empty());
    }

    #[test]
    fn deeper_single_branch_names_the_innermost_block() {
        let src = "local x = 0;\nif (a)\n{\n\tif (b)\n\t{\n\t\tx = 1;\n\t\tOut = x;\n\t}\n}\n";
        let m = msgs(src);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("line 5"), "{m:?}");
    }

    #[test]
    fn static_local_is_exempt() {
        let src = "static local s = 0;\nif (a)\n{\n\ts = s + 1;\n}\n";
        assert!(msgs(src).is_empty());
    }

    #[test]
    fn call_initializer_is_exempt() {
        // Moving the declaration into the if-block would change when the call
        // is sampled.
        let src = "local t = Library.System.Ticks();\nif (a)\n{\n\tOut = t;\n}\n";
        assert!(msgs(src).is_empty());
    }

    #[test]
    fn use_inside_expand_body_counts_at_the_expand_site() {
        // The expand body repeats; the declaration must stay outside it.
        let src = "local i = 0;\nexpand (V = 0 to 3)\n{\n\ti = i + 1;\n}\n";
        assert!(msgs(src).is_empty());
    }

    #[test]
    fn unused_local_is_l012s_finding_not_ours() {
        assert!(msgs("local dead = 1;\n").is_empty());
    }

    #[test]
    fn redeclared_name_is_skipped() {
        let src =
            "if (a)\n{\n\tlocal x = 1;\n\tOut = x;\n}\nlocal x = 2;\nif (b)\n{\n\tOut = x;\n}\n";
        assert!(msgs(src).is_empty());
    }

    #[test]
    fn member_property_with_same_name_is_not_a_use() {
        // `Foo.sum` does not reference the local `sum`; only the nested real
        // use counts, so the rule still flags.
        let src = "local sum = 0;\nOut = Foo.sum;\nif (a)\n{\n\tsum = 1;\n\tValue = sum;\n}\n";
        let m = msgs(src);
        assert_eq!(m.len(), 1, "{m:?}");
    }
}

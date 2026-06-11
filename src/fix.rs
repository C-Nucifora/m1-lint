//! Autofix: collect mechanical edits, apply them, and verify the result is
//! syntactically valid and semantically equivalent (mirrors m1-fmt's guarantee).

use m1_core::{Cst, Kind, Node};

use crate::registry::Registry;

/// A single text replacement over a byte range of the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub byte_range: std::ops::Range<usize>,
    pub replacement: String,
}

/// Why a fix was abandoned.
#[derive(Debug)]
pub enum FixError {
    /// The fixed buffer introduced new syntax errors.
    NewSyntaxErrors,
    /// The fixed buffer changed the semantic token sequence beyond the
    /// sanctioned operator substitutions.
    TokensChanged,
}

impl std::fmt::Display for FixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixError::NewSyntaxErrors => write!(f, "fix would introduce syntax errors"),
            FixError::TokensChanged => write!(f, "fix would change program semantics"),
        }
    }
}

/// Applies enabled rules' fixes to a source buffer.
pub struct Fixer<'a> {
    registry: &'a Registry,
}

impl<'a> Fixer<'a> {
    pub fn new(registry: &'a Registry) -> Self {
        Self { registry }
    }

    /// Returns `Ok(Some(fixed))` if any safe edit applied, `Ok(None)` if there
    /// was nothing to fix, or `Err` if **no** edit can be applied safely.
    ///
    /// The whole batch is validated first (the fast common path). If that batch
    /// would introduce syntax errors or change the token stream, the batch is
    /// *not* discarded wholesale — that would throw away every independent safe
    /// fix because of one bad edit (#75/#76). Instead we fall back to applying
    /// edits incrementally, keeping only those that individually preserve the
    /// safety invariant and dropping just the offending one(s). `Err` is
    /// returned only when not a single edit survives.
    pub fn fix_source(&self, source: &str) -> Result<Option<String>, FixError> {
        let before = m1_core::parse(source);
        let lines: Vec<&str> = source.split('\n').collect();

        let mut edits: Vec<Edit> = Vec::new();
        for rule in self.registry.rules() {
            rule.fix_file(source, &lines, &mut edits);
        }
        let root = before.root();
        collect_node_edits(self.registry, &root, source, &mut edits);
        // Syntax repair: insert any statement-terminating `;` the parser had to
        // synthesise as a zero-width MISSING node. This is always safe — it only
        // makes the grammar-required token explicit, reducing the syntax-error
        // count — so it is applied independently of the lint rules.
        repair_missing_semicolons(&root, &mut edits);

        if edits.is_empty() {
            return Ok(None);
        }

        // Fast path: try the whole batch at once.
        let candidate = apply_edits(source, edits.clone());
        if is_safe(&before, &candidate) {
            return Ok(Some(candidate));
        }

        // Fallback: an edit in the batch is unsafe. Rather than discard every
        // safe fix with it, apply edits one at a time, keeping an edit only when
        // it (together with those already accepted) stays safe. This salvages
        // every independent safe fix and drops only the genuinely unsafe ones.
        let kept = safe_edit_subset(source, &before, edits);
        if kept.is_empty() {
            // Surface the reason the offending edit was rejected.
            let after = m1_core::parse(&candidate);
            if after.syntax_diagnostics().len() > before.syntax_diagnostics().len() {
                return Err(FixError::NewSyntaxErrors);
            }
            return Err(FixError::TokensChanged);
        }
        Ok(Some(apply_edits(source, kept)))
    }
}

/// Whether `candidate` is a safe rewrite of the `before` parse: no new syntax
/// errors and a token stream equivalent (modulo sanctioned operator rewrites),
/// or — for paren-inserting fixes like L024, which add tokens — a parse tree
/// equivalent modulo redundant paren wrappers.
fn is_safe(before: &Cst, candidate: &str) -> bool {
    let after = m1_core::parse(candidate);
    after.syntax_diagnostics().len() <= before.syntax_diagnostics().len()
        && (tokens_equivalent(before, &after) || trees_equivalent_modulo_parens(before, &after))
}

/// Whether `after` is the `before` parse with zero or more *redundant* paren
/// wrappers inserted: the trees are identical except that a
/// `ParenthesizedExpression` in `after` with no counterpart in `before` is
/// transparent. Wrapping a complete subexpression node (what L024's fix does)
/// passes; a paren insertion that re-associates anything — `a + b * c` →
/// `(a + b) * c` — produces a structurally different tree and is rejected.
/// Leaf comparison accepts the same sanctioned operator rewrites and
/// MISSING-token fills as [`tokens_equivalent`], so a mixed fix batch (e.g.
/// L004 + L024) still validates as a whole.
fn trees_equivalent_modulo_parens(before: &Cst, after: &Cst) -> bool {
    // Iterative pairwise compare (explicit worklist): this runs on every fix
    // validation, and a recursive compare overflows the stack on the same
    // deeply nested input the iterative rule walks were hardened for (#128).
    let mut stack: Vec<(Node, Node)> = vec![(before.root(), after.root())];
    while let Some((a, b)) = stack.pop() {
        // A paren wrapper only present on the after side is transparent.
        if b.kind() == Kind::ParenthesizedExpression && a.kind() != Kind::ParenthesizedExpression {
            let inner: Vec<Node> = b
                .children()
                .into_iter()
                .filter(|c| !matches!(c.kind(), Kind::LParen | Kind::RParen))
                .collect();
            if inner.len() != 1 {
                return false;
            }
            stack.push((a, inner[0]));
            continue;
        }
        let (ka, kb) = (a.children(), b.children());
        if ka.is_empty() && kb.is_empty() {
            let leaf_ok = (a.kind() == b.kind() && a.text() == b.text())
                || sanctioned(a.text(), b.text())
                || (a.kind() == b.kind() && a.is_missing() && !b.is_missing());
            if !leaf_ok {
                return false;
            }
            continue;
        }
        if a.kind() != b.kind() || ka.len() != kb.len() {
            return false;
        }
        stack.extend(ka.into_iter().zip(kb));
    }
    true
}

/// Greedily select the largest prefix-stable subset of `edits` that keeps the
/// rewrite safe. Edits are considered in source order (matching `apply_edits`'s
/// overlap handling); each is accepted only if applying the running set stays
/// safe, so one unsafe edit no longer poisons the independent safe ones (#75).
fn safe_edit_subset(source: &str, before: &Cst, mut edits: Vec<Edit>) -> Vec<Edit> {
    edits.sort_by_key(|e| e.byte_range.start);
    edits.dedup();
    // Track accepted *indices* and apply trials by reference: cloning the
    // accepted Vec<Edit> per candidate made the selection O(N²) in bytes
    // cloned on exactly the files that hit this fallback hardest (#129).
    let mut kept: Vec<usize> = Vec::new();
    for i in 0..edits.len() {
        let trial: Vec<&Edit> = kept
            .iter()
            .map(|&j| &edits[j])
            .chain(std::iter::once(&edits[i]))
            .collect();
        if is_safe(before, &apply_edit_refs(source, &trial)) {
            kept.push(i);
        }
        // else: this edit (with the accepted set) is unsafe — drop just it.
    }
    let mut keep = vec![false; edits.len()];
    for &i in &kept {
        keep[i] = true;
    }
    edits
        .into_iter()
        .zip(keep)
        .filter_map(|(e, k)| k.then_some(e))
        .collect()
}

/// Walk the tree and emit an insert-`;` edit at every zero-width MISSING
/// semicolon node (a statement the parser recovered as missing its terminator).
///
/// Iterative (explicit stack), matching `runner::walk`: a deeply nested
/// expression chain must not crash `--fix` when plain linting survives it —
/// worse here, since `--fix` is the destructive mode (#128).
fn repair_missing_semicolons(root: &Node, edits: &mut Vec<Edit>) {
    let mut stack: Vec<Node> = vec![*root];
    while let Some(node) = stack.pop() {
        if node.is_missing() && node.kind() == Kind::Semicolon {
            let at = node.byte_range().start;
            edits.push(Edit {
                byte_range: at..at,
                replacement: ";".into(),
            });
            // A MISSING node is a zero-width leaf; nothing lies beneath it.
            continue;
        }
        stack.extend(node.children().into_iter().rev());
    }
}

/// Run every rule's `fix_node` over the tree, pre-order. Iterative for the same
/// reason as [`repair_missing_semicolons`] (#128).
fn collect_node_edits(reg: &Registry, root: &Node, source: &str, edits: &mut Vec<Edit>) {
    let mut stack: Vec<Node> = vec![*root];
    while let Some(node) = stack.pop() {
        for rule in reg.rules() {
            rule.fix_node(&node, source, edits);
        }
        stack.extend(node.children().into_iter().rev());
    }
}

/// Apply edits right-to-left after dropping any that overlap an earlier one.
pub fn apply_edits(source: &str, edits: Vec<Edit>) -> String {
    let refs: Vec<&Edit> = edits.iter().collect();
    apply_edit_refs(source, &refs)
}

/// The borrowed counterpart of [`apply_edits`], so `safe_edit_subset`'s trial
/// applications need not clone the accepted edit set each round (#129).
fn apply_edit_refs(source: &str, edits: &[&Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.to_vec();
    sorted.sort_by_key(|e| e.byte_range.start);
    let mut kept: Vec<&Edit> = Vec::new();
    let mut last_end = 0usize;
    for e in sorted {
        if e.byte_range.start >= last_end {
            last_end = e.byte_range.end;
            kept.push(e);
        }
        // else: overlapping edit skipped; a later --fix run handles it.
    }
    let mut out = source.to_string();
    for e in kept.into_iter().rev() {
        out.replace_range(e.byte_range.clone(), &e.replacement);
    }
    out
}

/// Sanctioned operator rewrites that `--fix` is allowed to make.
fn sanctioned(a: &str, b: &str) -> bool {
    matches!(
        (a, b),
        ("==", "eq") | ("!=", "neq") | ("&&", "and") | ("||", "or") | ("!", "not")
    )
}

/// A non-trivia leaf token: its kind, text, and whether it is a zero-width
/// MISSING node the parser inserted during error recovery.
struct Tok {
    kind: Kind,
    text: String,
    missing: bool,
}

/// Non-trivia leaf tokens in source order. Iterative for the same reason as
/// the fixer walks: token collection runs on every fix validation and must not
/// overflow on adversarial nesting (#128).
fn semantic_tokens(cst: &Cst) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut stack: Vec<Node> = vec![cst.root()];
    while let Some(node) = stack.pop() {
        let children = node.children();
        if children.is_empty() {
            match node.kind() {
                Kind::LineComment | Kind::BlockComment => {}
                k => out.push(Tok {
                    kind: k,
                    text: node.text().to_string(),
                    missing: node.is_missing(),
                }),
            }
            continue;
        }
        // Reverse push preserves source order on pop.
        stack.extend(children.into_iter().rev());
    }
    out
}

fn tokens_equivalent(before: &Cst, after: &Cst) -> bool {
    let a = semantic_tokens(before);
    let b = semantic_tokens(after);
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        (x.kind == y.kind && x.text == y.text)
            || sanctioned(&x.text, &y.text)
            // Filling a MISSING token (e.g. inserting the required `;`) with the
            // present token of the same kind is a safe repair: the grammar already
            // required that token, so making it explicit changes no meaning.
            || (x.missing && !y.missing && x.kind == y.kind)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;
    use crate::rules::Rule;

    /// A test-only rule that emits a deliberately *unsafe* edit: it renames an
    /// identifier `a` to `zzz`, which changes the token stream (not a sanctioned
    /// rewrite). Used to prove the fixer salvages other rules' safe edits when
    /// one rule's edit must be rejected (#75).
    struct UnsafeRenameA;
    impl Rule for UnsafeRenameA {
        fn code(&self) -> LintCode {
            LintCode::L004
        }
        fn name(&self) -> &'static str {
            "test-unsafe-rename"
        }
        fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<Edit>) {
            if !node.children().is_empty() {
                return;
            }
            if node.text() == "a" {
                let _ = source;
                edits.push(Edit {
                    byte_range: node.byte_range(),
                    replacement: "zzz".into(),
                });
            }
        }
    }

    #[test]
    fn unsafe_edit_does_not_discard_independent_safe_fixes() {
        // One file with: a token-changing edit (rename `a`->`zzz`, unsafe) on
        // line 2, plus an independent safe fix (L002 trailing whitespace) on
        // line 1. The unsafe edit must be dropped while the safe fix is kept,
        // rather than the whole batch being discarded (#75).
        let mut r = Registry::empty();
        r.register(Box::new(
            crate::rules::l002_trailing_whitespace::TrailingWhitespace,
        ));
        r.register(Box::new(UnsafeRenameA));
        let fixer = Fixer::new(&r);

        let src = "Result = 1;   \nValue = a + b;\n";
        let out = fixer.fix_source(src).unwrap();
        // The L002 trailing whitespace on line 1 is removed; the unsafe rename
        // is dropped, so `a` survives unchanged.
        assert_eq!(out.as_deref(), Some("Result = 1;\nValue = a + b;\n"));
    }

    #[test]
    fn glued_operator_fix_coexists_with_other_fixes() {
        // The L004 glued-operator fix (now token-safe) and an unrelated L002
        // trailing-whitespace fix must both apply in the same file (#76 + #75).
        let r = Registry::default();
        let fixer = Fixer::new(&r);
        let src = "Result = 1;   \nValue = a==b;\n";
        let out = fixer.fix_source(src).unwrap();
        assert_eq!(out.as_deref(), Some("Result = 1;\nValue = a eq b;\n"));
    }

    #[test]
    fn inserts_a_missing_semicolon() {
        // A missing statement terminator is a syntax error the parser recovers
        // from with a zero-width MISSING `;` node. `--fix` should repair it by
        // inserting the `;`, since the exact required token is unambiguous.
        let reg = Registry::default();
        let fixer = Fixer::new(&reg);
        let out = fixer.fix_source("x = 1\ny = 2;\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = 1;\ny = 2;\n"));
    }

    #[test]
    fn missing_semicolon_fix_converges_and_clears_the_error() {
        let runner = crate::runner::Runner::new(Registry::default());
        let fixed = runner
            .fix_source_stable("Result = 1\n")
            .unwrap()
            .expect("the missing semicolon should be inserted");
        assert_eq!(fixed, "Result = 1;\n");
        // The repaired source has no remaining syntax errors.
        assert!(m1_core::parse(&fixed).syntax_diagnostics().is_empty());
        // And it is a fixed point.
        assert_eq!(runner.fix_source_stable(&fixed).unwrap(), None);
    }

    #[test]
    fn deeply_nested_expression_does_not_overflow_fix() {
        // The runner's walk has been iterative since the stack-overflow DoS
        // fix, but the fixer's walks stayed recursive — so a file that linted
        // fine crashed `--fix` (#128). Both fixer walks must survive the same
        // adversarial depth the runner test uses.
        let reg = Registry::default();
        let fixer = Fixer::new(&reg);
        let source = format!("Out = {}1;\n", "1+".repeat(50_000));
        // Must return (fix or no fix), not abort with SIGSEGV/SIGABRT.
        let _ = fixer.fix_source(&source);
    }

    #[test]
    fn safe_edit_subset_keeps_source_order_and_drops_unsafe() {
        // Three candidate edits, the middle one unsafe: the subset keeps the
        // outer two in source order. (Guards the index-tracking rewrite, #129.)
        let src = "Result = 1;   \nValue = a + b;   \n";
        let before = m1_core::parse(src);
        let ws1 = src.find("   \n").unwrap();
        let ws2 = src.rfind("   \n").unwrap();
        let edits = vec![
            Edit {
                byte_range: ws1..ws1 + 3,
                replacement: String::new(),
            },
            Edit {
                // Renames `a` -> `zzz`: token change, unsafe.
                byte_range: src.find('a').unwrap()..src.find('a').unwrap() + 1,
                replacement: "zzz".into(),
            },
            Edit {
                byte_range: ws2..ws2 + 3,
                replacement: String::new(),
            },
        ];
        let kept = safe_edit_subset(src, &before, edits);
        assert_eq!(kept.len(), 2);
        assert!(
            kept.windows(2)
                .all(|w| w[0].byte_range.start <= w[1].byte_range.start)
        );
        assert_eq!(apply_edits(src, kept), "Result = 1;\nValue = a + b;\n");
    }

    #[test]
    fn apply_edits_right_to_left() {
        let edits = vec![
            Edit {
                byte_range: 0..1,
                replacement: "X".into(),
            },
            Edit {
                byte_range: 4..5,
                replacement: "Y".into(),
            },
        ];
        assert_eq!(apply_edits("abcde", edits), "Xbcde".replacen("e", "Y", 1));
    }

    #[test]
    fn overlapping_edit_dropped() {
        let edits = vec![
            Edit {
                byte_range: 0..3,
                replacement: "XY".into(),
            },
            Edit {
                byte_range: 2..4,
                replacement: "ZZ".into(),
            },
        ];
        // Second overlaps the first; only the first applies.
        assert_eq!(apply_edits("abcd", edits), "XYd");
    }
}

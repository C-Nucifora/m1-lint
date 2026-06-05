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
/// errors and a token stream equivalent (modulo sanctioned operator rewrites).
fn is_safe(before: &Cst, candidate: &str) -> bool {
    let after = m1_core::parse(candidate);
    after.syntax_diagnostics().len() <= before.syntax_diagnostics().len()
        && tokens_equivalent(before, &after)
}

/// Greedily select the largest prefix-stable subset of `edits` that keeps the
/// rewrite safe. Edits are considered in source order (matching `apply_edits`'s
/// overlap handling); each is accepted only if applying the running set stays
/// safe, so one unsafe edit no longer poisons the independent safe ones (#75).
fn safe_edit_subset(source: &str, before: &Cst, mut edits: Vec<Edit>) -> Vec<Edit> {
    edits.sort_by_key(|e| e.byte_range.start);
    edits.dedup();
    let mut kept: Vec<Edit> = Vec::new();
    for edit in edits {
        let mut trial = kept.clone();
        trial.push(edit.clone());
        if is_safe(before, &apply_edits(source, trial.clone())) {
            kept = trial;
        }
        // else: this edit (with the accepted set) is unsafe — drop just it.
    }
    kept
}

fn collect_node_edits(reg: &Registry, node: &Node, source: &str, edits: &mut Vec<Edit>) {
    for rule in reg.rules() {
        rule.fix_node(node, source, edits);
    }
    for child in node.children() {
        collect_node_edits(reg, &child, source, edits);
    }
}

/// Apply edits right-to-left after dropping any that overlap an earlier one.
pub fn apply_edits(source: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|e| e.byte_range.start);
    let mut kept: Vec<Edit> = Vec::new();
    let mut last_end = 0usize;
    for e in edits {
        if e.byte_range.start >= last_end {
            last_end = e.byte_range.end;
            kept.push(e);
        }
        // else: overlapping edit skipped; a later --fix run handles it.
    }
    let mut out = source.to_string();
    for e in kept.into_iter().rev() {
        out.replace_range(e.byte_range, &e.replacement);
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

/// Non-trivia leaf tokens as `(Kind, text)` in source order.
fn semantic_tokens(cst: &Cst) -> Vec<(Kind, String)> {
    let mut out = Vec::new();
    collect_tokens(&cst.root(), &mut out);
    out
}

fn collect_tokens(node: &Node, out: &mut Vec<(Kind, String)>) {
    let children = node.children();
    if children.is_empty() {
        match node.kind() {
            Kind::LineComment | Kind::BlockComment => {}
            k => out.push((k, node.text().to_string())),
        }
        return;
    }
    for c in children {
        collect_tokens(&c, out);
    }
}

fn tokens_equivalent(before: &Cst, after: &Cst) -> bool {
    let a = semantic_tokens(before);
    let b = semantic_tokens(after);
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (x.0 == y.0 && x.1 == y.1) || sanctioned(&x.1, &y.1))
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

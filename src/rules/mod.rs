//! Lint rules.
//!
//! Each rule is a zero-sized struct implementing [`Rule`].

use crate::diagnostic::LintDiagnostic;

pub mod l001_line_too_long;
pub mod l002_trailing_whitespace;
pub mod l003_missing_final_newline;
pub mod l004_eq_operator_preferred;
pub mod l005_logical_operator_preferred;
pub mod l006_float_eq_comparison;
pub mod l007_operator_spacing;
pub mod l008_nesting_too_deep;
pub mod l009_cyclomatic_complexity;
pub mod l010_tab_indentation;
pub mod l011_comment_style;
pub mod l012_unused_local;
pub mod l014_expand_undefined_variable;
pub mod l015_local_missing_initializer;
pub mod l016_local_variable_naming;
pub mod l017_magic_number;
pub mod l018_semicolon_spacing;
pub mod l019_cognitive_complexity;
pub mod l020_object_naming;
pub mod l021_one_statement_per_line;
pub mod l022_keyword_paren_spacing;
pub mod l023_call_paren_spacing;
pub mod l024_ternary_condition_parens;
pub mod l025_local_scope;
pub mod l026_top_level_indentation;
pub mod l027_file_final_blank_line;
pub mod l028_brace_style;

/// Build an [`Edit`][crate::fix::Edit] that replaces a symbolic binary operator
/// (`==`/`!=`/`&&`/`||`) with its keyword form (`eq`/`neq`/`and`/`or`),
/// inserting a separating space on any side where the operand is glued.
///
/// A bare keyword glued to an operand (`a==b` -> `aeqb`) merges into a single
/// identifier — a semantics change the fixer rejects, so the glued form would
/// stay permanently un-fixable. Padding the replacement keeps the token stream
/// equivalent (`a eq b`) so the fix actually applies (#76). A side that already
/// has whitespace (or sits at a file boundary) gets no extra space.
pub(crate) fn keyword_operator_edit(
    source: &str,
    byte_range: std::ops::Range<usize>,
    keyword: &str,
) -> crate::fix::Edit {
    let bytes = source.as_bytes();
    let glued = |b: Option<u8>| matches!(b, Some(c) if !c.is_ascii_whitespace());
    let before = glued(byte_range.start.checked_sub(1).map(|i| bytes[i]));
    let after = glued(bytes.get(byte_range.end).copied());
    let mut replacement = String::with_capacity(keyword.len() + 2);
    if before {
        replacement.push(' ');
    }
    replacement.push_str(keyword);
    if after {
        replacement.push(' ');
    }
    crate::fix::Edit {
        byte_range,
        replacement,
    }
}

/// Whether `kind` opens a complexity scope: the top-level [`SourceFile`] and
/// each [`WhenStatement`] are scored independently for both the cyclomatic
/// (L009) and cognitive (L019) complexity rules.
///
/// Both rules depend on this single definition so their notion of "a scope"
/// stays in lock-step: L019 explicitly mirrors L009's scope model (a nested
/// `when` is its own scope, not counted toward its parent). If the grammar ever
/// adds another scope construct (e.g. a user-function body), editing this one
/// predicate updates both rules together — a one-sided change can no longer make
/// the two complexity rules silently disagree about what a scope is.
///
/// This lives here rather than in `m1-core::kind_pred` because `kind_pred` is
/// for operator-token groupings shared across m1-fmt/m1-typecheck; `when`/scope
/// is a structural-node concept currently private to m1-lint's complexity rules.
///
/// [`SourceFile`]: m1_core::Kind::SourceFile
/// [`WhenStatement`]: m1_core::Kind::WhenStatement
pub(crate) fn is_complexity_scope(kind: m1_core::Kind) -> bool {
    matches!(
        kind,
        m1_core::Kind::WhenStatement | m1_core::Kind::SourceFile
    )
}

/// A lint rule.
///
/// Rules implement one or both of [`check_file`][Rule::check_file] and
/// [`check_node`][Rule::check_node]. The default implementations are no-ops so
/// each rule only needs to override what it uses.
pub trait Rule: Send + Sync {
    /// The machine-readable code for this rule, e.g. `LintCode::L001`.
    fn code(&self) -> crate::diagnostic::LintCode;

    /// A short human-readable name, e.g. `"line-too-long"`.
    fn name(&self) -> &'static str;

    /// Called once per file before the CST walk.
    ///
    /// `source` is the raw file contents. `lines` is the source split on `\n`;
    /// each element has the trailing newline stripped.
    fn check_file(&self, source: &str, lines: &[&str], diags: &mut Vec<LintDiagnostic>) {
        let _ = (source, lines, diags);
    }

    /// Called once per file with the already-parsed CST.
    ///
    /// This is the CST-aware counterpart of [`check_file`][Rule::check_file]: a
    /// file-scope pass that needs the parse tree (e.g. a two-phase analysis that
    /// first collects declarations across the whole file, then revisits uses).
    /// The runner parses the source exactly once and hands the same [`Cst`] here,
    /// so a rule must never re-`parse` the source itself. `source` is the raw file
    /// contents and `lines` is the source split on `\n` (trailing newline
    /// stripped), matching [`check_file`][Rule::check_file].
    ///
    /// [`Cst`]: m1_core::Cst
    fn check_file_cst(
        &self,
        cst: &m1_core::Cst,
        source: &str,
        lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {
        let _ = (cst, source, lines, diags);
    }

    /// Called for every node in the CST (depth-first, pre-order).
    fn check_node(&self, node: &m1_core::Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        let _ = (node, source, diags);
    }

    /// Emit autofix edits for this node. Default: no fix.
    fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        let _ = (node, source, edits);
    }

    /// Emit autofix edits at file scope. Default: no fix.
    fn fix_file(&self, source: &str, lines: &[&str], edits: &mut Vec<crate::fix::Edit>) {
        let _ = (source, lines, edits);
    }
}

#[cfg(test)]
mod tests {
    use super::is_complexity_scope;
    use m1_core::Kind;

    #[test]
    fn complexity_scope_covers_source_file_and_when() {
        // The two scope constructs L009 and L019 score independently.
        assert!(is_complexity_scope(Kind::SourceFile));
        assert!(is_complexity_scope(Kind::WhenStatement));
    }

    #[test]
    fn complexity_scope_excludes_non_scope_constructs() {
        // Control-flow that lives *inside* a scope is not itself a scope.
        assert!(!is_complexity_scope(Kind::IfStatement));
        assert!(!is_complexity_scope(Kind::ElseClause));
        assert!(!is_complexity_scope(Kind::ExpandStatement));
        assert!(!is_complexity_scope(Kind::IsClause));
        assert!(!is_complexity_scope(Kind::Block));
    }
}

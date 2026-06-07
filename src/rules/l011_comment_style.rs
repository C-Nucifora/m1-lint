//! L011 — comment-style
//!
//! Flags line comments with no space after `//` (e.g. `//foo`).

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L011 — flags `//foo` (missing space after `//`).
pub struct CommentStyle;

/// True if `text` is a line comment needing a space after `//`.
fn needs_space(text: &str) -> bool {
    // On a CRLF file the LineComment token includes the trailing `\r` (`//\r`).
    // That `\r` is a line-ending artifact, not comment content; without stripping
    // it, a bare `//\r` looks like `//` + a non-space char and L011 would insert a
    // space that L002 then strips — so `--fix` oscillates and never converges (#82).
    let text = text.strip_suffix('\r').unwrap_or(text);
    let bytes = text.as_bytes();
    if !text.starts_with("//") {
        return false;
    }
    match bytes.get(2) {
        None => false,       // bare `//`
        Some(b'/') => false, // `///`, separators like `////`
        Some(b' ') | Some(b'\t') => false,
        Some(_) => true,
    }
}

impl Rule for CommentStyle {
    fn code(&self) -> LintCode {
        LintCode::L011
    }
    fn name(&self) -> &'static str {
        "comment-style"
    }

    fn check_node(&self, node: &Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::LineComment || !needs_space(node.text()) {
            return;
        }
        diags.push(LintDiagnostic::new(
            LintCode::L011,
            node.range(),
            node.byte_range(),
            Severity::Warning,
            "add a space after `//`".to_string(),
        ));
    }

    fn fix_node(&self, node: &m1_core::Node, _source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if node.kind() != Kind::LineComment || !needs_space(node.text()) {
            return;
        }
        // Insert one space just after the `//`.
        let at = node.byte_range().start + 2;
        edits.push(crate::fix::Edit {
            byte_range: at..at,
            replacement: " ".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn runner() -> Runner {
        let mut r = Registry::empty();
        r.register(Box::new(CommentStyle));
        Runner::new(r)
    }

    #[test]
    fn flags_missing_space() {
        let result = runner().run_source("//hello\nx = 1;\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L011);
    }

    #[test]
    fn no_diagnostic_with_space_or_separator() {
        let result = runner().run_source("// good\n//// sep\nx = 1;\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn fixes_missing_space() {
        let mut r = Registry::empty();
        r.register(Box::new(CommentStyle));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("//hello\nx = 1;\n").unwrap();
        assert_eq!(out.as_deref(), Some("// hello\nx = 1;\n"));
    }

    #[test]
    fn bare_comment_with_crlf_does_not_need_a_space() {
        // #82: on a CRLF file the LineComment token includes the trailing `\r`
        // (`//\r`). That `\r` is a line-ending artifact, not comment content, so
        // it must not be treated as "missing space after //" — otherwise L011
        // re-inserts a space that L002 strips, and `--fix` oscillates forever.
        assert!(!needs_space("//\r"), "bare `//` + CR needs no space");
        assert!(needs_space("//x\r"), "real content still needs a space");
    }

    #[test]
    fn fix_converges_on_crlf_trailing_space_comment() {
        // End-to-end with the default ruleset: `// \r\n` must reach a fixed point
        // (`//\r\n`) and stay there, not bounce between L002 and L011 (#82).
        let runner = Runner::new(crate::registry::Registry::default());
        let fixed = runner
            .fix_source_stable("// \r\nValue = 1;\r\n")
            .unwrap()
            .expect("the trailing space should be fixed");
        // No trailing whitespace remains, and the CRLF endings are preserved.
        assert!(
            !fixed.contains(" \r"),
            "trailing space before CRLF should be gone: {fixed:?}"
        );
        assert!(fixed.contains("//\r\n"), "got: {fixed:?}");
        // Re-running the fixer finds nothing more to do (true fixed point).
        assert_eq!(
            runner.fix_source_stable(&fixed).unwrap(),
            None,
            "fixer must have converged: {fixed:?}"
        );
    }
}

//! L028 — brace-style
//!
//! The M1 Build Development Manual's Code Layout section (p.65) mandates **Allman**
//! braces: *"Align braces with keyword"* and *"Use a separate line for each
//! brace."* `m1-fmt` already reformats to this (brace_style defaults to Allman,
//! K&R configurable); this rule is the lint-side gate so a project that runs
//! `m1-lint` but not `m1-fmt` still gets brace-placement feedback.
//!
//! The opening `{` is the style differentiator (mirroring how `m1-fmt` emits it):
//! - **Allman** (default, manual): the `{` sits on its **own line**; flag a `{`
//!   glued to the construct line (`if (a) {`).
//! - **K&R** (`brace-style = "kr"`): the `{` is **attached** to the construct
//!   line; flag a `{` that was given its own line.
//!
//! The closing `}` is on its own line in *both* styles, so a `}` sharing its line
//! with code is flagged regardless of style.
//!
//! Only the brace's *line placement* is judged — column alignment is indentation
//! (L010/L026), and the actual reformat is `m1-fmt`'s job (this rule is not
//! `--fix`able; it flags, `m1-fmt` fixes).

use crate::config::BraceStyle;
use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L028 — flags braces whose line placement does not match the configured
/// [`BraceStyle`] (default: Allman, per the manual).
pub struct BraceStylePlacement {
    pub style: BraceStyle,
}

/// Whether the brace at `offset` is the first non-whitespace on its source line.
fn on_own_line(source: &str, offset: usize) -> bool {
    let line_start = source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[line_start..offset]
        .bytes()
        .all(|b| b == b' ' || b == b'\t')
}

/// Point [`Range`][m1_core::Range] at the brace byte offset.
fn point(source: &str, offset: usize) -> m1_core::Range {
    let pos = m1_core::byte_to_position(source, offset);
    m1_core::Range {
        start: pos,
        end: pos,
    }
}

impl Rule for BraceStylePlacement {
    fn code(&self) -> LintCode {
        LintCode::L028
    }
    fn name(&self) -> &'static str {
        "brace-style"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::Block {
            return;
        }
        let kids = node.children();
        let lbrace = kids.iter().find(|c| c.kind() == Kind::LBrace);
        let rbrace = kids.iter().find(|c| c.kind() == Kind::RBrace);

        if let Some(lb) = lbrace {
            let off = lb.byte_range().start;
            let own = on_own_line(source, off);
            let bad = match self.style {
                // Allman wants the `{` on its own line; flag it when glued.
                BraceStyle::Allman => !own,
                // K&R wants the `{` attached; flag it when given its own line.
                BraceStyle::Kr => own,
            };
            if bad {
                let msg = match self.style {
                    BraceStyle::Allman => {
                        "opening brace `{` must be on its own line (Allman style)"
                    }
                    BraceStyle::Kr => {
                        "opening brace `{` must be on the same line as the statement (K&R style)"
                    }
                };
                diags.push(LintDiagnostic::new(
                    LintCode::L028,
                    point(source, off),
                    off..off,
                    Severity::Warning,
                    msg.to_string(),
                ));
            }
        }

        // The closing `}` is on its own line in both styles.
        if let Some(rb) = rbrace {
            let off = rb.byte_range().start;
            if !on_own_line(source, off) {
                diags.push(LintDiagnostic::new(
                    LintCode::L028,
                    point(source, off),
                    off..off,
                    Severity::Warning,
                    "closing brace `}` must be on its own line".to_string(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn count_with(style: BraceStyle, src: &str) -> usize {
        let mut r = Registry::empty();
        r.register(Box::new(BraceStylePlacement { style }));
        Runner::new(r)
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L028)
            .count()
    }

    // Allman (default): braces each on their own line pass; the K&R `) {` form is
    // flagged.
    #[test]
    fn allman_accepts_allman_braces() {
        assert_eq!(
            count_with(BraceStyle::Allman, "if (a)\n{\n\tx = 1;\n}\n"),
            0
        );
    }

    #[test]
    fn allman_flags_kr_opening_brace() {
        // `{` glued to the `if` line — one flag (the opening brace).
        assert_eq!(count_with(BraceStyle::Allman, "if (a) {\n\tx = 1;\n}\n"), 1);
    }

    #[test]
    fn allman_flags_inline_block_both_braces() {
        // `{` attached and `}` sharing the line → both flagged.
        assert_eq!(count_with(BraceStyle::Allman, "if (a) { x = 1; }\n"), 2);
    }

    // K&R (opt-in via config): the inverse — attached `{` passes, an own-line `{`
    // is flagged.
    #[test]
    fn kr_accepts_attached_brace() {
        assert_eq!(count_with(BraceStyle::Kr, "if (a) {\n\tx = 1;\n}\n"), 0);
    }

    #[test]
    fn kr_flags_allman_opening_brace() {
        assert_eq!(count_with(BraceStyle::Kr, "if (a)\n{\n\tx = 1;\n}\n"), 1);
    }

    // The closing `}` must be on its own line in both styles.
    #[test]
    fn closing_brace_must_be_alone_in_both_styles() {
        assert_eq!(count_with(BraceStyle::Allman, "if (a)\n{\n\tx = 1; }\n"), 1);
        assert_eq!(count_with(BraceStyle::Kr, "if (a) {\n\tx = 1; }\n"), 1);
    }

    // Nested blocks each get judged.
    #[test]
    fn nested_blocks_are_each_checked() {
        let src = "when (Mode)\n{\n\tis (Off) {\n\t\tx = 1;\n\t}\n}\n";
        // The inner `is (Off) {` has a K&R opening brace under Allman → 1 flag.
        assert_eq!(count_with(BraceStyle::Allman, src), 1);
    }
}

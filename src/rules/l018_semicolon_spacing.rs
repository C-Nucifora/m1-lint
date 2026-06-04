//! L018 — semicolon-spacing
//!
//! The M1 manual's *Code Layout and Format* section mandates: "Do not put
//! spaces in front of semicolons." `m1-fmt` already removes such a gap; this
//! rule flags it for lint-only CI gates, and `--fix` deletes the gap (mirroring
//! L007's token-adjacency approach). Default-on, like the other unconditional
//! manual mandates — the false-positive risk is near zero. (#70)

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L018 — flags a `;` preceded by same-line whitespace (e.g. `A = 1 ;`).
pub struct SemicolonSpacing;

/// The byte offset where the run of same-line whitespace immediately before
/// `semi_start` begins, or `None` if the preceding byte is not horizontal
/// whitespace or the whitespace run is not preceded on the same line by a
/// non-whitespace character.
///
/// We require a same-line non-whitespace predecessor so a `;` sitting alone on
/// an indented line (its leading indentation) is never mistaken for a violation.
fn gap_start(source: &[u8], semi_start: usize) -> Option<usize> {
    if semi_start == 0 {
        return None;
    }
    let prev = source[semi_start - 1];
    if prev != b' ' && prev != b'\t' {
        return None;
    }
    // Walk back over the horizontal-whitespace run.
    let mut i = semi_start;
    while i > 0 && (source[i - 1] == b' ' || source[i - 1] == b'\t') {
        i -= 1;
    }
    // `i` is the first byte of the whitespace run. If it is the start of the
    // file or sits right after a newline, the whitespace is leading indentation,
    // not a gap before the semicolon — don't flag.
    if i == 0 || source[i - 1] == b'\n' {
        return None;
    }
    Some(i)
}

impl Rule for SemicolonSpacing {
    fn code(&self) -> LintCode {
        LintCode::L018
    }
    fn name(&self) -> &'static str {
        "semicolon-spacing"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        if node.kind() != Kind::Semicolon {
            return;
        }
        let br = node.byte_range();
        if let Some(gap) = gap_start(source.as_bytes(), br.start) {
            // Report the whitespace gap itself.
            let start = byte_to_position(source, gap);
            let end = byte_to_position(source, br.start);
            diags.push(LintDiagnostic::new(
                LintCode::L018,
                m1_core::Range { start, end },
                gap..br.start,
                Severity::Warning,
                "remove space before `;`".to_string(),
            ));
        }
    }

    fn fix_node(&self, node: &m1_core::Node, source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if node.kind() != Kind::Semicolon {
            return;
        }
        let br = node.byte_range();
        if let Some(gap) = gap_start(source.as_bytes(), br.start) {
            edits.push(crate::fix::Edit {
                byte_range: gap..br.start,
                replacement: String::new(),
            });
        }
    }
}

/// Convert a byte offset to a 0-based line/column [`m1_core::Position`].
fn byte_to_position(source: &str, byte: usize) -> m1_core::Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    m1_core::Position { line, column: col }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn runner() -> Runner {
        let mut r = Registry::empty();
        r.register(Box::new(SemicolonSpacing));
        Runner::new(r)
    }

    #[test]
    fn flags_space_before_semicolon() {
        let result = runner().run_source("A = 1 ;\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L018);
    }

    #[test]
    fn flags_tab_before_semicolon() {
        let result = runner().run_source("A = 1\t;\n");
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L018));
    }

    #[test]
    fn flags_multiple_spaces_before_semicolon() {
        let result = runner().run_source("A = 1   ;\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, LintCode::L018);
    }

    #[test]
    fn no_diagnostic_when_tight() {
        let result = runner().run_source("A = 1;\n");
        assert!(result.diagnostics.iter().all(|d| d.code != LintCode::L018));
    }

    #[test]
    fn fixes_space_before_semicolon() {
        let mut r = Registry::empty();
        r.register(Box::new(SemicolonSpacing));
        let fixer = crate::fix::Fixer::new(&r);
        let out = fixer.fix_source("A = 1 ;\n").unwrap();
        assert_eq!(out.as_deref(), Some("A = 1;\n"));
    }

    #[test]
    fn fixes_multi_space_and_tab() {
        let mut r = Registry::empty();
        r.register(Box::new(SemicolonSpacing));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(
            fixer.fix_source("A = 1  \t ;\n").unwrap().as_deref(),
            Some("A = 1;\n")
        );
    }

    #[test]
    fn matches_corpus_pattern() {
        // EV-M1 Control.Slip Control.Update.m1scr:21 ends `...0.1) ) ;`.
        let result = runner().run_source("local x = Min(Clamp(a, 0.0, 0.1) ) ;\n");
        assert!(result.diagnostics.iter().any(|d| d.code == LintCode::L018));
    }

    #[test]
    fn does_not_flag_leading_indentation_only_semicolon() {
        // A `;` whose only same-line predecessor is indentation (no code before
        // it) is not a gap-before-semicolon; don't flag it.
        let result = runner().run_source("A = 1;\n\t;\n");
        // (The lone `;` line may be a parse oddity, but L018 must not flag the
        // indentation itself.)
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| d.code != LintCode::L018 || d.inner.byte_range.start != 7),
            "leading indentation before a line-initial `;` must not be flagged"
        );
    }
}

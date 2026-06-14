//! L029 — indentation-depth
//!
//! Manual p.65 ("Code Layout and Format") lists two layout bullets about depth
//! that the character-only and column-zero rules don't cover: *"Indent
//! subsequent lines by one tab stop"* and *"Indent conditional block by one tab
//! stop."* So a nested block body must sit exactly one indentation level deeper
//! than the construct that encloses it.
//!
//! The existing indentation rules check orthogonal things and leave a gap:
//! - **L010** ([`Indentation`][crate::rules::l010_tab_indentation::Indentation])
//!   judges only the indent *character* (tab vs space), not the depth — a body
//!   indented by zero tabs or three tabs passes as long as it uses tabs.
//! - **L026**
//!   ([`TopLevelIndentation`][crate::rules::l026_top_level_indentation::TopLevelIndentation])
//!   checks only that *top-level* statements begin in column 1 (depth 0).
//!
//! Nothing verifies the per-level depth of a *nested* statement, so an
//! under-indented (`if (a)\n{\nResult = 1;\n}`) or over-indented body slips
//! through every rule. This rule closes that gap: it is the lint-side gate so a
//! project that runs `m1-lint` but not `m1-fmt` still gets depth feedback.
//!
//! **Opt-in** (off by default, like L017/L027): the real EV-M1/AV-M1 corpora
//! keep a house-style layout (under-indented Allman bodies, K&R, spaces) that
//! this rule flags en masse, so it ships off and is enabled with `--select
//! L029` rather than drowning the default output.
//!
//! The expected indent depth of a statement is the number of brace-delimited
//! bodies enclosing it — each [`Block`] ancestor plus each [`WhenStatement`]
//! ancestor (a `when`'s `{ … }` are its direct children, not a `Block`, so its
//! `is`/`else` clauses are one level in). The actual depth is the run of leading
//! indentation characters on the line where the statement *starts*. A mismatch
//! is flagged. Like L010 and L028 this is **not** `--fix`able — `m1-fmt`
//! performs the reflow; the rule only reports.
//!
//! False-positive safety (matching L010's exemptions):
//! - **Top-level (depth 0) statements are L026's job**, not flagged here.
//! - Only the line where a statement *starts* is judged; the continuation lines
//!   of a wrapped statement are exempt (their alignment is L010/manual layout,
//!   not block depth).
//! - A statement that shares its line with other code (`{ x = 1;` under K&R, or
//!   `a = 1; b = 2;`) is skipped — its leading run isn't block indentation
//!   (that's L028 / L021 territory).
//! - The indent *character* is L010's exclusive domain: if a line's indentation
//!   uses the wrong character at all (e.g. spaces under the tab style, or mixed
//!   tab+space), this rule defers to L010 rather than also reporting a depth of
//!   zero — the same physical problem reported twice is noise. A genuinely
//!   under-indented Allman body (correct character, just too few — including
//!   *zero* tabs) has no wrong character and is still flagged, which is the real
//!   gap this rule closes.
//!
//! [`Block`]: m1_core::Kind::Block
//! [`WhenStatement`]: m1_core::Kind::WhenStatement

use crate::config::IndentStyle;
use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

/// L029 — flags a nested statement whose leading indentation depth is not
/// exactly one level per enclosing brace-delimited body.
pub struct IndentationDepth {
    /// The configured indentation character; the expected/actual depth is
    /// measured in this character (shared with L010 via `cfg.indent_style`).
    pub style: IndentStyle,
}

/// The statement-like node kinds whose start line carries block indentation.
///
/// Mirrors L021's notion of a "statement": these are the direct children of a
/// [`Block`][m1_core::Kind::Block] that occupy their own line and so must be
/// indented one level deeper than the enclosing construct.
fn is_statement(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::AssignmentStatement
            | Kind::ExpressionStatement
            | Kind::LocalDeclaration
            | Kind::IfStatement
            | Kind::WhenStatement
            | Kind::ExpandStatement
    )
}

/// The expected indentation depth of `node`: the number of enclosing
/// brace-delimited bodies (one indent level per `{ … }`).
///
/// Each [`Block`][m1_core::Kind::Block] ancestor is one level (an `if`/`else`
/// body, and the body of an `is`/`else` clause). A [`WhenStatement`] adds a
/// further level even though the grammar does *not* wrap its body in a `Block`:
/// the `when`'s own `{ … }` are direct children of the `WhenStatement`, so its
/// `is`/`else` clauses sit one level in. Counting `WhenStatement` ancestors
/// alongside `Block`s makes the visual depth (`when { is { … } }` is two levels)
/// match the structural count.
fn brace_depth(node: &Node) -> usize {
    let mut depth = 0usize;
    let mut current = node.parent();
    while let Some(parent) = current {
        if matches!(parent.kind(), Kind::Block | Kind::WhenStatement) {
            depth += 1;
        }
        current = parent.parent();
    }
    depth
}

impl IndentationDepth {
    /// The character this rule measures depth in.
    fn indent_char(&self) -> char {
        match self.style {
            IndentStyle::Tab => '\t',
            IndentStyle::Spaces => ' ',
        }
    }

    /// A short word for the configured unit, for the diagnostic message.
    fn unit(&self, n: usize) -> &'static str {
        match self.style {
            IndentStyle::Tab => {
                if n == 1 {
                    "tab"
                } else {
                    "tabs"
                }
            }
            IndentStyle::Spaces => {
                if n == 1 {
                    "space"
                } else {
                    "spaces"
                }
            }
        }
    }
}

impl Rule for IndentationDepth {
    fn code(&self) -> LintCode {
        LintCode::L029
    }
    fn name(&self) -> &'static str {
        "indentation-depth"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        if !is_statement(node.kind()) || node.is_error() || node.is_missing() {
            return;
        }
        let expected = brace_depth(node);
        // Depth 0 (top-level) is L026's column-1 rule, not this one.
        if expected == 0 {
            return;
        }

        let start = node.byte_range().start;
        let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = &source[line_start..start];

        // Only judge a statement that is the first thing on its line. If other
        // code precedes it on the line (`{ x = 1;` under K&R, or `a = 1; b = 2;`),
        // the leading run isn't this statement's block indentation — that's L028
        // / L021 territory, and the run is the *previous* token's, not ours.
        if !prefix.chars().all(|c| c == ' ' || c == '\t') {
            return;
        }

        let want = self.indent_char();
        let other = match self.style {
            IndentStyle::Tab => ' ',
            IndentStyle::Spaces => '\t',
        };
        // The indent *character* is L010's exclusive domain. If this line's
        // indentation uses the wrong character at all, defer to L010 rather than
        // double-flagging the same line: a space-indented body under the tab
        // style already gets one L010 warning, and adding a depth warning that
        // counts "0 tabs" on top is pure noise (it's the same physical problem).
        // A genuinely under-indented Allman block — correct character, just too
        // few of it, including *zero* (an empty prefix) — has no wrong character
        // and is still flagged, which is the real gap this rule closes.
        if prefix.contains(other) {
            return;
        }

        // Count the leading run of the configured indent character.
        let actual = prefix.chars().take_while(|&c| c == want).count();
        if actual == expected {
            return;
        }

        let pos = m1_core::byte_to_position(source, line_start);
        let end = m1_core::byte_to_position(source, start);
        diags.push(LintDiagnostic::new(
            LintCode::L029,
            m1_core::Range { start: pos, end },
            line_start..start,
            Severity::Warning,
            format!(
                "indent this nested block by {} {} (found {}); manual p.65: \
                 indent each nested block one tab stop",
                expected,
                self.unit(expected),
                actual
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn count_with(style: IndentStyle, src: &str) -> usize {
        let mut r = Registry::empty();
        r.register(Box::new(IndentationDepth { style }));
        Runner::new(r)
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L029)
            .count()
    }

    fn count(src: &str) -> usize {
        count_with(IndentStyle::Tab, src)
    }

    // The headline finding: a body indented by zero tabs (under-indented) used
    // to pass every rule.
    #[test]
    fn under_indented_body_is_flagged() {
        assert_eq!(count("if (a)\n{\nResult = 1;\n}\n"), 1);
    }

    // The other half of the finding: a three-tab body where one is expected.
    #[test]
    fn over_indented_body_is_flagged() {
        assert_eq!(count("if (a)\n{\n\t\t\tResult = 1;\n}\n"), 1);
    }

    #[test]
    fn correctly_indented_body_is_clean() {
        assert_eq!(count("if (a)\n{\n\tResult = 1;\n}\n"), 0);
    }

    #[test]
    fn each_level_adds_one_tab() {
        // depth 1 → 1 tab, depth 2 → 2 tabs.
        let src = "if (a)\n{\n\tif (b)\n\t{\n\t\tx = 1;\n\t}\n}\n";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn nested_body_under_indented_is_flagged() {
        // The inner body sits at one tab but needs two.
        let src = "if (a)\n{\n\tif (b)\n\t{\n\tx = 1;\n\t}\n}\n";
        assert_eq!(count(src), 1);
    }

    #[test]
    fn top_level_is_not_flagged_here() {
        // Depth 0 is L026's column-1 rule; an indented top-level statement is
        // not L029's to flag (it has no Block ancestor).
        assert_eq!(count("\tlocal x = 1;\n"), 0);
        assert_eq!(count("local x = 1;\n"), 0);
    }

    #[test]
    fn when_is_clause_bodies_are_checked() {
        // A `when … is` body is two levels in: the `when`'s own braces (the
        // grammar makes them direct children of WhenStatement, not a Block) plus
        // the is-clause's Block. So a correct is-body sits at TWO tabs.
        let clean = "when (M)\n{\n\tis (Off)\n\t{\n\t\tx = 1;\n\t}\n}\n";
        assert_eq!(count(clean), 0);
        let bad = "when (M)\n{\n\tis (Off)\n\t{\n\tx = 1;\n\t}\n}\n";
        assert_eq!(count(bad), 1);
    }

    #[test]
    fn statement_sharing_its_line_is_not_flagged_here() {
        // `a = 1; b = 2;` on one line: the second statement's "indentation" is
        // really the gap after the first — L021's domain, not L029's. Only the
        // first statement (which does start the line) is judged for depth.
        let src = "if (a)\n{\n\tx = 1; y = 2;\n}\n";
        // The first statement `x = 1;` is correctly at one tab → no L029.
        assert_eq!(count(src), 0);
    }

    #[test]
    fn kr_attached_brace_body_judged_by_depth_only() {
        // Under K&R brace placement the body is still inside a Block at depth 1;
        // a correctly one-tab body is clean (brace *placement* is L028's job).
        let src = "if (a) {\n\tx = 1;\n}\n";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn continuation_lines_are_exempt() {
        // The wrapped second line of a single statement is not itself a
        // statement start, so its alignment isn't judged for block depth.
        let src = "if (a)\n{\n\tx = foo +\n\t\tbar;\n}\n";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn wrong_character_is_left_to_l010() {
        // A space-indented body under the tab style: the *character* is L010's
        // exclusive domain. L029 must NOT also flag it (the line already gets one
        // L010 warning; a "found 0 tabs" depth warning on top is the same
        // physical problem reported twice). L029 defers when the wrong character
        // appears in the indentation.
        let src = "if (a)\n{\n    x = 1;\n}\n";
        assert_eq!(count_with(IndentStyle::Tab, src), 0);
    }

    #[test]
    fn under_indented_allman_block_with_correct_character_still_flags() {
        // The real-corpus gap: an Allman block whose body sits at column 0 — no
        // wrong character at all, just zero tabs where one is expected. This is
        // exactly what L010 (tabs are the right char, there are none to fault)
        // and L026 (top-level only) both miss, and what L029 exists to catch.
        let src = "if (a)\n{\nResult = 1;\nOut = 2;\n}\n";
        assert_eq!(count(src), 2);
    }

    #[test]
    fn mixed_indentation_defers_to_l010() {
        // A tab followed by spaces under the tab style: the spaces make it a
        // wrong-character line in L010's eyes, so L029 stays out of it.
        let src = "if (a)\n{\n\t  x = 1;\n}\n";
        assert_eq!(count_with(IndentStyle::Tab, src), 0);
    }

    #[test]
    fn spaces_style_measures_in_spaces() {
        // Under the spaces style the unit is spaces; the manual's "one tab stop"
        // maps to one indent level in the configured character. We only assert
        // that depth mismatch is detected, not a particular space width — width
        // is m1-fmt's indent_width, not a lint concern.
        let clean_one_space = "if (a)\n{\n x = 1;\n}\n";
        assert_eq!(count_with(IndentStyle::Spaces, clean_one_space), 0);
        let zero_space = "if (a)\n{\nx = 1;\n}\n";
        assert_eq!(count_with(IndentStyle::Spaces, zero_space), 1);
    }

    #[test]
    fn not_fixable() {
        // Mirrors L010/L028: detect-only, m1-fmt performs the reflow.
        assert!(!LintCode::L029.fixable());
    }
}

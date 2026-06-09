//! L022 — keyword-paren-spacing
//!
//! Manual p.65: "Put a space between a keyword and a parenthesis" — `if (`,
//! `when (`, `is (`, `expand (`. Flags `if(cond)` and friends; `--fix`
//! inserts the single space.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Kind, Node, Severity};

pub struct KeywordParenSpacing;

/// The `(keyword, "(")` token pair of a construct node, when both are direct
/// children and the paren immediately follows the keyword in token order.
fn keyword_and_paren<'a>(node: &Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    let kids = node.children();
    let kw_pos = kids
        .iter()
        .position(|c| matches!(c.kind(), Kind::If | Kind::When | Kind::Is | Kind::Expand))?;
    let paren = kids.get(kw_pos + 1)?;
    if paren.kind() != Kind::LParen {
        return None;
    }
    Some((kids[kw_pos], *paren))
}

fn gap(node: &Node) -> Option<(usize, usize)> {
    let (kw, paren) = keyword_and_paren(node)?;
    let (kw_end, paren_start) = (kw.byte_range().end, paren.byte_range().start);
    if kw_end == paren_start {
        Some((kw_end, paren_start))
    } else {
        None
    }
}

impl Rule for KeywordParenSpacing {
    fn code(&self) -> LintCode {
        LintCode::L022
    }
    fn name(&self) -> &'static str {
        "keyword-paren-spacing"
    }

    fn check_node(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        if let Some((at, _)) = gap(node) {
            let pos = m1_core::byte_to_position(source, at);
            let kw = keyword_and_paren(node).map(|(k, _)| k.text().to_string());
            diags.push(LintDiagnostic::new(
                LintCode::L022,
                m1_core::Range {
                    start: pos,
                    end: pos,
                },
                at..at,
                Severity::Warning,
                format!(
                    "put a space between `{}` and `(`",
                    kw.as_deref().unwrap_or("keyword")
                ),
            ));
        }
    }

    fn fix_node(&self, node: &m1_core::Node, _source: &str, edits: &mut Vec<crate::fix::Edit>) {
        if let Some((at, _)) = gap(node) {
            edits.push(crate::fix::Edit {
                byte_range: at..at,
                replacement: " ".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn runner() -> Runner {
        let mut r = Registry::empty();
        r.register(Box::new(KeywordParenSpacing));
        Runner::new(r)
    }

    fn count(src: &str) -> usize {
        runner()
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L022)
            .count()
    }

    #[test]
    fn flags_glued_if_when_is_expand() {
        assert_eq!(count("if(a)\n{\n\tx = 1;\n}\n"), 1);
        assert_eq!(
            count("when(Mode)\n{\n\tis(Off)\n\t{\n\t\tx = 1;\n\t}\n}\n"),
            2
        );
        assert_eq!(count("expand(I = 0 to 3)\n{\n\ty = 1;\n}\n"), 1);
    }

    #[test]
    fn spaced_keywords_are_fine() {
        assert_eq!(count("if (a)\n{\n\tx = 1;\n}\n"), 0);
        assert_eq!(
            count("when (Mode)\n{\n\tis (Off)\n\t{\n\t\tx = 1;\n\t}\n}\n"),
            0
        );
    }

    #[test]
    fn fix_inserts_the_space() {
        let mut r = Registry::empty();
        r.register(Box::new(KeywordParenSpacing));
        let fixer = crate::fix::Fixer::new(&r);
        assert_eq!(
            fixer
                .fix_source("if(a)\n{\n\tx = 1;\n}\n")
                .unwrap()
                .as_deref(),
            Some("if (a)\n{\n\tx = 1;\n}\n")
        );
    }
}

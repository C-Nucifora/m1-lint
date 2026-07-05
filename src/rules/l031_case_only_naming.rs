//! L031 — case-only-naming
//!
//! The M1 Build Development Manual (p.64, Naming Conventions): "Don't
//! distinguish local variable names from other object names only by
//! uppercase/lowercase writing." Differentiating a local from a channel/object
//! by case alone breaks reference-maintenance (the manual's own warning) and
//! readability — `local pressure` alongside `Fuel.Pressure` reads as the same
//! name.
//!
//! Pure CST analysis (no project needed): collect the script's local variable
//! names and every other identifier segment it references, then flag a local
//! that matches a distinct object name case-insensitively. Opt-in (off by
//! default), like the other manual-layout rules the real corpora predate.

use crate::diagnostic::{LintCode, LintDiagnostic};
use crate::rules::Rule;
use m1_core::{Cst, Field, Kind, Severity};
use std::collections::HashSet;

pub struct CaseOnlyNaming;

impl Rule for CaseOnlyNaming {
    fn code(&self) -> LintCode {
        LintCode::L031
    }
    fn name(&self) -> &'static str {
        "case-only-naming"
    }

    fn check_file_cst(
        &self,
        cst: &Cst,
        _source: &str,
        _lines: &[&str],
        diags: &mut Vec<LintDiagnostic>,
    ) {
        // Local declarations: (name, name-node range, byte range) for the diag.
        let mut locals: Vec<(String, m1_core::Range, std::ops::Range<usize>)> = Vec::new();
        // Every identifier segment referenced anywhere in the script.
        let mut idents: HashSet<String> = HashSet::new();

        for node in cst.root().descendants() {
            match node.kind() {
                Kind::LocalDeclaration => {
                    if let Some(name) = node.child_by_field(Field::Name) {
                        locals.push((name.text().to_string(), name.range(), name.byte_range()));
                    }
                }
                Kind::Identifier => {
                    idents.insert(node.text().to_string());
                }
                _ => {}
            }
        }

        let local_set: HashSet<&str> = locals.iter().map(|(n, _, _)| n.as_str()).collect();
        // Object names = referenced identifiers that are not themselves locals.
        for (lname, range, byte_range) in &locals {
            let clash = idents.iter().find(|o| {
                o.as_str() != lname
                    && o.eq_ignore_ascii_case(lname)
                    && !local_set.contains(o.as_str())
            });
            if let Some(obj) = clash {
                diags.push(LintDiagnostic::new(
                    LintCode::L031,
                    *range,
                    byte_range.clone(),
                    Severity::Warning,
                    format!(
                        "local `{lname}` differs from object name `{obj}` only by letter case \
                         (manual p.64: do not distinguish locals from object names by case only)"
                    ),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn l031_count(src: &str) -> usize {
        // L031 is opt-in — register it directly.
        let mut r = Registry::empty();
        r.register(Box::new(super::CaseOnlyNaming));
        Runner::new(r)
            .run_source(src)
            .diagnostics
            .iter()
            .filter(|d| d.code == LintCode::L031)
            .count()
    }

    #[test]
    fn flags_local_matching_object_by_case_only() {
        // `pressure` (local) vs `Fuel.Pressure` (object leaf `Pressure`).
        assert_eq!(
            l031_count("local pressure = 1.0;\nFuel.Pressure = pressure;\n"),
            1
        );
    }

    #[test]
    fn clean_when_names_are_distinct() {
        assert_eq!(
            l031_count("local rawPressure = 1.0;\nFuel.Pressure = rawPressure;\n"),
            0
        );
    }

    #[test]
    fn clean_when_local_only_matches_itself() {
        // A local used as itself (same case) is not a case-only clash.
        assert_eq!(
            l031_count("local pressure = 1.0;\nOut.Value = pressure;\n"),
            0
        );
    }
}

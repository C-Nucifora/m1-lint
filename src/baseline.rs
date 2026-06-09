//! Baseline files — incremental adoption support (#111).
//!
//! `--write-baseline FILE` snapshots the current findings; later runs with
//! `--baseline FILE` suppress findings recorded there and report only new
//! ones, so a legacy codebase can adopt the linter without first fixing (or
//! blanket-ignoring) hundreds of pre-existing violations.
//!
//! Entries are anchored on the *trimmed content* of the offending line (plus
//! code and file path), not the line number — inserting unrelated lines above
//! a baselined finding must not resurrect it. Matching is multiset-aware: a
//! file with three identical baselined lines suppresses three findings, and a
//! fourth (new) identical one still reports.

use crate::diagnostic::LintDiagnostic;
use std::collections::HashMap;
use std::path::Path;

/// One baselined finding key: `(code, file, trimmed line content)` → count.
#[derive(Debug, Default, Clone)]
pub struct Baseline {
    entries: HashMap<(String, String, String), usize>,
}

/// The trimmed source line a diagnostic starts on.
fn line_of<'a>(source: &'a str, d: &LintDiagnostic) -> &'a str {
    source
        .split('\n')
        .nth(d.inner.range.start.line as usize)
        .unwrap_or("")
        .trim()
}

impl Baseline {
    /// Record every diagnostic of one file into the baseline.
    pub fn record(&mut self, file: &str, source: &str, diagnostics: &[LintDiagnostic]) {
        for d in diagnostics {
            let key = (
                d.code.to_string(),
                file.to_string(),
                line_of(source, d).to_string(),
            );
            *self.entries.entry(key).or_insert(0) += 1;
        }
    }

    /// Drop the diagnostics already recorded for `file`, multiset-style:
    /// each baseline entry suppresses at most its recorded count.
    pub fn filter(&self, file: &str, source: &str, diagnostics: &mut Vec<LintDiagnostic>) {
        if self.entries.is_empty() {
            return;
        }
        let mut budget = self.entries.clone();
        diagnostics.retain(|d| {
            let key = (
                d.code.to_string(),
                file.to_string(),
                line_of(source, d).to_string(),
            );
            match budget.get_mut(&key) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    false // baselined — suppress
                }
                _ => true,
            }
        });
    }

    /// Serialise to the on-disk JSON form (sorted for stable diffs).
    pub fn to_json(&self) -> String {
        let mut entries: Vec<(&(String, String, String), &usize)> = self.entries.iter().collect();
        entries.sort();
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|((code, file, line), count)| {
                serde_json::json!({
                    "code": code, "file": file, "line": line, "count": count,
                })
            })
            .collect();
        serde_json::json!({"version": 1, "entries": arr}).to_string()
    }

    /// Parse the on-disk JSON form.
    pub fn from_json(text: &str) -> Result<Baseline, String> {
        let doc: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut b = Baseline::default();
        let entries = doc
            .get("entries")
            .and_then(|e| e.as_array())
            .ok_or("baseline missing entries array")?;
        for e in entries {
            let get = |k: &str| {
                e.get(k)
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| format!("baseline entry missing {k}"))
            };
            let count = e.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            b.entries
                .insert((get("code")?, get("file")?, get("line")?), count);
        }
        Ok(b)
    }

    /// Load a baseline file from disk.
    pub fn load(path: &Path) -> Result<Baseline, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_json(&text)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::runner::Runner;

    fn lint(src: &str) -> Vec<LintDiagnostic> {
        Runner::new(Registry::from_config(&crate::config::Config::default()))
            .run_source(src)
            .diagnostics
    }

    #[test]
    fn round_trips_and_filters() {
        let src = "X = a == b;\n";
        let diags = lint(src);
        assert!(!diags.is_empty(), "fixture should produce findings");

        let mut b = Baseline::default();
        b.record("demo.m1scr", src, &diags);
        let b2 = Baseline::from_json(&b.to_json()).unwrap();

        let mut filtered = diags.clone();
        b2.filter("demo.m1scr", src, &mut filtered);
        assert!(filtered.is_empty(), "all recorded findings suppressed");
    }

    #[test]
    fn line_anchoring_survives_insertions() {
        let src = "X = a == b;\n";
        let mut b = Baseline::default();
        b.record("demo.m1scr", src, &lint(src));

        // Same offending line, now shifted down two lines.
        let moved = "Y = 1;\nZ = 2;\nX = a == b;\n";
        let mut diags = lint(moved);
        b.filter("demo.m1scr", moved, &mut diags);
        assert!(
            diags.iter().all(|d| d.inner.range.start.line != 2),
            "baselined finding must stay suppressed after moving lines: {diags:?}"
        );
    }

    #[test]
    fn new_findings_still_report() {
        let src = "X = a == b;\n";
        let mut b = Baseline::default();
        b.record("demo.m1scr", src, &lint(src));

        // A NEW, different violation appears.
        let newer = "X = a == b;\nW = c != d;\n";
        let mut diags = lint(newer);
        b.filter("demo.m1scr", newer, &mut diags);
        assert!(
            diags.iter().any(|d| d.inner.range.start.line == 1),
            "the new finding must remain: {diags:?}"
        );
    }

    #[test]
    fn other_files_are_not_suppressed() {
        let src = "X = a == b;\n";
        let mut b = Baseline::default();
        b.record("one.m1scr", src, &lint(src));
        let mut diags = lint(src);
        b.filter("two.m1scr", src, &mut diags);
        assert!(!diags.is_empty(), "different file must not be suppressed");
    }

    #[test]
    fn multiset_budget_is_respected() {
        // One baselined occurrence; two identical lines now → one reports.
        let src = "X = a == b;\n";
        let mut b = Baseline::default();
        b.record("demo.m1scr", src, &lint(src));
        let doubled = "X = a == b;\nX = a == b;\n";
        let mut diags = lint(doubled);
        let before = diags.len();
        b.filter("demo.m1scr", doubled, &mut diags);
        assert_eq!(
            diags.len(),
            before / 2,
            "exactly the baselined count is suppressed: {diags:?}"
        );
    }
}

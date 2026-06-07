//! Runner — orchestrates parsing and rule execution.

use std::path::Path;

use m1_core::Node;

use crate::diagnostic::LintDiagnostic;
use crate::registry::Registry;

/// The result of linting a single file.
#[derive(Debug, Default)]
pub struct RunResult {
    /// Diagnostics from lint rules.
    pub diagnostics: Vec<LintDiagnostic>,
    /// Syntax errors from the parser.
    pub syntax_errors: Vec<m1_core::Diagnostic>,
}

/// Runs all registered rules over a source file.
pub struct Runner {
    registry: Registry,
}

impl Runner {
    /// Construct a runner from a registry.
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    /// Lint a source string (no file I/O).
    pub fn run_source(&self, source: &str) -> RunResult {
        let cst = m1_core::parse(source);
        let mut result = RunResult {
            // `syntax_diagnostics` already returns an owned Vec.
            syntax_errors: cst.syntax_diagnostics(),
            ..Default::default()
        };

        // Split into lines for file-level rules.
        let lines: Vec<&str> = source.split('\n').collect();

        for rule in self.registry.rules() {
            rule.check_file(source, &lines, &mut result.diagnostics);
            // CST-aware file pass: hand each rule the CST already parsed above so
            // a rule needing the parse tree at file scope never re-parses.
            rule.check_file_cst(&cst, source, &lines, &mut result.diagnostics);
        }

        // Walk the CST depth-first (pre-order).
        let root = cst.root();
        self.walk(&root, source, &mut result.diagnostics);

        // Honour `// @m1:allow(L0xx)` annotations: drop suppressed diagnostics.
        suppress_allowed(source, &cst, &mut result.diagnostics);

        // Sort diagnostics by start position.
        result
            .diagnostics
            .sort_by_key(|d| (d.inner.range.start.line, d.inner.range.start.column));

        result
    }

    /// Lint a file on disk.
    ///
    /// Reads through the shared tolerant decoder ([`m1_workspace::read_text`]):
    /// MoTeC `.m1scr` files declare UTF-8 but emit Windows-1252 bytes for
    /// non-ASCII characters (a yaw-rate unit `°/s` stores `°` as the single byte
    /// `0xB0`). A strict UTF-8 read would reject such a valid file as
    /// "unreadable" and skip it; the tolerant decode lints it instead (#66).
    pub fn run_file(&self, path: &Path) -> std::io::Result<RunResult> {
        let source = m1_workspace::read_text(path)?;
        Ok(self.run_source(&source))
    }

    /// Apply safe autofixes to a source string, one pass. See [`crate::fix::Fixer`].
    pub fn fix_source(&self, source: &str) -> Result<Option<String>, crate::fix::FixError> {
        crate::fix::Fixer::new(&self.registry).fix_source(source)
    }

    /// Apply autofixes repeatedly until the source stabilises (no further
    /// edits) or a safety cap is hit. A single pass drops edits that overlap an
    /// earlier-accepted edit, so two rules fixing the same token would leave the
    /// file partially fixed and re-trigger on the next run. Iterating applies
    /// the dropped fixes too, making `--fix` idempotent in one invocation (#13).
    ///
    /// Returns `Ok(Some(fixed))` if any pass changed the source, `Ok(None)` if
    /// it was already clean. An unsafe fix on the first pass propagates as
    /// before; once some safe fixes have applied, an unsafe later pass simply
    /// stops the loop and keeps what was safely applied.
    pub fn fix_source_stable(&self, source: &str) -> Result<Option<String>, crate::fix::FixError> {
        const MAX_PASSES: usize = 10;
        let mut current = source.to_string();
        let mut changed = false;
        for _ in 0..MAX_PASSES {
            match self.fix_source(&current) {
                Ok(Some(next)) => {
                    current = next;
                    changed = true;
                }
                Ok(None) => break,
                Err(_) if changed => break, // keep the safe fixes already applied
                Err(e) => return Err(e),
            }
        }
        Ok(changed.then_some(current))
    }

    /// Apply safe autofixes to a file, writing it back when changed.
    /// Returns `Ok(true)` if the file was modified.
    pub fn fix_file(&self, path: &Path) -> std::io::Result<bool> {
        let source = m1_workspace::read_text(path)?;
        match self.fix_source_stable(&source) {
            Ok(Some(fixed)) => {
                // Atomic write: a crash / ENOSPC / I/O error mid-write must never
                // truncate the original script. Write a same-directory temp,
                // fsync, then rename over the target (#83). Shared with m1-fmt via
                // the `m1_workspace::atomic_write` helper.
                m1_workspace::atomic_write(path, fixed.as_bytes())?;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(e) => Err(std::io::Error::other(e.to_string())),
        }
    }

    fn walk(&self, node: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
        for rule in self.registry.rules() {
            rule.check_node(node, source, diags);
        }
        for child in node.children() {
            self.walk(&child, source, diags);
        }
    }
}

/// Drop diagnostics suppressed by an `// @m1:allow(L0xx, …)` annotation
/// (m1-core#33). `@allow(L010)` suppresses only the listed codes; a bare
/// `@allow` suppresses every code on its target construct.
///
/// Suppression is **line-based**: a diagnostic is dropped when its line falls
/// within the annotated construct's line span. Lint diagnostics are
/// line-oriented (trailing whitespace sits *past* a statement's `;`, beyond the
/// statement node's byte range), so a line span is the right granularity.
///
/// The seed registry is used for parsing so annotations owned by other tools
/// (`@m1:requires-finite`, …) are not treated as unknown here; m1-lint consumes
/// only `@allow`.
fn suppress_allowed(source: &str, cst: &m1_core::Cst, diags: &mut Vec<LintDiagnostic>) {
    let anns = m1_core::annotations(cst, &m1_core::Registry::seed());
    // (start_line, end_line inclusive, annotation) for each `@allow` with a target.
    let spans: Vec<(u32, u32, &m1_core::Annotation)> = anns
        .all()
        .iter()
        .filter(|a| a.kind == "allow")
        .filter_map(|a| {
            let t = a.target_byte_range.as_ref()?;
            Some((
                byte_line(source, t.start),
                byte_line(source, t.end.saturating_sub(1)),
                a,
            ))
        })
        .collect();
    if spans.is_empty() {
        return;
    }
    diags.retain(|d| {
        let line = d.inner.range.start.line;
        let code = d.code.to_string();
        !spans.iter().any(|(start, end, a)| {
            line >= *start && line <= *end && (a.args.is_empty() || a.has_positional(&code))
        })
    });
}

/// 0-based line number of `byte` within `source` (count of preceding newlines).
fn byte_line(source: &str, byte: usize) -> u32 {
    let b = byte.min(source.len());
    source.as_bytes()[..b]
        .iter()
        .filter(|&&c| c == b'\n')
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::LintCode;
    use crate::registry::Registry;

    #[test]
    fn empty_registry_no_diagnostics() {
        let runner = Runner::new(Registry::empty());
        let result = runner.run_source("x = 1;\n");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn runner_does_not_panic_on_empty_source() {
        let runner = Runner::new(Registry::empty());
        let result = runner.run_source("");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp() {
        // #83: the fixed content must reach disk via a temp-file + rename, so a
        // crash mid-write can never truncate the original. Verify the helper
        // writes the new bytes and leaves no stray `.tmp` sibling behind.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Widget.m1scr");
        std::fs::write(&path, b"old contents\n").unwrap();
        m1_workspace::atomic_write(&path, b"new contents\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new contents\n");
        // No leftover temp file in the directory.
        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(strays.is_empty(), "temp files left behind: {strays:?}");
    }

    #[test]
    fn fix_file_uses_atomic_write() {
        // End-to-end: --fix path rewrites the file in place with the fixed source.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Widget.m1scr");
        std::fs::write(&path, "x = a == b;\n").unwrap();
        let runner = Runner::new(crate::registry::Registry::default());
        let changed = runner.fix_file(&path).unwrap();
        assert!(changed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x = a eq b;\n");
    }

    #[test]
    fn fix_source_rewrites_eq_eq() {
        let runner = Runner::new(crate::registry::Registry::default());
        let out = runner.fix_source("x = a == b;\n").unwrap();
        assert_eq!(out.as_deref(), Some("x = a eq b;\n"));
    }

    #[test]
    fn fix_source_none_when_clean() {
        let runner = Runner::new(crate::registry::Registry::default());
        assert_eq!(runner.fix_source("x = a eq b;\n").unwrap(), None);
    }

    #[test]
    fn fix_source_stable_reaches_fixed_point() {
        let runner = Runner::new(crate::registry::Registry::default());
        // Multiple rules fire (==/&& rewrites). The stable fixer loops until no
        // edit remains, so the result is a fixed point — re-running finds
        // nothing more, even if a single pass had to drop overlapping edits (#13).
        let src = "x = a == b && c;\n";
        let fixed = runner
            .fix_source_stable(src)
            .unwrap()
            .expect("should apply at least one fix");
        assert_eq!(
            runner.fix_source(&fixed).unwrap(),
            None,
            "stable fix must be a fixed point, got residue in {fixed:?}"
        );
    }

    #[test]
    fn fix_source_stable_none_when_clean() {
        let runner = Runner::new(crate::registry::Registry::default());
        assert_eq!(runner.fix_source_stable("x = a eq b;\n").unwrap(), None);
    }

    fn has(diags: &[LintDiagnostic], code: LintCode) -> bool {
        diags.iter().any(|d| d.code == code)
    }

    #[test]
    fn allow_annotation_suppresses_listed_code() {
        let runner = Runner::new(Registry::default());
        // Trailing whitespace on the assignment line fires L002 (and sits past
        // the statement's `;`, so this also exercises line- vs byte-span).
        let dirty = "Ratio = 2; \n";
        assert!(has(&runner.run_source(dirty).diagnostics, LintCode::L002));

        let allowed = "// @m1:allow(L002)\nRatio = 2; \n";
        assert!(!has(
            &runner.run_source(allowed).diagnostics,
            LintCode::L002
        ));
    }

    #[test]
    fn allow_annotation_does_not_suppress_other_codes() {
        let runner = Runner::new(Registry::default());
        // Allow L010 only; the L002 trailing-whitespace must still fire.
        let src = "// @m1:allow(L010)\nRatio = 2; \n";
        assert!(has(&runner.run_source(src).diagnostics, LintCode::L002));
    }

    #[test]
    fn bare_allow_suppresses_every_code_on_the_line() {
        let runner = Runner::new(Registry::default());
        let src = "// @m1:allow\nRatio = 2; \n";
        assert!(!has(&runner.run_source(src).diagnostics, LintCode::L002));
    }

    #[test]
    fn trailing_allow_suppresses_on_the_same_line() {
        let runner = Runner::new(Registry::default());
        // Trailing-form annotation attaches to the statement it follows.
        let src = "Ratio = 2; // @m1:allow(L002)\n";
        // The space before `//` is not line-trailing, so introduce one another way:
        let dirty = "Ratio = 2 ; \n"; // trailing space after the ;
        assert!(has(&runner.run_source(dirty).diagnostics, LintCode::L002));
        let allowed = "Ratio = 2 ; // @m1:allow(L002)\n";
        assert!(!has(
            &runner.run_source(allowed).diagnostics,
            LintCode::L002
        ));
        let _ = src;
    }
}

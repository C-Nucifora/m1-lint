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
        }

        // Walk the CST depth-first (pre-order).
        let root = cst.root();
        self.walk(&root, source, &mut result.diagnostics);

        // Sort diagnostics by start position.
        result
            .diagnostics
            .sort_by_key(|d| (d.inner.range.start.line, d.inner.range.start.column));

        result
    }

    /// Lint a file on disk.
    pub fn run_file(&self, path: &Path) -> std::io::Result<RunResult> {
        let source = std::fs::read_to_string(path)?;
        Ok(self.run_source(&source))
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

#[cfg(test)]
mod tests {
    use super::*;
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
}

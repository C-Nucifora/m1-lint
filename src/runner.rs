//! Runner — orchestrates parsing and rule execution.

use std::path::Path;

use crate::registry::Registry;

/// The result of linting a single file. (Stub; implemented in task 3.)
#[derive(Debug, Default)]
pub struct RunResult {
    /// Diagnostics from lint rules.
    pub diagnostics: Vec<crate::diagnostic::LintDiagnostic>,
    /// Syntax errors from the parser.
    pub syntax_errors: Vec<m1_core::Diagnostic>,
}

/// Runs all registered rules over a source file. (Stub.)
pub struct Runner {
    #[allow(dead_code)]
    registry: Registry,
}

impl Runner {
    /// Construct a runner from a registry.
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    /// Lint a file on disk. (Stub.)
    pub fn run_file(&self, path: &Path) -> std::io::Result<RunResult> {
        let _ = std::fs::read_to_string(path)?;
        Ok(RunResult::default())
    }
}

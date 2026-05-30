//! Lint-specific diagnostic types.

use m1_core::{Diagnostic, Range, Severity};
use std::fmt;

/// A lint rule code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintCode {
    /// L001 — line-too-long
    L001,
    /// L002 — trailing-whitespace
    L002,
    /// L003 — missing-final-newline
    L003,
    /// L004 — eq-operator-preferred
    L004,
    /// L005 — logical-operator-preferred
    L005,
    /// L006 — float-eq-comparison
    L006,
    /// L007 — operator-spacing
    L007,
    /// L008 — nesting-too-deep
    L008,
    /// L009 — cyclomatic-complexity
    L009,
}

impl fmt::Display for LintCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LintCode::L001 => write!(f, "L001"),
            LintCode::L002 => write!(f, "L002"),
            LintCode::L003 => write!(f, "L003"),
            LintCode::L004 => write!(f, "L004"),
            LintCode::L005 => write!(f, "L005"),
            LintCode::L006 => write!(f, "L006"),
            LintCode::L007 => write!(f, "L007"),
            LintCode::L008 => write!(f, "L008"),
            LintCode::L009 => write!(f, "L009"),
        }
    }
}

/// A diagnostic emitted by a lint rule.
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    /// The lint rule code.
    pub code: LintCode,
    /// The underlying diagnostic (range, severity, message).
    pub inner: Diagnostic,
}

impl LintDiagnostic {
    /// Construct a new `LintDiagnostic`.
    ///
    /// The `inner.code` field is set to the placeholder
    /// `m1_core::Code::SyntaxError`; `m1_core::Code` has no lint variant. The
    /// meaningful code is [`LintDiagnostic::code`].
    pub fn new(
        code: LintCode,
        range: Range,
        byte_range: std::ops::Range<usize>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            inner: Diagnostic {
                range,
                byte_range,
                severity,
                code: m1_core::Code::SyntaxError,
                message: message.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_codes() {
        assert_eq!(LintCode::L001.to_string(), "L001");
        assert_eq!(LintCode::L009.to_string(), "L009");
    }
}

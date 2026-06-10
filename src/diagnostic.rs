//! Lint-specific diagnostic types.

use m1_core::{Diagnostic, Range, Severity};
use std::fmt;

use crate::config::Config;
use crate::rules::Rule;

/// The single source of truth for the lint rule set.
///
/// Each entry binds a [`LintCode`] variant to everything that varies per rule —
/// its stable name, its fixability and default-on flags, and how to construct
/// the boxed [`Rule`] from a [`Config`]. The macro derives the `LintCode` enum,
/// its `Display`/[`LintCode::name`]/[`LintCode::all_codes`]/[`LintCode::fixable`]/
/// [`LintCode::off_by_default`] surface, *and* [`LintCode::build_rule`] (used by
/// [`Registry::from_config`][crate::registry::Registry::from_config]) from one
/// place, so adding a rule is a single new entry rather than ~5 lock-step edits
/// scattered across files.
///
/// Entry grammar: `Variant => name, fixable, off_by_default, |cfg| <Rule expr>`.
macro_rules! define_rules {
    (
        $(
            $(#[$meta:meta])*
            $variant:ident => $name:literal, $fixable:literal, $off:literal, |$cfg:ident| $build:expr
        ),+ $(,)?
    ) => {
        /// A lint rule code.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum LintCode {
            $(
                $(#[$meta])*
                $variant,
            )+
        }

        impl fmt::Display for LintCode {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $( LintCode::$variant => write!(f, stringify!($variant)), )+
                }
            }
        }

        impl LintCode {
            /// Every lint code, in numeric order.
            pub fn all_codes() -> &'static [LintCode] {
                &[ $( LintCode::$variant, )+ ]
            }

            /// Stable human-readable rule name.
            pub fn name(&self) -> &'static str {
                match self {
                    $( LintCode::$variant => $name, )+
                }
            }

            /// Whether `m1-lint --fix` can mechanically fix this rule's diagnostics.
            pub fn fixable(&self) -> bool {
                match self {
                    $( LintCode::$variant => $fixable, )+
                }
            }

            /// Whether this rule is *off by default* (still selectable via
            /// `--select` or `.m1lint.toml`). L017 (magic-number) is
            /// manual-recommended but fires very often on real scaling/threshold
            /// code, so it ships opt-in to avoid drowning the default output.
            pub fn off_by_default(&self) -> bool {
                match self {
                    $( LintCode::$variant => $off, )+
                }
            }

            /// Construct the boxed [`Rule`] for this code, applying the
            /// configured thresholds from `cfg`. This is the construction half of
            /// the single source of truth consumed by
            /// [`Registry::from_config`][crate::registry::Registry::from_config].
            pub(crate) fn build_rule(&self, cfg: &Config) -> Box<dyn Rule> {
                use crate::rules::*;
                match self {
                    $(
                        LintCode::$variant => {
                            #[allow(unused_variables)]
                            let $cfg = cfg;
                            Box::new($build)
                        }
                    )+
                }
            }
        }
    };
}

define_rules! {
    /// L001 — line-too-long
    L001 => "line-too-long", false, false,
        |cfg| l001_line_too_long::LineTooLong { max_len: cfg.max_line_length },
    /// L002 — trailing-whitespace
    L002 => "trailing-whitespace", true, false,
        |cfg| l002_trailing_whitespace::TrailingWhitespace,
    /// L003 — missing-final-newline
    L003 => "missing-final-newline", true, false,
        |cfg| l003_missing_final_newline::MissingFinalNewline,
    /// L004 — eq-operator-preferred
    L004 => "eq-operator-preferred", true, false,
        |cfg| l004_eq_operator_preferred::EqOperatorPreferred,
    /// L005 — logical-operator-preferred
    L005 => "logical-operator-preferred", true, false,
        |cfg| l005_logical_operator_preferred::LogicalOperatorPreferred,
    /// L006 — float-eq-comparison
    L006 => "float-eq-comparison", false, false,
        |cfg| l006_float_eq_comparison::FloatEqComparison,
    /// L007 — operator-spacing
    L007 => "operator-spacing", true, false,
        |cfg| l007_operator_spacing::OperatorSpacing,
    /// L008 — nesting-too-deep
    L008 => "nesting-too-deep", false, false,
        |cfg| l008_nesting_too_deep::NestingTooDeep { max_depth: cfg.max_nesting_depth },
    /// L009 — cyclomatic-complexity
    L009 => "cyclomatic-complexity", false, false,
        |cfg| l009_cyclomatic_complexity::CyclomaticComplexity { max_complexity: cfg.max_complexity },
    /// L010 — indentation-style (L010 is historically "tab-for-indentation")
    L010 => "indentation-style", false, false,
        |cfg| l010_tab_indentation::Indentation { style: cfg.indent_style },
    /// L011 — comment-style
    L011 => "comment-style", true, false,
        |cfg| l011_comment_style::CommentStyle { max_line_length: cfg.max_line_length },
    /// L012 — unused-local
    L012 => "unused-local", false, false,
        |cfg| l012_unused_local::UnusedLocal,
    /// L014 — expand-undefined-variable (L013 is reserved for the DBC-range rule)
    L014 => "expand-undefined-variable", false, false,
        |cfg| l014_expand_undefined_variable::ExpandUndefinedVariable,
    /// L015 — local-missing-initializer
    L015 => "local-missing-initializer", false, false,
        |cfg| l015_local_missing_initializer::LocalMissingInitializer,
    /// L016 — local-variable-naming
    L016 => "local-variable-naming", false, false,
        |cfg| l016_local_variable_naming::LocalVariableNaming,
    /// L017 — magic-number
    L017 => "magic-number", false, true,
        |cfg| l017_magic_number::MagicNumber,
    /// L018 — semicolon-spacing
    L018 => "semicolon-spacing", true, false,
        |cfg| l018_semicolon_spacing::SemicolonSpacing,
    /// L019 — cognitive-complexity
    L019 => "cognitive-complexity", false, false,
        |cfg| l019_cognitive_complexity::CognitiveComplexity { max_complexity: cfg.max_cognitive_complexity },
    /// L020 — object-naming (manual p.64: objects begin with an uppercase letter)
    L020 => "object-naming", false, false,
        |cfg| l020_object_naming::ObjectNaming,
    /// L021 — one-statement-per-line (manual p.65)
    L021 => "one-statement-per-line", false, false,
        |cfg| l021_one_statement_per_line::OneStatementPerLine,
    /// L022 — keyword-paren-spacing (manual p.65: `if (`, not `if(`)
    L022 => "keyword-paren-spacing", true, false,
        |cfg| l022_keyword_paren_spacing::KeywordParenSpacing,
    /// L023 — call-paren-spacing (manual p.65: `Func(`, not `Func (`)
    L023 => "call-paren-spacing", true, false,
        |cfg| l023_call_paren_spacing::CallParenSpacing,
    /// L024 — ternary-condition-parens (manual p.67: `(condition) ? a : b`)
    L024 => "ternary-condition-parens", true, false,
        |cfg| l024_ternary_condition_parens::TernaryConditionParens,
    /// L025 — local-scope-too-wide (manual p.67: most constrained scope)
    L025 => "local-scope-too-wide", false, false,
        |cfg| l025_local_scope::LocalScopeTooWide,
}

impl LintCode {
    /// Parse a code string such as `"L004"`.
    pub fn from_code_str(s: &str) -> Option<LintCode> {
        LintCode::all_codes()
            .iter()
            .copied()
            .find(|c| c.to_string() == s)
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

    #[test]
    fn round_trips_code_str() {
        assert_eq!(LintCode::from_code_str("L004"), Some(LintCode::L004));
        assert_eq!(LintCode::from_code_str("L011"), Some(LintCode::L011));
        assert_eq!(LintCode::from_code_str("nope"), None);
    }

    #[test]
    fn all_codes_has_eighteen() {
        assert_eq!(LintCode::all_codes().len(), 24);
    }

    #[test]
    fn fixable_flags() {
        assert!(LintCode::L004.fixable());
        assert!(!LintCode::L001.fixable());
        assert!(!LintCode::L006.fixable());
    }
}

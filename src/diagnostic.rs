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
/// Entry grammar:
/// `Variant => name, severity, fixable, off_by_default, summary, |cfg| <Rule expr>`.
macro_rules! define_rules {
    (
        $(
            $(#[$meta:meta])*
            $variant:ident => $name:literal, $severity:literal, $fixable:literal, $off:literal,
                $summary:literal, |$cfg:ident| $build:expr
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

            /// The rule's static default severity — the [`Severity`] its
            /// diagnostics carry absent configuration. As a string
            /// (`"error"` / `"warning"`) because that is the catalogue's
            /// interchange form (`--rules --format json`, schema v2).
            pub fn severity(&self) -> &'static str {
                match self {
                    $( LintCode::$variant => $severity, )+
                }
            }

            /// One-line imperative summary of what the rule flags, matching the
            /// README "Rules" table. Exported in the catalogue (schema v2) so
            /// editor pickers don't have to hand-maintain a copy.
            pub fn summary(&self) -> &'static str {
                match self {
                    $( LintCode::$variant => $summary, )+
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
    L001 => "line-too-long", "warning", false, false,
        "line exceeds the configured maximum length",
        |cfg| l001_line_too_long::LineTooLong { max_len: cfg.max_line_length },
    /// L002 — trailing-whitespace
    L002 => "trailing-whitespace", "warning", true, false,
        "trailing whitespace at end of line",
        |cfg| l002_trailing_whitespace::TrailingWhitespace,
    /// L003 — missing-final-newline
    L003 => "missing-final-newline", "warning", true, false,
        "file does not end with a newline",
        |cfg| l003_missing_final_newline::MissingFinalNewline,
    /// L004 — eq-operator-preferred
    L004 => "eq-operator-preferred", "warning", true, false,
        "prefer `eq`/`neq` over `==`/`!=`",
        |cfg| l004_eq_operator_preferred::EqOperatorPreferred,
    /// L005 — logical-operator-preferred
    L005 => "logical-operator-preferred", "warning", true, false,
        "prefer the spelled logical operators (and/or/not)",
        |cfg| l005_logical_operator_preferred::LogicalOperatorPreferred,
    /// L006 — float-eq-comparison
    L006 => "float-eq-comparison", "error", false, false,
        "float compared with an equality operator",
        |cfg| l006_float_eq_comparison::FloatEqComparison,
    /// L007 — operator-spacing
    L007 => "operator-spacing", "warning", true, false,
        "missing space around an operator",
        |cfg| l007_operator_spacing::OperatorSpacing,
    /// L008 — nesting-too-deep
    L008 => "nesting-too-deep", "warning", false, false,
        "block nesting exceeds the configured depth",
        |cfg| l008_nesting_too_deep::NestingTooDeep { max_depth: cfg.max_nesting_depth },
    /// L009 — cyclomatic-complexity
    L009 => "cyclomatic-complexity", "warning", false, false,
        "cyclomatic complexity exceeds the configured ceiling",
        |cfg| l009_cyclomatic_complexity::CyclomaticComplexity { max_complexity: cfg.max_complexity },
    /// L010 — indentation-style (L010 is historically "tab-for-indentation")
    L010 => "indentation-style", "warning", false, false,
        "indentation does not match the configured style",
        |cfg| l010_tab_indentation::Indentation { style: cfg.indent_style },
    /// L011 — comment-style
    L011 => "comment-style", "warning", true, false,
        "`//` comment needs a space after the slashes",
        |cfg| l011_comment_style::CommentStyle { max_line_length: cfg.max_line_length },
    /// L012 — unused-local
    L012 => "unused-local", "warning", false, false,
        "local binding is never used",
        |cfg| l012_unused_local::UnusedLocal,
    /// L014 — expand-undefined-variable (L013 is reserved for the DBC-range rule)
    L014 => "expand-undefined-variable", "warning", false, false,
        "expand body references an undefined $(VAR)",
        |cfg| l014_expand_undefined_variable::ExpandUndefinedVariable,
    /// L015 — local-missing-initializer
    L015 => "local-missing-initializer", "warning", false, false,
        "local declared without an initializer",
        |cfg| l015_local_missing_initializer::LocalMissingInitializer,
    /// L016 — local-variable-naming
    L016 => "local-variable-naming", "warning", false, false,
        "local name does not follow the naming convention",
        |cfg| l016_local_variable_naming::LocalVariableNaming,
    /// L017 — magic-number
    L017 => "magic-number", "warning", false, true,
        "unnamed numeric literal (magic number)",
        |cfg| l017_magic_number::MagicNumber,
    /// L018 — semicolon-spacing
    L018 => "semicolon-spacing", "warning", true, false,
        "incorrect spacing around a semicolon",
        |cfg| l018_semicolon_spacing::SemicolonSpacing,
    /// L019 — cognitive-complexity
    L019 => "cognitive-complexity", "warning", false, false,
        "cognitive complexity exceeds the configured ceiling",
        |cfg| l019_cognitive_complexity::CognitiveComplexity { max_complexity: cfg.max_cognitive_complexity },
    /// L020 — object-naming (manual p.64: objects begin with an uppercase letter)
    L020 => "object-naming", "warning", false, false,
        "object names begin with an uppercase letter",
        |cfg| l020_object_naming::ObjectNaming,
    /// L021 — one-statement-per-line (manual p.65)
    L021 => "one-statement-per-line", "warning", true, false,
        "write only one statement per line",
        |cfg| l021_one_statement_per_line::OneStatementPerLine,
    /// L022 — keyword-paren-spacing (manual p.65: `if (`, not `if(`)
    L022 => "keyword-paren-spacing", "warning", true, false,
        "put a space between a keyword and its parenthesis",
        |cfg| l022_keyword_paren_spacing::KeywordParenSpacing,
    /// L023 — call-paren-spacing (manual p.65: `Func(`, not `Func (`)
    L023 => "call-paren-spacing", "warning", true, false,
        "no space between a function name and its parenthesis",
        |cfg| l023_call_paren_spacing::CallParenSpacing,
    /// L024 — ternary-condition-parens (manual p.67: `(condition) ? a : b`)
    L024 => "ternary-condition-parens", "warning", true, false,
        "wrap a ternary condition in parentheses",
        |cfg| l024_ternary_condition_parens::TernaryConditionParens,
    /// L025 — local-scope-too-wide (manual p.67: most constrained scope)
    L025 => "local-scope-too-wide", "warning", false, false,
        "local declared in a wider scope than its uses need",
        |cfg| l025_local_scope::LocalScopeTooWide,
    /// L026 — top-level-indentation (manual p.65: all code begins in the first column)
    L026 => "top-level-indentation", "warning", true, false,
        "top-level code begins in the first column",
        |cfg| l026_top_level_indentation::TopLevelIndentation,
    /// L027 — file-final-blank-line (manual p.65: functions and methods end with
    /// a blank line; an .m1scr file *is* a method/function body). Opt-in: the
    /// real corpora don't follow it and m1-fmt's default trailing-newline
    /// normalisation strips the blank line — enable it together with the
    /// formatter knob that preserves one (see the rule docs).
    L027 => "file-final-blank-line", "warning", true, true,
        "function/method script ends with a blank line",
        |cfg| l027_file_final_blank_line::FileFinalBlankLine,
    /// L028 — brace-style (manual p.65: "a separate line for each brace" — Allman).
    /// Default-on, like L010: the manual mandates Allman, so a K&R brace is flagged
    /// by default; `brace-style = "kr"` flips it. Not `--fix`able — m1-fmt performs
    /// the reformat.
    L028 => "brace-style", "warning", false, false,
        "braces follow the configured style (default Allman, manual p.65)",
        |cfg| l028_brace_style::BraceStylePlacement { style: cfg.brace_style },
    /// L029 — indentation-depth (manual p.65: "indent conditional block by one
    /// tab stop"). Flags a nested statement whose leading indentation is not one
    /// level per enclosing block. Not `--fix`able — m1-fmt performs the reflow.
    /// Measured in the configured `indent_style`. Opt-in (off by default): the
    /// real corpora keep house-style layout (under-indented Allman bodies, K&R,
    /// spaces) that this rule flags en masse, so — like L017/L027 — it ships
    /// off and is enabled with `--select L029` (run alongside m1-fmt, which
    /// performs the reflow this rule only reports).
    L029 => "indentation-depth", "warning", false, true,
        "nested block is not indented one level per enclosing block (manual p.65)",
        |cfg| l029_indentation_depth::IndentationDepth { style: cfg.indent_style },
    /// L030 — clause-parentheses (manual p.65: "use parentheses to clarify
    /// clauses in an expression"; its example wraps each comparison sub-clause:
    /// `((a > b) and (b < c))`). Flags a relational/equality comparison that is
    /// an operand of a logical `and`/`or` and is not already parenthesized.
    /// `--fix` wraps the complete comparison subexpression — never changes the
    /// parse. Opt-in (off by default): the real corpora write compound booleans
    /// unparenthesized, so — like L017/L027/L029 — it ships off and is enabled
    /// with `--select L030`.
    L030 => "clause-parentheses", "warning", true, true,
        "wrap a comparison clause of a logical and/or in parentheses (manual p.65)",
        |cfg| l030_clause_parentheses::ClauseParentheses,
}

impl LintCode {
    /// Parse a code string such as `"L004"`.
    pub fn from_code_str(s: &str) -> Option<LintCode> {
        LintCode::all_codes()
            .iter()
            .copied()
            .find(|c| c.to_string() == s)
    }

    /// The README anchor fragment for this rule's `## Rules` subheading, used as
    /// the SARIF `helpUri` fragment so a code-scanning alert deep-links to the
    /// rule's docs. Matches GitHub's auto-generated slug for the heading
    /// `### <name> (<CODE>)` — lowercased, parentheses dropped, spaces to
    /// hyphens — i.e. `<name>-<code-lowercased>` (e.g. `line-too-long-l001`).
    /// A regression test asserts every fragment resolves to a real README
    /// anchor so this can't rot (#148).
    pub fn help_anchor(&self) -> String {
        format!("{}-{}", self.name(), self.to_string().to_lowercase())
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
    /// The `inner.code` field is the broad *category*
    /// (`m1_core::Code::LintError`); the specific rule identity is
    /// [`LintDiagnostic::code`] (the `L0xx` `LintCode`).
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
                code: m1_core::Code::LintError,
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
    fn all_codes_is_complete_and_ordered() {
        // Derived bounds rather than a hand-counted length (the old test name
        // said "eighteen" while asserting 24 — #133): the list must run from
        // L001 to the current last code with no duplicates.
        let codes = LintCode::all_codes();
        assert_eq!(codes.first(), Some(&LintCode::L001));
        assert_eq!(codes.last(), Some(&LintCode::L030));
        let mut sorted = codes.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "duplicate codes in all_codes()");
    }

    #[test]
    fn fixable_flags() {
        assert!(LintCode::L004.fixable());
        assert!(!LintCode::L001.fixable());
        assert!(!LintCode::L006.fixable());
    }
}

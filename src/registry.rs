//! Rule registry — collects all active rules.

use crate::config::Config;
use crate::diagnostic::LintCode;
use crate::rules::Rule;

/// Holds all registered lint rules.
pub struct Registry {
    pub(crate) rules: Vec<Box<dyn Rule>>,
}

impl Registry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Register a rule.
    pub fn register(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
    }

    /// All registered rules.
    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    /// Build a registry containing exactly the rules enabled by `cfg`, with
    /// the configured thresholds.
    pub fn from_config(cfg: &Config) -> Self {
        use crate::rules::*;
        let mut r = Self::empty();
        for code in &cfg.enabled {
            match code {
                LintCode::L001 => r.register(Box::new(l001_line_too_long::LineTooLong {
                    max_len: cfg.max_line_length,
                })),
                LintCode::L002 => {
                    r.register(Box::new(l002_trailing_whitespace::TrailingWhitespace))
                }
                LintCode::L003 => {
                    r.register(Box::new(l003_missing_final_newline::MissingFinalNewline))
                }
                LintCode::L004 => {
                    r.register(Box::new(l004_eq_operator_preferred::EqOperatorPreferred))
                }
                LintCode::L005 => r.register(Box::new(
                    l005_logical_operator_preferred::LogicalOperatorPreferred,
                )),
                LintCode::L006 => r.register(Box::new(l006_float_eq_comparison::FloatEqComparison)),
                LintCode::L007 => r.register(Box::new(l007_operator_spacing::OperatorSpacing)),
                LintCode::L008 => r.register(Box::new(l008_nesting_too_deep::NestingTooDeep {
                    max_depth: cfg.max_nesting_depth,
                })),
                LintCode::L009 => {
                    r.register(Box::new(l009_cyclomatic_complexity::CyclomaticComplexity {
                        max_complexity: cfg.max_complexity,
                    }))
                }
                LintCode::L010 => r.register(Box::new(l010_tab_indentation::Indentation {
                    style: cfg.indent_style,
                })),
                LintCode::L011 => r.register(Box::new(l011_comment_style::CommentStyle)),
                LintCode::L012 => r.register(Box::new(l012_unused_local::UnusedLocal)),
                LintCode::L014 => r.register(Box::new(
                    l014_expand_undefined_variable::ExpandUndefinedVariable,
                )),
                LintCode::L015 => r.register(Box::new(
                    l015_local_missing_initializer::LocalMissingInitializer,
                )),
                LintCode::L016 => {
                    r.register(Box::new(l016_local_variable_naming::LocalVariableNaming))
                }
                LintCode::L017 => r.register(Box::new(l017_magic_number::MagicNumber)),
            }
        }
        r
    }
}

impl Default for Registry {
    /// The default rule set: every rule enabled at default thresholds. This is
    /// the single source of truth for "all rules" — `from_config(&Config::default())`
    /// — so new rules can never be silently omitted from a parallel hardcoded list.
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_has_no_rules() {
        let r = Registry::empty();
        assert_eq!(r.rules().len(), 0);
    }

    #[test]
    fn from_config_applies_custom_thresholds() {
        // A line of 50 chars is fine at the default (88) but over a custom 40.
        let src = format!("// {}\n", "x".repeat(50));
        let cfg = crate::config::Config {
            max_line_length: 40,
            ..Default::default()
        };
        let run = crate::runner::Runner::new(Registry::from_config(&cfg));
        assert!(
            run.run_source(&src)
                .diagnostics
                .iter()
                .any(|d| d.code == LintCode::L001),
            "custom max_line_length should be applied by from_config"
        );
        // ...and the default registry must NOT flag it.
        let run_default = crate::runner::Runner::new(Registry::default());
        assert!(
            run_default
                .run_source(&src)
                .diagnostics
                .iter()
                .all(|d| d.code != LintCode::L001),
            "default (88) should not flag a 50-char line"
        );
    }

    #[test]
    fn from_config_respects_select() {
        let mut cfg = crate::config::Config::default();
        cfg.apply_filters(Some(vec!["L004".into()]), None).unwrap();
        let r = Registry::from_config(&cfg);
        assert_eq!(r.rules().len(), 1);
        assert_eq!(r.rules()[0].code(), LintCode::L004);
    }
}

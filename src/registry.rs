//! Rule registry — collects all active rules.

use crate::config::Config;
use crate::rules::Rule;

/// Holds all registered lint rules.
pub struct Registry {
    pub(crate) rules: Vec<Box<dyn Rule>>,
    /// Per-rule severity overrides from `[severity]` (#110), applied by the
    /// runner after rules emit.
    pub(crate) severity_overrides:
        std::collections::BTreeMap<crate::diagnostic::LintCode, m1_core::Severity>,
}

impl Registry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            severity_overrides: std::collections::BTreeMap::new(),
        }
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
    ///
    /// The per-code construction lives on `LintCode::build_rule` — the single
    /// source of truth generated alongside the enum itself — so this no longer
    /// carries a parallel match that has to stay in lock-step with the enum.
    pub fn from_config(cfg: &Config) -> Self {
        let mut r = Self::empty();
        for code in &cfg.enabled {
            r.register(code.build_rule(cfg));
        }
        r.severity_overrides = cfg.severity_overrides.clone();
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
    use crate::diagnostic::LintCode;

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

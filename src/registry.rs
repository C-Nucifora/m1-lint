//! Rule registry — collects all active rules.

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

    /// Returns the default registry for m1-lint v1.
    pub fn default_v1() -> Self {
        let mut r = Self::empty();
        r.register(Box::new(crate::rules::l001_line_too_long::LineTooLong));
        r.register(Box::new(
            crate::rules::l002_trailing_whitespace::TrailingWhitespace,
        ));
        r.register(Box::new(
            crate::rules::l003_missing_final_newline::MissingFinalNewline,
        ));
        r.register(Box::new(
            crate::rules::l004_eq_operator_preferred::EqOperatorPreferred,
        ));
        r.register(Box::new(
            crate::rules::l005_logical_operator_preferred::LogicalOperatorPreferred,
        ));
        r
    }

    /// All registered rules.
    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
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
}

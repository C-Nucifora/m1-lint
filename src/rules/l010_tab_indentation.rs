//! L010 — tab-for-indentation (stub; full impl in task 7)

use crate::diagnostic::LintCode;
use crate::rules::Rule;

pub struct TabIndentation;

impl Rule for TabIndentation {
    fn code(&self) -> LintCode { LintCode::L010 }
    fn name(&self) -> &'static str { "tab-for-indentation" }
}

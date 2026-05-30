//! L011 — comment-style (stub; full impl in task 7)

use crate::diagnostic::LintCode;
use crate::rules::Rule;

pub struct CommentStyle;

impl Rule for CommentStyle {
    fn code(&self) -> LintCode { LintCode::L011 }
    fn name(&self) -> &'static str { "comment-style" }
}

//! `char-op-string`: a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error).
//!
//! The analysis lives in [`crate::domain::char_op_string_report`], which also backs the
//! standalone `inspect char-op-string` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::char_op_string_report::examine_call;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "char-op-string",
    RuleCategory::Malformed,
    Severity::Error,
    "a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error)",
    Fixability::ReportOnly,
);

/// Every function `examine_call` recognizes as requiring character arguments.
const HEADS: [NormalizedHead; 25] = [
    NormalizedHead::new("char="),
    NormalizedHead::new("char/="),
    NormalizedHead::new("char<"),
    NormalizedHead::new("char>"),
    NormalizedHead::new("char<="),
    NormalizedHead::new("char>="),
    NormalizedHead::new("char-equal"),
    NormalizedHead::new("char-not-equal"),
    NormalizedHead::new("char-lessp"),
    NormalizedHead::new("char-greaterp"),
    NormalizedHead::new("char-not-lessp"),
    NormalizedHead::new("char-not-greaterp"),
    NormalizedHead::new("char-code"),
    NormalizedHead::new("char-int"),
    NormalizedHead::new("char-upcase"),
    NormalizedHead::new("char-downcase"),
    NormalizedHead::new("char-name"),
    NormalizedHead::new("digit-char-p"),
    NormalizedHead::new("alpha-char-p"),
    NormalizedHead::new("alphanumericp"),
    NormalizedHead::new("upper-case-p"),
    NormalizedHead::new("lower-case-p"),
    NormalizedHead::new("both-case-p"),
    NormalizedHead::new("graphic-char-p"),
    NormalizedHead::new("standard-char-p"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()> {
        let mut char_call_count = 0;
        let mut items = Vec::new();
        examine_call(view, context.path(), &mut char_call_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} is given string literal {}; it requires a character (type error)",
                    item.operator, item.literal
                ),
            );
        }
        Ok(())
    }
}

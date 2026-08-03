//! `hash-table-literal-string-key-under-eql`: a literal string key on a hash
//! table whose test compares by identity.
//!
//! The analysis lives in
//! [`crate::hash_table_literal_string_key_under_eql::domain`], which documents
//! why the rule is scoped to literal keys and what that scoping costs. This
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::hash_table_literal_string_key_under_eql::domain::examine_hash_table_literal_string_key;

pub const META: RuleMeta = RuleMeta::new(
    "hash-table-literal-string-key-under-eql",
    RuleCategory::Suspicious,
    // Every such lookup misses, always, silently. That is a wrong answer
    // rather than a slow or ugly one.
    Severity::Error,
    "a literal string key on a hash table whose eq/eql test compares by identity",
    // No fix. The repair is `:test #'equal` at the *construction* site, which
    // is a different form from the finding's, and `#'equalp` is right instead
    // whenever case should not matter — a choice about the data, not the code.
    Fixability::ReportOnly,
);

/// The two accessors that take `(key table)`. `setf` of `gethash` reaches this
/// through the `gethash` place itself, which the engine visits as its own node.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("gethash"),
    NormalizedHead::new("remhash"),
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
    ) -> LintResult<()> {
        // `binding_table()` is a whole-file semantic build, so it must not be
        // asked for before the literal-key gate. That gate lives inside
        // `examine_…`, so the call has to be *deferred into* it: passing
        // `context.binding_table()` here evaluates it now, on every `gethash`
        // in the file. That is not hypothetical — it is what this rule did
        // when first written, and it measured 1667047 ns/invocation.
        let mut keyed_accessor_count = 0;
        let mut items = Vec::new();
        examine_hash_table_literal_string_key(
            context.tree(),
            || context.binding_table(),
            view,
            &mut keyed_accessor_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

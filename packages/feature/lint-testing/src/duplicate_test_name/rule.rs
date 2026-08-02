//! `duplicate-test-name`: two top-level test definitions in one file sharing a
//! name.
//!
//! The analysis lives in [`crate::duplicate_test_name::domain`], which also
//! backs the standalone `inspect duplicate-test-name` command; this module only
//! registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::duplicate_test_name::domain::build_duplicate_test_name_report;
use crate::support::TEST_DIALECTS;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `Duplicate` — "the same key, place, test, or name given twice" names this
/// exactly.
///
/// `Severity::Error`, following `duplicate-defmethod-signature`, which is the
/// existing file-scoped duplicate-definition rule. The consequence is not a
/// judgement call: loading the file keeps the later definition and discards the
/// earlier one, so a test that was written and reviewed does not run, and
/// nothing in a green suite says so.
pub const META: RuleMeta = RuleMeta::new(
    "duplicate-test-name",
    RuleCategory::Duplicate,
    Severity::Error,
    "two test definitions in one file sharing a name",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// `WholeTree`, alone among this package's six rules, and for the same
    /// reason as `duplicate-parameters`: whether a definition shadows another
    /// is not a question about one node, so a per-node filter can only answer
    /// it by re-deriving the file's other definitions on every match.
    ///
    /// That is what the first version did, and it cost T×T on a file of T
    /// tests. `Heads` looks cheaper — it runs nothing on a file with no test
    /// definitions — but the run it makes cheap is one `WholeTree` already
    /// costs a single pass over the top-level forms, and the run it makes
    /// ruinous is the one that matters.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&TEST_DIALECTS)
    }

    /// The report already walks evaluated code only, so — unlike this
    /// package's five head-filtered rules — there is no `is_unevaluated_at`
    /// call to make here: a `'(deftest …)` never reaches `findings` at all.
    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let report =
            build_duplicate_test_name_report(context.path(), context.dialect(), context.tree())?;
        for item in report.findings {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The structural half of the cost regression, next to the timed one in
    /// `domain`: `Heads` means one `check` per test definition, and a `check`
    /// that has to look at the rest of the file is then quadratic by
    /// construction no matter how it is written. `WholeTree` means one `check`
    /// per file.
    #[test]
    fn the_correlation_runs_once_per_file_not_once_per_definition() {
        assert_eq!(RULE.head_filter(), HeadFilter::WholeTree);
    }
}
